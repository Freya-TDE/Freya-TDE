//! # tdr-core – the Freya TDE component model
//!
//! The neutral form model everything else builds on: dependency graph,
//! component packs, code generator, preview and designer. It knows no UI
//! toolkit.
//!
//! | Concept | Type |
//! |---|---|
//! | Non-visual component (pool, timer, router …) | [`Component`] |
//! | Visual component: component + layout + children | [`Widget`] |
//! | Uniform access to both | trait [`Element`] |
//! | A designer form or data module (`.ftdr`, RON) | [`Form`] |
//! | Reference, possibly to another form (`services::db_pool`) | [`ComponentPath`] |
//! | Description of a component type | [`ComponentType`], [`TypeRef`] |
//! | Properties and their type system | [`PropertyDef`], [`PropertyType`], [`Value`], [`PropertyValue`] |
//! | Abstract events, user handler stubs | [`EventDef`], [`HandlerName`] |
//! | What components offer and need | [`Provide`], [`Require`], [`Capability`] |
//! | Platform-specific values (`cfg` branches) | [`Platform`] |
//! | All known types of a project | [`TypeRegistry`] |
//! | Design-time checks of one form | [`validate()`], [`Diagnostic`] |
//! | Project-wide dependency graph: requirements, cycles, init order, Cargo deps | [`graph`] |
//! | Rename/remove across all forms | [`project`] |
//! | Component packs and backend palettes from `pack.toml` | [`pack`] |
//! | The project file (`.ptdr`, RON) | [`ProjectFile`] |
//! | Code generation: dry run, then write | [`codegen`] |
//! | UI backend: event loop, form and run templates | [`BackendDescriptor`] |
//! | Preview: debug build, compiler messages traced to elements | [`preview`] |
//! | The common widget core | [`builtin`] |
//!
//! Everything is library API with machine-readable results: diagnostics
//! serialize with stable `code`s, code generation is a dry run until it is
//! explicitly written, and output is deterministic – so a designer, a CLI
//! or an AI can drive the same functions.
//!
//! ```
//! use tdr_core::*;
//!
//! let name = |n: &str| ComponentName::new(n).unwrap();
//!
//! // A form with one button whose click goes to a handler stub.
//! let button = Widget::from_component(
//!     Component::new(name("ok_button"), builtin::button())
//!         .with("caption", "OK")
//!         .on("on_click", HandlerName::new("ok_clicked").unwrap()),
//! );
//! let mut form = Form::new(name("customers"), "egui", Widget::new(name("main_form"), builtin::form()));
//! form.add_widget(&name("main_form"), button).unwrap();
//!
//! // Check it against the known types.
//! let registry = TypeRegistry::with_builtins();
//! assert!(form.validate(&ValidationContext::new(&registry)).is_empty());
//!
//! // Store as .ftdr and read back.
//! let text = form.to_ron().unwrap();
//! assert_eq!(Form::from_ron(&text).unwrap(), form);
//! ```

mod backend;
pub mod builtin;
mod capability;
pub mod codegen;
mod component;
mod component_type;
pub mod data;
mod element;
mod error;
mod event;
mod form;
pub mod graph;
mod name;
pub mod pack;
mod platform;
pub mod preview;
pub mod project;
mod project_file;
mod property;
mod registry;
pub mod schema;
mod type_ref;
mod validate;
mod value;
mod widget;

pub use backend::{BackendDescriptor, BackendFile, EventLoop, FileOwner, FileScope};
pub use capability::{Capability, Condition, Provide, Require, RequireKind};
pub use component::Component;
pub use component_type::{
    CargoDependency, CodegenSpec, ComponentType, DataSpec, ListSpec, ShareMode, TableSpec, TypeKind,
};
pub use element::Element;
pub use error::ModelError;
pub use event::{EventDef, EventParam, Events};
pub use form::Form;
pub use name::{ComponentName, ComponentPath, HandlerName, is_rust_identifier};
pub use platform::Platform;
pub use project_file::{GitSettings, ProjectFile, ProjectKind, is_package_name};
pub use property::{Properties, PropertyDef, PropertyValue};
pub use registry::TypeRegistry;
pub use type_ref::{Origin, TypeRef};
pub use validate::{Diagnostic, Issue, Severity, ValidationContext, validate};
pub use value::{Color, Mismatch, Point, PropertyType, Size, Value};
pub use widget::{
    Align, Anchors, ContainerLayout, GridCell, Insets, LayoutKind, Placement, Visual, Widget,
};
