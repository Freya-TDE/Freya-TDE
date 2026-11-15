//! Anthropic Messages API (`POST /v1/messages`).

use serde_json::{Value, json};

use super::client::ModelInfo;
use super::message::{ChatRequest, ChatResponse, Message, Part, Role, StopReason, Usage};
use super::{AiError, codes};

/// API version sent in `anthropic-version`.
pub const VERSION: &str = "2023-06-01";

pub fn request(model: &str, req: &ChatRequest) -> Value {
    let messages: Vec<Value> = req.messages.iter().map(message).collect();
    let mut body = json!({
        "model": model,
        "max_tokens": req.max_tokens,
        "messages": messages,
    });
    if !req.system.is_empty() {
        body["system"] = json!(req.system);
    }
    if !req.tools.is_empty() {
        body["tools"] = req
            .tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.input_schema,
                })
            })
            .collect();
    }
    if let Some(t) = req.temperature {
        body["temperature"] = json!(t);
    }
    body
}

fn message(m: &Message) -> Value {
    let content: Vec<Value> = m
        .parts
        .iter()
        .map(|p| match p {
            Part::Text { text } => json!({ "type": "text", "text": text }),
            Part::ToolCall {
                id, name, input, ..
            } => {
                json!({ "type": "tool_use", "id": id, "name": name, "input": input })
            }
            Part::ToolResult {
                id,
                content,
                is_error,
                ..
            } => json!({
                "type": "tool_result",
                "tool_use_id": id,
                "content": content,
                "is_error": is_error,
            }),
        })
        .collect();
    let role = match m.role {
        Role::User => "user",
        Role::Assistant => "assistant",
    };
    json!({ "role": role, "content": content })
}

pub fn response(v: &Value) -> Result<ChatResponse, AiError> {
    let blocks = v["content"]
        .as_array()
        .ok_or_else(|| AiError::new(codes::BAD_RESPONSE, "Antwort ohne content"))?;
    let mut parts = Vec::new();
    for b in blocks {
        match b["type"].as_str() {
            Some("text") => parts.push(Part::Text {
                text: b["text"].as_str().unwrap_or_default().to_owned(),
            }),
            Some("tool_use") => parts.push(Part::ToolCall {
                id: b["id"].as_str().unwrap_or_default().to_owned(),
                name: b["name"].as_str().unwrap_or_default().to_owned(),
                input: b["input"].clone(),
                signature: None,
            }),
            // Denkblöcke u. a. werden nicht gebraucht.
            _ => {}
        }
    }
    let stop = match v["stop_reason"].as_str() {
        Some("end_turn" | "stop_sequence") => StopReason::EndTurn,
        Some("tool_use") => StopReason::ToolUse,
        Some("max_tokens") => StopReason::MaxTokens,
        Some("refusal") => StopReason::Refusal,
        other => StopReason::Other(other.unwrap_or_default().to_owned()),
    };
    Ok(ChatResponse {
        message: Message {
            role: Role::Assistant,
            parts,
        },
        stop,
        usage: Usage {
            input_tokens: v["usage"]["input_tokens"].as_u64().unwrap_or(0),
            output_tokens: v["usage"]["output_tokens"].as_u64().unwrap_or(0),
        },
        model: v["model"].as_str().unwrap_or_default().to_owned(),
    })
}

pub fn models(v: &Value) -> Vec<ModelInfo> {
    v["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            Some(ModelInfo {
                id: m["id"].as_str()?.to_owned(),
                name: m["display_name"].as_str().map(str::to_owned),
            })
        })
        .collect()
}
