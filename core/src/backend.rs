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
    /// Another program owns the UI loop (e.g. Flutter): the Rust side is a
    /// library (`src/lib.rs`, no `main.rs`) that the host calls. `run()`
    /// creates the application like `main-thread` (`started` in `lib.rs`)
    /// and hands it and the runtime to the backend's `run` template, which
    /// keeps them for the host's calls.
    Host,
}

/// Who owns a file of [`BackendDescriptor::files`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileOwner {
    /// Written on every run, read-only, with checksum header (`//`
    /// comments, e.g. Rust or Dart).
    #[default]
    Generator,
    /// Created once, then the user's.
    User,
}

/// How often a file of [`BackendDescriptor::files`] is produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileScope {
    /// Once per project; context like the `run` template plus `project`.
    #[default]
    Project,
    /// Once per form with widgets; context like the `form` template plus
    /// `project`.
    Form,
}

/// An additional file of the generated project (e.g. the Dart side of a
/// Flutter UI). `path` and `template` are templates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendFile {
    /// Path relative to the project, e.g. `flutter/lib/{{ form.name }}.g.dart`.
    pub path: String,
    pub template: String,
    #[serde(default)]
    pub owner: FileOwner,
    #[serde(default)]
    pub each: FileScope,
    /// Copy `template` as it is (no template syntax), e.g. for generated
    /// bindings that contain `{{`.
    #[serde(default)]
    pub raw: bool,
}

/// The description of a UI backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackendDescriptor {
    /// Backend id, the pack name (e.g. `"egui"`).
    pub name: String,
    /// Name shown to the user (the pack's `title`, default: `name`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
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
    /// Template for statements at the start of `run()`, before `App::init`
    /// – for toolkits whose widgets can only be created after the toolkit
    /// was initialised (e.g. `gtk4::init()?;`). Same context as `run`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup: Option<String>,
    /// Template for items appended to `app.gen.rs` – shared helpers of the
    /// backend's generated code (e.g. a widget state type the forms use as
    /// `crate::app::…`). Same context as `run`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    /// The pack's `license-notice`: shown before a project uses the backend
    /// (the user decides).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license_notice: Option<String>,
    /// Additional files (Dart sources, `pubspec.yaml` …).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<BackendFile>,
    /// `[lib] crate-type` entries the project needs (e.g. `cdylib` for a
    /// library loaded by the host); added to `Cargo.toml`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub crate_types: Vec<String>,
    /// Properties every widget gets under this backend (e.g. shadows,
    /// animations), in addition to its type's own.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub widget_properties: Vec<crate::PropertyDef>,
    /// Events every widget gets under this backend (e.g. hover, file drop).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub widget_events: Vec<crate::EventDef>,
}

impl BackendDescriptor {
    /// The name to show: `title`, else `name`.
    pub fn display_name(&self) -> &str {
        if self.title.is_empty() {
            &self.name
        } else {
            &self.title
        }
    }

    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            title: name.clone(),
            name,
            event_loop: EventLoop::default(),
            absolute_positioning: false,
            platforms: BTreeSet::new(),
            cargo: Vec::new(),
            form: None,
            run: None,
            setup: None,
            app: None,
            license_notice: None,
            files: Vec::new(),
            crate_types: Vec::new(),
            widget_properties: Vec::new(),
            widget_events: Vec::new(),
        }
    }
}
