//! Pack descriptions: component types declared in `pack.toml` files.
//!
//! A *component pack* (SQLx, Axum, Tokio …) or a *backend palette*
//! describes its component types declaratively. Packs live in a directory
//! of their own (`components/sqlx/pack.toml`); code generation templates
//! will sit next to the manifest.
//!
//! Loading happens in two stages so pack authors get precise messages:
//!
//! 1. TOML syntax and shape – unknown keys are errors (a typo like
//!    `defualt = 5` is reported with line and column, not ignored);
//! 2. meaning – every problem is collected with its location
//!    (`component Pool, property max_connections: …`).
//!
//! # Schema
//!
//! ```toml
//! [pack]
//! name = "sqlx"                         # id; types become pack:sqlx/Name
//! title = "SQLx"                        # optional: name shown to the user (default: name)
//! version = "0.1.0"
//! kind = "components"                   # or "backend" (palette of a UI backend)
//! description = "…"                     # optional
//! authors = ["…"]                       # optional
//! license = "MIT OR Apache-2.0"         # license of the pack itself
//! generated-code-license = "MIT"        # license of the code it generates (mandatory)
//! license-notice = """…"""              # optional: shown before the pack is used (e.g. license terms of a dependency)
//!
//! [[component]]
//! name = "Pool"                         # PascalCase
//! kind = "non-visual"                   # "non-visual" (default), "widget", "container"; widgets also get enabled, visible, tooltip
//! category = "Database"                 # optional palette group
//! description = "…"                     # optional
//! icon = "database"                     # optional
//! look = "switch"                       # optional: how the designer draws it (see ComponentType::look)
//! provides = ["sqlx.pool"]              # or [{ capability = "…", description = "…" }]
//! requires = [{ capability = "tokio.runtime" }]
//! cargo = [{ crate = "sqlx", version = "0.9", features = ["postgres"], default-features = true }]
//! # only if a property has a value: { crate = "sqlx", …, features = ["mysql"], when = { driver = "MySql" } }
//! # only if a property contains a text (lists: any entry): { crate = "chrono", version = "0.4", uses = { columns = "chrono::" } }
//!
//! [component.properties.max_connections]   # snake_case, order is kept
//! type = "int"
//! min = 1
//! max = 1000
//! default = 5
//!
//! [component.events.on_row]
//! params = [{ name = "row", type = "&sqlx::postgres::PgRow" }]   # type may be a template over props/refs
//! returns = "bool"                      # optional
//!
//! [component.codegen]                   # optional: how the component appears in generated code
//! type = "sqlx::PgPool"                 # Rust type of the field (template)
//! init = "PgPoolOptions::new().connect({{ rust.url }}).await?"   # expression (template)
//! # init-file = "templates/pool.rs.j2"  # instead of `init`, relative to the pack directory
//! async = true                          # init uses .await (needs an async runtime)
//! share = "clone"                       # how dependents get it: "clone" (default), "arc", "ref"
//! # entry = "#[tokio::main]"            # makes this component the async runtime
//! # start = "self.{{ name }}_connect();"   # statements when the form starts (or start-file)
//! # methods = "pub fn {{ name }}_send(&self, …) { … }"   # items in the form's impl (or methods-file)
//! # attach-to = "router"               # a reference property …
//! # attach = ".route({{ rust.path }}, {{ this }}.route())"   # … applied to its target in App::init
//! # helpers = "pub struct WsHub { … }"  # items for app.gen.rs, once per type (or helpers-file)
//!
//! [component.data]                      # optional: a data source for list widgets (see tdr_core::data)
//! fetch = "sqlx::query(…).fetch_all(…).await?"   # rows, in an async block; `source` = the component's value
//! get = "sqlx::Row::try_get::<{{ type }}, _>({{ row }}, {{ column | rust_str }})?"
//! columns = "field: String; value: String"   # optional: columns the source always has (template over props)
//! update = "…"                          # optional (all three or none): write an edited row back,
//! insert = "…"                          #   see DataSpec; context `row`, `old`, `table`, `keys`,
//! delete = "…"                          #   `columns`; the source then has `table` and `key`
//!
//! [component.list]                      # optional, backend widgets: can be filled from a data source
//! set-items = "…"                       # statements putting `items: Vec<String>` into the widget
//! selected = "…"                        # expression: Option<usize>
//!
//! [component.table]                     # optional, backend widgets: a data grid filled from a data source
//! set-rows = "…"                        # statements using `headers: Vec<String>`, `rows: Vec<Vec<String>>`
//! selected = "…"                        # expression: Option<usize>
//! select = "…"                          # optional: statements selecting `index: Option<usize>`
//!
//! [[extend]]                            # backend packs only: add properties to existing types
//! type = "core/Button"
//! [extend.properties.small]
//! type = "bool"
//! [extend.codegen]                      # … and/or say how the backend generates the type
//! [extend.list]                         # … and/or how it fills a core list widget
//!                                       #     (set-items, selected; like [component.list])
//! type = "MyButton"
//! init = "MyButton::new({{ rust.caption }})"
//!
//! [backend]                             # backend packs only: the backend descriptor
//! event-loop = "main-thread"            # or "async"; see BackendDescriptor
//! absolute-positioning = true           # default false
//! platforms = ["windows", "linux"]      # optional, empty = all
//! cargo = [{ crate = "eframe", version = "0.33" }]
//! form = "impl {{ form.struct }} { … }" # template for <form>.gen.rs (or form-file)
//! run = "…"                             # template for the event loop in run() (or run-file)
//! setup = "gtk4::init()?;"              # optional: statements before App::init (or setup-file)
//! app = "pub struct Control<T> { … }"   # optional: items appended to app.gen.rs (or app-file)
//! crate-type = ["cdylib"]               # optional: `[lib] crate-type` of the project (event-loop "host")
//! [[backend.files]]                     # optional: more files (Dart sources, pubspec.yaml …)
//! path = "flutter/lib/{{ form.name }}.g.dart"   # template
//! template-file = "templates/form.dart.j2"      # or template
//! each = "form"                         # "project" (default) or "form" (forms with widgets)
//! owner = "generator"                   # or "user" (created once)
//! raw = false                           # true = copy as is, no template syntax
//! ```
//!
//! `[component.codegen]` of a runtime component may also give
//! `runtime = "tokio::runtime::Builder::new_multi_thread().enable_all().build()?"`,
//! used when the UI loop owns the main thread.
//!
//! ## Property keys
//!
//! | Key | Meaning |
//! |---|---|
//! | `type` | `bool`, `int`, `float`, `text`, `enum`, `color`, `point`, `size`, `list`, `reference`, `code`, `custom` |
//! | `min`, `max` | bounds for `int` / `float` |
//! | `variants` | for `enum` |
//! | `item` | element type of a `list`: `"text"` or `{ type = "int", min = 0 }` |
//! | `capability` | for `reference`: the target must provide it |
//! | `custom` | for `custom`: the backend type name, e.g. `"egui.Stroke"` |
//! | `default` | default value; colors `"#rrggbb"` or `[r, g, b]`, points `[x, y]`, sizes `[w, h]` |
//! | `required` | `true` = no default, the user must set it |
//! | `backend` | property exists only for this UI backend |
//! | `platforms` | only on `windows`, `linux`, `macos`, `android`, `ios`, `web` or `cfg(…)` |
//! | `category`, `description` | for the object inspector |
//!
//! Without `default` and `required`, a natural default applies (`false`,
//! `0` or the lower bound, `""`, the first enum variant, black, `[0, 0]`,
//! an empty list, not connected).
//!
//! ## Requirements
//!
//! `requires = [{ capability = "…", via = "pool", optional = false, when = { … } }]`
//! where `via` names a `reference` property with the same capability and
//! `when` is one of `{ property-set = "name" }`, `{ referenced-by = "cap" }`,
//! `{ project-provides = "cap" }` (omitted = always).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::backend::{BackendDescriptor, BackendFile, EventLoop, FileOwner, FileScope};
use crate::capability::{Capability, Condition, Provide, Require, RequireKind};
use crate::component_type::{CargoDependency, CodegenSpec, ComponentType, ShareMode, TypeKind};
use crate::error::ModelError;
use crate::event::EventDef;
use crate::name::is_rust_identifier;
use crate::platform::Platform;
use crate::property::PropertyDef;
use crate::registry::TypeRegistry;
use crate::type_ref::{Origin, TypeRef};
use crate::validate::reference_capability;
use crate::value::{Color, Point, PropertyType, Size, Value};

/// File name of a pack manifest inside its directory.
pub const MANIFEST: &str = "pack.toml";

// --- Raw TOML schema ------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPack {
    pack: RawInfo,
    #[serde(default)]
    component: Vec<RawComponent>,
    #[serde(default)]
    extend: Vec<RawExtend>,
    backend: Option<RawBackend>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawBackend {
    #[serde(default)]
    event_loop: EventLoop,
    #[serde(default)]
    absolute_positioning: bool,
    #[serde(default)]
    platforms: Vec<String>,
    #[serde(default)]
    cargo: Vec<RawCargo>,
    form: Option<String>,
    form_file: Option<String>,
    run: Option<String>,
    run_file: Option<String>,
    setup: Option<String>,
    setup_file: Option<String>,
    app: Option<String>,
    app_file: Option<String>,
    #[serde(default)]
    files: Vec<RawBackendFile>,
    #[serde(default)]
    crate_type: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawBackendFile {
    path: String,
    template: Option<String>,
    template_file: Option<String>,
    /// A folder of the pack copied as it is (every file, recursively) to
    /// `path/…`, e.g. vendored build tools.
    dir: Option<String>,
    #[serde(default)]
    owner: FileOwner,
    #[serde(default)]
    each: FileScope,
    #[serde(default)]
    raw: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawInfo {
    name: String,
    /// Display name (default: `name`).
    title: Option<String>,
    version: String,
    #[serde(default)]
    kind: PackKind,
    #[serde(default)]
    description: String,
    #[serde(default)]
    authors: Vec<String>,
    license: String,
    generated_code_license: String,
    license_notice: Option<String>,
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
enum RawKind {
    #[default]
    NonVisual,
    Widget,
    Container,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawComponent {
    name: String,
    #[serde(default)]
    kind: RawKind,
    #[serde(default)]
    category: String,
    #[serde(default)]
    description: String,
    icon: Option<String>,
    look: Option<String>,
    #[serde(default)]
    properties: IndexMap<String, RawProperty>,
    #[serde(default)]
    events: IndexMap<String, RawEvent>,
    #[serde(default)]
    provides: Vec<RawProvide>,
    #[serde(default)]
    requires: Vec<RawRequire>,
    #[serde(default)]
    cargo: Vec<RawCargo>,
    codegen: Option<RawCodegen>,
    data: Option<RawData>,
    list: Option<RawList>,
    table: Option<RawTable>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawTable {
    set_rows: String,
    selected: String,
    select: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawData {
    fetch: String,
    get: String,
    columns: Option<String>,
    update: Option<String>,
    insert: Option<String>,
    delete: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawList {
    set_items: String,
    selected: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawCodegen {
    #[serde(rename = "type")]
    rust_type: String,
    init: Option<String>,
    init_file: Option<String>,
    #[serde(rename = "async", default)]
    is_async: bool,
    #[serde(default)]
    share: ShareMode,
    entry: Option<String>,
    runtime: Option<String>,
    start: Option<String>,
    start_file: Option<String>,
    methods: Option<String>,
    methods_file: Option<String>,
    attach_to: Option<String>,
    attach: Option<String>,
    helpers: Option<String>,
    helpers_file: Option<String>,
}

/// The type part of a property (also used for list items).
#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
struct RawTypeSpec {
    #[serde(rename = "type")]
    ty: String,
    min: Option<toml::Value>,
    max: Option<toml::Value>,
    variants: Option<Vec<String>>,
    item: Option<RawItem>,
    capability: Option<String>,
    custom: Option<String>,
}

#[derive(Deserialize, Clone)]
#[serde(untagged)]
enum RawItem {
    Name(String),
    Spec(Box<RawTypeSpec>),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProperty {
    #[serde(rename = "type")]
    ty: String,
    min: Option<toml::Value>,
    max: Option<toml::Value>,
    variants: Option<Vec<String>>,
    item: Option<RawItem>,
    capability: Option<String>,
    custom: Option<String>,
    default: Option<toml::Value>,
    #[serde(default)]
    required: bool,
    backend: Option<String>,
    #[serde(default)]
    platforms: Vec<String>,
    #[serde(default)]
    category: String,
    #[serde(default)]
    description: String,
}

impl RawProperty {
    fn type_spec(&self) -> RawTypeSpec {
        RawTypeSpec {
            ty: self.ty.clone(),
            min: self.min.clone(),
            max: self.max.clone(),
            variants: self.variants.clone(),
            item: self.item.clone(),
            capability: self.capability.clone(),
            custom: self.custom.clone(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEvent {
    #[serde(default)]
    params: Vec<RawParam>,
    returns: Option<String>,
    #[serde(default)]
    description: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawParam {
    name: String,
    #[serde(rename = "type")]
    rust_type: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawProvide {
    Name(String),
    Full {
        capability: String,
        #[serde(default)]
        description: String,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRequire {
    capability: String,
    via: Option<String>,
    #[serde(default)]
    optional: bool,
    when: Option<RawCondition>,
    #[serde(default)]
    description: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawCondition {
    property_set: Option<String>,
    referenced_by: Option<String>,
    project_provides: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawCargo {
    #[serde(rename = "crate")]
    crate_name: String,
    version: String,
    #[serde(default)]
    features: Vec<String>,
    #[serde(default = "yes")]
    default_features: bool,
    #[serde(default)]
    when: BTreeMap<String, toml::Value>,
    #[serde(default)]
    uses: BTreeMap<String, String>,
}

fn yes() -> bool {
    true
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawExtend {
    #[serde(rename = "type")]
    type_ref: String,
    #[serde(default)]
    properties: IndexMap<String, RawProperty>,
    codegen: Option<RawCodegen>,
    /// Fills a core list widget (e.g. `core/ImageScrollBox`) from a data source.
    list: Option<RawList>,
}

// --- Public model ----------------------------------------------------------------------

/// What a pack provides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackKind {
    /// Non-visual components (SQLx, Axum, Tokio …); types are `pack:<name>/…`.
    #[default]
    Components,
    /// The widget palette of a UI backend; types are `backend:<name>/…` and
    /// it may add properties to existing types (`[[extend]]`).
    Backend,
}

/// The `[pack]` section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackInfo {
    pub name: String,
    /// Name shown to the user (wizard, palette); `title` in the manifest,
    /// otherwise `name`. Lets a backend id differ from the product name
    /// (`freya-gui` is shown as `freya`).
    pub title: String,
    pub version: String,
    pub kind: PackKind,
    pub description: String,
    pub authors: Vec<String>,
    /// License of the pack itself.
    pub license: String,
    /// License of the code the pack generates into user projects.
    pub generated_code_license: String,
    /// What a user should know before using the pack, e.g. the license
    /// terms of a library the generated project depends on. The IDE shows it
    /// and lets the user decide.
    pub license_notice: Option<String>,
}

/// Properties and code generation a backend pack adds to an existing type.
#[derive(Debug, Clone, PartialEq)]
pub struct Extension {
    pub type_ref: TypeRef,
    pub properties: Vec<PropertyDef>,
    /// How the backend generates code for the type.
    pub codegen: Option<CodegenSpec>,
    /// How the backend fills the (list) widget from a data source.
    pub list: Option<crate::component_type::ListSpec>,
}

/// A loaded, validated pack.
#[derive(Debug, Clone, PartialEq)]
pub struct Pack {
    pub info: PackInfo,
    pub types: Vec<ComponentType>,
    /// Additions to existing types (backend packs only).
    pub extensions: Vec<Extension>,
    /// The `[backend]` section (backend packs only).
    pub backend: Option<BackendDescriptor>,
    /// Directory the pack was loaded from (templates live there).
    pub dir: Option<PathBuf>,
}

/// A problem in a pack description, with its location.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PackProblem {
    /// e.g. `component Pool, property max_connections`.
    pub location: String,
    pub message: String,
}

impl fmt::Display for PackProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.location.is_empty() {
            f.write_str(&self.message)
        } else {
            write!(f, "{}: {}", self.location, self.message)
        }
    }
}

/// Why a pack could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum PackError {
    #[error("{path}: {message}")]
    Io { path: PathBuf, message: String },
    /// TOML syntax or shape error; the message contains line and column.
    #[error("{0}")]
    Syntax(String),
    #[error("{}", format_problems(.file.as_deref(), .problems))]
    Invalid {
        /// The manifest, when loaded from disk.
        file: Option<PathBuf>,
        problems: Vec<PackProblem>,
    },
}

fn format_problems(file: Option<&Path>, problems: &[PackProblem]) -> String {
    let mut out = match file {
        Some(file) => format!(
            "{}: {} problem(s) in the pack description:",
            file.display(),
            problems.len()
        ),
        None => format!("{} problem(s) in the pack description:", problems.len()),
    };
    for p in problems {
        out.push_str("\n  - ");
        out.push_str(&p.to_string());
    }
    out
}

impl PackError {
    /// The individual problems (empty for I/O and syntax errors).
    pub fn problems(&self) -> &[PackProblem] {
        match self {
            PackError::Invalid { problems, .. } => problems,
            _ => &[],
        }
    }
}

// --- Conversion ----------------------------------------------------------------------------

struct Checker<'d> {
    problems: Vec<PackProblem>,
    /// The pack directory, for `init-file`.
    dir: Option<&'d Path>,
}

impl Checker<'_> {
    fn problem(&mut self, location: &str, message: impl Into<String>) {
        self.problems.push(PackProblem {
            location: location.to_owned(),
            message: message.into(),
        });
    }

    fn capability(&mut self, location: &str, text: &str) -> Option<Capability> {
        match Capability::new(text) {
            Ok(c) => Some(c),
            Err(e) => {
                self.problem(location, e.to_string());
                None
            }
        }
    }

    fn bound_int(&mut self, location: &str, key: &str, value: &Option<toml::Value>) -> Option<i64> {
        match value {
            None => None,
            Some(toml::Value::Integer(i)) => Some(*i),
            Some(other) => {
                self.problem(
                    location,
                    format!("`{key}` must be an integer, found {}", other.type_str()),
                );
                None
            }
        }
    }

    fn bound_float(
        &mut self,
        location: &str,
        key: &str,
        value: &Option<toml::Value>,
    ) -> Option<f64> {
        match value {
            None => None,
            Some(toml::Value::Float(f)) => Some(*f),
            Some(toml::Value::Integer(i)) => Some(*i as f64),
            Some(other) => {
                self.problem(
                    location,
                    format!("`{key}` must be a number, found {}", other.type_str()),
                );
                None
            }
        }
    }

    fn property_type(&mut self, location: &str, spec: &RawTypeSpec) -> Option<PropertyType> {
        // Type-specific keys must belong to the type.
        let allowed: &[&str] = match spec.ty.as_str() {
            "int" | "float" => &["min", "max"],
            "enum" => &["variants"],
            "list" => &["item"],
            "reference" => &["capability"],
            "custom" => &["custom"],
            _ => &[],
        };
        let present = [
            ("min", spec.min.is_some()),
            ("max", spec.max.is_some()),
            ("variants", spec.variants.is_some()),
            ("item", spec.item.is_some()),
            ("capability", spec.capability.is_some()),
            ("custom", spec.custom.is_some()),
        ];
        for (key, _) in present
            .iter()
            .filter(|(k, set)| *set && !allowed.contains(k))
        {
            self.problem(
                location,
                format!("`{key}` is not allowed for type {}", spec.ty),
            );
        }

        Some(match spec.ty.as_str() {
            "bool" => PropertyType::Bool,
            "text" => PropertyType::Text,
            "color" => PropertyType::Color,
            "point" => PropertyType::Point,
            "size" => PropertyType::Size,
            "code" => PropertyType::Code,
            "int" => {
                let (min, max) = (
                    self.bound_int(location, "min", &spec.min),
                    self.bound_int(location, "max", &spec.max),
                );
                if let (Some(a), Some(b)) = (min, max)
                    && a > b
                {
                    self.problem(location, format!("min {a} is greater than max {b}"));
                }
                PropertyType::Int { min, max }
            }
            "float" => {
                let (min, max) = (
                    self.bound_float(location, "min", &spec.min),
                    self.bound_float(location, "max", &spec.max),
                );
                if let (Some(a), Some(b)) = (min, max)
                    && a > b
                {
                    self.problem(location, format!("min {a} is greater than max {b}"));
                }
                PropertyType::Float { min, max }
            }
            "enum" => {
                let variants = spec.variants.clone().unwrap_or_default();
                if variants.is_empty() {
                    self.problem(location, "an enum needs `variants`");
                }
                let unique: BTreeSet<&String> = variants.iter().collect();
                if unique.len() != variants.len() {
                    self.problem(location, "enum variants must be unique");
                }
                PropertyType::Enum(variants)
            }
            "list" => {
                let item = match &spec.item {
                    None => {
                        self.problem(location, "a list needs `item` (the element type)");
                        return None;
                    }
                    Some(RawItem::Name(name)) => RawTypeSpec {
                        ty: name.clone(),
                        min: None,
                        max: None,
                        variants: None,
                        item: None,
                        capability: None,
                        custom: None,
                    },
                    Some(RawItem::Spec(spec)) => (**spec).clone(),
                };
                if item.ty == "list" {
                    self.problem(location, "lists of lists are not supported");
                    return None;
                }
                PropertyType::List(Box::new(self.property_type(location, &item)?))
            }
            "reference" => {
                let capability = match &spec.capability {
                    Some(c) => Some(self.capability(location, c)?),
                    None => None,
                };
                PropertyType::Reference(capability)
            }
            "custom" => match &spec.custom {
                Some(name) if !name.trim().is_empty() => PropertyType::Custom(name.clone()),
                _ => {
                    self.problem(
                        location,
                        "a custom type needs `custom` (the backend type name)",
                    );
                    return None;
                }
            },
            other => {
                self.problem(
                    location,
                    format!(
                        "unknown type {other:?}; expected bool, int, float, text, enum, color, point, size, list, reference, code or custom"
                    ),
                );
                return None;
            }
        })
    }

    fn property(&mut self, location: &str, name: &str, raw: &RawProperty) -> Option<PropertyDef> {
        if !is_rust_identifier(name) || name.chars().any(|c| c.is_ascii_uppercase()) {
            self.problem(
                location,
                "property names must be snake_case Rust identifiers",
            );
        }
        let ty = self.property_type(location, &raw.type_spec())?;
        let default = match (&raw.default, raw.required) {
            (Some(_), true) => {
                self.problem(location, "a required property cannot have a default");
                None
            }
            (Some(value), false) => match convert_value(value, &ty) {
                Ok(v) => Some(v),
                Err(message) => {
                    self.problem(location, format!("default: {message}"));
                    None
                }
            },
            (None, true) => None,
            (None, false) => Some(natural_default(&ty)),
        };
        let mut platforms = BTreeSet::new();
        for p in &raw.platforms {
            match parse_platform(p) {
                Some(platform) => {
                    platforms.insert(platform);
                }
                None => self.problem(
                    location,
                    format!("unknown platform {p:?}; expected windows, linux, macos, android, ios, web or cfg(…)"),
                ),
            }
        }
        Some(PropertyDef {
            name: name.to_owned(),
            ty,
            default,
            backend: raw.backend.clone(),
            platforms,
            category: raw.category.clone(),
            description: raw.description.clone(),
        })
    }

    fn component(&mut self, info: &PackInfo, raw: &RawComponent) -> Option<ComponentType> {
        let location = format!("component {}", raw.name);
        let valid_name = raw.name.starts_with(|c: char| c.is_ascii_uppercase())
            && raw
                .name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !valid_name {
            self.problem(&location, "component names must be PascalCase, e.g. `Pool`");
        }
        let kind = match raw.kind {
            RawKind::NonVisual => TypeKind::NonVisual,
            RawKind::Widget => TypeKind::Widget { container: false },
            RawKind::Container => TypeKind::Widget { container: true },
        };
        if info.kind == PackKind::Components && kind.is_widget() {
            self.problem(
                &location,
                "component packs are non-visual; widgets belong to backend packs",
            );
        }
        let type_ref = match info.kind {
            PackKind::Components => TypeRef::pack(&info.name, &raw.name),
            PackKind::Backend => TypeRef::backend(&info.name, &raw.name),
        };
        let mut ty = ComponentType::new(type_ref, kind)
            .with_category(raw.category.clone())
            .with_description(raw.description.clone());
        ty.icon = raw.icon.clone();
        ty.look = raw.look.clone();

        for (name, property) in &raw.properties {
            let loc = format!("{location}, property {name}");
            if let Some(def) = self.property(&loc, name, property) {
                ty.properties.push(def);
            }
        }
        // Wie die Kern-Widgets: enabled, visible, tooltip (sofern nicht
        // selbst definiert).
        if kind.is_widget() {
            for def in crate::builtin::common() {
                if !ty.properties.iter().any(|p| p.name == def.name) {
                    ty.properties.push(def);
                }
            }
        }

        for (name, event) in &raw.events {
            let loc = format!("{location}, event {name}");
            if !is_rust_identifier(name) || name.chars().any(|c| c.is_ascii_uppercase()) {
                self.problem(
                    &loc,
                    "event names must be snake_case Rust identifiers, e.g. `on_click`",
                );
            }
            let mut def = EventDef::new(name.clone()).with_description(event.description.clone());
            for param in &event.params {
                if !is_rust_identifier(&param.name) {
                    self.problem(
                        &loc,
                        format!("parameter name {:?} is not a Rust identifier", param.name),
                    );
                }
                if param.rust_type.trim().is_empty() {
                    self.problem(&loc, format!("parameter {:?} needs a type", param.name));
                }
                def = def.with_param(param.name.clone(), param.rust_type.clone());
            }
            def.returns = event.returns.clone();
            ty.events.push(def);
        }

        let mut provided = BTreeSet::new();
        for provide in &raw.provides {
            let (capability, description) = match provide {
                RawProvide::Name(c) => (c, String::new()),
                RawProvide::Full {
                    capability,
                    description,
                } => (capability, description.clone()),
            };
            let loc = format!("{location}, provides");
            if let Some(capability) = self.capability(&loc, capability) {
                if !provided.insert(capability.clone()) {
                    self.problem(&loc, format!("{capability} is listed twice"));
                }
                ty.provides.push(Provide {
                    capability,
                    description,
                });
            }
        }

        for require in &raw.requires {
            let loc = format!("{location}, requires {}", require.capability);
            let Some(capability) = self.capability(&loc, &require.capability) else {
                continue;
            };
            if let Some(via) = &require.via {
                match ty.property(via).map(|p| p.ty.clone()) {
                    None => self.problem(
                        &loc,
                        format!("`via` names {via:?}, which is not a property of this component"),
                    ),
                    Some(property_type) => {
                        let is_reference = match &property_type {
                            PropertyType::Reference(_) => true,
                            PropertyType::List(inner) => {
                                matches!(inner.as_ref(), PropertyType::Reference(_))
                            }
                            _ => false,
                        };
                        match reference_capability(&property_type) {
                            _ if !is_reference => {
                                self.problem(&loc, format!("property {via:?} is not a reference"))
                            }
                            None => self.problem(
                                &loc,
                                format!(
                                    "property {via:?} must declare `capability = \"{capability}\"`"
                                ),
                            ),
                            Some(c) if c != &capability => self.problem(
                                &loc,
                                format!("property {via:?} references {c}, not {capability}"),
                            ),
                            Some(_) => {}
                        }
                    }
                }
            }
            let condition = match &require.when {
                None => Condition::Always,
                Some(when) => {
                    let set = [
                        when.property_set.is_some(),
                        when.referenced_by.is_some(),
                        when.project_provides.is_some(),
                    ]
                    .iter()
                    .filter(|s| **s)
                    .count();
                    if set != 1 {
                        self.problem(&loc, "`when` needs exactly one of property-set, referenced-by, project-provides");
                        continue;
                    }
                    if let Some(p) = &when.property_set {
                        if ty.property(p).is_none() {
                            self.problem(
                                &loc,
                                format!("`when.property-set` names {p:?}, which is not a property"),
                            );
                        }
                        Condition::PropertySet(p.clone())
                    } else if let Some(c) = &when.referenced_by {
                        match self.capability(&loc, c) {
                            Some(c) => Condition::ReferencedBy(c),
                            None => continue,
                        }
                    } else {
                        match self
                            .capability(&loc, when.project_provides.as_deref().unwrap_or_default())
                        {
                            Some(c) => Condition::ProjectProvides(c),
                            None => continue,
                        }
                    }
                }
            };
            ty.requires.push(Require {
                capability,
                kind: if require.optional {
                    RequireKind::Optional
                } else {
                    RequireKind::Mandatory
                },
                via: require.via.clone(),
                condition,
                description: require.description.clone(),
            });
        }

        for cargo in &raw.cargo {
            let dep = self.cargo(&format!("{location}, cargo"), cargo);
            ty.cargo.push(dep);
        }

        if let Some(codegen) = &raw.codegen {
            ty.codegen = self.codegen(&format!("{location}, codegen"), kind, codegen);
        }
        if let Some(data) = &raw.data {
            if kind.is_widget() {
                self.problem(
                    &format!("{location}, data"),
                    "a data source must be non-visual",
                );
            }
            self.check_templates(
                &format!("{location}, data"),
                &[("fetch", &data.fetch), ("get", &data.get)],
            );
            for (part, text) in [
                ("columns", &data.columns),
                ("update", &data.update),
                ("insert", &data.insert),
                ("delete", &data.delete),
            ] {
                if let Some(text) = text {
                    self.check_templates(&format!("{location}, data"), &[(part, text)]);
                }
            }
            let writes = [&data.update, &data.insert, &data.delete];
            if writes.iter().any(|w| w.is_some()) && !writes.iter().all(|w| w.is_some()) {
                self.problem(
                    &format!("{location}, data"),
                    "give all of `update`, `insert` and `delete` (or none)",
                );
            }
            ty.data = Some(crate::component_type::DataSpec {
                fetch: data.fetch.clone(),
                get: data.get.clone(),
                columns: data.columns.clone(),
                update: data.update.clone(),
                insert: data.insert.clone(),
                delete: data.delete.clone(),
            });
        }
        if let Some(list) = &raw.list {
            if !kind.is_widget() {
                self.problem(
                    &format!("{location}, list"),
                    "only widgets can be list widgets",
                );
            }
            self.check_templates(
                &format!("{location}, list"),
                &[("set-items", &list.set_items), ("selected", &list.selected)],
            );
            ty.list = Some(crate::component_type::ListSpec {
                set_items: list.set_items.clone(),
                selected: list.selected.clone(),
            });
        }
        if let Some(table) = &raw.table {
            if !kind.is_widget() {
                self.problem(
                    &format!("{location}, table"),
                    "only widgets can be table widgets",
                );
            }
            self.check_templates(
                &format!("{location}, table"),
                &[("set-rows", &table.set_rows), ("selected", &table.selected)],
            );
            if let Some(select) = &table.select {
                self.check_templates(&format!("{location}, table"), &[("select", select)]);
            }
            ty.table = Some(crate::component_type::TableSpec {
                set_rows: table.set_rows.clone(),
                selected: table.selected.clone(),
                select: table.select.clone(),
            });
        }
        crate::data::complete(&mut ty);
        Some(ty)
    }

    fn check_templates(&mut self, location: &str, templates: &[(&str, &String)]) {
        for (part, source) in templates {
            if source.trim().is_empty() {
                self.problem(location, format!("`{part}` must not be empty"));
            } else if let Err(message) = crate::codegen::check_template(source) {
                self.problem(location, format!("template `{part}`: {message}"));
            }
        }
    }

    fn codegen(&mut self, location: &str, kind: TypeKind, raw: &RawCodegen) -> Option<CodegenSpec> {
        let init = match (&raw.init, &raw.init_file) {
            (Some(init), None) => init.clone(),
            (None, Some(file)) => self.template_file(location, file)?,
            _ => {
                self.problem(location, "needs exactly one of `init` and `init-file`");
                return None;
            }
        };
        if (raw.entry.is_some() || raw.runtime.is_some()) && kind.is_widget() {
            self.problem(
                location,
                "only non-visual components can provide `entry` or `runtime`",
            );
        }
        let start = self.template(location, "start", &raw.start, &raw.start_file);
        let methods = self.template(location, "methods", &raw.methods, &raw.methods_file);
        let helpers = self.template(location, "helpers", &raw.helpers, &raw.helpers_file);
        if (start.is_some() || methods.is_some()) && kind.is_widget() {
            self.problem(
                location,
                "only non-visual components can provide `start` or `methods`",
            );
        }
        if raw.attach.is_some() != raw.attach_to.is_some() {
            self.problem(location, "`attach` and `attach-to` go together");
        }
        let templates = [
            ("type", Some(&raw.rust_type)),
            ("init", Some(&init)),
            ("entry", raw.entry.as_ref()),
            ("runtime", raw.runtime.as_ref()),
            ("attach", raw.attach.as_ref()),
        ];
        for (part, source) in templates {
            let Some(source) = source else { continue };
            if source.trim().is_empty() {
                self.problem(location, format!("`{part}` must not be empty"));
            } else if let Err(message) = crate::codegen::check_template(source) {
                self.problem(location, format!("template `{part}`: {message}"));
            }
        }
        Some(CodegenSpec {
            rust_type: raw.rust_type.clone(),
            init,
            is_async: raw.is_async,
            share: raw.share,
            entry: raw.entry.clone(),
            runtime: raw.runtime.clone(),
            start,
            methods,
            attach_to: raw.attach_to.clone(),
            attach: raw.attach.clone(),
            helpers,
        })
    }

    /// An inline template or one from a file (`key` / `key-file`).
    fn template(
        &mut self,
        location: &str,
        key: &str,
        inline: &Option<String>,
        file: &Option<String>,
    ) -> Option<String> {
        let source = match (inline, file) {
            (None, None) => return None,
            (Some(text), None) => text.clone(),
            (None, Some(file)) => self.template_file(location, file)?,
            (Some(_), Some(_)) => {
                self.problem(
                    location,
                    format!("give either `{key}` or `{key}-file`, not both"),
                );
                return None;
            }
        };
        if let Err(message) = crate::codegen::check_template(&source) {
            self.problem(location, format!("template `{key}`: {message}"));
        }
        Some(source)
    }

    fn backend(&mut self, info: &PackInfo, raw: &RawBackend) -> BackendDescriptor {
        let location = "backend";
        let mut backend = BackendDescriptor::new(info.name.clone());
        backend.license_notice = info.license_notice.clone();
        backend.title = info.title.clone();
        backend.event_loop = raw.event_loop;
        backend.absolute_positioning = raw.absolute_positioning;
        for p in &raw.platforms {
            match parse_platform(p) {
                Some(platform) => {
                    backend.platforms.insert(platform);
                }
                None => self.problem(location, format!("unknown platform {p:?}")),
            }
        }
        backend.cargo = raw
            .cargo
            .iter()
            .map(|c| self.cargo(&format!("{location}, cargo"), c))
            .collect();
        backend.form = self.template(location, "form", &raw.form, &raw.form_file);
        backend.run = self.template(location, "run", &raw.run, &raw.run_file);
        backend.setup = self.template(location, "setup", &raw.setup, &raw.setup_file);
        backend.app = self.template(location, "app", &raw.app, &raw.app_file);
        backend.crate_types = raw.crate_type.clone();
        for f in &raw.files {
            let location = format!("{location}, files {:?}", f.path);
            if let Err(message) = crate::codegen::check_template(&f.path) {
                self.problem(&location, format!("`path`: {message}"));
            }
            if let Some(dir) = &f.dir {
                if f.template.is_some() || f.template_file.is_some() {
                    self.problem(&location, "give `dir` without `template`/`template-file`");
                    continue;
                }
                for (relative, text) in self.dir_files(&location, dir) {
                    backend.files.push(BackendFile {
                        path: format!("{}/{relative}", f.path.trim_end_matches('/')),
                        template: text,
                        owner: f.owner,
                        each: f.each,
                        raw: true,
                    });
                }
                continue;
            }
            let template = if f.raw {
                match (&f.template, &f.template_file) {
                    (Some(text), None) => Some(text.clone()),
                    (None, Some(file)) => self.template_file(&location, file),
                    _ => {
                        self.problem(&location, "give either `template` or `template-file`");
                        None
                    }
                }
            } else {
                let template = self.template(&location, "template", &f.template, &f.template_file);
                if template.is_none() {
                    self.problem(&location, "give either `template` or `template-file`");
                }
                template
            };
            if let Some(template) = template {
                backend.files.push(BackendFile {
                    path: f.path.clone(),
                    template,
                    owner: f.owner,
                    each: f.each,
                    raw: f.raw,
                });
            }
        }
        backend
    }

    fn cargo(&mut self, location: &str, cargo: &RawCargo) -> CargoDependency {
        let valid = !cargo.crate_name.is_empty()
            && cargo
                .crate_name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        if !valid {
            self.problem(
                location,
                format!("invalid crate name {:?}", cargo.crate_name),
            );
        }
        if cargo.version.trim().is_empty() {
            self.problem(
                location,
                format!("crate {} needs a version", cargo.crate_name),
            );
        }
        CargoDependency {
            crate_name: cargo.crate_name.clone(),
            version: cargo.version.clone(),
            features: cargo.features.clone(),
            default_features: cargo.default_features,
            when: cargo
                .when
                .iter()
                .map(|(name, value)| {
                    let text = match value {
                        toml::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    (name.clone(), text)
                })
                .collect(),
            uses: cargo.uses.clone(),
        }
    }

    /// Reads a template file relative to the pack directory.
    /// Every file below `dir` of the pack as (relative path with `/`, text),
    /// sorted (deterministic output).
    fn dir_files(&mut self, location: &str, dir: &str) -> Vec<(String, String)> {
        let relative = Path::new(dir);
        let inside = relative
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)));
        if !inside {
            self.problem(
                location,
                format!("`dir` {dir:?} must be a relative path inside the pack"),
            );
            return Vec::new();
        }
        let Some(root) = self.dir else {
            self.problem(
                location,
                "`dir` needs a pack directory; load the pack with Pack::load",
            );
            return Vec::new();
        };
        let base = root.join(relative);
        let mut files = Vec::new();
        let mut pending = vec![base.clone()];
        while let Some(folder) = pending.pop() {
            let entries = match std::fs::read_dir(&folder) {
                Ok(entries) => entries,
                Err(e) => {
                    self.problem(location, format!("cannot read {}: {e}", folder.display()));
                    continue;
                }
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    pending.push(path);
                    continue;
                }
                let name = path
                    .strip_prefix(&base)
                    .unwrap_or(&path)
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                match std::fs::read_to_string(&path) {
                    Ok(text) => files.push((name, text)),
                    Err(e) => {
                        self.problem(location, format!("cannot read {}: {e}", path.display()))
                    }
                }
            }
        }
        if files.is_empty() {
            self.problem(location, format!("`dir` {dir:?} has no files"));
        }
        files.sort();
        files
    }

    fn template_file(&mut self, location: &str, file: &str) -> Option<String> {
        let relative = Path::new(file);
        let inside = relative
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)));
        if !inside {
            self.problem(
                location,
                format!("`init-file` {file:?} must be a relative path inside the pack"),
            );
            return None;
        }
        let Some(dir) = self.dir else {
            self.problem(
                location,
                "`init-file` needs a pack directory; load the pack with Pack::load",
            );
            return None;
        };
        match std::fs::read_to_string(dir.join(relative)) {
            Ok(text) => Some(text),
            Err(e) => {
                self.problem(location, format!("cannot read {file:?}: {e}"));
                None
            }
        }
    }
}

fn parse_platform(text: &str) -> Option<Platform> {
    Some(match text {
        "windows" => Platform::Windows,
        "linux" => Platform::Linux,
        "macos" => Platform::MacOs,
        "android" => Platform::Android,
        "ios" => Platform::Ios,
        "web" => Platform::Web,
        other => {
            let inner = other.strip_prefix("cfg(")?.strip_suffix(')')?;
            if inner.trim().is_empty() {
                return None;
            }
            Platform::Other(inner.to_owned())
        }
    })
}

/// The default a property gets when the pack names none.
fn natural_default(ty: &PropertyType) -> Value {
    match ty {
        PropertyType::Bool => Value::Bool(false),
        PropertyType::Int { min, max } => Value::Int(0i64.clamp(
            min.unwrap_or(i64::MIN),
            max.unwrap_or(i64::MAX).max(min.unwrap_or(i64::MIN)),
        )),
        PropertyType::Float { min, max } => {
            let (lo, hi) = (min.unwrap_or(f64::MIN), max.unwrap_or(f64::MAX));
            Value::Float(0f64.clamp(lo, hi.max(lo)))
        }
        PropertyType::Text => Value::Text(String::new()),
        PropertyType::Enum(variants) => Value::Enum(variants.first().cloned().unwrap_or_default()),
        PropertyType::Color => Value::Color(Color::rgb(0, 0, 0)),
        PropertyType::Point => Value::Point(Point::default()),
        PropertyType::Size => Value::Size(Size::default()),
        PropertyType::List(_) => Value::List(Vec::new()),
        PropertyType::Reference(_) => Value::Reference(None),
        PropertyType::Code => Value::Code(String::new()),
        PropertyType::Custom(name) => Value::Custom {
            type_name: name.clone(),
            data: String::new(),
        },
    }
}

fn int_pair(value: &toml::Value, a: &str, b: &str) -> Option<(i32, i32)> {
    let num = |v: &toml::Value| v.as_integer().and_then(|i| i32::try_from(i).ok());
    match value {
        toml::Value::Array(items) if items.len() == 2 => Some((num(&items[0])?, num(&items[1])?)),
        toml::Value::Table(t) if t.len() == 2 => Some((num(t.get(a)?)?, num(t.get(b)?)?)),
        _ => None,
    }
}

fn parse_color(value: &toml::Value) -> Option<Color> {
    match value {
        toml::Value::String(s) => {
            let hex = s.strip_prefix('#')?;
            let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
            match hex.len() {
                6 => Some(Color::rgb(byte(0)?, byte(2)?, byte(4)?)),
                8 => Some(Color::rgba(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
                _ => None,
            }
        }
        toml::Value::Array(items) if items.len() == 3 || items.len() == 4 => {
            let c: Option<Vec<u8>> = items
                .iter()
                .map(|v| v.as_integer().and_then(|i| u8::try_from(i).ok()))
                .collect();
            let c = c?;
            Some(Color::rgba(
                c[0],
                c[1],
                c[2],
                c.get(3).copied().unwrap_or(255),
            ))
        }
        _ => None,
    }
}

/// Converts a TOML default to a model value of type `ty` and checks it.
fn convert_value(value: &toml::Value, ty: &PropertyType) -> Result<Value, String> {
    let wrong = || format!("expected {}, found {}", ty.kind_name(), value.type_str());
    let converted = match (ty, value) {
        (PropertyType::Bool, toml::Value::Boolean(b)) => Value::Bool(*b),
        (PropertyType::Int { .. }, toml::Value::Integer(i)) => Value::Int(*i),
        (PropertyType::Float { .. }, toml::Value::Float(f)) => Value::Float(*f),
        (PropertyType::Float { .. }, toml::Value::Integer(i)) => Value::Float(*i as f64),
        (PropertyType::Text, toml::Value::String(s)) => Value::Text(s.clone()),
        (PropertyType::Code, toml::Value::String(s)) => Value::Code(s.clone()),
        (PropertyType::Enum(_), toml::Value::String(s)) => Value::Enum(s.clone()),
        (PropertyType::Custom(name), toml::Value::String(s)) => Value::Custom {
            type_name: name.clone(),
            data: s.clone(),
        },
        (PropertyType::Color, v) => Value::Color(parse_color(v).ok_or_else(|| {
            format!("expected a color \"#rrggbb\", \"#rrggbbaa\" or [r, g, b(, a)], found {v}")
        })?),
        (PropertyType::Point, v) => {
            let (x, y) = int_pair(v, "x", "y")
                .ok_or_else(|| format!("expected a point [x, y], found {v}"))?;
            Value::Point(Point { x, y })
        }
        (PropertyType::Size, v) => {
            let (width, height) = int_pair(v, "width", "height")
                .ok_or_else(|| format!("expected a size [width, height], found {v}"))?;
            Value::Size(Size { width, height })
        }
        (PropertyType::List(inner), toml::Value::Array(items)) => Value::List(
            items
                .iter()
                .enumerate()
                .map(|(i, item)| convert_value(item, inner).map_err(|e| format!("item {i}: {e}")))
                .collect::<Result<_, _>>()?,
        ),
        (PropertyType::Reference(_), _) => {
            return Err("references cannot have a default (they start unconnected)".to_owned());
        }
        _ => return Err(wrong()),
    };
    converted.check(ty).map_err(|m| m.to_string())?;
    Ok(converted)
}

fn valid_pack_name(name: &str) -> bool {
    !name.is_empty()
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

fn valid_version(version: &str) -> bool {
    let core = version.split(['-', '+']).next().unwrap_or_default();
    let parts: Vec<&str> = core.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

impl Pack {
    /// Parses and validates a `pack.toml`.
    /// Template files (`init-file`) cannot be read without a directory; use
    /// [`Pack::load`] for packs that have them.
    pub fn parse(text: &str) -> Result<Self, PackError> {
        Self::parse_in(text, None)
    }

    fn parse_in(text: &str, dir: Option<&Path>) -> Result<Self, PackError> {
        let raw: RawPack = toml::from_str(text).map_err(|e| PackError::Syntax(e.to_string()))?;
        let mut checker = Checker {
            problems: Vec::new(),
            dir,
        };

        let info = PackInfo {
            title: raw
                .pack
                .title
                .map(|t| t.trim().to_owned())
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| raw.pack.name.clone()),
            name: raw.pack.name,
            version: raw.pack.version,
            kind: raw.pack.kind,
            description: raw.pack.description,
            authors: raw.pack.authors,
            license: raw.pack.license,
            generated_code_license: raw.pack.generated_code_license,
            license_notice: raw
                .pack
                .license_notice
                .map(|n| n.trim().to_owned())
                .filter(|n| !n.is_empty()),
        };
        if !valid_pack_name(&info.name) {
            checker.problem(
                "pack",
                format!(
                    "invalid pack name {:?}: lowercase letters, digits, `-`, `_`",
                    info.name
                ),
            );
        }
        if !valid_version(&info.version) {
            checker.problem(
                "pack",
                format!(
                    "version {:?} is not of the form major.minor.patch",
                    info.version
                ),
            );
        }
        if info.license.trim().is_empty() {
            checker.problem("pack", "`license` must not be empty");
        }
        if info.generated_code_license.trim().is_empty() {
            checker.problem(
                "pack",
                "`generated-code-license` must name the license of the generated code",
            );
        }

        let mut names = BTreeSet::new();
        let mut types = Vec::new();
        for component in &raw.component {
            if !names.insert(component.name.clone()) {
                checker.problem(&format!("component {}", component.name), "defined twice");
            }
            if let Some(ty) = checker.component(&info, component) {
                types.push(ty);
            }
        }

        let mut extensions = Vec::new();
        for extend in &raw.extend {
            let location = format!("extend {}", extend.type_ref);
            if info.kind != PackKind::Backend {
                checker.problem(&location, "only backend packs may extend existing types");
            }
            let type_ref = match extend.type_ref.parse::<TypeRef>() {
                Ok(t) => t,
                Err(e) => {
                    checker.problem(&location, e.to_string());
                    continue;
                }
            };
            let mut properties = Vec::new();
            for (name, property) in &extend.properties {
                let loc = format!("{location}, property {name}");
                if let Some(def) = checker.property(&loc, name, property) {
                    properties.push(def);
                }
            }
            // Extensions target widgets of the core or other palettes.
            let kind = TypeKind::Widget { container: false };
            let codegen = extend
                .codegen
                .as_ref()
                .and_then(|c| checker.codegen(&format!("{location}, codegen"), kind, c));
            let list = extend.list.as_ref().map(|list| {
                checker.check_templates(
                    &format!("{location}, list"),
                    &[("set-items", &list.set_items), ("selected", &list.selected)],
                );
                crate::component_type::ListSpec {
                    set_items: list.set_items.clone(),
                    selected: list.selected.clone(),
                }
            });
            extensions.push(Extension {
                type_ref,
                properties,
                codegen,
                list,
            });
        }

        let backend = match (&raw.backend, info.kind) {
            (Some(raw), PackKind::Backend) => Some(checker.backend(&info, raw)),
            (Some(_), PackKind::Components) => {
                checker.problem(
                    "backend",
                    "only packs with kind = \"backend\" may have [backend]",
                );
                None
            }
            (None, _) => None,
        };

        if checker.problems.is_empty() {
            Ok(Self {
                info,
                types,
                extensions,
                backend,
                dir: None,
            })
        } else {
            Err(PackError::Invalid {
                file: None,
                problems: checker.problems,
            })
        }
    }

    /// Loads `pack.toml` from a pack directory (or the given file).
    pub fn load(path: &Path) -> Result<Self, PackError> {
        let (file, dir) = if path.is_dir() {
            (path.join(MANIFEST), path.to_path_buf())
        } else {
            (
                path.to_path_buf(),
                path.parent().map(Path::to_path_buf).unwrap_or_default(),
            )
        };
        let text = std::fs::read_to_string(&file).map_err(|e| PackError::Io {
            path: file.clone(),
            message: e.to_string(),
        })?;
        let mut pack = Self::parse_in(&text, Some(&dir)).map_err(|e| match e {
            PackError::Syntax(message) => {
                PackError::Syntax(format!("{}: {message}", file.display()))
            }
            PackError::Invalid { problems, .. } => PackError::Invalid {
                file: Some(file.clone()),
                problems,
            },
            other => other,
        })?;
        pack.dir = Some(dir);
        Ok(pack)
    }

    /// Where the pack's types come from.
    pub fn origin(&self) -> Origin {
        match self.info.kind {
            PackKind::Components => Origin::Pack(self.info.name.clone()),
            PackKind::Backend => Origin::Backend(self.info.name.clone()),
        }
    }

    /// Registers all types and extensions. Fails on the first conflict
    /// (type already registered, extension of an unknown type, extension
    /// property that already exists); nothing is registered then.
    pub fn register(&self, registry: &mut TypeRegistry) -> Result<(), ModelError> {
        let mut staged = registry.clone();
        for ty in &self.types {
            staged.register(ty.clone())?;
        }
        for extension in &self.extensions {
            let type_ref = &extension.type_ref;
            for property in &extension.properties {
                if staged
                    .property_def(type_ref, &self.info.name, &property.name)
                    .is_some()
                {
                    return Err(ModelError::DuplicateProperty {
                        type_ref: type_ref.clone(),
                        property: property.name.clone(),
                    });
                }
            }
            staged.extend(
                self.info.name.clone(),
                type_ref,
                extension.properties.clone(),
            )?;
            if let Some(codegen) = &extension.codegen {
                staged.extend_codegen(self.info.name.clone(), type_ref, codegen.clone())?;
            }
            if let Some(list) = &extension.list {
                staged.extend_list(self.info.name.clone(), type_ref, list.clone())?;
            }
        }
        if let Some(backend) = &self.backend {
            staged.register_backend(backend.clone())?;
        }
        *registry = staged;
        Ok(())
    }
}

/// Loads every pack below `dir` (each subdirectory with a `pack.toml`), in
/// alphabetical order. Returns the loaded packs and the failures.
pub fn load_all(dir: &Path) -> (Vec<Pack>, Vec<PackError>) {
    let mut packs = Vec::new();
    let mut errors = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            errors.push(PackError::Io {
                path: dir.to_path_buf(),
                message: e.to_string(),
            });
            return (packs, errors);
        }
    };
    let mut dirs: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join(MANIFEST).is_file())
        .collect();
    dirs.sort();
    for dir in dirs {
        match Pack::load(&dir) {
            Ok(pack) => packs.push(pack),
            Err(e) => errors.push(e),
        }
    }
    (packs, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_defaults_respect_bounds() {
        assert_eq!(
            natural_default(&PropertyType::Int {
                min: Some(1),
                max: None
            }),
            Value::Int(1)
        );
        assert_eq!(
            natural_default(&PropertyType::Int {
                min: None,
                max: Some(-5)
            }),
            Value::Int(-5)
        );
        assert_eq!(
            natural_default(&PropertyType::Enum(vec!["A".into(), "B".into()])),
            Value::Enum("A".into())
        );
    }

    #[test]
    fn value_conversion() {
        let v = |text: &str| -> toml::Value {
            toml::from_str::<toml::Table>(&format!("v = {text}")).unwrap()["v"].clone()
        };
        assert_eq!(
            convert_value(&v("\"#ff8000\""), &PropertyType::Color),
            Ok(Value::Color(Color::rgb(255, 128, 0)))
        );
        assert_eq!(
            convert_value(&v("[1, 2, 3, 4]"), &PropertyType::Color),
            Ok(Value::Color(Color::rgba(1, 2, 3, 4)))
        );
        assert_eq!(
            convert_value(&v("[3, 4]"), &PropertyType::Point),
            Ok(Value::Point(Point { x: 3, y: 4 }))
        );
        assert_eq!(
            convert_value(&v("{ width = 800, height = 600 }"), &PropertyType::Size),
            Ok(Value::Size(Size {
                width: 800,
                height: 600
            }))
        );
        assert_eq!(
            convert_value(
                &v("2"),
                &PropertyType::Float {
                    min: None,
                    max: None
                }
            ),
            Ok(Value::Float(2.0))
        );
        let list = PropertyType::List(Box::new(PropertyType::Text));
        assert!(
            convert_value(&v("[\"a\", 1]"), &list)
                .unwrap_err()
                .starts_with("item 1")
        );
        assert!(convert_value(&v("\"x\""), &PropertyType::Reference(None)).is_err());
        assert!(
            convert_value(
                &v("5"),
                &PropertyType::Int {
                    min: Some(10),
                    max: None
                }
            )
            .is_err()
        );
    }

    #[test]
    fn platforms_and_names() {
        assert_eq!(parse_platform("macos"), Some(Platform::MacOs));
        assert_eq!(
            parse_platform("cfg(unix)"),
            Some(Platform::Other("unix".into()))
        );
        assert_eq!(parse_platform("amiga"), None);
        assert!(valid_version("1.2.3-beta.1"));
        assert!(!valid_version("1.2"));
        assert!(valid_pack_name("my-pack2"));
        assert!(!valid_pack_name("MyPack"));
    }
}
