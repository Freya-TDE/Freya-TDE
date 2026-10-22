//! Events: abstract in the model (e.g. `on_click`). The UI backend decides
//! whether it becomes a callback, a polled return value or a message
//! handler; the user only writes the handler stub.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::name::HandlerName;

/// A parameter passed to an event handler.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventParam {
    pub name: String,
    /// The Rust type as written in the generated stub, e.g. `bool`, `&str`.
    pub rust_type: String,
}

/// An event declared by a component type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventDef {
    /// Name in snake_case, e.g. `on_click`, `on_text_changed`.
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<EventParam>,
    /// Return type of the handler, if the backend asks for one (e.g. `bool`
    /// for "close allowed?").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub returns: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

impl EventDef {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            params: Vec::new(),
            returns: None,
            description: String::new(),
        }
    }

    pub fn with_param(mut self, name: impl Into<String>, rust_type: impl Into<String>) -> Self {
        self.params.push(EventParam {
            name: name.into(),
            rust_type: rust_type.into(),
        });
        self
    }

    pub fn returning(mut self, rust_type: impl Into<String>) -> Self {
        self.returns = Some(rust_type.into());
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }
}

/// Event name → handler on an instance. One handler may serve several
/// events (as in Delphi).
pub type Events = BTreeMap<String, HandlerName>;
