//! [`ComponentType`]: the description of a component type – its properties,
//! events, provides/requires and Cargo dependencies.
//!
//! Types come from the core ([`crate::builtin`]), from UI backends and from
//! component packs. Packs and backends describe them declaratively (step 3 of
//! the roadmap); this module only defines the data.

use std::collections::BTreeMap;

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
    /// Only needed when every listed property of the component has the
    /// given value (`driver = "MySql"`; enums by variant, bools as
    /// `"true"`/`"false"`, numbers as written). Empty = always.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub when: BTreeMap<String, String>,
    /// Only needed when every listed property contains the text (in a list:
    /// any entry), e.g. `columns = "chrono::"` – a type that needs the crate.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub uses: BTreeMap<String, String>,
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
            when: BTreeMap::new(),
            uses: BTreeMap::new(),
        }
    }

    /// Only needed when property `name` has `value`.
    pub fn when(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.when.insert(name.into(), value.into());
        self
    }

    /// Only needed when property `name` contains `text`.
    pub fn uses(mut self, name: impl Into<String>, text: impl Into<String>) -> Self {
        self.uses.insert(name.into(), text.into());
        self
    }

    /// Is the dependency needed for a component with these property values?
    /// `value_of` returns a property's value (set or default).
    pub fn applies(&self, value_of: impl Fn(&str) -> Option<crate::Value>) -> bool {
        self.when.iter().all(|(name, wanted)| {
            value_of(name)
                .and_then(|v| v.as_condition_text())
                .as_deref()
                == Some(wanted.as_str())
        }) && self
            .uses
            .iter()
            .all(|(name, text)| value_of(name).is_some_and(|v| v.contains_text(text)))
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
    /// Statements run when the form (or data module) starts, e.g. connect
    /// or start serving. Context as for `init` plus `form_struct`; `rust`
    /// refers to elements of the same form as `self.<name>`. Code that runs
    /// later on Tokio reaches the handlers through the form's queue
    /// (`self.freya_queue.push(move |f| f.<handler>(…))`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    /// Items added to the form's `impl` (e.g. `pub fn <name>_send(&self, …)`);
    /// same context as `start`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub methods: Option<String>,
    /// Property (a reference) naming the component this one attaches to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attach_to: Option<String>,
    /// Applied to the value of the `attach_to` target in `App::init` right
    /// after this component is created: `let target = target<attach>;`
    /// (e.g. `.route("/ws", {{ this }}.route())`); `this` is this
    /// component's variable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attach: Option<String>,
    /// Items for `app.gen.rs` (types and functions the other templates use
    /// as `crate::app::…`), written once per type if it is used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub helpers: Option<String>,
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
            start: None,
            methods: None,
            attach_to: None,
            attach: None,
            helpers: None,
        }
    }

    /// Does the component run code when its form starts or add methods to
    /// it? Then the form gets an event queue (`freya_queue`).
    pub fn is_live(&self) -> bool {
        self.start.is_some() || self.methods.is_some()
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
    /// How the designer draws a widget of this type (see
    /// `tdr_designer::render`): `switch`, `slider`, `spin`, `progress`,
    /// `dropdown`, `memo`, `list`, `table`, `radio`, `separator`, `image`, `frame`,
    /// `link`, `tabs`, or `menubar` for a non-visual main menu. `None` = a
    /// box with icon and name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub look: Option<String>,
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
    /// A data source (e.g. an SQL query): how it delivers rows. See
    /// [`crate::data`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<DataSpec>,
    /// A list widget that can be filled from a data source. See
    /// [`crate::data`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list: Option<ListSpec>,
    /// A table widget (data grid) that can be filled from a data source.
    /// See [`crate::data`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<TableSpec>,
}

/// How a table widget shows rows and reports its selection – MiniJinja
/// templates of its backend pack.
///
/// `set_rows` are statements putting `headers: Vec<String>` and
/// `rows: Vec<Vec<String>>` (one text per shown column) into the widget;
/// `selected` is an expression of type `Option<usize>`. Context as for
/// [`ListSpec`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableSpec {
    pub set_rows: String,
    pub selected: String,
    /// Statements selecting row `index: Option<usize>` (kept after the rows
    /// are shown again while editing).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub select: Option<String>,
}

/// How a data source delivers rows – MiniJinja templates of its pack.
///
/// `fetch` is an expression inside an `async` block returning
/// `Result<_, crate::app::Error>` that yields the rows (`?` and `.await`
/// allowed); context: `source` (the source's value, a variable), `props`,
/// `rust`. `get` is an expression reading one column of a row (`?`
/// allowed); context: `row`, `column`, `type`. The optional `columns`
/// renders the columns the source always has (`field: String; value:
/// String`, separated by `;`) from `props` – used when the `columns`
/// property is empty.
///
/// The optional `update`, `insert` and `delete` write an edited row back
/// (data grids with `editable`): expressions inside an `async` block
/// returning `Result<_, crate::app::Error>`; context: `source`, `props`,
/// `rust`, `table`, `keys` (key columns), `columns` (the other columns,
/// for `insert` without generated keys: all), `row` (the new row, a
/// variable) and `old` (the row as loaded, for `update`/`delete`). The
/// source must then have `table` and `key` set (see [`crate::data`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataSpec {
    pub fetch: String,
    pub get: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insert: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delete: Option<String>,
}

impl DataSpec {
    /// Can the source write rows back (all three templates given)?
    pub fn writable(&self) -> bool {
        self.update.is_some() && self.insert.is_some() && self.delete.is_some()
    }
}

/// How a list widget shows entries and reports its selection – MiniJinja
/// templates of its backend pack.
///
/// `set_items` are statements putting `items: Vec<String>` into the widget;
/// `selected` is an expression of type `Option<usize>`. Context: `name`
/// (the widget), `root` (the form's root widget), `form`, `props` (the
/// widget's properties, e.g. `editable`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListSpec {
    pub set_items: String,
    pub selected: String,
}

impl ComponentType {
    pub fn new(type_ref: TypeRef, kind: TypeKind) -> Self {
        Self {
            type_ref,
            kind,
            category: String::new(),
            description: String::new(),
            icon: None,
            look: None,
            properties: Vec::new(),
            events: Vec::new(),
            provides: Vec::new(),
            requires: Vec::new(),
            cargo: Vec::new(),
            codegen: None,
            data: None,
            list: None,
            table: None,
        }
    }

    /// Icon name, e.g. `"button"` (the IDE's icon set or the pack's `icons/<name>.png`).
    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// How the designer draws it, e.g. `"switch"`.
    pub fn with_look(mut self, look: impl Into<String>) -> Self {
        self.look = Some(look.into());
        self
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
