//! [`ComponentType`]: the description of a component type – its properties,
//! events, provides/requires and Cargo dependencies.
//!
//! Types come from the core ([`crate::builtin`]), from UI backends and from
//! component packs. Packs and backends describe them declaratively (step 3 of
//! the roadmap); this module only defines the data.

use serde::{Deserialize, Serialize};

use crate::capability::{Provide, Require};
use crate::event::EventDef;
use crate::property::PropertyDef;
use crate::type_ref::TypeRef;

/// Visual or not – decides whether instances are [`crate::Widget`]s or
/// plain [`crate::Component`]s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TypeKind {
    /// Non-visual (pool, timer, router …): shown as an icon only.
    NonVisual,
    /// A widget; `container` widgets may have children.
    Widget { container: bool },
}

impl TypeKind {
    pub fn is_widget(self) -> bool {
        matches!(self, TypeKind::Widget { .. })
    }

    pub fn is_container(self) -> bool {
        matches!(self, TypeKind::Widget { container: true })
    }
}

/// A Cargo dependency a component type brings into the project when used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CargoDependency {
    /// Crate name as on crates.io.
    #[serde(rename = "crate")]
    pub crate_name: String,
    /// Version requirement, e.g. `"0.8"`.
    pub version: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub default_features: bool,
}

fn yes() -> bool {
    true
}

fn is_true(v: &bool) -> bool {
    *v
}

impl CargoDependency {
    pub fn new(crate_name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            crate_name: crate_name.into(),
            version: version.into(),
            features: Vec::new(),
            default_features: true,
        }
    }

    pub fn with_features<S: Into<String>>(mut self, features: impl IntoIterator<Item = S>) -> Self {
        self.features.extend(features.into_iter().map(Into::into));
        self
    }
}

/// How a component is passed to components that reference it in the
/// generated initialisation code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShareMode {
    /// `dependency.clone()` – for cheap handles (pools, routers, handles).
    #[default]
    Clone,
    /// The value is wrapped in `Arc`; dependents get `Arc::clone(&dependency)`.
    Arc,
    /// Dependents get `&dependency` during their initialisation only.
    Ref,
}

/// Code generation for a component type: the Rust type of its field and the
/// expression creating it. `rust_type`, `init` and `entry` are MiniJinja
/// templates (see [`crate::codegen`] for their context).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodegenSpec {
    /// Rust type of the generated field, e.g. `sqlx::PgPool`.
    pub rust_type: String,
    /// Rust expression creating the value; may use `?` and, if `is_async`,
    /// `.await`.
    pub init: String,
    /// Does `init` need an async context (`.await`)? Then the project needs
    /// a component with an `entry` attribute (an async runtime).
    #[serde(default)]
    pub is_async: bool,
    #[serde(default)]
    pub share: ShareMode,
    /// Attribute for the generated entry function, e.g.
    /// `#[tokio::main]`, used when the backend's event loop is async. A type
    /// with `entry` or `runtime` is the project's async runtime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    /// Expression building the runtime by hand (e.g.
    /// `tokio::runtime::Builder::new_multi_thread().enable_all().build()?`),
    /// used when the UI loop owns the main thread. The value must have a
    /// `block_on` method.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<String>,
}

impl CodegenSpec {
    /// A spec with the given field type and init expression.
    pub fn new(rust_type: impl Into<String>, init: impl Into<String>) -> Self {
        Self {
            rust_type: rust_type.into(),
            init: init.into(),
            is_async: false,
            share: ShareMode::default(),
            entry: None,
            runtime: None,
        }
    }

    /// Does the type provide the async runtime?
    pub fn is_runtime(&self) -> bool {
        self.entry.is_some() || self.runtime.is_some()
    }
}

/// The full description of a component type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentType {
    pub type_ref: TypeRef,
    pub kind: TypeKind,
    /// Palette group, e.g. `"Standard"`, `"Database"`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub category: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Icon name for palette and design surface.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<PropertyDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<EventDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provides: Vec<Provide>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<Require>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cargo: Vec<CargoDependency>,
    /// Code generation; `None` for types the UI backend generates (widgets)
    /// or that have no code yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codegen: Option<CodegenSpec>,
}

impl ComponentType {
    pub fn new(type_ref: TypeRef, kind: TypeKind) -> Self {
        Self {
            type_ref,
            kind,
            category: String::new(),
            description: String::new(),
            icon: None,
            properties: Vec::new(),
            events: Vec::new(),
            provides: Vec::new(),
            requires: Vec::new(),
            cargo: Vec::new(),
            codegen: None,
        }
    }

    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = category.into();
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn with_property(mut self, property: PropertyDef) -> Self {
        self.properties.push(property);
        self
    }

    pub fn with_properties(mut self, properties: impl IntoIterator<Item = PropertyDef>) -> Self {
        self.properties.extend(properties);
        self
    }

    pub fn with_event(mut self, event: EventDef) -> Self {
        self.events.push(event);
        self
    }

    pub fn with_provide(mut self, provide: Provide) -> Self {
        self.provides.push(provide);
        self
    }

    pub fn with_require(mut self, require: Require) -> Self {
        self.requires.push(require);
        self
    }

    pub fn with_cargo(mut self, dependency: CargoDependency) -> Self {
        self.cargo.push(dependency);
        self
    }

    pub fn with_codegen(mut self, codegen: CodegenSpec) -> Self {
        self.codegen = Some(codegen);
        self
    }

    pub fn property(&self, name: &str) -> Option<&PropertyDef> {
        self.properties.iter().find(|p| p.name == name)
    }

    pub fn event(&self, name: &str) -> Option<&EventDef> {
        self.events.iter().find(|e| e.name == name)
    }

    /// Does this type provide `capability`?
    pub fn provides(&self, capability: &crate::Capability) -> bool {
        self.provides.iter().any(|p| &p.capability == capability)
    }
}
