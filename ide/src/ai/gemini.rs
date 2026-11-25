//! Google Gemini (`POST /v1beta/models/<model>:generateContent`, streamed
//! `:streamGenerateContent?alt=sse`).

use crate::tr;
use serde_json::{Map, Value, json};

use super::client::ModelInfo;
use super::message::{ChatRequest, ChatResponse, Message, Part, Role, StopReason, Usage};
use super::{AiError, codes};

/// Prefix of call ids made up here (Gemini does not always send one);
/// they are not sent back.
pub(super) const LOCAL_ID: &str = "gemini_call_";

/// The request path; with `stream` the answer comes as server-sent events.
pub fn path(model: &str, stream: bool) -> String {
    if stream {
        format!("/v1beta/models/{model}:streamGenerateContent?alt=sse")
    } else {
        format!("/v1beta/models/{model}:generateContent")
    }
}

pub fn request(req: &ChatRequest) -> Value {
    let contents: Vec<Value> = req.messages.iter().map(message).collect();
    let mut config = json!({ "maxOutputTokens": req.max_tokens });
    if let Some(t) = req.temperature {
        config["temperature"] = json!(t);
    }
    let mut body = json!({ "contents": contents, "generationConfig": config });
    if !req.system.is_empty() {
        body["systemInstruction"] = json!({ "parts": [{ "text": req.system }] });
    }
    if !req.tools.is_empty() {
        let decls: Vec<Value> = req
            .tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "parameters": schema(&t.input_schema),
                })
            })
            .collect();
        body["tools"] = json!([{ "functionDeclarations": decls }]);
    }
    body
}

/// Gemini takes a subset of JSON Schema (OpenAPI 3): drop what it rejects.
fn schema(v: &Value) -> Value {
    match v {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(k, _)| !matches!(k.as_str(), "additionalProperties" | "$schema"))
                .map(|(k, v)| (k.clone(), schema(v)))
                .collect::<Map<_, _>>(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(schema).collect()),
        other => other.clone(),
    }
}

fn message(m: &Message) -> Value {
    let parts: Vec<Value> = m
        .parts
        .iter()
        .map(|p| match p {
            Part::Text { text } => json!({ "text": text }),
            Part::ToolCall {
                id,
                name,
                input,
                signature,
            } => {
                let mut call = json!({ "name": name, "args": input });
                if !id.starts_with(LOCAL_ID) {
                    call["id"] = json!(id);
                }
                let mut part = json!({ "functionCall": call });
                if let Some(sig) = signature {
                    part["thoughtSignature"] = json!(sig);
                }
                part
            }
            Part::ToolResult {
                id,
                name,
                content,
                is_error,
            } => {
                let key = if *is_error { "error" } else { "result" };
                let mut resp = json!({ "name": name, "response": { key: content } });
                if !id.starts_with(LOCAL_ID) {
                    resp["id"] = json!(id);
                }
                json!({ "functionResponse": resp })
            }
        })
        .collect();
    let role = match m.role {
        Role::User => "user",
        Role::Assistant => "model",
    };
    json!({ "role": role, "parts": parts })
}

pub fn response(v: &Value) -> Result<ChatResponse, AiError> {
    let usage = Usage {
        input_tokens: v["usageMetadata"]["promptTokenCount"].as_u64().unwrap_or(0),
        output_tokens: v["usageMetadata"]["candidatesTokenCount"]
            .as_u64()
            .unwrap_or(0),
    };
    let model = v["modelVersion"].as_str().unwrap_or_default().to_owned();
    let Some(candidate) = v["candidates"].get(0) else {
        // Ohne Kandidaten: die Anfrage wurde blockiert.
        if let Some(reason) = v["promptFeedback"]["blockReason"].as_str() {
            return Ok(ChatResponse {
                message: Message {
                    role: Role::Assistant,
                    parts: Vec::new(),
                },
                stop: StopReason::Refusal,
                usage,
                model: format!("{model} ({reason})").trim().to_owned(),
            });
        }
        return Err(AiError::new(
            codes::BAD_RESPONSE,
            tr!("ai.msg.no_field", field = "candidates"),
        ));
    };
    let mut parts = Vec::new();
    for (n, p) in candidate["content"]["parts"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        if p["thought"].as_bool() == Some(true) {
            continue;
        }
        if let Some(text) = p["text"].as_str() {
            parts.push(Part::Text {
                text: text.to_owned(),
            });
        } else if let Some(call) = p.get("functionCall") {
            parts.push(Part::ToolCall {
                id: call["id"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map_or_else(|| format!("{LOCAL_ID}{n}"), str::to_owned),
                name: call["name"].as_str().unwrap_or_default().to_owned(),
                input: match &call["args"] {
                    Value::Null => json!({}),
                    a => a.clone(),
                },
                signature: p["thoughtSignature"].as_str().map(str::to_owned),
            });
        }
    }
    let has_calls = parts.iter().any(|p| matches!(p, Part::ToolCall { .. }));
    let stop = match candidate["finishReason"].as_str() {
        _ if has_calls => StopReason::ToolUse,
        Some("STOP") | None => StopReason::EndTurn,
        Some("MAX_TOKENS") => StopReason::MaxTokens,
        Some("SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII") => {
            StopReason::Refusal
        }
        Some(other) => StopReason::Other(other.to_owned()),
    };
    Ok(ChatResponse {
        message: Message {
            role: Role::Assistant,
            parts,
        },
        stop,
        usage,
        model,
    })
}

/// `GET /v1beta/models`: only models that can generate content.
pub fn models(v: &Value) -> Vec<ModelInfo> {
    v["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| {
            m["supportedGenerationMethods"]
                .as_array()
                .is_none_or(|ms| ms.iter().any(|x| x == "generateContent"))
        })
        .filter_map(|m| {
            let name = m["name"].as_str()?;
            Some(ModelInfo {
                id: name.strip_prefix("models/").unwrap_or(name).to_owned(),
                name: m["displayName"].as_str().map(str::to_owned),
            })
        })
        .collect()
}
