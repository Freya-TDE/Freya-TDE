//! [`BackendDescriptor`]: what a UI backend (egui, Slint, GTK, native …)
//! tells the core about itself, declared in the `[backend]` section of its
//! `pack.toml` (`kind = "backend"`).
//!
//! The designer draws widgets itself (from the palette and the per-platform
//! theme tables); the backend only contributes code: its widget types and
//! extensions carry [`crate::CodegenSpec`]s, the descriptor adds the
//! form-level code (`form`) and the event loop (`run`). The real look is
//! shown by building and running the generated project in debug mode
//! ([`crate::preview`]).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::component_type::CargoDependency;
use crate::platform::Platform;

/// How the UI event loop and the async runtime share the program.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EventLoop {
    /// The UI loop owns the main thread (most toolkits). An async runtime is
    /// built by hand and runs on background threads; `App::init` runs via
    /// `block_on`, `started` is synchronous.
    #[default]
    MainThread,
    /// The backend is async itself: `run()` carries the runtime's entry
    /// attribute (e.g. `#[tokio::main]`) and `started` is async.
    Async,
}

/// The description of a UI backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendDescriptor {
    /// Backend id, the pack name (e.g. `"egui"`).
    pub name: String,
    #[serde(default)]
    pub event_loop: EventLoop,
    /// May widgets be placed at absolute positions?
    #[serde(default)]
    pub absolute_positioning: bool,
    /// Platforms the backend runs on; empty = all.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub platforms: BTreeSet<Platform>,
    /// Crates every project with this backend needs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cargo: Vec<CargoDependency>,
    /// Template for items added to each `<form>.gen.rs` – typically an
    /// `impl` block that builds or draws the widget tree and calls the
    /// event handlers. See [`crate::codegen`] for its context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub form: Option<String>,
    /// Template for the body of the event loop in `run()`, executed after
    /// `started`; it must evaluate to `Result<(), Error>`. `None` = no UI
    /// loop (`run` returns after `started`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
}

impl BackendDescriptor {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            event_loop: EventLoop::default(),
            absolute_positioning: false,
            platforms: BTreeSet::new(),
            cargo: Vec::new(),
            form: None,
            run: None,
        }
    }
}
