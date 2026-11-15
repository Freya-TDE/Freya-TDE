//! The neutral message model: every protocol maps it to its own format.
//!
//! *Neutrales Nachrichtenmodell, das jedes Protokoll in sein Format übersetzt.*

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Who wrote a message. The system prompt is part of [`ChatRequest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
}

/// One piece of a message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Part {
    Text {
        text: String,
    },
    /// The model asks to run a tool. `signature` is opaque provider data
    /// that must go back unchanged with the call (Gemini's
    /// `thoughtSignature`).
    ToolCall {
        id: String,
        name: String,
        input: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
    /// The result of a tool, sent back in a user message. `name` repeats
    /// the tool (Gemini needs it).
    ToolResult {
        id: String,
        name: String,
        content: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        is_error: bool,
    },
}

/// A message of the conversation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub parts: Vec<Part>,
}

impl Message {
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            parts: vec![Part::Text { text: text.into() }],
        }
    }

    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            parts: vec![Part::Text { text: text.into() }],
        }
    }

    /// The text parts, joined.
    pub fn text(&self) -> String {
        text_of(&self.parts)
    }

    /// The tool calls in this message.
    pub fn tool_calls(&self) -> impl Iterator<Item = (&str, &str, &Value)> {
        self.parts.iter().filter_map(|p| match p {
            Part::ToolCall {
                id, name, input, ..
            } => Some((id.as_str(), name.as_str(), input)),
            _ => None,
        })
    }
}

fn text_of(parts: &[Part]) -> String {
    parts
        .iter()
        .filter_map(|p| match p {
            Part::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

/// A tool the model may call: name, description, JSON schema of the input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// A request to the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatRequest {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub system: String,
    pub messages: Vec<Message>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolSpec>,
    /// Upper limit of the answer in tokens.
    pub max_tokens: u32,
    /// Sampling temperature; `None` = the model's default (some models
    /// accept no other value).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
}

impl ChatRequest {
    /// Default answer limit.
    pub const MAX_TOKENS: u32 = 8192;

    pub fn new(messages: Vec<Message>) -> Self {
        Self {
            system: String::new(),
            messages,
            tools: Vec::new(),
            max_tokens: Self::MAX_TOKENS,
            temperature: None,
        }
    }
}

/// Why the model stopped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// The answer is complete.
    EndTurn,
    /// The model wants tool results.
    ToolUse,
    /// The answer hit `max_tokens`.
    MaxTokens,
    /// The model or a filter refused.
    Refusal,
    /// Anything else, with the provider's word for it.
    Other(String),
}

/// Token counts, if the provider reports them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// The model's answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatResponse {
    /// The assistant message (text and tool calls).
    pub message: Message,
    pub stop: StopReason,
    pub usage: Usage,
    /// The model that answered, as the server names it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model: String,
}

impl ChatResponse {
    /// The text of the answer.
    pub fn text(&self) -> String {
        self.message.text()
    }
}
