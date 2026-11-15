//! AI integration without UI: provider clients, a neutral message model
//! and (built on top) tools, permissions and skills.
//!
//! Three wire protocols cover the usual cloud services and local models,
//! also in the local network ([`AiProtocol`](crate::settings::AiProtocol)):
//! Anthropic Messages, OpenAI-compatible Chat Completions (OpenAI, xAI,
//! Mistral, DeepSeek, Groq, OpenRouter, Ollama, LM Studio, llama.cpp,
//! vLLM …) and Google Gemini. The [`Client`] speaks HTTP synchronously
//! (call it from a worker thread); requests and answers use the neutral
//! types in [`message`], which every protocol maps to its own format.
//!
//! Errors carry stable codes ([`codes`]) and are serialisable, like all
//! diagnostics of Freya TDE.
//!
//! *KI-Anbindung ohne Oberfläche: Clients für die Anbieter (drei
//! Protokolle), ein neutrales Nachrichtenmodell, darauf Werkzeuge, Rechte und
//! Skills. Synchron, für Hintergrund-Threads; Fehler mit stabilen Codes.*
//!
//! ```no_run
//! use freya_tde::ai::{ChatRequest, Client, Message};
//! use freya_tde::settings::{AiProfile, AiProvider};
//!
//! let mut profile = AiProfile::new("Ollama", AiProvider::Ollama);
//! profile.model = "llama3.2".into();
//! profile.endpoint = Some("http://192.168.1.20:11434".into());
//! let client = Client::new(&profile, None).unwrap();
//! let models = client.models().unwrap();
//! let answer = client
//!     .chat(&ChatRequest::new(vec![Message::user("Hallo!")]))
//!     .unwrap();
//! println!("{models:?}\n{}", answer.text());
//! ```

mod anthropic;
mod client;
pub mod conversation;
mod gemini;
pub mod message;
#[cfg(test)]
mod mock;
mod openai;
mod security;
pub mod skills;
pub mod tools;

pub use client::{Client, ModelInfo};
pub use conversation::{Conversation, Decision, Event, Turn, run, system_prompt};
pub use message::{ChatRequest, ChatResponse, Message, Part, Role, StopReason, ToolSpec, Usage};
pub use security::{is_loopback, key_transport_risk};
pub use skills::SkillTable;
pub use tools::{Access, Action, Proposal, Workspace};

use serde::Serialize;

/// Stable error codes of the AI integration.
pub mod codes {
    /// The profile has no endpoint and the provider no default.
    pub const ENDPOINT_MISSING: &str = "endpoint_missing";
    /// The profile names no model (needed to chat).
    pub const MODEL_MISSING: &str = "model_missing";
    /// The provider needs an API key and none is stored.
    pub const KEY_MISSING: &str = "key_missing";
    /// The profile does not exist.
    pub const PROFILE_UNKNOWN: &str = "profile_unknown";
    /// The server could not be reached (no route, refused, unknown host).
    pub const UNREACHABLE: &str = "unreachable";
    /// The server did not answer in time.
    pub const TIMEOUT: &str = "timeout";
    /// TLS failed (certificate, handshake).
    pub const TLS_FAILED: &str = "tls_failed";
    /// The key was rejected (HTTP 401/403).
    pub const AUTH_FAILED: &str = "auth_failed";
    /// Too many requests or quota used up (HTTP 429).
    pub const RATE_LIMITED: &str = "rate_limited";
    /// Endpoint or model not found (HTTP 404).
    pub const NOT_FOUND: &str = "not_found";
    /// Any other HTTP error status.
    pub const HTTP_ERROR: &str = "http_error";
    /// The answer could not be read.
    pub const BAD_RESPONSE: &str = "bad_response";
    /// An API key would travel unencrypted (`http://`) to another machine.
    pub const INSECURE_KEY_TRANSPORT: &str = "insecure_key_transport";
    /// The AI needed more model calls for one message than allowed.
    pub const TOO_MANY_STEPS: &str = "too_many_steps";
}

/// An error of the AI integration with a stable `code`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[error("{code}: {detail}")]
pub struct AiError {
    pub code: &'static str,
    pub detail: String,
    /// HTTP status, if the server answered with an error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
}

impl AiError {
    pub fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
            status: None,
        }
    }
}

/// A warning about a profile (does not block).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AiIssue {
    pub code: &'static str,
    pub detail: String,
}
