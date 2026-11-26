//! OpenAI-compatible Chat Completions (`POST <base>/chat/completions`):
//! OpenAI, xAI, Mistral, DeepSeek, Groq, OpenRouter and the local servers
//! (Ollama, LM Studio, llama.cpp, vLLM …).

use crate::tr;
use serde_json::{Value, json};

use super::client::ModelInfo;
use super::message::{ChatRequest, ChatResponse, Message, Part, Role, StopReason, Usage};
use super::{AiError, codes};

/// `completion_tokens`: OpenAI's own API wants `max_completion_tokens`
/// (newer models reject `max_tokens`); the others know `max_tokens`.
pub fn request(model: &str, req: &ChatRequest, completion_tokens: bool) -> Value {
    let mut messages = Vec::new();
    if !req.system.is_empty() {
        messages.push(json!({ "role": "system", "content": req.system }));
    }
    for m in &req.messages {
        push_message(&mut messages, m);
    }
    let mut body = json!({ "model": model, "messages": messages });
    let limit = if completion_tokens {
        "max_completion_tokens"
    } else {
        "max_tokens"
    };
    body[limit] = json!(req.max_tokens);
    if !req.tools.is_empty() {
        body["tools"] = req
            .tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema,
                    },
                })
            })
            .collect();
    }
    if let Some(t) = req.temperature {
        body["temperature"] = json!(t);
    }
    body
}

fn push_message(out: &mut Vec<Value>, m: &Message) {
    let text = m.text();
    match m.role {
        Role::Assistant => {
            let calls: Vec<Value> = m
                .tool_calls()
                .map(|(id, name, input)| {
                    json!({
                        "id": id,
                        "type": "function",
                        "function": { "name": name, "arguments": input.to_string() },
                    })
                })
                .collect();
            let mut msg = json!({ "role": "assistant" });
            msg["content"] = if text.is_empty() && !calls.is_empty() {
                Value::Null
            } else {
                json!(text)
            };
            if !calls.is_empty() {
                msg["tool_calls"] = json!(calls);
            }
            out.push(msg);
        }
        Role::User => {
            // Werkzeugergebnisse sind eigene Nachrichten mit der Rolle `tool`.
            for p in &m.parts {
                if let Part::ToolResult {
                    id,
                    content,
                    is_error,
                    ..
                } = p
                {
                    let content = if *is_error {
                        format!("Error: {content}")
                    } else {
                        content.clone()
                    };
                    out.push(json!({ "role": "tool", "tool_call_id": id, "content": content }));
                }
            }
            if !text.is_empty() {
                out.push(json!({ "role": "user", "content": text }));
            }
        }
    }
}

pub fn response(v: &Value) -> Result<ChatResponse, AiError> {
    let choice = &v["choices"][0];
    if choice.is_null() {
        return Err(AiError::new(
            codes::BAD_RESPONSE,
            tr!("ai.msg.no_field", field = "choices"),
        ));
    }
    let msg = &choice["message"];
    let mut parts = Vec::new();
    match &msg["content"] {
        Value::String(s) if !s.is_empty() => parts.push(Part::Text { text: s.clone() }),
        // Manche Server liefern Teile statt Text.
        Value::Array(items) => {
            for i in items {
                if let Some(t) = i["text"].as_str() {
                    parts.push(Part::Text { text: t.to_owned() });
                }
            }
        }
        _ => {}
    }
    for (n, call) in msg["tool_calls"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let f = &call["function"];
        // `arguments` ist JSON als Text; Ollama liefert teils schon ein Objekt.
        let input = match &f["arguments"] {
            Value::String(s) if s.trim().is_empty() => json!({}),
            Value::String(s) => serde_json::from_str(s).unwrap_or_else(|_| json!(s)),
            Value::Null => json!({}),
            other => other.clone(),
        };
        parts.push(Part::ToolCall {
            id: call["id"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map_or_else(|| format!("call_{n}"), str::to_owned),
            name: f["name"].as_str().unwrap_or_default().to_owned(),
            input,
            signature: None,
        });
    }
    let has_calls = parts.iter().any(|p| matches!(p, Part::ToolCall { .. }));
    let stop = match choice["finish_reason"].as_str() {
        Some("tool_calls" | "function_call") => StopReason::ToolUse,
        // Manche lokalen Server melden `stop`, obwohl Werkzeuge gerufen werden.
        _ if has_calls => StopReason::ToolUse,
        Some("stop") | None => StopReason::EndTurn,
        Some("length") => StopReason::MaxTokens,
        Some("content_filter") => StopReason::Refusal,
        Some(other) => StopReason::Other(other.to_owned()),
    };
    Ok(ChatResponse {
        message: Message {
            role: Role::Assistant,
            parts,
        },
        stop,
        usage: Usage {
            input_tokens: v["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
            output_tokens: v["usage"]["completion_tokens"].as_u64().unwrap_or(0),
        },
        model: v["model"].as_str().unwrap_or_default().to_owned(),
    })
}

/// `GET <base>/models`.
pub fn models(v: &Value) -> Vec<ModelInfo> {
    v["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            Some(ModelInfo {
                id: m["id"].as_str()?.to_owned(),
                name: m["name"].as_str().map(str::to_owned),
            })
        })
        .collect()
}

/// Ollama's own list, `GET /api/tags`.
pub fn ollama_models(v: &Value) -> Vec<ModelInfo> {
    v["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            Some(ModelInfo {
                id: m["name"].as_str().or(m["model"].as_str())?.to_owned(),
                name: None,
            })
        })
        .collect()
}
