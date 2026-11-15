//! Streamed answers (server-sent events) of the three protocols.
//!
//! Each protocol's events are collected into the JSON the same request
//! would have returned without streaming; the usual `response` functions
//! read it. Text arrives piece by piece through a callback.

use std::io::BufRead;

use serde_json::{Value, json};

use super::{AiError, codes};
use crate::settings::AiProtocol;

/// Reads server-sent events from `reader` into `collector` until the
/// stream ends or the collector is done.
pub(super) fn read(
    reader: impl BufRead,
    collector: &mut Collector,
    on_text: &mut dyn FnMut(&str),
) -> Result<(), AiError> {
    let mut event: Option<String> = None;
    let mut data = String::new();
    for line in reader.lines() {
        let line = line.map_err(|e| AiError::new(codes::UNREACHABLE, e.to_string()))?;
        let line = line.strip_suffix('\r').unwrap_or(&line);
        if line.is_empty() {
            if !data.is_empty() && collector.feed(event.as_deref(), &data, on_text)? {
                return Ok(());
            }
            event = None;
            data.clear();
        } else if let Some(v) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(v.strip_prefix(' ').unwrap_or(v));
        } else if let Some(v) = line.strip_prefix("event:") {
            event = Some(v.trim().to_owned());
        }
        // `id:`, `retry:` und Kommentare (`:`) werden nicht gebraucht.
    }
    if !data.is_empty() {
        collector.feed(event.as_deref(), &data, on_text)?;
    }
    Ok(())
}

/// Collects the events of one streamed answer.
#[derive(Debug)]
pub(super) enum Collector {
    Anthropic(Anthropic),
    OpenAi(OpenAi),
    Gemini(Gemini),
}

impl Collector {
    pub fn new(protocol: AiProtocol) -> Self {
        match protocol {
            AiProtocol::Anthropic => Self::Anthropic(Anthropic::default()),
            AiProtocol::OpenAi => Self::OpenAi(OpenAi::default()),
            AiProtocol::Gemini => Self::Gemini(Gemini::default()),
        }
    }

    /// Takes one event; `true` once the answer is complete.
    fn feed(
        &mut self,
        event: Option<&str>,
        data: &str,
        on_text: &mut dyn FnMut(&str),
    ) -> Result<bool, AiError> {
        if data.trim() == "[DONE]" {
            return Ok(true);
        }
        let v: Value = serde_json::from_str(data).map_err(|e| {
            AiError::new(codes::BAD_RESPONSE, format!("Ereignis ist kein JSON: {e}"))
        })?;
        if event == Some("error") || v.get("error").is_some_and(|e| !e.is_null()) {
            return Err(stream_error(&v));
        }
        Ok(match self {
            Self::Anthropic(c) => c.feed(&v, on_text),
            Self::OpenAi(c) => c.feed(&v, on_text),
            Self::Gemini(c) => c.feed(&v, on_text),
        })
    }

    /// The answer as the protocol returns it without streaming.
    pub fn finish(self) -> Value {
        match self {
            Self::Anthropic(c) => c.finish(),
            Self::OpenAi(c) => c.finish(),
            Self::Gemini(c) => c.finish(),
        }
    }
}

/// An error event in the middle of the stream.
fn stream_error(v: &Value) -> AiError {
    let e = &v["error"];
    let message = e["message"]
        .as_str()
        .or(e.as_str())
        .unwrap_or("Fehler im Datenstrom");
    let code = match e["type"].as_str().or(e["status"].as_str()) {
        Some("overloaded_error" | "rate_limit_error" | "RESOURCE_EXHAUSTED") => codes::RATE_LIMITED,
        Some("authentication_error" | "permission_error" | "PERMISSION_DENIED") => {
            codes::AUTH_FAILED
        }
        _ => codes::HTTP_ERROR,
    };
    AiError::new(code, message.to_owned())
}

/// Anthropic: `message_start`, `content_block_start/delta/stop`,
/// `message_delta`, `message_stop`.
#[derive(Debug, Default)]
pub(super) struct Anthropic {
    model: String,
    blocks: Vec<Value>,
    /// Partial JSON of each tool call's input, by block index.
    inputs: Vec<String>,
    stop_reason: Value,
    input_tokens: u64,
    output_tokens: u64,
}

impl Anthropic {
    fn feed(&mut self, v: &Value, on_text: &mut dyn FnMut(&str)) -> bool {
        let index = v["index"].as_u64().unwrap_or(0) as usize;
        match v["type"].as_str() {
            Some("message_start") => {
                let m = &v["message"];
                self.model = m["model"].as_str().unwrap_or_default().to_owned();
                self.input_tokens = m["usage"]["input_tokens"].as_u64().unwrap_or(0);
                self.output_tokens = m["usage"]["output_tokens"].as_u64().unwrap_or(0);
            }
            Some("content_block_start") => {
                if self.blocks.len() <= index {
                    self.blocks.resize(index + 1, Value::Null);
                    self.inputs.resize(index + 1, String::new());
                }
                self.blocks[index] = v["content_block"].clone();
            }
            Some("content_block_delta") => {
                let d = &v["delta"];
                match (d["type"].as_str(), self.blocks.get_mut(index)) {
                    (Some("text_delta"), Some(block)) => {
                        let piece = d["text"].as_str().unwrap_or_default();
                        let text = block["text"].as_str().unwrap_or_default().to_owned() + piece;
                        block["text"] = json!(text);
                        on_text(piece);
                    }
                    (Some("input_json_delta"), Some(_)) => {
                        self.inputs[index].push_str(d["partial_json"].as_str().unwrap_or_default());
                    }
                    // Denken und Signaturen werden nicht gebraucht.
                    _ => {}
                }
            }
            Some("content_block_stop") => self.close(index),
            Some("message_delta") => {
                self.stop_reason = v["delta"]["stop_reason"].clone();
                if let Some(n) = v["usage"]["output_tokens"].as_u64() {
                    self.output_tokens = n;
                }
                if let Some(n) = v["usage"]["input_tokens"].as_u64() {
                    self.input_tokens = n;
                }
            }
            Some("message_stop") => return true,
            _ => {}
        }
        false
    }

    /// Turns the collected input JSON of a tool call into its `input`.
    fn close(&mut self, index: usize) {
        let (Some(block), Some(input)) = (self.blocks.get_mut(index), self.inputs.get_mut(index))
        else {
            return;
        };
        if block["type"] == "tool_use" && !input.is_empty() {
            block["input"] = serde_json::from_str(input).unwrap_or_else(|_| json!({}));
            input.clear();
        } else if block["type"] == "tool_use" && block["input"].is_null() {
            block["input"] = json!({});
        }
    }

    fn finish(mut self) -> Value {
        for i in 0..self.blocks.len() {
            self.close(i);
        }
        json!({
            "model": self.model,
            "content": self.blocks.into_iter().filter(|b| !b.is_null()).collect::<Vec<_>>(),
            "stop_reason": self.stop_reason,
            "usage": { "input_tokens": self.input_tokens, "output_tokens": self.output_tokens },
        })
    }
}

/// OpenAI-compatible: `choices[0].delta` with `content` and `tool_calls`
/// (by `index`), `finish_reason`, at the end `[DONE]`.
#[derive(Debug, Default)]
pub(super) struct OpenAi {
    model: String,
    text: String,
    /// `(id, name, arguments)` by `index`.
    calls: Vec<(String, String, String)>,
    finish_reason: Value,
    usage: Value,
}

impl OpenAi {
    fn feed(&mut self, v: &Value, on_text: &mut dyn FnMut(&str)) -> bool {
        if let Some(m) = v["model"].as_str() {
            m.clone_into(&mut self.model);
        }
        if v["usage"].is_object() {
            self.usage = v["usage"].clone();
        }
        let choice = &v["choices"][0];
        let delta = &choice["delta"];
        if let Some(piece) = delta["content"].as_str()
            && !piece.is_empty()
        {
            self.text.push_str(piece);
            on_text(piece);
        }
        for (n, call) in delta["tool_calls"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            let index = call["index"].as_u64().map_or(n, |i| i as usize);
            if self.calls.len() <= index {
                self.calls.resize(index + 1, Default::default());
            }
            let slot = &mut self.calls[index];
            if let Some(id) = call["id"].as_str().filter(|s| !s.is_empty()) {
                id.clone_into(&mut slot.0);
            }
            let f = &call["function"];
            if let Some(name) = f["name"].as_str()
                && slot.1.is_empty()
            {
                name.clone_into(&mut slot.1);
            }
            match &f["arguments"] {
                Value::String(s) => slot.2.push_str(s),
                Value::Null => {}
                // Ollama liefert teils schon ein Objekt.
                other => slot.2 = other.to_string(),
            }
        }
        if !choice["finish_reason"].is_null() {
            self.finish_reason = choice["finish_reason"].clone();
        }
        false
    }

    fn finish(self) -> Value {
        let calls: Vec<Value> = self
            .calls
            .into_iter()
            .filter(|(_, name, _)| !name.is_empty())
            .map(|(id, name, args)| {
                json!({ "id": id, "type": "function", "function": { "name": name, "arguments": args } })
            })
            .collect();
        let mut message = json!({ "role": "assistant", "content": self.text });
        if !calls.is_empty() {
            message["tool_calls"] = json!(calls);
        }
        json!({
            "model": self.model,
            "choices": [{ "message": message, "finish_reason": self.finish_reason }],
            "usage": self.usage,
        })
    }
}

/// Gemini (`alt=sse`): each event is a complete `GenerateContentResponse`
/// with the next parts.
#[derive(Debug, Default)]
pub(super) struct Gemini {
    model: Value,
    parts: Vec<Value>,
    finish_reason: Value,
    usage: Value,
    feedback: Value,
    candidates: bool,
}

impl Gemini {
    fn feed(&mut self, v: &Value, on_text: &mut dyn FnMut(&str)) -> bool {
        if !v["modelVersion"].is_null() {
            self.model = v["modelVersion"].clone();
        }
        if !v["usageMetadata"].is_null() {
            self.usage = v["usageMetadata"].clone();
        }
        if !v["promptFeedback"].is_null() {
            self.feedback = v["promptFeedback"].clone();
        }
        let Some(candidate) = v["candidates"].get(0) else {
            return false;
        };
        self.candidates = true;
        for p in candidate["content"]["parts"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let thought = p["thought"].as_bool() == Some(true);
            match (p["text"].as_str(), self.parts.last_mut()) {
                (Some(piece), Some(last))
                    if last["text"].is_string()
                        && (last["thought"].as_bool() == Some(true)) == thought
                        && p.get("thoughtSignature").is_none() =>
                {
                    let text = last["text"].as_str().unwrap_or_default().to_owned() + piece;
                    last["text"] = json!(text);
                    if !thought {
                        on_text(piece);
                    }
                }
                (Some(piece), _) => {
                    self.parts.push(p.clone());
                    if !thought {
                        on_text(piece);
                    }
                }
                (None, _) => self.parts.push(p.clone()),
            }
        }
        if !candidate["finishReason"].is_null() {
            self.finish_reason = candidate["finishReason"].clone();
        }
        false
    }

    fn finish(self) -> Value {
        let mut v = json!({ "modelVersion": self.model, "usageMetadata": self.usage });
        if self.candidates {
            v["candidates"] = json!([{
                "content": { "role": "model", "parts": self.parts },
                "finishReason": self.finish_reason,
            }]);
        }
        if !self.feedback.is_null() {
            v["promptFeedback"] = self.feedback;
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(protocol: AiProtocol, sse: &str) -> (Result<Value, AiError>, String) {
        let mut collector = Collector::new(protocol);
        let mut seen = String::new();
        let result = read(sse.as_bytes(), &mut collector, &mut |t| seen.push_str(t))
            .map(|()| collector.finish());
        (result, seen)
    }

    #[test]
    fn anthropic_events() {
        let sse = "event: message_start\n\
data: {\"type\":\"message_start\",\"message\":{\"model\":\"m1\",\"usage\":{\"input_tokens\":9,\"output_tokens\":1}}}\n\n\
event: ping\ndata: {\"type\":\"ping\"}\n\n\
event: content_block_start\n\
data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hal\"}}\n\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"lo\"}}\n\n\
event: content_block_start\n\
data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"t1\",\"name\":\"read_file\",\"input\":{}}}\n\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"path\\\": \\\"a\"}}\n\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\".rs\\\"}\"}}\n\n\
event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\n\
event: message_delta\n\
data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":17}}\n\n\
event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
        let (v, seen) = collect(AiProtocol::Anthropic, sse);
        let r = crate::ai::anthropic::response(&v.unwrap()).unwrap();
        assert_eq!(seen, "Hallo");
        assert_eq!(r.text(), "Hallo");
        assert_eq!(r.stop, crate::ai::StopReason::ToolUse);
        assert_eq!(r.message.tool_calls().next().unwrap().2["path"], "a.rs");
        assert_eq!((r.usage.input_tokens, r.usage.output_tokens), (9, 17));
        assert_eq!(r.model, "m1");
    }

    #[test]
    fn openai_chunks() {
        let sse = "data: {\"model\":\"m2\",\"choices\":[{\"delta\":{\"role\":\"assistant\",\"content\":\"Ich \"}}]}\n\n\
data: {\"choices\":[{\"delta\":{\"content\":\"sehe nach.\"}}]}\n\n\
data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\"type\":\"function\",\"function\":{\"name\":\"read_file\",\"arguments\":\"\"}}]}}]}\n\n\
data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"path\\\":\"}}]}}]}\n\n\
data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"b.rs\\\"}\"}}]}}]}\n\n\
data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n\
data: {\"choices\":[],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":6}}\n\n\
data: [DONE]\n\n";
        let (v, seen) = collect(AiProtocol::OpenAi, sse);
        let r = crate::ai::openai::response(&v.unwrap()).unwrap();
        assert_eq!(seen, "Ich sehe nach.");
        assert_eq!(r.text(), "Ich sehe nach.");
        assert_eq!(r.stop, crate::ai::StopReason::ToolUse);
        let call = r.message.tool_calls().next().unwrap();
        assert_eq!((call.0, call.1), ("c1", "read_file"));
        assert_eq!(call.2["path"], "b.rs");
        assert_eq!(r.usage.output_tokens, 6);
    }

    #[test]
    fn gemini_chunks() {
        let sse = "data: {\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"denke\",\"thought\":true}]}}]}\r\n\r\n\
data: {\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"Gu\"}]}}]}\r\n\r\n\
data: {\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"ten Tag\"}]}}]}\r\n\r\n\
data: {\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"functionCall\":{\"name\":\"read_file\",\"args\":{\"path\":\"c.rs\"}},\"thoughtSignature\":\"s\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":3,\"candidatesTokenCount\":4},\"modelVersion\":\"g1\"}\r\n\r\n";
        let (v, seen) = collect(AiProtocol::Gemini, sse);
        let r = crate::ai::gemini::response(&v.unwrap()).unwrap();
        assert_eq!(seen, "Guten Tag");
        assert_eq!(r.text(), "Guten Tag");
        assert_eq!(r.stop, crate::ai::StopReason::ToolUse);
        match &r.message.parts[1] {
            crate::ai::Part::ToolCall {
                signature, input, ..
            } => {
                assert_eq!(signature.as_deref(), Some("s"));
                assert_eq!(input["path"], "c.rs");
            }
            p => panic!("{p:?}"),
        }
        assert_eq!(r.model, "g1");
    }

    #[test]
    fn error_events() {
        let (r, _) = collect(
            AiProtocol::Anthropic,
            "event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n\n",
        );
        let e = r.unwrap_err();
        assert_eq!(
            (e.code, e.detail.as_str()),
            (codes::RATE_LIMITED, "Overloaded")
        );
        let (r, _) = collect(
            AiProtocol::OpenAi,
            "data: {\"error\":{\"message\":\"kaputt\"}}\n\n",
        );
        assert_eq!(r.unwrap_err().code, codes::HTTP_ERROR);
        let (r, _) = collect(AiProtocol::OpenAi, "data: {kein json\n\n");
        assert_eq!(r.unwrap_err().code, codes::BAD_RESPONSE);
    }
}
