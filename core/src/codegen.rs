//! Code generation: from forms, packs and the dependency graph to a Rust
//! project.
//!
//! Generation is a pure function: [`generate`] reads existing files through
//! a [`FileSource`] and returns a [`Generation`] – every file with its new
//! content and whether it would be created, updated or stay unchanged, plus
//! all diagnostics. Nothing is written until [`write()`] is called, so the
//! result can be previewed, diffed or checked by a human or an AI first.
//! The output is deterministic: the same input gives byte-identical files.
//!
//! # Project layout
//!
//! | File | Owner | Content |
//! |---|---|---|
//! | `Cargo.toml` | user | dependencies of all used types and the backend are merged in; nothing is removed, formatting and comments stay |
//! | `src/main.rs` | user | created once: `main()` and `started(&mut app)` (programs) |
//! | `src/lib.rs` | user | created once: `pub use app::*;` (libraries, [`ProjectKind::Library`](crate::ProjectKind): data modules only, no `run()`, `App::init` on the caller's runtime) |
//! | `src/app.gen.rs` | generator | module list, `App` with all forms, `App::init` in dependency order, `run()` with the event loop |
//! | `src/<form>.gen.rs` | generator | the form's struct with one field per generated component, plus the backend's `form` template (e.g. `ui()` calling the handlers) |
//! | `src/<form>.rs` | user | created once with `include!("<form>.gen.rs")` and event handler stubs; later only missing stubs are appended |
//! | backend `files` | generator or user | further files of the backend ([`crate::BackendDescriptor::files`], e.g. the Dart side of Flutter): per project or per form with widgets; generator files get the header with checksum, user files are created once |
//!
//! With the event loop `host` (another program drives the UI, e.g. Flutter)
//! there is no `src/main.rs`: the backend brings `src/lib.rs` through its
//! `files`, and its `crate-type`s are added to `[lib]` in `Cargo.toml`.
//!
//! Only Freya TDE writes `*.gen.rs`: [`write()`] makes them read-only, and
//! each carries a checksum of its content. A generated file edited
//! elsewhere is reported ([`CodegenIssue::GeneratedFileModified`]) before
//! it is overwritten. User files are never overwritten.
//!
//! [`Generation::source_map`] maps lines of generated files to elements, so
//! compiler errors of a debug build point back to the designer
//! ([`crate::preview`]).
//!
//! # Templates
//!
//! A component type's [`CodegenSpec`] holds MiniJinja templates (undefined
//! values are errors). A UI backend gives the specs of its widgets and of
//! the core widgets (`[[extend]]`). Context of `type`, `init`, `entry` and
//! `runtime`:
//!
//! | Name | Content |
//! |---|---|
//! | `name`, `form` | component and form name |
//! | `props.<property>` | the value as data: bool, number, string, `{r, g, b, a}`, `{x, y}`, `{width, height}`, list; references as `"form::name"` or none |
//! | `rust.<property>` | the value as a Rust expression: literals, `vec![…]`, the dependency for references (by the target's [`ShareMode`]), `cfg` blocks for platform overrides; none if unset or unconnected |
//! | `events.<event>` | the handler name or none |
//! | `refs.<property>` | for a set reference: the target's `name`, `form`, `type` and `props` (e.g. `refs.pool.props.driver`); missing if unset – test with `is defined` |
//!
//! The backend's `form` template ([`crate::BackendDescriptor::form`]) gets
//! `form` (`{name, struct, data_module}`), `root` (the widget tree or none),
//! `widgets` (all widgets, depth-first) and `components` (non-visual). Each
//! node has `name`, `type` (`"core/Button"`), `type_name` (`"Button"`),
//! `field` (has a generated field), `props`, `rust` (references as
//! `self.<name>`), `events`, `children`, `placement`
//! (`{kind: "layout", stretch, align, cell, width, height, width_percent,
//! height_percent}` – `width`/`height` the requested size, 0 = natural;
//! `*_percent` a share of the parent's content area, 0 = not set, goes
//! first – or `{kind: "absolute", x, y, width, height, anchors: {left, top,
//! right, bottom, any}, margin_right, margin_bottom, area: {width, height}}`
//! – the margins to the right and bottom edge of the design area `area`) and `layout`
//! (`{kind, columns, name, spacing, padding}` or none).
//!
//! Live components (`start`/`methods` in `[component.codegen]`, e.g.
//! WebSocket, HTTP, Axum) give their form (or data module) a queue
//! `freya_queue: crate::app::Queue<Self>`; `start` runs in
//! `freya_start_data`, `methods` go into the form's `impl`, queued handler
//! calls run in `freya_take_data` (the backend's `data_bound` is then set).
//! A live data module runs as a Tokio task (`freya_run`, called by `run()`
//! after `started`; with `host` the backend serves it). `attach` statements
//! follow their component in `App::init`, `helpers` go to `app.gen.rs`
//! once per type. Forms in the `run`/`app` contexts have `live`.
//!
//! The backend's `run` template gets `forms` (list of `{name, struct,
//! data_module}`), `app` (the variable holding the `App`) and `runtime`
//! (the runtime variable or none; with `host` it may be moved), `pictures`
//! (the project shows pictures) and `types` (every type used in the
//! project as text, e.g. `backend:freya-gui/Calendar`, sorted). It must
//! evaluate to `Result<(), Error>`.
//! The optional `setup` template gets the same context; its statements run
//! first in `run()`, before `App::init` (e.g. `gtk4::init()?;`). The
//! optional `app` template (same context) adds items to `app.gen.rs`, e.g.
//! helpers the forms use as `crate::app::…`.
//!
//! Backend `files`: their `path` and content are templates. Per project
//! they get the `run` context plus `project` (`{name, lib, platforms}`,
//! `lib` = the crate's library name); per form the `form` context plus
//! `project`. `raw = true` copies the content as it is.
//!
//! The filter `rust_str` writes a text as a Rust string literal with escapes
//! (`{{ item | rust_str }}` → `"Don't \"x\""`), e.g. for single list items;
//! `dart_str` writes a single-quoted Dart string literal (`'Don\'t \$x'`);
//! `url_join` puts a base URL in front of a path (`{{ base | url_join(path) }}`,
//! absolute URLs stay).
//!
//! Every rendered piece is parsed with `syn`; errors are reported for the
//! component and template part concerned.
//!
//! # Event loop and async
//!
//! The backend's [`EventLoop`] decides. `main-thread` (default): `run()` is
//! synchronous, builds the runtime with the runtime component's `runtime`
//! template, runs `App::init` via `block_on`, calls `started` and then the
//! UI loop. `async`: `run()` carries the runtime's `entry` attribute
//! (`#[tokio::main]`) and `started` is async. Types whose `init` awaits
//! (`is_async`) need a runtime component in the project.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::{self, Write as _};
use std::path::{Path, PathBuf};

use minijinja::value::Value as JValue;
use minijinja::{AutoEscape, Environment, UndefinedBehavior};
use serde::Serialize;
use toml_edit::{Array, DocumentMut, InlineTable, Item, Table};

use crate::backend::{BackendDescriptor, EventLoop, FileOwner, FileScope};
use crate::component_type::{CargoDependency, CodegenSpec, ComponentType, ShareMode};
use crate::element::Element;
use crate::event::EventParam;
use crate::form::Form;
use crate::graph::{self, ProjectIssue};
use crate::name::{ComponentName, ComponentPath, HandlerName};
use crate::project_file::ProjectFile;
use crate::project_file::ProjectKind;
use crate::property::PropertyValue;
use crate::registry::TypeRegistry;
use crate::type_ref::TypeRef;
use crate::validate::{Issue, Severity, ValidationContext};
use crate::value::Value;
use crate::widget::{LayoutKind, Placement, Widget};

/// First line of every generated file.
pub const GENERATED_MARK: &str = "// @generated by Freya TDE";

/// Start of the second line of every generated file, followed by the
/// checksum of everything after the header.
pub const CHECKSUM_MARK: &str = "// checksum: fnv1a64:";

/// Form names that would clash with the generated files.
const RESERVED_FORM_NAMES: &[&str] = &["app", "main"];

// --- Input ------------------------------------------------------------------------------

/// Read access to the existing project files, by path relative to the
/// project directory (`/`-separated, e.g. `src/main.rs`).
pub trait FileSource {
    /// The file's content, `None` if it does not exist.
    fn read(&self, path: &str) -> Option<String>;
}

/// The project directory on disk.
impl FileSource for PathBuf {
    fn read(&self, path: &str) -> Option<String> {
        read_file(self, path)
    }
}

fn read_file(dir: &Path, path: &str) -> Option<String> {
    std::fs::read_to_string(dir.join(path)).ok()
}

/// Files in memory (tests, previews, an AI working on a draft).
impl FileSource for BTreeMap<String, String> {
    fn read(&self, path: &str) -> Option<String> {
        self.get(path).cloned()
    }
}

// --- Output ------------------------------------------------------------------------------

/// Who owns a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    /// Rewritten on every run (`*.gen.rs`).
    Generated,
    /// The user's code: created once, later only stubs are appended.
    User,
    /// `Cargo.toml`: dependencies are merged in.
    Manifest,
}

/// What [`write()`] would do with a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Change {
    Create,
    Update,
    Unchanged,
}

/// One file of the generated project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GeneratedFile {
    /// Relative to the project directory, `/`-separated.
    pub path: String,
    pub kind: FileKind,
    pub change: Change,
    /// The complete new content.
    pub content: String,
    /// The content the generation was based on (`None` = file did not
    /// exist). [`write()`] refuses to write if the file changed meanwhile.
    #[serde(skip)]
    pub previous: Option<String>,
}

/// What is wrong. Serialized as `{"code": "template", "detail": …}`; the
/// codes are stable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum CodegenIssue {
    /// From form validation.
    Form(Issue),
    /// From the dependency graph.
    Project(ProjectIssue),
    /// A non-visual type has no code generation; the component is left out.
    NoCodegen(TypeRef),
    /// The project's UI backend is not loaded; the project is generated
    /// without UI code and event loop.
    NoBackendDescriptor(String),
    /// A template could not be rendered.
    Template {
        /// `type`, `init`, `entry`, `runtime`, or the backend's `form` / `run`.
        part: String,
        message: String,
        /// Line within the template.
        line: Option<usize>,
    },
    /// Rendered or generated code is not valid Rust.
    InvalidRust { part: String, message: String },
    /// A component awaits during initialisation, but no component provides
    /// an async runtime (`entry`).
    AsyncWithoutRuntime,
    /// More than one component provides the async runtime.
    MultipleRuntimes(Vec<ComponentPath>),
    /// The runtime component has no template for the backend's event loop
    /// (`entry` for `async`, `runtime` for `main-thread`).
    RuntimeUnsupported { event_loop: EventLoop },
    /// A generated file was changed outside Freya TDE and will be
    /// overwritten (`foreign`: it was never generated – nothing is written).
    GeneratedFileModified { file: String, foreign: bool },
    /// One handler serves events with different signatures.
    HandlerConflict {
        handler: HandlerName,
        events: Vec<String>,
    },
    /// The form name clashes with a generated file or type.
    ReservedFormName(ComponentName),
    /// Two forms map to the same struct name.
    StructNameClash {
        name: String,
        forms: Vec<ComponentName>,
    },
    /// A user file cannot be parsed, so missing handler stubs cannot be
    /// determined; the file is left alone.
    UserFileUnparsable { file: String, message: String },
    /// A list widget's data binding is incomplete or wrong (unknown source
    /// or column, no async runtime …).
    DataBinding { message: String },
    /// `started` in `main.rs` is missing or its asyncness does not match
    /// the project (async runtime or not).
    StartedSignature { file: String, expected: String },
    /// `Cargo.toml` cannot be read or changed.
    CargoManifest { message: String },
    /// A library project has a form with widgets; libraries have data
    /// modules only (the form's UI code is left out).
    LibraryWithUi(ComponentName),
    /// The own fields of a form (`state`) are wrong (syntax, a name used
    /// twice or by a component); they are left out.
    FormState {
        form: ComponentName,
        message: String,
    },
    /// The project pins another version than a used type asks for; the
    /// project's version is kept.
    CargoVersionDiffers {
        crate_name: String,
        project: String,
        required: String,
    },
}

impl fmt::Display for CodegenIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodegenIssue::Form(issue) => issue.fmt(f),
            CodegenIssue::Project(issue) => issue.fmt(f),
            CodegenIssue::NoBackendDescriptor(b) => write!(
                f,
                "UI backend {b:?} is not loaded; no UI code and no event loop are generated"
            ),
            CodegenIssue::RuntimeUnsupported { event_loop } => write!(
                f,
                "the runtime has no template for the {} event loop (`{}`)",
                match event_loop {
                    EventLoop::MainThread => "main-thread",
                    EventLoop::Async => "async",
                    EventLoop::Host => "host",
                },
                match event_loop {
                    EventLoop::MainThread | EventLoop::Host => "runtime",
                    EventLoop::Async => "entry",
                }
            ),
            CodegenIssue::GeneratedFileModified { file, foreign } => {
                if *foreign {
                    write!(
                        f,
                        "{file} was not generated by Freya TDE; it is not overwritten"
                    )
                } else {
                    write!(
                        f,
                        "{file} was changed outside Freya TDE; the changes are overwritten"
                    )
                }
            }
            CodegenIssue::NoCodegen(t) => {
                write!(
                    f,
                    "type {t} has no code generation; the component is left out"
                )
            }
            CodegenIssue::Template {
                part,
                message,
                line,
            } => match line {
                Some(line) => write!(f, "template `{part}`, line {line}: {message}"),
                None => write!(f, "template `{part}`: {message}"),
            },
            CodegenIssue::InvalidRust { part, message } => {
                write!(f, "{part} is not valid Rust: {message}")
            }
            CodegenIssue::FormState { form, message } => {
                write!(f, "own fields (state) of form {form}: {message}")
            }
            CodegenIssue::LibraryWithUi(form) => write!(
                f,
                "form {form} has widgets, but a library has data modules only"
            ),
            CodegenIssue::AsyncWithoutRuntime => {
                f.write_str("initialisation is async, but no component provides an async runtime")
            }
            CodegenIssue::MultipleRuntimes(paths) => {
                let paths: Vec<String> = paths.iter().map(ToString::to_string).collect();
                write!(
                    f,
                    "only one component may provide the async runtime: {}",
                    paths.join(", ")
                )
            }
            CodegenIssue::HandlerConflict { handler, events } => write!(
                f,
                "handler {handler} serves events with different signatures: {}",
                events.join(", ")
            ),
            CodegenIssue::ReservedFormName(n) => {
                write!(f, "form name {n} is reserved for generated code")
            }
            CodegenIssue::StructNameClash { name, forms } => {
                let forms: Vec<&str> = forms.iter().map(ComponentName::as_str).collect();
                write!(f, "forms {} all become struct {name}", forms.join(", "))
            }
            CodegenIssue::DataBinding { message } => write!(f, "data binding: {message}"),
            CodegenIssue::UserFileUnparsable { file, message } => write!(
                f,
                "{file} cannot be parsed ({message}); missing handler stubs were not added"
            ),
            CodegenIssue::StartedSignature { file, expected } => {
                write!(f, "{file} needs `{expected}`")
            }
            CodegenIssue::CargoManifest { message } => write!(f, "Cargo.toml: {message}"),
            CodegenIssue::CargoVersionDiffers {
                crate_name,
                project,
                required,
            } => write!(
                f,
                "Cargo.toml requires {crate_name} {project:?}, the components ask for {required:?}; keeping {project:?}"
            ),
        }
    }
}

/// One finding of the code generation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CodegenDiagnostic {
    pub severity: Severity,
    /// The element concerned, fully qualified.
    pub element: Option<ComponentPath>,
    pub issue: CodegenIssue,
}

impl fmt::Display for CodegenDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let level = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        match &self.element {
            Some(path) => write!(f, "{level}: {path}: {}", self.issue),
            None => write!(f, "{level}: {}", self.issue),
        }
    }
}

/// Lines of a generated file that belong to one element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceSpan {
    pub file: String,
    /// First and last line, 1-based, inclusive.
    pub first_line: usize,
    pub last_line: usize,
    pub element: ComponentPath,
}

/// The result of [`generate`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Generation {
    pub files: Vec<GeneratedFile>,
    /// Generated components in initialisation order.
    pub order: Vec<ComponentPath>,
    /// Which lines of the generated files belong to which element.
    pub source_map: Vec<SourceSpan>,
    /// Form validation, dependency graph and code generation findings.
    pub diagnostics: Vec<CodegenDiagnostic>,
}

impl Generation {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }

    pub fn file(&self, path: &str) -> Option<&GeneratedFile> {
        self.files.iter().find(|f| f.path == path)
    }

    /// Files [`write()`] would create or update.
    pub fn changed(&self) -> impl Iterator<Item = &GeneratedFile> {
        self.files.iter().filter(|f| f.change != Change::Unchanged)
    }

    /// The element a line of a generated file belongs to.
    pub fn element_at(&self, file: &str, line: usize) -> Option<&ComponentPath> {
        self.source_map
            .iter()
            .find(|s| s.file == file && (s.first_line..=s.last_line).contains(&line))
            .map(|s| &s.element)
    }
}

/// Why [`write()`] wrote nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum WriteError {
    #[error("code generation reported {0} error(s); nothing was written")]
    HasErrors(usize),
    #[error("{0} was changed after the code was generated; generate again")]
    Modified(String),
    #[error("{path}: {message}")]
    Io { path: String, message: String },
}

/// Writes the created and updated files of `generation` below `dir`.
///
/// Refuses if the generation has errors or if a file changed after it was
/// generated (so user edits are never lost). Returns the written paths.
pub fn write(dir: &Path, generation: &Generation) -> Result<Vec<String>, WriteError> {
    let errors = generation
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    if errors > 0 {
        return Err(WriteError::HasErrors(errors));
    }
    let changed: Vec<&GeneratedFile> = generation.changed().collect();
    for file in &changed {
        if read_file(dir, &file.path) != file.previous {
            return Err(WriteError::Modified(file.path.clone()));
        }
    }
    let io = |path: &Path, e: std::io::Error| WriteError::Io {
        path: path.display().to_string(),
        message: e.to_string(),
    };
    let mut written = Vec::new();
    for file in changed {
        let target = dir.join(&file.path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| io(parent, e))?;
        }
        // Write next to the target and rename, so a crash never leaves a
        // half-written file.
        let temp = target.with_extension("tdr-tmp");
        std::fs::write(&temp, &file.content).map_err(|e| io(&temp, e))?;
        if file.kind == FileKind::Generated {
            // Only Freya TDE writes generated files.
            set_read_only(&temp, true).map_err(|e| io(&temp, e))?;
            if target.exists() {
                // Windows cannot replace a read-only file.
                set_read_only(&target, false).map_err(|e| io(&target, e))?;
            }
        }
        std::fs::rename(&temp, &target).map_err(|e| io(&target, e))?;
        written.push(file.path.clone());
    }
    Ok(written)
}

fn set_read_only(path: &Path, read_only: bool) -> std::io::Result<()> {
    let mut permissions = std::fs::metadata(path)?.permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(read_only);
    std::fs::set_permissions(path, permissions)
}

/// FNV-1a, 64 bit: stable across Rust versions and platforms.
fn checksum(text: &str) -> String {
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    format!("{hash:016x}")
}

/// Header plus body of a generated file.
fn generated_file(what: &str, body: &str) -> String {
    format!(
        "{GENERATED_MARK} from {what} – do not edit, changes are overwritten.\n{CHECKSUM_MARK}{}\n\n{body}",
        checksum(body)
    )
}

/// Was an existing generated file changed after it was written? `None`
/// = unchanged, `Some(true)` = never generated, `Some(false)` = edited.
fn tampered(content: &str) -> Option<bool> {
    let mut lines = content.splitn(3, '\n');
    let (Some(mark), Some(sum), Some(rest)) = (lines.next(), lines.next(), lines.next()) else {
        return Some(true);
    };
    if !mark.starts_with(GENERATED_MARK) {
        return Some(true);
    }
    let body = rest.strip_prefix('\n').unwrap_or(rest);
    match sum.strip_prefix(CHECKSUM_MARK) {
        Some(sum) if sum == checksum(body) => None,
        _ => Some(false),
    }
}

// --- Templates ---------------------------------------------------------------------------

fn environment() -> Environment<'static> {
    let mut env = Environment::new();
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env.set_auto_escape_callback(|_| AutoEscape::None);
    // Text als Rust-Stringliteral (auch für Einträge einer Liste).
    env.add_filter("rust_str", |text: String| format!("{text:?}"));
    env.add_filter("pascal", |text: String| pascal_case(&text));
    env.add_filter("dart_str", |text: String| dart_string(&text));
    // Basis-URL und Pfad: `{{ base | url_join(url) }}`; eine absolute URL
    // (mit `://`) oder eine leere Basis lässt `url` stehen.
    env.add_filter("url_join", |base: String, url: String| {
        url_join(&base, &url)
    });
    env
}

/// `base` and `url` joined with one `/` (see the filter `url_join`).
fn url_join(base: &str, url: &str) -> String {
    if base.is_empty() || url.contains("://") {
        url.to_owned()
    } else if url.is_empty() {
        base.to_owned()
    } else {
        format!(
            "{}/{}",
            base.trim_end_matches('/'),
            url.trim_start_matches('/')
        )
    }
}

/// Checks the syntax of a template (used when packs are loaded).
pub(crate) fn check_template(source: &str) -> Result<(), String> {
    environment()
        .template_from_str(source)
        .map(drop)
        .map_err(|e| describe(&e).0)
}

fn describe(error: &minijinja::Error) -> (String, Option<usize>) {
    let message = match error.detail() {
        Some(detail) => format!("{}: {detail}", error.kind()),
        None => error.kind().to_string(),
    };
    (message, error.line())
}

// --- Rust expressions --------------------------------------------------------------------

/// The value as a Rust expression; `None` if there is nothing to write
/// (unconnected reference, empty code).
fn literal(value: &Value, resolve: &dyn Fn(&ComponentPath) -> Option<String>) -> Option<String> {
    Some(match value {
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(x) => format!("{x:?}"),
        Value::Text(s) | Value::Enum(s) => format!("{s:?}"),
        Value::Color(c) => format!("[{}, {}, {}, {}]", c.r, c.g, c.b, c.a),
        Value::Point(p) => format!("({}, {})", p.x, p.y),
        Value::Size(s) => format!("({}, {})", s.width, s.height),
        Value::List(items) => {
            let items: Vec<String> = items.iter().filter_map(|v| literal(v, resolve)).collect();
            format!("vec![{}]", items.join(", "))
        }
        Value::Reference(target) => resolve(target.as_ref()?)?,
        Value::Code(code) | Value::Custom { data: code, .. } => {
            if code.trim().is_empty() {
                return None;
            }
            code.clone()
        }
    })
}

/// A property value as Rust expression; platform overrides become a block
/// with one `cfg`-guarded binding per platform.
fn property_expr(
    value: &PropertyValue,
    resolve: &dyn Fn(&ComponentPath) -> Option<String>,
) -> Option<String> {
    let base = literal(&value.value, resolve)?;
    if value.platforms.is_empty() {
        return Some(base);
    }
    let mut block = String::from("{ ");
    let mut predicates = Vec::new();
    for (platform, v) in &value.platforms {
        let expr = literal(v, resolve).unwrap_or_else(|| base.clone());
        let predicate = platform.cfg();
        let _ = write!(block, "#[cfg({predicate})] let value = {expr}; ");
        predicates.push(predicate);
    }
    let _ = write!(
        block,
        "#[cfg(not(any({})))] let value = {base}; value }}",
        predicates.join(", ")
    );
    Some(block)
}

/// The value as template data.
fn data(value: &Value) -> JValue {
    let map = |pairs: [(&str, i64); 2]| {
        JValue::from(
            pairs
                .into_iter()
                .map(|(k, v)| (k.to_owned(), JValue::from(v)))
                .collect::<BTreeMap<String, JValue>>(),
        )
    };
    match value {
        Value::Bool(b) => JValue::from(*b),
        Value::Int(i) => JValue::from(*i),
        Value::Float(x) => JValue::from(*x),
        Value::Text(s) | Value::Enum(s) | Value::Code(s) => JValue::from(s.as_str()),
        Value::Custom { data, .. } => JValue::from(data.as_str()),
        Value::Color(c) => JValue::from(
            [("r", c.r), ("g", c.g), ("b", c.b), ("a", c.a)]
                .into_iter()
                .map(|(k, v)| (k.to_owned(), JValue::from(v)))
                .collect::<BTreeMap<String, JValue>>(),
        ),
        Value::Point(p) => map([("x", p.x.into()), ("y", p.y.into())]),
        Value::Size(s) => map([("width", s.width.into()), ("height", s.height.into())]),
        Value::List(items) => JValue::from(items.iter().map(data).collect::<Vec<_>>()),
        Value::Reference(target) => target
            .as_ref()
            .map_or(JValue::from(()), |t| JValue::from(t.to_string())),
    }
}

fn share_expr(var: &str, share: ShareMode) -> String {
    match share {
        ShareMode::Clone => format!("{var}.clone()"),
        ShareMode::Arc => format!("std::sync::Arc::clone(&{var})"),
        ShareMode::Ref => format!("&{var}"),
    }
}

/// `main_form` → `MainForm`.
fn pascal_case(name: &str) -> String {
    name.split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|c| c.to_ascii_uppercase().to_string() + chars.as_str())
                .unwrap_or_default()
        })
        .collect()
}

/// Parses and pretty-prints a generated Rust file; on a syntax error the
/// text is returned as is and the error reported.
fn format_rust(part: &str, text: &str, out: &mut Diagnostics) -> String {
    match syn::parse_file(text) {
        Ok(file) => spaced(&prettyplease::unparse(&file)),
        Err(e) => {
            out.error(
                None,
                CodegenIssue::InvalidRust {
                    part: part.to_owned(),
                    message: e.to_string(),
                },
            );
            text.to_owned()
        }
    }
}

/// Puts blank lines back between items, which `prettyplease` leaves out:
/// after the inner doc block, after a closing `}` of an item or block at the
/// top two levels, and between top-level items ending in `;`.
fn spaced(text: &str) -> String {
    let indent = |line: &str| line.len() - line.trim_start().len();
    let mut out = String::with_capacity(text.len() + text.len() / 8);
    let mut previous: Option<&str> = None;
    for line in text.lines() {
        if let Some(prev) = previous {
            let after_inner_doc = prev.starts_with("//!") && !line.starts_with("//!");
            let same_level = indent(prev) == indent(line) && indent(line) <= 4;
            let closes = line.trim_start().starts_with(['}', ')', ']']);
            let ends_item = prev.trim() == "}" || (indent(line) == 0 && prev.ends_with(';'));
            if after_inner_doc || (same_level && !closes && ends_item) {
                out.push('\n');
            }
        }
        out.push_str(line);
        out.push('\n');
        previous = Some(line);
    }
    out
}

// --- Generator ---------------------------------------------------------------------------

#[derive(Default)]
struct Diagnostics(Vec<CodegenDiagnostic>);

impl Diagnostics {
    fn push(&mut self, severity: Severity, element: Option<ComponentPath>, issue: CodegenIssue) {
        self.0.push(CodegenDiagnostic {
            severity,
            element,
            issue,
        });
    }

    fn error(&mut self, element: Option<ComponentPath>, issue: CodegenIssue) {
        self.push(Severity::Error, element, issue);
    }

    fn warning(&mut self, element: Option<ComponentPath>, issue: CodegenIssue) {
        self.push(Severity::Warning, element, issue);
    }
}

/// A component with its generated code.
struct Generated {
    path: ComponentPath,
    /// Local variable in `App::init`.
    var: String,
    /// Field type in the form struct.
    field_type: String,
    type_ref: TypeRef,
    /// `let var = …;`
    statement: String,
    is_async: bool,
    entry: Option<String>,
    runtime: Option<String>,
    is_runtime: bool,
    /// Runs code when its form starts or adds methods (the form gets a queue).
    live: bool,
}

/// Is `form` live: does a generated component of it start code or add
/// methods? Then it has an event queue (`freya_queue`).
fn is_live(form: &Form, generated: &[Generated]) -> bool {
    generated
        .iter()
        .any(|g| g.live && g.path.form.as_ref() == Some(&form.name))
}

/// How references are written in a template's `rust` values.
type Resolve<'r> = &'r dyn Fn(&ComponentPath) -> Option<String>;

/// The property values of an element as template data (with defaults).
fn props_data(
    form: &Form,
    element: &dyn Element,
    ty: &ComponentType,
    registry: &TypeRegistry,
) -> BTreeMap<String, JValue> {
    let component = element.component();
    registry
        .property_defs(&ty.type_ref, &form.backend)
        .into_iter()
        .filter_map(|def| {
            let value = match (component.get(&def.name), &def.default) {
                (Some(value), _) => value.value.clone(),
                (None, Some(default)) => default.clone(),
                (None, None) => return None,
            };
            Some((def.name.clone(), data(&value)))
        })
        .collect()
}

/// `refs` for templates: per reference property the target component
/// (`name`, `form`, `type` = type reference, `props`), so a template can
/// depend on what it points to (`refs.pool.props.driver`). Unset or
/// dangling references are missing (validation reports them).
fn refs_data(
    forms: &[Form],
    form: &Form,
    element: &dyn Element,
    registry: &TypeRegistry,
) -> JValue {
    let component = element.component();
    let mut refs = BTreeMap::new();
    for (name, value) in &component.properties {
        let Value::Reference(Some(path)) = &value.value else {
            continue;
        };
        let target = path.qualified(&form.name);
        let Some(target_form) = forms.iter().find(|f| target.form.as_ref() == Some(&f.name)) else {
            continue;
        };
        let Some(target_element) = target_form.element(&target.name) else {
            continue;
        };
        let Some(target_ty) = registry.get(target_element.type_ref()) else {
            continue;
        };
        refs.insert(
            name.clone(),
            minijinja::context! {
                name => target.name.as_str(),
                form => target_form.name.as_str(),
                type => target_ty.type_ref.to_string(),
                props => props_data(target_form, target_element, target_ty, registry),
            },
        );
    }
    JValue::from(refs)
}

/// The columns a data source always has (its `[component.data] columns`
/// template over `props`), e.g. `["field: String", "value: String"]` for a
/// Redis hash; `None` if the type has no such template.
pub fn implied_columns(
    form: &Form,
    element: &dyn Element,
    registry: &TypeRegistry,
) -> Option<Vec<String>> {
    let ty = registry.get(element.type_ref())?;
    let template = ty.data.as_ref()?.columns.as_ref()?;
    let context = minijinja::context! {
        name => element.name().as_str(),
        form => form.name.as_str(),
        props => props_data(form, element, ty, registry),
    };
    let text = environment().render_str(template, &context).ok()?;
    Some(
        text.split(';')
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .map(str::to_owned)
            .collect(),
    )
}

/// The parameters of `event` on `element`. Parameter types may be templates
/// (`&sqlx::{{ ... }}Row`) over `props` and `refs` of the element, so they can
/// follow its settings; a type that fails to render is kept as written.
pub fn event_params(
    forms: &[Form],
    form: &Form,
    element: &dyn Element,
    registry: &TypeRegistry,
    def: &crate::EventDef,
) -> Vec<EventParam> {
    if !def
        .params
        .iter()
        .any(|p| p.rust_type.contains("{{") || p.rust_type.contains("{%"))
    {
        return def.params.clone();
    }
    let Some(ty) = registry.get(element.type_ref()) else {
        return def.params.clone();
    };
    let env = environment();
    let context = minijinja::context! {
        name => element.name().as_str(),
        form => form.name.as_str(),
        props => props_data(form, element, ty, registry),
        refs => refs_data(forms, form, element, registry),
    };
    def.params
        .iter()
        .map(|p| {
            let rust_type = env
                .render_str(&p.rust_type, &context)
                .map(|t| t.trim().to_owned())
                .unwrap_or_else(|_| p.rust_type.clone());
            EventParam {
                name: p.name.clone(),
                rust_type,
            }
        })
        .collect()
}

/// `props`, `rust` and `events` of an element.
fn element_data(
    form: &Form,
    element: &dyn Element,
    ty: &ComponentType,
    registry: &TypeRegistry,
    resolve: Resolve<'_>,
) -> (JValue, JValue, JValue) {
    let component = element.component();
    let mut props = BTreeMap::new();
    let mut rust = BTreeMap::new();
    for def in registry.property_defs(&ty.type_ref, &form.backend) {
        let value = match (component.get(&def.name), &def.default) {
            (Some(value), _) => value.clone(),
            (None, Some(default)) => PropertyValue::new(default.clone()),
            (None, None) => continue,
        };
        props.insert(def.name.clone(), data(&value.value));
        rust.insert(
            def.name.clone(),
            property_expr(&value, resolve).map_or(JValue::from(()), JValue::from),
        );
    }
    let events: BTreeMap<String, JValue> = ty
        .events
        .iter()
        .map(|e| {
            let handler = component.events.get(&e.name);
            (
                e.name.clone(),
                handler.map_or(JValue::from(()), |h| JValue::from(h.as_str())),
            )
        })
        .collect();
    (
        JValue::from(props),
        JValue::from(rust),
        JValue::from(events),
    )
}

/// Text of a column value `expr` of type `rust_type` for lists and grids:
/// `to_string()`, `{:?}` for vectors (bytes, arrays), empty for `None`.
fn cell_text(expr: &str, rust_type: &str) -> String {
    let rust_type = rust_type.trim();
    let (optional, inner) = match rust_type
        .strip_prefix("Option<")
        .and_then(|t| t.strip_suffix('>'))
    {
        Some(inner) => (true, inner.trim()),
        None => (false, rust_type),
    };
    let text = |v: &str| {
        if inner.starts_with("Vec<") || inner.starts_with("std::vec::Vec<") {
            format!("format!(\"{{:?}}\", {v})")
        } else {
            format!("{v}.to_string()")
        }
    };
    if optional {
        format!("{expr}.as_ref().map(|v| {}).unwrap_or_default()", text("v"))
    } else {
        text(expr)
    }
}

/// Renders a template; on failure the issue is recorded and `None`
/// returned.
fn render(
    env: &Environment<'static>,
    part: &str,
    source: &str,
    context: &JValue,
    issues: &mut Vec<CodegenIssue>,
) -> Option<String> {
    let rendered = env
        .template_from_named_str(part, source)
        .and_then(|t| t.render(context));
    match rendered {
        Ok(text) => Some(text.trim().to_owned()),
        Err(e) => {
            let (message, line) = describe(&e);
            issues.push(CodegenIssue::Template {
                part: part.to_owned(),
                message,
                line,
            });
            None
        }
    }
}

/// A text as a single-quoted Dart string literal (`$` and `\` escaped).
fn dart_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('\'');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '$' => out.push_str("\\$"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{{{:x}}}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('\'');
    out
}

/// Records an [`CodegenIssue::InvalidRust`] if `result` failed.
fn check_rust(issues: &mut Vec<CodegenIssue>, part: &str, text: &str, result: syn::Result<()>) {
    if let Err(e) = result {
        issues.push(CodegenIssue::InvalidRust {
            part: format!("rendered `{part}` ({text})"),
            message: e.to_string(),
        });
    }
}

/// Renders the templates of one component.
#[allow(clippy::too_many_arguments)]
fn render_component(
    env: &Environment<'static>,
    forms: &[Form],
    form: &Form,
    element: &dyn Element,
    ty: &ComponentType,
    spec: &CodegenSpec,
    registry: &TypeRegistry,
    known: &HashMap<ComponentPath, (String, ShareMode)>,
) -> Result<Generated, Vec<CodegenIssue>> {
    let component = element.component();
    let resolve = |target: &ComponentPath| {
        known
            .get(&target.qualified(&form.name))
            .map(|(var, share)| share_expr(var, *share))
    };
    let (props, rust, events) = element_data(form, element, ty, registry, &resolve);
    let context = minijinja::context! {
        name => component.name.as_str(),
        form => form.name.as_str(),
        props => props,
        rust => rust,
        events => events,
        refs => refs_data(forms, form, element, registry),
    };

    let mut issues = Vec::new();
    let rust_type = render(env, "type", &spec.rust_type, &context, &mut issues);
    let init = render(env, "init", &spec.init, &context, &mut issues);
    let entry = spec
        .entry
        .as_ref()
        .and_then(|e| render(env, "entry", e, &context, &mut issues));
    let runtime = spec
        .runtime
        .as_ref()
        .and_then(|r| render(env, "runtime", r, &context, &mut issues));

    if let Some(t) = &rust_type {
        check_rust(
            &mut issues,
            "type",
            t,
            syn::parse_str::<syn::Type>(t).map(drop),
        );
    }
    if let Some(i) = &init {
        check_rust(
            &mut issues,
            "init",
            i,
            syn::parse_str::<syn::Expr>(i).map(drop),
        );
    }
    if let Some(r) = &runtime {
        check_rust(
            &mut issues,
            "runtime",
            r,
            syn::parse_str::<syn::Expr>(r).map(drop),
        );
    }
    if let Some(e) = &entry {
        let parsed = syn::parse_str::<syn::ItemFn>(&format!("{e} fn entry() {{}}"));
        let result = match parsed {
            Ok(f) if f.attrs.is_empty() => Err(syn::Error::new(
                f.sig.ident.span(),
                "expected an attribute such as #[tokio::main]",
            )),
            other => other.map(drop),
        };
        check_rust(&mut issues, "entry", e, result);
    }
    let (Some(rust_type), Some(init), true) = (rust_type, init, issues.is_empty()) else {
        return Err(issues);
    };

    let path = ComponentPath::on(form.name.clone(), component.name.clone());
    let var = format!("{}__{}", form.name, component.name);
    let (field_type, mut statement) = match spec.share {
        ShareMode::Arc => (
            format!("std::sync::Arc<{rust_type}>"),
            format!("let {var} = std::sync::Arc::new({init});"),
        ),
        ShareMode::Clone | ShareMode::Ref => (rust_type, format!("let {var} = {init};")),
    };
    // Anhängen an eine andere Komponente (z. B. Route am Router), gleich
    // nach dem Anlegen; ein leerer Verweis hängt nichts an.
    if let (Some(template), Some(property)) = (&spec.attach, &spec.attach_to) {
        let target = match component.get(property).map(|v| &v.value) {
            Some(Value::Reference(Some(target))) => known
                .get(&target.qualified(&form.name))
                .map(|(v, _)| v.clone()),
            _ => None,
        };
        if let Some(target) = target {
            let context = minijinja::context! { this => var.as_str(), ..context };
            let mut issues = Vec::new();
            if let Some(text) = render(env, "attach", template, &context, &mut issues) {
                let expr = format!("{target}{text}");
                check_rust(
                    &mut issues,
                    "attach",
                    &expr,
                    syn::parse_str::<syn::Expr>(&expr).map(drop),
                );
                if issues.is_empty() {
                    let _ = write!(statement, "\nlet {target} = {expr};");
                }
            }
            if !issues.is_empty() {
                return Err(issues);
            }
        }
    }
    Ok(Generated {
        path,
        var,
        field_type,
        type_ref: ty.type_ref.clone(),
        statement,
        is_async: spec.is_async,
        entry,
        runtime,
        is_runtime: spec.is_runtime(),
        live: spec.is_live(),
    })
}

/// What live components add to their form: statements for
/// `freya_start_data` and methods.
#[derive(Default)]
struct FormRuntime {
    starts: Vec<String>,
    methods: Vec<String>,
}

/// Renders `start` and `methods` of the live components of `form`.
fn form_runtime(
    env: &Environment<'static>,
    forms: &[Form],
    form: &Form,
    generated: &[Generated],
    registry: &TypeRegistry,
    out: &mut Diagnostics,
) -> FormRuntime {
    let mut runtime = FormRuntime::default();
    let fields: HashSet<&ComponentName> = form_components(form, generated)
        .iter()
        .map(|g| &g.path.name)
        .collect();
    let resolve = |target: &ComponentPath| {
        let own = target.target_form(&form.name) == &form.name;
        (own && fields.contains(&target.name)).then(|| format!("self.{}", target.name))
    };
    for g in form_components(form, generated)
        .into_iter()
        .filter(|g| g.live)
    {
        let (Some(element), Some(ty)) = (form.element(&g.path.name), registry.get(&g.type_ref))
        else {
            continue;
        };
        let Some(spec) = registry.codegen(&g.type_ref, &form.backend) else {
            continue;
        };
        let (props, rust, events) = element_data(form, element, ty, registry, &resolve);
        let context = minijinja::context! {
            name => g.path.name.as_str(),
            form => form.name.as_str(),
            form_struct => pascal_case(form.name.as_str()),
            props => props,
            rust => rust,
            events => events,
            refs => refs_data(forms, form, element, registry),
        };
        let mut issues = Vec::new();
        if let Some(template) = &spec.start
            && let Some(text) = render(env, "start", template, &context, &mut issues)
        {
            let block = format!("{{ {text} }}");
            check_rust(
                &mut issues,
                "start",
                &text,
                syn::parse_str::<syn::Block>(&block).map(drop),
            );
            runtime.starts.push(block);
        }
        if let Some(template) = &spec.methods
            && let Some(text) = render(env, "methods", template, &context, &mut issues)
        {
            check_rust(
                &mut issues,
                "methods",
                &text,
                syn::parse_str::<syn::ItemImpl>(&format!("impl X {{ {text} }}")).map(drop),
            );
            runtime.methods.push(text);
        }
        for issue in issues {
            out.error(Some(g.path.clone()), issue);
        }
    }
    runtime
}

/// How `run()` starts the program.
enum Start {
    /// A library: no `run()`; `init` is async if a component is (the
    /// caller's runtime).
    Library { is_async: bool },
    /// Synchronous `run()`, no runtime.
    Plain,
    /// Async `run()` with this entry attribute.
    Entry(String),
    /// Synchronous `run()` that builds the runtime with this expression.
    Runtime(String),
}

impl Start {
    fn init_is_async(&self) -> bool {
        match self {
            Start::Library { is_async } => *is_async,
            Start::Plain => false,
            _ => true,
        }
    }

    fn started_is_async(&self) -> bool {
        matches!(self, Start::Entry(_))
    }
}

/// Generates the project. See the [module documentation](self).
pub fn generate(
    project: &ProjectFile,
    forms: &[Form],
    registry: &TypeRegistry,
    source: &dyn FileSource,
) -> Generation {
    let mut out = Diagnostics::default();
    let library = project.kind == ProjectKind::Library;
    // Eine Bibliothek hat keine Oberfläche: das UI-Backend zählt nicht.
    let backend = registry.backend(&project.backend).filter(|_| !library);
    if library {
        for form in forms.iter().filter(|f| !f.is_data_module()) {
            out.error(
                Some(ComponentPath::on(form.name.clone(), form_anchor(form))),
                CodegenIssue::LibraryWithUi(form.name.clone()),
            );
        }
    } else if backend.is_none() {
        out.warning(
            None,
            CodegenIssue::NoBackendDescriptor(project.backend.clone()),
        );
    }

    // Checks of the model.
    let context = ValidationContext::for_backend(registry, &project.backend);
    for form in forms {
        for d in form.validate(&context) {
            let element = d.element.map(|e| ComponentPath::on(form.name.clone(), e));
            out.push(d.severity, element, CodegenIssue::Form(d.issue));
        }
    }
    let analysis = graph::analyze(forms, registry);
    for d in analysis.diagnostics {
        out.push(d.severity, d.element, CodegenIssue::Project(d.issue));
    }
    check_form_names(forms, &mut out);

    // Components in initialisation order.
    let env = environment();
    let mut generated: Vec<Generated> = Vec::new();
    let mut known: HashMap<ComponentPath, (String, ShareMode)> = HashMap::new();
    for path in &analysis.order {
        let Some(form) = forms.iter().find(|f| path.form.as_ref() == Some(&f.name)) else {
            continue;
        };
        let Some(element) = form.element(&path.name) else {
            continue;
        };
        let Some(ty) = registry.get(element.type_ref()) else {
            continue;
        };
        let Some(spec) = registry.codegen(&ty.type_ref, &form.backend) else {
            // Widgets without code are drawn by the backend's form template.
            if !ty.kind.is_widget() {
                out.warning(
                    Some(path.clone()),
                    CodegenIssue::NoCodegen(ty.type_ref.clone()),
                );
            }
            continue;
        };
        match render_component(&env, forms, form, element, ty, spec, registry, &known) {
            Ok(g) => {
                known.insert(g.path.clone(), (g.var.clone(), spec.share));
                generated.push(g);
            }
            Err(issues) => {
                for issue in issues {
                    out.error(Some(path.clone()), issue);
                }
            }
        }
    }

    let start = if library {
        let runtimes: Vec<&Generated> = generated.iter().filter(|g| g.is_runtime).collect();
        if runtimes.len() > 1 {
            out.error(
                None,
                CodegenIssue::MultipleRuntimes(runtimes.iter().map(|g| g.path.clone()).collect()),
            );
        }
        Start::Library {
            is_async: generated.iter().any(|g| g.is_async) || !runtimes.is_empty(),
        }
    } else {
        start(backend, &generated, &mut out)
    };
    let bindings = data_bindings(
        forms,
        &generated,
        registry,
        &project.backend,
        &env,
        &start,
        &mut out,
    );
    let states = form_states(forms, &generated, &mut out);
    let mut cargo = analysis.cargo.clone();
    if let Some(backend) = backend {
        merge_cargo(&mut cargo, &backend.cargo);
    }
    // Ein Host-Programm (z. B. Flutter) ruft die Rust-Seite als Bibliothek auf.
    let host = backend.is_some_and(|b| b.event_loop == EventLoop::Host);
    let crate_types = backend.map_or(&[][..], |b| b.crate_types.as_slice());

    let mut files = vec![manifest(project, &cargo, crate_types, source, &mut out)];
    if library {
        files.push(lib_file(source));
    } else if !host {
        // Bei `host` bringt das Backend `src/lib.rs` mit (`files`).
        files.push(main_file(start.started_is_async(), source, &mut out));
    }
    files.push(app_file(
        forms, &generated, &bindings, &states, &start, registry, backend, host, &env, source,
        &mut out,
    ));
    for form in forms {
        let form_bindings = bindings.get(&form.name).map_or(&[][..], Vec::as_slice);
        let runtime = form_runtime(&env, forms, form, &generated, registry, &mut out);
        let form_state = states.get(&form.name).map_or(&[][..], Vec::as_slice);
        files.push(form_gen_file(
            form,
            &generated,
            form_bindings,
            form_state,
            &runtime,
            registry,
            backend,
            &env,
            source,
            &mut out,
        ));
        files.push(form_user_file(forms, form, registry, source, &mut out));
    }
    if let Some(backend) = backend {
        backend_files(
            project, forms, &generated, &bindings, &start, registry, backend, &env, source,
            &mut files, &mut out,
        );
    }

    let mut source_map = Vec::new();
    for file in files.iter().filter(|f| f.kind == FileKind::Generated) {
        map_sources(file, forms, &generated, &mut source_map);
        if let Some(previous) = &file.previous
            && let Some(foreign) = tampered(previous)
        {
            let issue = CodegenIssue::GeneratedFileModified {
                file: file.path.clone(),
                foreign,
            };
            if foreign {
                out.error(None, issue);
            } else {
                out.warning(None, issue);
            }
        }
    }

    Generation {
        files,
        order: generated.into_iter().map(|g| g.path).collect(),
        source_map,
        diagnostics: out.0,
    }
}

/// Decides how `run()` starts: from the backend's event loop and the
/// project's runtime component.
fn start(
    backend: Option<&BackendDescriptor>,
    generated: &[Generated],
    out: &mut Diagnostics,
) -> Start {
    let runtimes: Vec<&Generated> = generated.iter().filter(|g| g.is_runtime).collect();
    if runtimes.len() > 1 {
        out.error(
            None,
            CodegenIssue::MultipleRuntimes(runtimes.iter().map(|g| g.path.clone()).collect()),
        );
    }
    let Some(runtime) = runtimes.first() else {
        for g in generated.iter().filter(|g| g.is_async) {
            out.error(Some(g.path.clone()), CodegenIssue::AsyncWithoutRuntime);
        }
        return Start::Plain;
    };
    // Without a backend there is no UI loop: the runtime may own `run()`.
    let event_loop = backend.map_or(EventLoop::Async, |b| b.event_loop);
    let chosen = match event_loop {
        EventLoop::Async => runtime.entry.clone().map(Start::Entry),
        EventLoop::MainThread | EventLoop::Host => runtime.runtime.clone().map(Start::Runtime),
    };
    chosen.unwrap_or_else(|| {
        out.error(
            Some(runtime.path.clone()),
            CodegenIssue::RuntimeUnsupported { event_loop },
        );
        Start::Plain
    })
}

/// Does `line` contain `word` not as part of a longer identifier?
fn contains_word(line: &str, word: &str) -> bool {
    let ident = |c: char| c == '_' || c.is_alphanumeric();
    line.match_indices(word)
        .any(|(at, _)| !line[..at].ends_with(ident) && !line[at + word.len()..].starts_with(ident))
}

/// Adds `extra` to `cargo` (features are merged), sorted by crate name.
fn merge_cargo(cargo: &mut Vec<CargoDependency>, extra: &[CargoDependency]) {
    for dep in extra {
        match cargo.iter_mut().find(|c| c.crate_name == dep.crate_name) {
            Some(existing) => {
                for feature in &dep.features {
                    if !existing.features.contains(feature) {
                        existing.features.push(feature.clone());
                    }
                }
                existing.features.sort();
                existing.default_features |= dep.default_features;
            }
            None => cargo.push(dep.clone()),
        }
    }
    cargo.sort_by(|a, b| a.crate_name.cmp(&b.crate_name));
}

/// Records which lines of a generated file belong to which element.
fn map_sources(
    file: &GeneratedFile,
    forms: &[Form],
    generated: &[Generated],
    out: &mut Vec<SourceSpan>,
) {
    let lines: Vec<&str> = file.content.lines().collect();
    let mut owner: Vec<Option<ComponentPath>> = vec![None; lines.len()];
    if file.path == APP {
        // `let form__name = …;` up to the next statement.
        let mut current: Option<ComponentPath> = None;
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("let ") || trimmed.starts_with("Ok(") || trimmed == "}" {
                current = generated
                    .iter()
                    .find(|g| trimmed.starts_with(&format!("let {} ", g.var)))
                    .map(|g| g.path.clone());
            }
            owner[i] = current.clone();
        }
    } else if let Some(form) = forms
        .iter()
        .find(|f| file.path == format!("src/{}.gen.rs", f.name))
    {
        // Fields, and lines of the form template naming the element or
        // calling one of its handlers.
        let elements = form.elements();
        for (i, line) in lines.iter().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            owner[i] = elements.iter().find_map(|e| {
                let mentioned = contains_word(line, e.name().as_str())
                    || e.component()
                        .events
                        .values()
                        .any(|h| contains_word(line, &format!("self.{h}")));
                mentioned.then(|| ComponentPath::on(form.name.clone(), e.name().clone()))
            });
        }
    }
    let mut i = 0;
    while i < lines.len() {
        let Some(element) = owner[i].clone() else {
            i += 1;
            continue;
        };
        let first = i;
        while i + 1 < lines.len() && owner[i + 1].as_ref() == Some(&element) {
            i += 1;
        }
        out.push(SourceSpan {
            file: file.path.clone(),
            first_line: first + 1,
            last_line: i + 1,
            element,
        });
        i += 1;
    }
}

fn check_form_names(forms: &[Form], out: &mut Diagnostics) {
    let mut structs: BTreeMap<String, Vec<ComponentName>> = BTreeMap::new();
    for form in forms {
        let name = pascal_case(form.name.as_str());
        if RESERVED_FORM_NAMES.contains(&form.name.as_str()) || name == "Self" {
            out.error(None, CodegenIssue::ReservedFormName(form.name.clone()));
        }
        structs.entry(name).or_default().push(form.name.clone());
    }
    for (name, forms) in structs {
        let distinct: BTreeSet<&ComponentName> = forms.iter().collect();
        if distinct.len() > 1 {
            out.error(None, CodegenIssue::StructNameClash { name, forms });
        }
    }
}

fn file(path: &str, kind: FileKind, content: String, source: &dyn FileSource) -> GeneratedFile {
    let previous = source.read(path);
    let change = match &previous {
        None => Change::Create,
        Some(p) if *p == content => Change::Unchanged,
        Some(_) => Change::Update,
    };
    GeneratedFile {
        path: path.to_owned(),
        kind,
        change,
        content,
        previous,
    }
}

// --- Cargo.toml ----------------------------------------------------------------------------

const MANIFEST: &str = "Cargo.toml";

fn manifest(
    project: &ProjectFile,
    dependencies: &[CargoDependency],
    crate_types: &[String],
    source: &dyn FileSource,
    out: &mut Diagnostics,
) -> GeneratedFile {
    let mut doc = match source.read(MANIFEST) {
        None => new_manifest(project),
        Some(text) => match text.parse::<DocumentMut>() {
            Ok(doc) => doc,
            Err(e) => {
                out.error(
                    None,
                    CodegenIssue::CargoManifest {
                        message: e.to_string(),
                    },
                );
                return file(MANIFEST, FileKind::Manifest, text, source);
            }
        },
    };
    merge_dependencies(&mut doc, dependencies, out);
    merge_crate_types(&mut doc, crate_types);
    file(MANIFEST, FileKind::Manifest, doc.to_string(), source)
}

fn new_manifest(project: &ProjectFile) -> DocumentMut {
    let mut package = Table::new();
    package["name"] = toml_edit::value(project.name.as_str());
    package["version"] = toml_edit::value("0.1.0");
    package["edition"] = toml_edit::value("2024");
    package["rust-version"] = toml_edit::value(project.rust_version.as_str());
    let mut doc = DocumentMut::new();
    doc["package"] = Item::Table(package);
    doc["dependencies"] = Item::Table(Table::new());
    doc
}

/// Adds missing `[lib] crate-type` entries; existing ones stay.
fn merge_crate_types(doc: &mut DocumentMut, crate_types: &[String]) {
    if crate_types.is_empty() {
        return;
    }
    let lib = doc
        .as_table_mut()
        .entry("lib")
        .or_insert_with(|| Item::Table(Table::new()));
    let Some(lib) = lib.as_table_like_mut() else {
        return;
    };
    let entry = lib
        .entry("crate-type")
        .or_insert(toml_edit::value(Array::new()));
    let Some(array) = entry.as_array_mut() else {
        return;
    };
    for crate_type in crate_types {
        if !array.iter().any(|v| v.as_str() == Some(crate_type)) {
            array.push(crate_type.as_str());
        }
    }
}

fn features_array(features: &[String]) -> Array {
    features.iter().map(String::as_str).collect()
}

fn merge_dependencies(
    doc: &mut DocumentMut,
    dependencies: &[CargoDependency],
    out: &mut Diagnostics,
) {
    let table = doc
        .entry("dependencies")
        .or_insert_with(|| Item::Table(Table::new()));
    let Some(table) = table.as_table_like_mut() else {
        out.error(
            None,
            CodegenIssue::CargoManifest {
                message: "[dependencies] is not a table".to_owned(),
            },
        );
        return;
    };
    for dep in dependencies {
        let Some(item) = table.get_mut(&dep.crate_name) else {
            let item = if dep.features.is_empty() && dep.default_features {
                toml_edit::value(dep.version.as_str())
            } else {
                let mut t = InlineTable::new();
                t.insert("version", dep.version.as_str().into());
                if !dep.features.is_empty() {
                    t.insert("features", features_array(&dep.features).into());
                }
                if !dep.default_features {
                    t.insert("default-features", false.into());
                }
                toml_edit::value(t)
            };
            table.insert(&dep.crate_name, item);
            continue;
        };

        // Present: keep the user's version, add missing features.
        let version = item
            .as_str()
            .or_else(|| item.get("version").and_then(Item::as_str))
            .map(str::to_owned);
        if let Some(version) = &version
            && version != &dep.version
        {
            out.warning(
                None,
                CodegenIssue::CargoVersionDiffers {
                    crate_name: dep.crate_name.clone(),
                    project: version.clone(),
                    required: dep.version.clone(),
                },
            );
        }
        if dep.features.is_empty() {
            continue;
        }
        if let Some(version) = item.as_str().map(str::to_owned) {
            let mut t = InlineTable::new();
            t.insert("version", version.into());
            t.insert("features", features_array(&dep.features).into());
            *item = toml_edit::value(t);
            continue;
        }
        let Some(t) = item.as_table_like_mut() else {
            continue;
        };
        match t.get_mut("features").and_then(Item::as_array_mut) {
            Some(existing) => {
                let present: BTreeSet<String> = existing
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect();
                for feature in dep.features.iter().filter(|f| !present.contains(*f)) {
                    existing.push(feature.as_str());
                }
            }
            None => {
                t.insert("features", toml_edit::value(features_array(&dep.features)));
            }
        }
    }
}

// --- main.rs and app.gen.rs ------------------------------------------------------------

const MAIN: &str = "src/main.rs";
const LIB: &str = "src/lib.rs";
const APP: &str = "src/app.gen.rs";

fn started_signature(is_async: bool) -> String {
    let asyncness = if is_async { "async " } else { "" };
    format!("{asyncness}fn started(app: &mut app::App) -> Result<(), app::Error>")
}

fn main_file(is_async: bool, source: &dyn FileSource, out: &mut Diagnostics) -> GeneratedFile {
    let expected = started_signature(is_async);
    let Some(existing) = source.read(MAIN) else {
        let text = format!(
            r#"//! Entry point – your code. Freya TDE creates this file once and never
//! changes it; the generated part is `app.gen.rs`.

#[path = "app.gen.rs"]
mod app;

fn main() {{
    if let Err(error) = app::run() {{
        eprintln!("error: {{error}}");
        std::process::exit(1);
    }}
}}

/// Called once all components are created, before the UI starts.
{expected} {{
    let _ = app;
    Ok(())
}}
"#
        );
        let content = format_rust(MAIN, &text, out);
        return file(MAIN, FileKind::User, content, source);
    };
    let matches = syn::parse_file(&existing).is_ok_and(|parsed| {
        parsed.items.iter().any(|item| {
            matches!(item, syn::Item::Fn(f)
                if f.sig.ident == "started" && f.sig.asyncness.is_some() == is_async)
        })
    });
    if !matches {
        out.warning(
            None,
            CodegenIssue::StartedSignature {
                file: MAIN.to_owned(),
                expected,
            },
        );
    }
    file(MAIN, FileKind::User, existing, source)
}

/// `src/lib.rs` of a library: created once, then the user's.
fn lib_file(source: &dyn FileSource) -> GeneratedFile {
    let content = source.read(LIB).unwrap_or_else(|| {
        "//! Library – your code. Freya TDE creates this file once and never\n\
         //! changes it; the generated part is `app.gen.rs`.\n\
         //!\n\
         //! `App::init()` creates all components of the data modules (async\n\
         //! components on the caller's Tokio runtime).\n\
         \n\
         #[path = \"app.gen.rs\"]\n\
         mod app;\n\
         \n\
         pub use app::*;\n"
            .to_owned()
    });
    file(LIB, FileKind::User, content, source)
}

/// `{name, struct, data_module}` of a form, for templates.
fn form_data(form: &Form, generated: &[Generated]) -> JValue {
    minijinja::context! {
        name => form.name.as_str(),
        struct => pascal_case(form.name.as_str()),
        data_module => form.is_data_module(),
        // Hat eine Ereignis-Warteschlange (`freya_queue`, `freya_take_data`).
        live => is_live(form, generated),
    }
}

#[allow(clippy::too_many_arguments)]
fn app_file(
    forms: &[Form],
    generated: &[Generated],
    bindings: &HashMap<ComponentName, Vec<Binding>>,
    states: &HashMap<ComponentName, Vec<crate::state::StateField>>,
    start: &Start,
    registry: &TypeRegistry,
    backend: Option<&BackendDescriptor>,
    host: bool,
    env: &Environment<'static>,
    source: &dyn FileSource,
    out: &mut Diagnostics,
) -> GeneratedFile {
    let mut code = String::new();
    for form in forms {
        let _ = writeln!(code, "#[path = \"{0}.rs\"]\npub mod {0};", form.name);
    }
    code.push_str(
        "/// Error type of the generated initialisation.\n\
         pub type Error = Box<dyn std::error::Error + Send + Sync>;\n\
         /// All forms and data modules of the project.\n\
         #[allow(dead_code)]\n\
         pub struct App {\n",
    );
    for form in forms {
        let _ = writeln!(
            code,
            "pub {0}: {0}::{1},",
            form.name,
            pascal_case(form.name.as_str())
        );
    }
    let asyncness = if start.init_is_async() { "async " } else { "" };
    let _ = write!(
        code,
        "}}\nimpl App {{\n\
         /// Creates all components, dependencies first.\n\
         #[allow(non_snake_case, clippy::redundant_clone)]\n\
         pub {asyncness}fn init() -> Result<Self, Error> {{\n"
    );
    for g in generated {
        code.push_str(&g.statement);
        code.push('\n');
    }
    // Datenquellen der Listen: Kopien, bevor die Formulare ihre Felder nehmen.
    for form in forms {
        for b in bindings.get(&form.name).into_iter().flatten() {
            let _ = writeln!(
                code,
                "let {}__{}__source = {}.clone();",
                form.name, b.widget, b.source_var
            );
        }
    }
    code.push_str("Ok(Self {\n");
    for form in forms {
        let _ = write!(
            code,
            "{}: {}::{} {{",
            form.name,
            form.name,
            pascal_case(form.name.as_str())
        );
        for g in form_components(form, generated) {
            let _ = write!(code, "{}: {},", g.path.name, g.var);
        }
        for b in bindings.get(&form.name).into_iter().flatten() {
            code.push_str(&b.init(form.name.as_str()));
        }
        for field in states.get(&form.name).into_iter().flatten() {
            let init = field.init.as_deref().unwrap_or("Default::default()");
            let _ = write!(code, "{}: {init},", field.name);
        }
        if is_live(form, generated) {
            code.push_str("freya_queue: Default::default(),");
        }
        code.push_str("},\n");
    }
    code.push_str("})\n}\n}\n");
    let live: Vec<&Form> = forms.iter().filter(|f| is_live(f, generated)).collect();
    if bindings.values().any(|b| !b.is_empty()) || !live.is_empty() {
        code.push_str(DATA_HELPERS);
    }
    if !live.is_empty() {
        code.push_str(QUEUE_HELPERS);
    }
    // Hilfen der Komponenten-Packs, einmal je Typ.
    let mut helped: Vec<&TypeRef> = Vec::new();
    for g in generated {
        if helped.contains(&&g.type_ref) {
            continue;
        }
        let Some(form) = forms.iter().find(|f| g.path.form.as_ref() == Some(&f.name)) else {
            continue;
        };
        let Some(template) = registry
            .codegen(&g.type_ref, &form.backend)
            .and_then(|spec| spec.helpers.as_ref())
        else {
            continue;
        };
        helped.push(&g.type_ref);
        let mut issues = Vec::new();
        if let Some(text) = render(
            env,
            "helpers",
            template,
            &minijinja::context! {},
            &mut issues,
        ) {
            check_rust(
                &mut issues,
                "helpers",
                &text,
                syn::parse_file(&text).map(drop),
            );
            if issues.is_empty() {
                code.push('\n');
                code.push_str(&text);
                code.push('\n');
            }
        }
        for issue in issues {
            out.error(Some(g.path.clone()), issue);
        }
    }
    // Datenmodule mit Ereignissen laufen auf Tokio weiter (bei `host`
    // bewahrt das Backend die ganze App auf und bedient sie selbst).
    let modules: String = live
        .iter()
        .filter(|f| f.is_data_module() && !host)
        .map(|f| format!("app.{}.freya_run();\n", f.name))
        .collect();

    // The event loop of the backend.
    let runtime_var = matches!(start, Start::Runtime(_)).then_some("runtime");
    let ui = backend.and_then(|b| b.run.as_ref()).and_then(|template| {
        let context = minijinja::context! {
            forms => forms.iter().map(|f| form_data(f, generated)).collect::<Vec<_>>(),
            app => "app",
            runtime => runtime_var,
            pictures => uses_pictures(forms),
            types => used_types(forms),
        };
        let mut issues = Vec::new();
        let rendered = render(env, "run", template, &context, &mut issues).filter(|text| {
            let parsed = syn::parse_str::<syn::Block>(&format!("{{ {text} }}")).map(drop);
            check_rust(&mut issues, "run", text, parsed);
            issues.is_empty()
        });
        for issue in issues {
            out.error(None, issue);
        }
        rendered
    });
    // Ohne Oberfläche, aber mit laufenden Datenmodulen: bis zum Abbruch warten.
    let ui = ui.unwrap_or_else(|| match start {
        _ if modules.is_empty() => "Ok(())".to_owned(),
        Start::Entry(_) => "std::future::pending::<()>().await;\nOk(())".to_owned(),
        Start::Runtime(_) => "runtime.block_on(std::future::pending::<()>());\nOk(())".to_owned(),
        _ => "Ok(())".to_owned(),
    });
    let setup = backend
        .and_then(|b| b.setup.as_ref())
        .and_then(|template| {
            let context = minijinja::context! {
                forms => forms.iter().map(|f| form_data(f, generated)).collect::<Vec<_>>(),
                app => "app",
                runtime => runtime_var,
            };
            let mut issues = Vec::new();
            let rendered = render(env, "setup", template, &context, &mut issues).filter(|text| {
                let parsed = syn::parse_str::<syn::Block>(&format!("{{ {text} }}")).map(drop);
                check_rust(&mut issues, "setup", text, parsed);
                issues.is_empty()
            });
            for issue in issues {
                out.error(None, issue);
            }
            rendered
        })
        .unwrap_or_default();
    // Gemeinsame Hilfen des Backends für app.gen.rs.
    let app_items = backend
        .and_then(|b| b.app.as_ref())
        .and_then(|template| {
            let context = minijinja::context! {
                forms => forms.iter().map(|f| form_data(f, generated)).collect::<Vec<_>>(),
                app => "app",
                runtime => runtime_var,
                // Bilder im Projekt: `crate::app::freya_image` steht bereit.
                pictures => uses_pictures(forms),
                // Alle Typen im Projekt (`backend:egui/Slider` …): Hilfen nur bei Bedarf.
                types => used_types(forms),
            };
            let mut issues = Vec::new();
            let rendered = render(env, "app", template, &context, &mut issues).filter(|text| {
                let parsed = syn::parse_str::<syn::File>(text).map(drop);
                check_rust(&mut issues, "app", text, parsed);
                issues.is_empty()
            });
            for issue in issues {
                out.error(None, issue);
            }
            rendered
        })
        .unwrap_or_default();

    if host {
        code.push_str(
            "/// Creates the application, calls `started` in `lib.rs` and hands it to the host.\n",
        );
    } else if !matches!(start, Start::Library { .. }) {
        code.push_str(
            "/// Creates the application, calls `started` in `main.rs` and runs the UI.\n",
        );
    }
    match start {
        Start::Library { .. } => {}
        Start::Runtime(runtime) if host => {
            // Die Laufzeit geht an das Backend, das sie für den Host aufbewahrt.
            let _ = write!(
                code,
                "pub fn run() -> Result<(), Error> {{\n{setup}\n\
                 let runtime = {runtime};\n\
                 let app = {{\n\
                 let _runtime_guard = runtime.enter();\n\
                 let mut app = runtime.block_on(App::init())?;\n\
                 crate::started(&mut app)?;\n\
                 app\n\
                 }};\n{ui}\n}}\n"
            );
        }
        Start::Entry(entry) => {
            let _ = write!(
                code,
                "{entry}\npub async fn run() -> Result<(), Error> {{\n{setup}\n\
                 let mut app = App::init().await?;\n\
                 crate::started(&mut app).await?;\n{modules}{ui}\n}}\n"
            );
        }
        Start::Runtime(runtime) => {
            let _ = write!(
                code,
                "pub fn run() -> Result<(), Error> {{\n{setup}\n\
                 let runtime = {runtime};\n\
                 // Tokio-Kontext für den UI-Thread (z. B. Daten im Hintergrund laden).\n\
                 let _runtime_guard = runtime.enter();\n\
                 let mut app = runtime.block_on(App::init())?;\n\
                 crate::started(&mut app)?;\n{modules}{ui}\n}}\n"
            );
        }
        Start::Plain => {
            let _ = write!(
                code,
                "pub fn run() -> Result<(), Error> {{\n{setup}\n\
                 let mut app = App::init()?;\n\
                 crate::started(&mut app)?;\n{ui}\n}}\n"
            );
        }
    }
    if !app_items.is_empty() {
        code.push('\n');
        code.push_str(&app_items);
        code.push('\n');
    }
    // Bilder (ImageBox, ImageScrollBox) bereitet der Kern einmal auf: alle
    // Backends zeigen Gamma und Transparenz gleich.
    if uses_pictures(forms) {
        code.push('\n');
        code.push_str(FREYA_IMAGE);
    }
    let content = generated_file("the project", &format_rust(APP, &code, out));
    file(APP, FileKind::Generated, content, source)
}

/// Every type used in the project (widgets and non-visual components), as
/// text (`core/Button`, `backend:freya-gui/Calendar`), sorted – for backend
/// templates that add helpers only when a type is used.
fn used_types(forms: &[Form]) -> Vec<String> {
    let mut types: Vec<String> = forms
        .iter()
        .flat_map(|f| f.elements().into_iter().map(|e| e.type_ref().to_string()))
        .collect();
    types.sort();
    types.dedup();
    types
}

/// Whether a form shows pictures (`core/ImageBox`, `core/ImageScrollBox`).
fn uses_pictures(forms: &[Form]) -> bool {
    let pictures = [
        crate::builtin::image_box(),
        crate::builtin::image_scroll_box(),
    ];
    forms.iter().any(|f| {
        f.widgets()
            .iter()
            .any(|w| pictures.contains(&w.component.type_ref))
    })
}

/// Picture preparation in `app.gen.rs` for `core/ImageBox` and
/// `core/ImageScrollBox` (crate `image`): decode, gamma, opacity, cache.
const FREYA_IMAGE: &str = r#"
/// A picture prepared for the user interface: RGBA, gamma and opacity applied.
#[allow(dead_code)]
#[derive(Clone)]
pub struct FreyaImage {
    pub width: u32,
    pub height: u32,
    pub rgba: std::sync::Arc<Vec<u8>>,
}

/// Loads the picture at `path` (relative to the working directory) with
/// gamma (1 = unchanged, above 1 lighter) and opacity (0…1); cached.
#[allow(dead_code)]
pub fn freya_image(path: &str, gamma: f64, opacity: f64) -> Result<FreyaImage, String> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    type Cache = Mutex<HashMap<(String, u64, u64), FreyaImage>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let gamma = gamma.clamp(0.1, 5.0);
    let opacity = opacity.clamp(0.0, 1.0);
    let key = (path.to_owned(), gamma.to_bits(), opacity.to_bits());
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(found) = cache.lock().map_err(|e| e.to_string())?.get(&key) {
        return Ok(found.clone());
    }
    let mut rgba = image::open(path)
        .map_err(|e| format!("{path}: {e}"))?
        .into_rgba8();
    if (gamma - 1.0).abs() > f64::EPSILON {
        let table: Vec<u8> = (0..=255u32)
            .map(|c| ((c as f64 / 255.0).powf(1.0 / gamma) * 255.0).round() as u8)
            .collect();
        for pixel in rgba.pixels_mut() {
            for channel in &mut pixel.0[..3] {
                *channel = table[*channel as usize];
            }
        }
    }
    if opacity < 1.0 {
        for pixel in rgba.pixels_mut() {
            pixel.0[3] = (pixel.0[3] as f64 * opacity).round() as u8;
        }
    }
    let (width, height) = rgba.dimensions();
    let picture = FreyaImage {
        width,
        height,
        rgba: std::sync::Arc::new(rgba.into_raw()),
    };
    cache
        .lock()
        .map_err(|e| e.to_string())?
        .insert(key, picture.clone());
    Ok(picture)
}

/// The prepared picture as PNG (for backends that show files or data URLs).
#[allow(dead_code)]
pub fn freya_image_png(picture: &FreyaImage) -> Result<Vec<u8>, String> {
    let buffer = image::RgbaImage::from_raw(picture.width, picture.height, (*picture.rgba).clone())
        .ok_or("picture buffer has the wrong size")?;
    let mut png = std::io::Cursor::new(Vec::new());
    buffer
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(png.into_inner())
}
"#;

/// The generated components of `form`, in the form's element order.
fn form_components<'g>(form: &Form, generated: &'g [Generated]) -> Vec<&'g Generated> {
    let by_path: HashMap<&ComponentPath, &Generated> =
        generated.iter().map(|g| (&g.path, g)).collect();
    form.elements()
        .into_iter()
        .filter_map(|e| {
            by_path
                .get(&ComponentPath::on(form.name.clone(), e.name().clone()))
                .copied()
        })
        .collect()
}

// --- Forms ----------------------------------------------------------------------------------

/// Template data of the form's elements; references become `self.<name>`.
struct FormData<'a> {
    form: &'a Form,
    registry: &'a TypeRegistry,
    fields: BTreeSet<&'a ComponentName>,
}

impl FormData<'_> {
    fn resolve(&self, target: &ComponentPath) -> Option<String> {
        let own = target.target_form(&self.form.name) == &self.form.name;
        (own && self.fields.contains(&target.name)).then(|| format!("self.{}", target.name))
    }

    fn node(
        &self,
        element: &dyn Element,
        children: Vec<JValue>,
        widget: Option<&Widget>,
    ) -> JValue {
        let type_ref = element.type_ref();
        let (props, rust, events) = match self.registry.get(type_ref) {
            Some(ty) => element_data(self.form, element, ty, self.registry, &|t| self.resolve(t)),
            None => (JValue::from(()), JValue::from(()), JValue::from(())),
        };
        minijinja::context! {
            name => element.name().as_str(),
            type => type_ref.to_string(),
            type_name => type_ref.name.as_str(),
            field => self.fields.contains(element.name()),
            props => props,
            rust => rust,
            events => events,
            children => children,
            placement => widget.map(|w| {
                let area = self
                    .form
                    .parent_of(&w.component.name)
                    .map(Widget::design_area)
                    .unwrap_or_default();
                placement_data(&w.visual.placement, area)
            }),
            layout => widget.and_then(|w| w.visual.layout.as_ref()).map(|l| {
                let (kind, columns, name) = match &l.kind {
                    LayoutKind::Vertical => ("vertical", None, None),
                    LayoutKind::Horizontal => ("horizontal", None, None),
                    LayoutKind::Grid { columns } => ("grid", Some(*columns), None),
                    LayoutKind::Stack => ("stack", None, None),
                    LayoutKind::Absolute => ("absolute", None, None),
                    LayoutKind::Custom(n) => ("custom", None, Some(n.as_str())),
                };
                minijinja::context! {
                    kind => kind,
                    columns => columns,
                    name => name,
                    spacing => l.spacing,
                    padding => minijinja::context! {
                        left => l.padding.left,
                        top => l.padding.top,
                        right => l.padding.right,
                        bottom => l.padding.bottom,
                    },
                }
            }),
        }
    }

    fn widget(&self, widget: &Widget) -> JValue {
        let children = widget.children.iter().map(|c| self.widget(c)).collect();
        self.node(widget, children, Some(widget))
    }
}

fn placement_data(placement: &Placement, area: crate::Size) -> JValue {
    match placement {
        Placement::Layout {
            stretch,
            align,
            cell,
            size,
            percent,
        } => minijinja::context! {
            kind => "layout",
            stretch => stretch,
            align => format!("{align:?}"),
            // Wunschgröße, 0 = natürliche Größe.
            width => size.width,
            height => size.height,
            // Anteil am Inhalt des Elternelements in Prozent, 0 = nicht gesetzt.
            width_percent => percent.width,
            height_percent => percent.height,
            cell => cell.as_ref().map(|c| minijinja::context! {
                row => c.row,
                column => c.column,
                row_span => c.row_span,
                column_span => c.column_span,
            }),
        },
        Placement::Absolute {
            position,
            size,
            anchors,
        } => minijinja::context! {
            kind => "absolute",
            x => position.x,
            y => position.y,
            width => size.width,
            height => size.height,
            anchors => minijinja::context! {
                left => anchors.left,
                top => anchors.top,
                right => anchors.right,
                bottom => anchors.bottom,
                // Abweichend von links oben verankert.
                any => !anchors.is_top_left(),
            },
            // Abstand zum rechten und unteren Rand der Entwurfsfläche.
            margin_right => area.width - position.x - size.width,
            margin_bottom => area.height - position.y - size.height,
            area => minijinja::context! { width => area.width, height => area.height },
        },
    }
}

/// Context of the backend's `form` template (and of per-form files).
fn form_context(
    form: &Form,
    generated: &[Generated],
    bindings: &[Binding],
    registry: &TypeRegistry,
) -> JValue {
    let components = form_components(form, generated);
    let data = FormData {
        form,
        registry,
        fields: components.iter().map(|g| &g.path.name).collect(),
    };
    minijinja::context! {
        form => form_data(form, generated),
        // Etwas kommt aus dem Hintergrund: geladene Daten oder Ereignisse.
        data_bound => !bindings.is_empty() || is_live(form, generated),
        root => form.root.as_ref().map(|r| data.widget(r)),
        widgets => form.widgets().into_iter().map(|w| data.widget(w)).collect::<Vec<_>>(),
        components => form.components.iter().map(|c| data.node(c, Vec::new(), None)).collect::<Vec<_>>(),
    }
}

/// The backend's additional files ([`BackendDescriptor::files`]).
#[allow(clippy::too_many_arguments)]
fn backend_files(
    project: &ProjectFile,
    forms: &[Form],
    generated: &[Generated],
    bindings: &HashMap<ComponentName, Vec<Binding>>,
    start: &Start,
    registry: &TypeRegistry,
    backend: &BackendDescriptor,
    env: &Environment<'static>,
    source: &dyn FileSource,
    files: &mut Vec<GeneratedFile>,
    out: &mut Diagnostics,
) {
    let project_data = minijinja::context! {
        name => project.name.as_str(),
        // Name der Bibliothek, wie Cargo ihn bildet.
        lib => project.name.replace('-', "_"),
        platforms => project.platforms.iter().map(ToString::to_string).collect::<Vec<_>>(),
    };
    let runtime_var = matches!(start, Start::Runtime(_)).then_some("runtime");
    let project_context = minijinja::context! {
        project => project_data.clone(),
        forms => forms.iter().map(|f| form_data(f, generated)).collect::<Vec<_>>(),
        app => "app",
        runtime => runtime_var,
    };
    for spec in &backend.files {
        let contexts: Vec<(Option<ComponentPath>, JValue)> = match spec.each {
            FileScope::Project => vec![(None, project_context.clone())],
            FileScope::Form => forms
                .iter()
                .filter(|f| !f.is_data_module())
                .map(|form| {
                    let form_bindings = bindings.get(&form.name).map_or(&[][..], Vec::as_slice);
                    let context = form_context(form, generated, form_bindings, registry);
                    (
                        Some(ComponentPath::on(form.name.clone(), form_anchor(form))),
                        minijinja::context! { project => project_data.clone(), ..context },
                    )
                })
                .collect(),
        };
        for (anchor, context) in contexts {
            let mut issues = Vec::new();
            let path = render(env, "files.path", &spec.path, &context, &mut issues);
            let path = path.map(|p| p.trim().to_owned()).filter(|p| {
                let inside = !p.is_empty()
                    && Path::new(p)
                        .components()
                        .all(|c| matches!(c, std::path::Component::Normal(_)));
                if !inside {
                    issues.push(CodegenIssue::Template {
                        part: "files.path".to_owned(),
                        message: format!("{p:?} is not a relative path inside the project"),
                        line: None,
                    });
                }
                inside
            });
            let body = if spec.raw {
                Some(spec.template.clone())
            } else {
                render(env, "files.template", &spec.template, &context, &mut issues)
                    .map(|text| format!("{text}\n"))
            };
            for issue in issues {
                out.error(anchor.clone(), issue);
            }
            let (Some(path), Some(body)) = (path, body) else {
                continue;
            };
            if files.iter().any(|f| f.path == path) {
                out.error(
                    anchor.clone(),
                    CodegenIssue::Template {
                        part: "files.path".to_owned(),
                        message: format!("{path} is generated twice"),
                        line: None,
                    },
                );
                continue;
            }
            files.push(match spec.owner {
                FileOwner::Generator => {
                    let what = anchor
                        .as_ref()
                        .and_then(|a| a.form.as_ref())
                        .map_or_else(|| "the project".to_owned(), |f| format!("form `{f}`"));
                    file(
                        &path,
                        FileKind::Generated,
                        generated_file(&what, &body),
                        source,
                    )
                }
                FileOwner::User => {
                    let content = source.read(&path).unwrap_or(body);
                    file(&path, FileKind::User, content, source)
                }
            });
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn form_gen_file(
    form: &Form,
    generated: &[Generated],
    bindings: &[Binding],
    state: &[crate::state::StateField],
    runtime: &FormRuntime,
    registry: &TypeRegistry,
    backend: Option<&BackendDescriptor>,
    env: &Environment<'static>,
    source: &dyn FileSource,
    out: &mut Diagnostics,
) -> GeneratedFile {
    let path = format!("src/{}.gen.rs", form.name);
    let what = if form.is_data_module() {
        "Data module"
    } else {
        "Form"
    };
    let components = form_components(form, generated);
    let mut code = format!(
        "/// {what} `{}`: its components, created by `App::init`.\n\
         #[allow(dead_code)]\n\
         pub struct {} {{\n",
        form.name,
        pascal_case(form.name.as_str())
    );
    for g in &components {
        let _ = writeln!(
            code,
            "/// `{}`\npub {}: {},",
            g.type_ref, g.path.name, g.field_type
        );
    }
    for b in bindings {
        code.push_str(&b.fields());
    }
    for field in state {
        let _ = writeln!(
            code,
            "/// Own field (designer: `state`).\npub {}: {},",
            field.name, field.rust_type
        );
    }
    let live = is_live(form, generated);
    if live {
        code.push_str(
            "/// Handler calls from background tasks (WebSocket, HTTP …), made by\n\
             /// `freya_take_data` on the form's thread.\n\
             pub freya_queue: crate::app::Queue<Self>,\n",
        );
    }
    code.push_str("}\n");
    if form.root.is_some() || live {
        code.push_str(&data_impl(form, bindings, runtime, live));
    }

    // The backend's code for the form (drawing, building, event dispatch).
    if let Some(template) = backend.and_then(|b| b.form.as_ref()) {
        let context = form_context(form, generated, bindings, registry);
        let mut issues = Vec::new();
        if let Some(text) = render(env, "form", template, &context, &mut issues) {
            let parsed = syn::parse_file(&text).map(drop);
            check_rust(&mut issues, "form", &text, parsed);
            if issues.is_empty() {
                code.push_str(&text);
                code.push('\n');
            }
        }
        let anchor = ComponentPath::on(form.name.clone(), form_anchor(form));
        for issue in issues {
            out.error(Some(anchor.clone()), issue);
        }
    }

    let content = generated_file(
        &format!("form `{}`", form.name),
        &format_rust(&path, &code, out),
    );
    file(&path, FileKind::Generated, content, source)
}

/// An event handler the user implements.
struct Handler {
    name: HandlerName,
    params: Vec<EventParam>,
    returns: Option<String>,
    /// `element.event` it serves.
    uses: Vec<String>,
}

/// The form's handlers in order of first use; handlers serving events with
/// different signatures are reported and left out.
fn handlers(
    forms: &[Form],
    form: &Form,
    registry: &TypeRegistry,
    out: &mut Diagnostics,
) -> Vec<Handler> {
    let mut list: Vec<Handler> = Vec::new();
    let mut conflicts: BTreeMap<HandlerName, Vec<String>> = BTreeMap::new();
    for element in form.elements() {
        let Some(ty) = registry.get(element.type_ref()) else {
            continue;
        };
        let component = element.component();
        for (event, handler) in &component.events {
            let Some(def) = ty.event(event) else { continue };
            let params = event_params(forms, form, element, registry, def);
            let use_ = format!("{}.{event}", component.name);
            match list.iter_mut().find(|h| &h.name == handler) {
                Some(h) if h.params == params && h.returns == def.returns => h.uses.push(use_),
                Some(h) => {
                    conflicts
                        .entry(handler.clone())
                        .or_insert_with(|| h.uses.clone())
                        .push(use_);
                }
                None => list.push(Handler {
                    name: handler.clone(),
                    params,
                    returns: def.returns.clone(),
                    uses: vec![use_],
                }),
            }
        }
    }
    for (handler, events) in conflicts {
        out.error(
            Some(ComponentPath::on(form.name.clone(), form_anchor(form))),
            CodegenIssue::HandlerConflict {
                handler: handler.clone(),
                events,
            },
        );
        list.retain(|h| h.name != handler);
    }
    list
}

/// The element form-wide findings are attached to: the root widget, or the
/// first component of a data module, or – for an empty data module – a
/// path made of the form name.
fn form_anchor(form: &Form) -> ComponentName {
    form.elements()
        .first()
        .map_or_else(|| form.name.clone(), |e| e.name().clone())
}

fn stub(handler: &Handler) -> String {
    let params: String = handler
        .params
        .iter()
        .map(|p| format!(", {}: {}", p.name, p.rust_type))
        .collect();
    let returns = handler
        .returns
        .as_ref()
        .map(|r| format!(" -> {r}"))
        .unwrap_or_default();
    let body = if handler.returns.is_some() {
        format!("todo!(\"{}\")", handler.name)
    } else {
        String::new()
    };
    let uses: Vec<String> = handler.uses.iter().map(|u| format!("`{u}`")).collect();
    format!(
        "/// Handles {}.\npub fn {}(&mut self{params}){returns} {{\n{body}\n}}\n",
        uses.join(", "),
        handler.name
    )
}

fn form_user_file(
    forms: &[Form],
    form: &Form,
    registry: &TypeRegistry,
    source: &dyn FileSource,
    out: &mut Diagnostics,
) -> GeneratedFile {
    let path = format!("src/{}.rs", form.name);
    let struct_name = pascal_case(form.name.as_str());
    let mut stubs = Vec::new();
    for handler in handlers(forms, form, registry, out) {
        let text = stub(&handler);
        match syn::parse_str::<syn::ImplItem>(&text) {
            Ok(_) => stubs.push((handler.name, text)),
            Err(e) => out.error(
                Some(ComponentPath::on(form.name.clone(), form_anchor(form))),
                CodegenIssue::InvalidRust {
                    part: format!("handler stub {}", handler.name),
                    message: e.to_string(),
                },
            ),
        }
    }

    let Some(existing) = source.read(&path) else {
        let what = if form.is_data_module() {
            "Data module"
        } else {
            "Form"
        };
        let mut code = format!(
            "//! {what} `{0}` – your code. Freya TDE generates `{0}.gen.rs` and only\n\
             //! appends missing event handler stubs to this file.\n\n\
             include!(\"{0}.gen.rs\");\n",
            form.name
        );
        if !stubs.is_empty() {
            let _ = writeln!(code, "impl {struct_name} {{");
            for (_, text) in &stubs {
                code.push_str(text);
            }
            code.push_str("}\n");
        }
        let content = format_rust(&path, &code, out);
        return file(&path, FileKind::User, content, source);
    };

    let parsed = match syn::parse_file(&existing) {
        Ok(parsed) => parsed,
        Err(e) => {
            if !stubs.is_empty() {
                out.warning(
                    None,
                    CodegenIssue::UserFileUnparsable {
                        file: path.clone(),
                        message: e.to_string(),
                    },
                );
            }
            return file(&path, FileKind::User, existing, source);
        }
    };
    let present: BTreeSet<String> = parsed
        .items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Impl(i) if i.trait_.is_none() => Some(i),
            _ => None,
        })
        .filter(|i| {
            matches!(i.self_ty.as_ref(), syn::Type::Path(p)
                if p.path.segments.last().is_some_and(|s| s.ident == struct_name))
        })
        .flat_map(|i| &i.items)
        .filter_map(|item| match item {
            syn::ImplItem::Fn(f) => Some(f.sig.ident.to_string()),
            _ => None,
        })
        .collect();
    let missing: Vec<&String> = stubs
        .iter()
        .filter(|(name, _)| !present.contains(name.as_str()))
        .map(|(_, text)| text)
        .collect();
    if missing.is_empty() {
        return file(&path, FileKind::User, existing, source);
    }
    let mut block = format!("/// Event handlers added by Freya TDE.\nimpl {struct_name} {{\n");
    for text in missing {
        block.push_str(text);
    }
    block.push_str("}\n");
    let block = format_rust(&path, &block, out);
    let mut content = existing;
    if !content.ends_with('\n') {
        content.push('\n');
    }
    content.push('\n');
    content.push_str(&block);
    file(&path, FileKind::User, content, source)
}

// --- Data binding ----------------------------------------------------------------------

/// A list or table widget filled from a data source (see [`crate::data`]).
struct Binding {
    widget: String,
    /// `App::init` variable of the data source.
    source_var: String,
    source_type: String,
    /// Rendered templates: rows of the source, showing them, the selection.
    fetch: String,
    start: bool,
    show: String,
    selected: String,
    kind: BindingKind,
}

enum BindingKind {
    List {
        value_field: String,
        value_type: String,
        get_text: String,
        get_value: Option<String>,
    },
    Table {
        row_struct: String,
        /// All columns of the source: name, type, rendered `get`.
        columns: Vec<(String, String, String)>,
        /// Shown columns: name, Rust type, header.
        shown: Vec<(String, String, String)>,
        /// Key column: name, type.
        value: Option<(String, String)>,
        /// Set when the table is `editable`.
        edit: Option<Edit>,
    },
}

/// Editing of a table widget (`editable`).
struct Edit {
    /// Rendered `update`/`insert`/`delete` of the source (automatic
    /// writing), all or none.
    writes: Option<Writes>,
    /// Handlers of `on_post`, `on_delete`, `on_post_error`.
    on_post: Option<String>,
    on_delete: Option<String>,
    on_post_error: Option<String>,
    /// Rendered `select` of the widget (statements, `index: Option<usize>`).
    select: Option<String>,
}

struct Writes {
    update: String,
    insert: String,
    delete: String,
    /// The database assigns the key: load again after an insert.
    generated_key: bool,
}

impl Binding {
    /// What a loaded entry is: `(value, text)` or `(row, cells)`.
    fn entry_type(&self) -> String {
        match &self.kind {
            BindingKind::List { value_type, .. } => format!("({value_type}, String)"),
            BindingKind::Table { row_struct, .. } => format!("({row_struct}, Vec<String>)"),
        }
    }

    /// Field keeping the loaded values or rows, and its type.
    fn store(&self) -> (String, String) {
        match &self.kind {
            BindingKind::List { value_type, .. } => (
                format!("{}_values", self.widget),
                format!("Vec<{value_type}>"),
            ),
            BindingKind::Table { row_struct, .. } => (
                format!("{}_rows", self.widget),
                format!("Vec<{row_struct}>"),
            ),
        }
    }

    /// Fields in the form struct.
    fn fields(&self) -> String {
        let (store, store_type) = self.store();
        format!(
            "/// Data source of `{w}`.\npub {w}_source: {source},\n\
             /// What `{w}` shows (values of its entries / its rows).\npub {store}: {store_type},\n\
             /// Rows of `{w}` loaded in the background.\npub {w}_pending: crate::app::Pending<Vec<{entry}>>,\n{edit}",
            w = self.widget,
            source = self.source_type,
            entry = self.entry_type(),
            edit = match (&self.kind, self.edit()) {
                (BindingKind::Table { row_struct, .. }, Some(_)) => format!(
                    "/// The row of `{w}` being edited (saved by `{w}_post`).\n\
                     pub {w}_editing: Option<crate::app::Editing<{row_struct}>>,\n\
                     /// Results of writing rows of `{w}` in the background.\n\
                     pub {w}_saved: crate::app::Results<crate::app::Saved>,\n",
                    w = self.widget
                ),
                _ => String::new(),
            },
        )
    }

    /// The fields in `App::init`'s struct literal.
    fn init(&self, form: &str) -> String {
        let mut init = format!(
            "{w}_source: {form}__{w}__source, {store}: Vec::new(), {w}_pending: crate::app::Pending::default(),",
            w = self.widget,
            store = self.store().0,
        );
        if self.edit().is_some() {
            let _ = write!(
                init,
                " {w}_editing: None, {w}_saved: crate::app::Results::default(),",
                w = self.widget
            );
        }
        init
    }

    fn edit(&self) -> Option<&Edit> {
        match &self.kind {
            BindingKind::Table { edit, .. } => edit.as_ref(),
            BindingKind::List { .. } => None,
        }
    }
}

/// The own fields (`state`) of each form, checked: syntax, names not used
/// by components or other generated fields.
fn form_states(
    forms: &[Form],
    generated: &[Generated],
    out: &mut Diagnostics,
) -> HashMap<ComponentName, Vec<crate::state::StateField>> {
    let mut all = HashMap::new();
    for form in forms {
        let Some(root) = &form.root else { continue };
        let items = crate::data::texts(root.component.get(crate::state::STATE).map(|v| &v.value));
        let fields = match crate::state::parse_state(&items) {
            Ok(fields) => fields,
            Err(message) => {
                out.error(
                    Some(ComponentPath::on(
                        form.name.clone(),
                        root.component.name.clone(),
                    )),
                    CodegenIssue::FormState {
                        form: form.name.clone(),
                        message,
                    },
                );
                continue;
            }
        };
        let taken: Vec<String> = form_components(form, generated)
            .iter()
            .map(|g| g.path.name.to_string())
            .collect();
        if let Some(clash) = fields.iter().find(|f| {
            taken.contains(&f.name) || f.name.starts_with("freya_") || f.name.contains("__")
        }) {
            out.error(
                Some(ComponentPath::on(
                    form.name.clone(),
                    root.component.name.clone(),
                )),
                CodegenIssue::FormState {
                    form: form.name.clone(),
                    message: format!("`{}` is already a component or a reserved name", clash.name),
                },
            );
            continue;
        }
        all.insert(form.name.clone(), fields);
    }
    all
}

/// The data bindings of all forms, checked.
fn data_bindings(
    forms: &[Form],
    generated: &[Generated],
    registry: &TypeRegistry,
    backend: &str,
    env: &Environment<'static>,
    start: &Start,
    out: &mut Diagnostics,
) -> HashMap<ComponentName, Vec<Binding>> {
    use crate::data::{
        COLUMNS, DATA_SOURCE, DISPLAY_FIELD, EDITABLE, FIELDS, GENERATED_KEY, HEADERS, KEY, LOAD,
        ON_DELETE, ON_POST, ON_POST_ERROR, TABLE, VALUE_FIELD, parse_columns, texts,
    };
    let mut all: HashMap<ComponentName, Vec<Binding>> = HashMap::new();
    for form in forms {
        let root_name = form.root.as_ref().map(|r| r.component.name.to_string());
        for widget in form.widgets() {
            let Some(ty) = registry.get(&widget.component.type_ref) else {
                continue;
            };
            // Kern-Listen (z. B. ImageScrollBox) füllt jedes Backend anders.
            let list = registry.list(&widget.component.type_ref, backend);
            let (show_template, selected_template, is_table) = match (list, &ty.table) {
                (Some(l), _) => (&l.set_items, &l.selected, false),
                (None, Some(t)) => (&t.set_rows, &t.selected, true),
                (None, None) => continue,
            };
            let Some(Value::Reference(Some(target))) =
                widget.component.get(DATA_SOURCE).map(|v| &v.value)
            else {
                continue;
            };
            let path = ComponentPath::on(form.name.clone(), widget.component.name.clone());
            let mut fail = |message: String| {
                out.error(Some(path.clone()), CodegenIssue::DataBinding { message });
            };
            let target_form = target.target_form(&form.name).clone();
            let target_path = ComponentPath::on(target_form.clone(), target.name.clone());
            let Some(source) = generated.iter().find(|g| g.path == target_path) else {
                fail(format!(
                    "data source {target} does not exist or has no code"
                ));
                continue;
            };
            let Some((source_form, element)) = forms
                .iter()
                .find(|f| f.name == target_form)
                .and_then(|f| Some((f, f.element(&target.name)?)))
            else {
                fail(format!("data source {target} does not exist"));
                continue;
            };
            let Some((source_ty, data)) = registry
                .get(element.type_ref())
                .and_then(|t| Some((t, t.data.as_ref()?)))
            else {
                fail(format!("{target} is not a data source"));
                continue;
            };
            let mut column_texts = texts(element.component().get(COLUMNS).map(|v| &v.value));
            if column_texts.is_empty() {
                column_texts = implied_columns(source_form, element, registry).unwrap_or_default();
            }
            let columns = match parse_columns(&column_texts) {
                Ok(columns) if !columns.is_empty() => columns,
                Ok(_) => {
                    fail(format!("{target} has no `columns`"));
                    continue;
                }
                Err(message) => {
                    fail(format!("{target}: {message}"));
                    continue;
                }
            };
            let text_prop = |name: &str| match widget.component.get(name).map(|v| &v.value) {
                Some(Value::Text(t)) => t.trim().to_owned(),
                _ => String::new(),
            };
            let names = || {
                columns
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let column = |name: &str| columns.iter().find(|c| c.name == name);
            let value_name = text_prop(VALUE_FIELD);
            let value = if value_name.is_empty() {
                None
            } else {
                match column(&value_name) {
                    Some(c) => Some(c),
                    None => {
                        fail(format!(
                            "{target} has no column `{value_name}` (columns: {})",
                            names()
                        ));
                        continue;
                    }
                }
            };
            if matches!(start, Start::Plain) {
                fail("loading needs an async runtime in the project (e.g. tokio Runtime)".into());
                continue;
            }
            let load_start = !matches!(
                widget.component.get(LOAD).map(|v| &v.value),
                Some(Value::Enum(l)) if l == "Manual"
            );

            // Templates of the source and of the widget.
            let (props, rust, _) =
                element_data(source_form, element, source_ty, registry, &|_| None);
            let mut issues = Vec::new();
            let fetch = render(
                env,
                "fetch",
                &data.fetch,
                &minijinja::context! { source => "source", props => props.clone(), rust => rust.clone() },
                &mut issues,
            );
            let get = |column: &crate::data::Column, var: &str, issues: &mut Vec<CodegenIssue>| {
                render(
                    env,
                    "get",
                    &data.get,
                    &minijinja::context! { row => var, column => column.name.as_str(), type => column.rust_type.as_str() },
                    issues,
                )
            };
            let widget_context = minijinja::context! {
                name => widget.component.name.as_str(),
                root => root_name.clone(),
                form => form_data(form, generated),
                props => props_data(form, widget, ty, registry),
            };
            let show_part = if is_table { "set-rows" } else { "set-items" };
            let show = render(env, show_part, show_template, &widget_context, &mut issues);
            let selected = render(
                env,
                "selected",
                selected_template,
                &widget_context,
                &mut issues,
            );

            let kind = if is_table {
                // Gezeigte Spalten (leer = alle) und ihre Titel.
                let fields = texts(widget.component.get(FIELDS).map(|v| &v.value));
                let headers = texts(widget.component.get(HEADERS).map(|v| &v.value));
                let shown_names: Vec<String> = if fields.is_empty() {
                    columns.iter().map(|c| c.name.clone()).collect()
                } else {
                    fields.iter().map(|f| f.trim().to_owned()).collect()
                };
                if let Some(missing) = shown_names.iter().find(|n| column(n).is_none()) {
                    fail(format!(
                        "{target} has no column `{missing}` (columns: {})",
                        names()
                    ));
                    continue;
                }
                let mut table_columns = Vec::new();
                for c in &columns {
                    if let Some(g) = get(c, "row", &mut issues) {
                        table_columns.push((c.name.clone(), c.rust_type.clone(), g));
                    }
                }
                let shown = shown_names
                    .iter()
                    .enumerate()
                    .map(|(i, n)| {
                        let rust_type = column(n).map(|c| c.rust_type.clone()).unwrap_or_default();
                        let header = headers
                            .get(i)
                            .filter(|h| !h.trim().is_empty())
                            .cloned()
                            .unwrap_or_else(|| n.clone());
                        (n.clone(), rust_type, header)
                    })
                    .collect();
                let editable = matches!(
                    widget.component.get(EDITABLE).map(|v| &v.value),
                    Some(Value::Bool(true))
                );
                let edit = if editable {
                    let handler = |event: &str| {
                        widget
                            .component
                            .events
                            .get(event)
                            .map(|h| h.as_str().to_owned())
                    };
                    let source_text =
                        |name: &str| match element.component().get(name).map(|v| &v.value) {
                            Some(Value::Text(t)) => t.trim().to_owned(),
                            _ => String::new(),
                        };
                    let table = source_text(TABLE);
                    let keys: Vec<String> = source_text(KEY)
                        .split(',')
                        .map(|k| k.trim().to_owned())
                        .filter(|k| !k.is_empty())
                        .collect();
                    let generated_key = !matches!(
                        element.component().get(GENERATED_KEY).map(|v| &v.value),
                        Some(Value::Bool(false))
                    );
                    let writes = match (&data.update, &data.insert, &data.delete) {
                        (Some(update), Some(insert), Some(delete)) if !table.is_empty() => {
                            if keys.is_empty() {
                                fail(format!("{target} has `table` but no `key`"));
                                continue;
                            }
                            if let Some(missing) = keys.iter().find(|k| column(k).is_none()) {
                                fail(format!(
                                    "{target}: key column `{missing}` is not in `columns` ({})",
                                    names()
                                ));
                                continue;
                            }
                            let others: Vec<String> = columns
                                .iter()
                                .map(|c| c.name.clone())
                                .filter(|n| !keys.contains(n))
                                .collect();
                            let all: Vec<String> = columns.iter().map(|c| c.name.clone()).collect();
                            let refs = refs_data(forms, source_form, element, registry);
                            let write =
                                |part: &str,
                                 text: &str,
                                 cols: &[String],
                                 issues: &mut Vec<CodegenIssue>| {
                                    render(
                                        env,
                                        part,
                                        text,
                                        &minijinja::context! {
                                            source => "source", props => props.clone(), rust => rust.clone(),
                                            refs => refs.clone(), table => table.as_str(), keys => keys.clone(),
                                            columns => cols.to_vec(), row => "row", old => "old",
                                        },
                                        issues,
                                    )
                                };
                            let inserted = if generated_key { &others } else { &all };
                            match (
                                write("update", update, &others, &mut issues),
                                write("insert", insert, inserted, &mut issues),
                                write("delete", delete, &others, &mut issues),
                            ) {
                                (Some(update), Some(insert), Some(delete)) => Some(Writes {
                                    update,
                                    insert,
                                    delete,
                                    generated_key,
                                }),
                                _ => None,
                            }
                        }
                        _ => None,
                    };
                    let select = ty
                        .table
                        .as_ref()
                        .and_then(|t| t.select.as_ref())
                        .and_then(|t| render(env, "select", t, &widget_context, &mut issues));
                    Some(Edit {
                        writes,
                        on_post: handler(ON_POST),
                        on_delete: handler(ON_DELETE),
                        on_post_error: handler(ON_POST_ERROR),
                        select,
                    })
                } else {
                    None
                };
                BindingKind::Table {
                    row_struct: format!("{}Row", pascal_case(widget.component.name.as_str())),
                    columns: table_columns,
                    shown,
                    value: value.map(|c| (c.name.clone(), c.rust_type.clone())),
                    edit,
                }
            } else {
                let display = text_prop(DISPLAY_FIELD);
                let Some(display) = column(&display) else {
                    fail(if display.is_empty() {
                        format!(
                            "`display_field` is empty (columns of {target}: {})",
                            names()
                        )
                    } else {
                        format!("{target} has no column `{display}` (columns: {})", names())
                    });
                    continue;
                };
                let Some(text) = get(display, "row", &mut issues) else {
                    for issue in issues {
                        out.error(Some(path.clone()), issue);
                    }
                    continue;
                };
                let text = cell_text(&format!("({text})"), &display.rust_type);
                BindingKind::List {
                    value_field: value.map_or_else(|| display.name.clone(), |c| c.name.clone()),
                    value_type: value.map_or_else(|| "String".to_owned(), |c| c.rust_type.clone()),
                    get_text: text,
                    get_value: value.and_then(|c| get(c, "row", &mut issues)),
                }
            };
            if !issues.is_empty() {
                for issue in issues {
                    out.error(Some(path.clone()), issue);
                }
                continue;
            }
            let (Some(fetch), Some(show), Some(selected)) = (fetch, show, selected) else {
                continue;
            };
            all.entry(form.name.clone()).or_default().push(Binding {
                widget: widget.component.name.to_string(),
                source_var: source.var.clone(),
                source_type: source.field_type.clone(),
                fetch,
                start: load_start,
                show,
                selected,
                kind,
            });
        }
    }
    all
}

/// Statements reporting `var` (a `String`) through `on_post_error`, or to stderr.
fn report_error(w: &str, edit: &Edit, var: &str) -> String {
    match &edit.on_post_error {
        Some(handler) => format!("self.{handler}(&{var});\n"),
        None => format!("eprintln!(\"{w}: {{{var}}}\");\n"),
    }
}

/// An expression `Result<T, String>` reading `text` as a cell of type
/// `rust_type`; `None` for types that cannot be edited (lists).
fn parse_cell(name: &str, rust_type: &str) -> Option<String> {
    let rust_type = rust_type.trim();
    let (optional, inner) = match rust_type
        .strip_prefix("Option<")
        .and_then(|t| t.strip_suffix('>'))
    {
        Some(inner) => (true, inner.trim()),
        None => (false, rust_type),
    };
    if inner.starts_with("Vec<") || inner.starts_with("std::vec::Vec<") {
        return None;
    }
    let string = matches!(inner, "String" | "std::string::String");
    let parse = if string {
        "Ok::<String, String>(text.to_owned())".to_owned()
    } else {
        format!("text.trim().parse::<{inner}>().map_err(|e| format!(\"{name}: {{e}}\"))")
    };
    Some(if optional {
        let empty = if string {
            "text.is_empty()"
        } else {
            "text.trim().is_empty()"
        };
        format!("if {empty} {{ Ok(None) }} else {{ {parse}.map(Some) }}")
    } else {
        parse
    })
}

/// Editing methods of an editable table widget (see [`crate::data`]).
fn edit_methods(
    b: &Binding,
    row_struct: &str,
    shown: &[(String, String, String)],
    edit: &Edit,
) -> String {
    let w = &b.widget;
    let selected = &b.selected;
    let show = &b.show;
    let headers: Vec<String> = shown
        .iter()
        .map(|(_, _, h)| format!("{h:?}.to_string()"))
        .collect();
    let cells: Vec<String> = shown
        .iter()
        .map(|(name, rust_type, _)| cell_text(&format!("row.{name}"), rust_type))
        .collect();
    let select = |index: &str| match &edit.select {
        Some(select) => format!("{{\nlet index: Option<usize> = {index};\n{select}\n}}\n"),
        None => format!("let _ = {index};\n"),
    };
    let mut arms = String::new();
    for (i, (name, rust_type, _)) in shown.iter().enumerate() {
        match parse_cell(name, rust_type) {
            Some(parse) => {
                let _ = writeln!(
                    arms,
                    "{i} => ({parse}).map(|value| editing.row.{name} = value),"
                );
            }
            None => {
                let _ = writeln!(
                    arms,
                    "{i} => Err(\"{name}: this column cannot be edited\".to_string()),"
                );
            }
        }
    }
    let report = report_error(w, edit, "error");
    let on_post = match &edit.on_post {
        Some(h) => format!(
            "if !self.{h}(&editing.row, editing.inserted) {{\n\
             self.{w}_editing = Some(editing);\n\
             return false;\n\
             }}\n"
        ),
        None => String::new(),
    };
    let on_delete = match &edit.on_delete {
        Some(h) => format!("if !self.{h}(&old) {{\nreturn false;\n}}\n"),
        None => String::new(),
    };
    let (post_write, delete_write) = match &edit.writes {
        Some(writes) => (
            format!(
                "let source = self.{w}_source.clone();\n\
                 let row = editing.row.clone();\n\
                 if editing.inserted {{\n\
                 crate::app::freya_spawn_write(self.{w}_saved.clone(), async move {{\n\
                 let _ = {insert};\n\
                 Ok(crate::app::Saved::Inserted)\n\
                 }});\n\
                 }} else if let Some(old) = self.{w}_rows.get(editing.index).cloned() {{\n\
                 crate::app::freya_spawn_write(self.{w}_saved.clone(), async move {{\n\
                 let _ = {update};\n\
                 Ok(crate::app::Saved::Updated)\n\
                 }});\n\
                 }}\n",
                insert = writes.insert,
                update = writes.update,
            ),
            format!(
                "let source = self.{w}_source.clone();\n\
                 let old = old.clone();\n\
                 crate::app::freya_spawn_write(self.{w}_saved.clone(), async move {{\n\
                 let _ = {delete};\n\
                 Ok(crate::app::Saved::Deleted)\n\
                 }});\n",
                delete = writes.delete,
            ),
        ),
        None => (String::new(), String::new()),
    };
    format!(
        "/// Shows the rows of `{w}` again (with the row being edited), keeping the selection.\n\
         fn {w}_show(&mut self) {{\n\
         let index: Option<usize> = {selected};\n\
         let headers: Vec<String> = vec![{headers}];\n\
         let rows: Vec<Vec<String>> = self.{w}_rows.iter().enumerate().map(|(i, row)| {{\n\
         let row = match &self.{w}_editing {{\n\
         Some(e) if e.index == i => &e.row,\n\
         _ => row,\n\
         }};\n\
         vec![{cells}]\n\
         }}).collect();\n\
         {show}\n\
         {select_kept}\
         }}\n\
         /// Changes cell `column` (counted among the shown columns) of row `index` of `{w}` to\n\
         /// `text` and edits the row (a different row being edited is saved first).\n\
         /// `Err`: the text does not fit the column (the row cannot be saved like this).\n\
         pub fn {w}_edit_cell(&mut self, index: usize, column: usize, text: &str) -> Result<(), String> {{\n\
         if self.{w}_editing.as_ref().is_some_and(|e| e.index != index) && !self.{w}_post() {{\n\
         return Err(\"the row being edited could not be saved\".to_string());\n\
         }}\n\
         if self.{w}_editing.is_none() {{\n\
         let Some(row) = self.{w}_rows.get(index).cloned() else {{\n\
         return Err(format!(\"no row {{index}}\"));\n\
         }};\n\
         self.{w}_editing = Some(crate::app::Editing {{ index, row, inserted: false, invalid: None }});\n\
         }}\n\
         let Some(editing) = self.{w}_editing.as_mut() else {{\n\
         return Ok(());\n\
         }};\n\
         let result: Result<(), String> = match column {{\n\
         {arms}\
         _ => Err(format!(\"no column {{column}}\")),\n\
         }};\n\
         editing.invalid = result.as_ref().err().cloned();\n\
         result\n\
         }}\n\
         /// Saves the row of `{w}` being edited (`on_post` may refuse; with `table`/`key` it is\n\
         /// also written in the background). `false`: the row stays in editing.\n\
         pub fn {w}_post(&mut self) -> bool {{\n\
         let Some(editing) = self.{w}_editing.take() else {{\n\
         return true;\n\
         }};\n\
         if let Some(error) = editing.invalid.clone() {{\n\
         self.{w}_editing = Some(editing);\n\
         {report}\
         return false;\n\
         }}\n\
         {on_post}\
         {post_write}\
         if let Some(slot) = self.{w}_rows.get_mut(editing.index) {{\n\
         *slot = editing.row;\n\
         }}\n\
         self.{w}_show();\n\
         true\n\
         }}\n\
         /// Discards the changes of the row of `{w}` being edited (a new row is removed).\n\
         pub fn {w}_cancel(&mut self) {{\n\
         if let Some(editing) = self.{w}_editing.take() {{\n\
         if editing.inserted && editing.index < self.{w}_rows.len() {{\n\
         self.{w}_rows.remove(editing.index);\n\
         }}\n\
         self.{w}_show();\n\
         }}\n\
         }}\n\
         /// Adds an empty row at the end of `{w}` and edits it (saved like any edited row).\n\
         pub fn {w}_insert(&mut self) -> bool {{\n\
         if !self.{w}_post() {{\n\
         return false;\n\
         }}\n\
         let row = {row_struct}::default();\n\
         self.{w}_rows.push(row.clone());\n\
         let index = self.{w}_rows.len() - 1;\n\
         self.{w}_editing = Some(crate::app::Editing {{ index, row, inserted: true, invalid: None }});\n\
         self.{w}_show();\n\
         {select_new}\
         true\n\
         }}\n\
         /// Deletes the selected row of `{w}` (`on_delete` may refuse; with `table`/`key` also in\n\
         /// the database).\n\
         pub fn {w}_delete(&mut self) -> bool {{\n\
         let index: Option<usize> = {selected};\n\
         let Some(index) = index.or(self.{w}_editing.as_ref().map(|e| e.index)) else {{\n\
         return false;\n\
         }};\n\
         if self.{w}_editing.as_ref().is_some_and(|e| e.index == index && e.inserted) {{\n\
         self.{w}_cancel();\n\
         return true;\n\
         }}\n\
         let Some(old) = self.{w}_rows.get(index).cloned() else {{\n\
         return false;\n\
         }};\n\
         let _ = &old;\n\
         {on_delete}\
         {delete_write}\
         match self.{w}_editing.as_mut() {{\n\
         Some(e) if e.index == index => self.{w}_editing = None,\n\
         Some(e) if e.index > index => e.index -= 1,\n\
         _ => {{}}\n\
         }}\n\
         self.{w}_rows.remove(index);\n\
         self.{w}_show();\n\
         true\n\
         }}\n\
         /// The backend calls this when the selection of `{w}` changes: leaving the row being\n\
         /// edited saves it (if that is refused, the row is selected again).\n\
         pub fn {w}_row_changed(&mut self, index: Option<usize>) {{\n\
         let Some(back) = self.{w}_editing.as_ref().map(|e| e.index) else {{\n\
         return;\n\
         }};\n\
         if index != Some(back) && !self.{w}_post() {{\n\
         {select_back}\
         }}\n\
         }}\n",
        headers = headers.join(", "),
        cells = cells.join(", "),
        select_kept = select("index"),
        select_new = select("Some(index)"),
        select_back = select("Some(back)"),
    )
}

/// Loading and taking over the data of a form's list and table widgets
/// (empty methods without bindings – the backends call them anyway).
fn data_impl(form: &Form, bindings: &[Binding], runtime: &FormRuntime, live: bool) -> String {
    let mut code = String::new();
    // Zeilen-Structs der Tabellen.
    for b in bindings {
        if let BindingKind::Table {
            row_struct,
            columns,
            edit,
            ..
        } = &b.kind
        {
            let _ = write!(
                code,
                "/// A row of `{}` (all columns of its data source).\n\
                 #[allow(dead_code)]\n#[derive(Debug, Clone{})]\npub struct {row_struct} {{\n",
                b.widget,
                if edit.is_some() { ", Default" } else { "" }
            );
            for (name, ty, _) in columns {
                let _ = writeln!(code, "pub {name}: {ty},");
            }
            code.push_str("}\n");
        }
    }
    let _ = write!(
        code,
        "#[allow(dead_code, unused_variables)]\nimpl {} {{\n",
        pascal_case(form.name.as_str())
    );
    for b in bindings {
        let w = &b.widget;
        let selected = &b.selected;
        match &b.kind {
            BindingKind::List {
                value_field,
                value_type,
                get_text,
                get_value,
            } => {
                let value = get_value
                    .clone()
                    .unwrap_or_else(|| "text.clone()".to_owned());
                let _ = write!(
                    code,
                    "/// Loads the entries of `{w}` from its data source in the background (Tokio);\n\
                     /// the form shows them when they arrive.\n\
                     pub fn {w}_reload(&self) {{\n\
                     let source = self.{w}_source.clone();\n\
                     crate::app::freya_spawn_load(self.{w}_pending.clone(), async move {{\n\
                     let rows = {fetch};\n\
                     let mut entries = Vec::new();\n\
                     for row in &rows {{\n\
                     let text: String = {get_text};\n\
                     let value: {value_type} = {value};\n\
                     entries.push((value, text));\n\
                     }}\n\
                     Ok(entries)\n\
                     }});\n\
                     }}\n\
                     /// The value (`{value_field}`) of the selected entry of `{w}`.\n\
                     pub fn {w}_selected_value(&self) -> Option<{value_type}> {{\n\
                     let index: Option<usize> = {selected};\n\
                     index.and_then(|i| self.{w}_values.get(i).cloned())\n\
                     }}\n",
                    fetch = b.fetch,
                );
            }
            BindingKind::Table {
                row_struct,
                columns,
                shown,
                value,
                edit,
            } => {
                if let Some(edit) = edit {
                    code.push_str(&edit_methods(b, row_struct, shown, edit));
                }
                let mut body = String::new();
                for (name, ty, get) in columns {
                    let _ = writeln!(body, "let col_{name}: {ty} = {get};");
                }
                let cells: Vec<String> = shown
                    .iter()
                    .map(|(name, rust_type, _)| cell_text(&format!("col_{name}"), rust_type))
                    .collect();
                let fields: Vec<String> = columns
                    .iter()
                    .map(|(name, _, _)| format!("{name}: col_{name}"))
                    .collect();
                let _ = write!(
                    code,
                    "/// Loads the rows of `{w}` from its data source in the background (Tokio);\n\
                     /// the form shows them when they arrive.\n\
                     pub fn {w}_reload(&self) {{\n\
                     let source = self.{w}_source.clone();\n\
                     crate::app::freya_spawn_load(self.{w}_pending.clone(), async move {{\n\
                     let rows = {fetch};\n\
                     let mut entries = Vec::new();\n\
                     for row in &rows {{\n\
                     {body}\
                     let cells = vec![{cells}];\n\
                     entries.push(({row_struct} {{ {fields} }}, cells));\n\
                     }}\n\
                     Ok(entries)\n\
                     }});\n\
                     }}\n\
                     /// The selected row of `{w}`.\n\
                     pub fn {w}_selected_row(&self) -> Option<&{row_struct}> {{\n\
                     let index: Option<usize> = {selected};\n\
                     index.and_then(|i| self.{w}_rows.get(i))\n\
                     }}\n",
                    fetch = b.fetch,
                    cells = cells.join(", "),
                    fields = fields.join(", "),
                );
                if let Some((name, ty)) = value {
                    let _ = write!(
                        code,
                        "/// The value (`{name}`) of the selected row of `{w}`.\n\
                         pub fn {w}_selected_value(&self) -> Option<{ty}> {{\n\
                         self.{w}_selected_row().map(|row| row.{name}.clone())\n\
                         }}\n"
                    );
                }
            }
        }
    }
    for methods in &runtime.methods {
        code.push_str(methods);
        code.push('\n');
    }
    code.push_str(
        "/// Starts the loads marked `Start` and the components that run in the\n\
         /// background (the backend calls it when the form opens).\n\
         pub fn freya_start_data(&self) {\n",
    );
    for b in bindings.iter().filter(|b| b.start) {
        let _ = writeln!(code, "self.{}_reload();", b.widget);
    }
    for start in &runtime.starts {
        code.push_str(start);
        code.push('\n');
    }
    code.push_str(
        "}\n/// Takes loaded rows into the list and table widgets; `true` if something arrived.\n\
         pub fn freya_take_data(&mut self) -> bool {\n\
         #[allow(unused_mut)]\n\
         let mut changed = false;\n",
    );
    for b in bindings {
        let w = &b.widget;
        let take = match &b.kind {
            BindingKind::List { value_type, .. } => format!(
                "let (values, items): (Vec<{value_type}>, Vec<String>) = entries.into_iter().unzip();\n\
                 self.{w}_values = values;\n{}\n",
                b.show
            ),
            BindingKind::Table {
                row_struct,
                shown,
                edit,
                ..
            } => {
                let headers: Vec<String> = shown
                    .iter()
                    .map(|(_, _, h)| format!("{h:?}.to_string()"))
                    .collect();
                format!(
                    "let (records, rows): (Vec<{row_struct}>, Vec<Vec<String>>) = entries.into_iter().unzip();\n\
                     self.{w}_rows = records;\n{}\
                     let headers: Vec<String> = vec![{}];\n{}\n",
                    if edit.is_some() {
                        format!("self.{w}_editing = None;\n")
                    } else {
                        String::new()
                    },
                    headers.join(", "),
                    b.show
                )
            }
        };
        if let Some(edit) = b.edit() {
            let report = report_error(w, edit, "error");
            let inserted = match &edit.writes {
                Some(writes) if writes.generated_key => {
                    format!("Ok(crate::app::Saved::Inserted) => self.{w}_reload(),\n")
                }
                _ => String::new(),
            };
            let _ = write!(
                code,
                "for result in self.{w}_saved.take_all() {{\n\
                 changed = true;\n\
                 match result {{\n\
                 {inserted}\
                 Ok(_) => {{}}\n\
                 Err(error) => {{\n\
                 let error = error.to_string();\n\
                 {report}\
                 self.{w}_reload();\n\
                 }}\n\
                 }}\n\
                 }}\n"
            );
        }
        let _ = write!(
            code,
            "if let Some(result) = self.{w}_pending.take() {{\n\
             changed = true;\n\
             match result {{\n\
             Ok(entries) => {{\n{take}}}\n\
             Err(error) => eprintln!(\"{w}: {{error}}\"),\n\
             }}\n\
             }}\n"
        );
    }
    if live {
        code.push_str(
            "for call in self.freya_queue.take() {\n\
             call(self);\n\
             changed = true;\n\
             }\n",
        );
    }
    code.push_str("changed\n}\n");
    if live && form.root.is_none() {
        code.push_str(
            "/// Runs the data module on Tokio: starts its components and calls\n\
             /// its handlers as events arrive (`run()` calls it).\n\
             pub fn freya_run(self) {\n\
             let queue = self.freya_queue.clone();\n\
             tokio::runtime::Handle::current().spawn(async move {\n\
             let mut module = self;\n\
             module.freya_start_data();\n\
             loop {\n\
             queue.wait().await;\n\
             module.freya_take_data();\n\
             }\n\
             });\n\
             }\n",
        );
    }
    code.push_str("}\n");
    code
}

/// Helpers in `app.gen.rs` when a form has data bindings.
const DATA_HELPERS: &str = r#"
/// The result of a background load, taken over by its form.
#[allow(dead_code)]
pub struct Pending<T>(std::sync::Arc<std::sync::Mutex<Option<Result<T, Error>>>>);

impl<T> Default for Pending<T> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<T> Clone for Pending<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

#[allow(dead_code)]
impl<T> Pending<T> {
    /// The result, once it is there.
    pub fn take(&self) -> Option<Result<T, Error>> {
        self.0.lock().ok()?.take()
    }

    fn put(&self, result: Result<T, Error>) {
        if let Ok(mut slot) = self.0.lock() {
            *slot = Some(result);
        }
    }
}

/// The row of a data grid being edited (`<grid>_edit_cell` … `<grid>_post`).
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Editing<R> {
    /// Index in the grid's rows.
    pub index: usize,
    pub row: R,
    /// A new row (`<grid>_insert`), not in the database yet.
    pub inserted: bool,
    /// Why the row cannot be saved (a cell text that does not fit).
    pub invalid: Option<String>,
}

/// What writing a row of a data grid did.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Saved {
    Updated,
    Inserted,
    Deleted,
}

/// Results of background writes, taken over by their form in order.
#[allow(dead_code)]
pub struct Results<T>(std::sync::Arc<std::sync::Mutex<Vec<Result<T, Error>>>>);

impl<T> Default for Results<T> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<T> Clone for Results<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

#[allow(dead_code)]
impl<T> Results<T> {
    /// All results that arrived since the last call.
    pub fn take_all(&self) -> Vec<Result<T, Error>> {
        self.0
            .lock()
            .map(|mut results| std::mem::take(&mut *results))
            .unwrap_or_default()
    }

    fn push(&self, result: Result<T, Error>) {
        if let Ok(mut results) = self.0.lock() {
            results.push(result);
        }
    }
}

/// Runs `write` on the Tokio runtime and adds its result to `results`.
#[allow(dead_code)]
pub fn freya_spawn_write<T: Send + 'static>(
    results: Results<T>,
    write: impl std::future::Future<Output = Result<T, Error>> + Send + 'static,
) {
    tokio::runtime::Handle::current().spawn(async move {
        results.push(write.await);
        freya_wake();
    });
}

/// Called when a background load is done (set by the UI backend).
static FREYA_WAKER: std::sync::Mutex<Option<Box<dyn Fn() + Send + Sync>>> =
    std::sync::Mutex::new(None);

/// Sets what wakes the UI when a background load is done.
#[allow(dead_code)]
pub fn freya_set_waker(waker: impl Fn() + Send + Sync + 'static) {
    if let Ok(mut slot) = FREYA_WAKER.lock() {
        *slot = Some(Box::new(waker));
    }
}

/// Runs `load` on the Tokio runtime and hands the result to `pending`.
#[allow(dead_code)]
pub fn freya_spawn_load<T: Send + 'static>(
    pending: Pending<T>,
    load: impl std::future::Future<Output = Result<T, Error>> + Send + 'static,
) {
    tokio::runtime::Handle::current().spawn(async move {
        pending.put(load.await);
        freya_wake();
    });
}

/// Wakes the UI (something arrived from the background).
pub fn freya_wake() {
    if let Ok(waker) = FREYA_WAKER.lock() {
        if let Some(waker) = waker.as_ref() {
            waker();
        }
    }
}
"#;

/// Helpers in `app.gen.rs` when a form or data module has an event queue.
const QUEUE_HELPERS: &str = r#"
/// Handler calls for a form or data module `T`, made on its thread
/// (`freya_take_data`); background tasks put them in with `push`.
pub struct Queue<T>(std::sync::Arc<QueueInner<T>>);

/// A call of a handler with the form.
pub type QueueCall<T> = Box<dyn FnOnce(&mut T) + Send>;

pub struct QueueInner<T> {
    calls: std::sync::Mutex<Vec<QueueCall<T>>>,
    notify: tokio::sync::Notify,
}

impl<T> Default for Queue<T> {
    fn default() -> Self {
        Self(std::sync::Arc::new(QueueInner {
            calls: std::sync::Mutex::new(Vec::new()),
            notify: tokio::sync::Notify::new(),
        }))
    }
}

impl<T> Clone for Queue<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

#[allow(dead_code)]
impl<T> Queue<T> {
    /// Queues a call and wakes the form.
    pub fn push(&self, call: impl FnOnce(&mut T) + Send + 'static) {
        if let Ok(mut calls) = self.0.calls.lock() {
            calls.push(Box::new(call));
        }
        self.0.notify.notify_one();
        freya_wake();
    }

    /// The queued calls, oldest first.
    pub fn take(&self) -> Vec<QueueCall<T>> {
        self.0
            .calls
            .lock()
            .map(|mut calls| std::mem::take(&mut *calls))
            .unwrap_or_default()
    }

    /// Waits until a call was queued.
    pub async fn wait(&self) {
        self.0.notify.notified().await;
    }
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Platform;

    #[test]
    fn literals() {
        let none = |_: &ComponentPath| None;
        assert_eq!(literal(&Value::Float(30.0), &none).unwrap(), "30.0");
        assert_eq!(
            literal(&Value::Text("a \"b\"".into()), &none).unwrap(),
            r#""a \"b\"""#
        );
        assert_eq!(literal(&Value::Reference(None), &none), None);
        assert_eq!(
            literal(
                &Value::List(vec![Value::Int(1), Value::Code(String::new())]),
                &none
            )
            .unwrap(),
            "vec![1]"
        );
        let v = PropertyValue::new("OK").on(Platform::MacOs, "Okay");
        let expr = property_expr(&v, &none).unwrap();
        assert!(syn::parse_str::<syn::Expr>(&expr).is_ok(), "{expr}");
        assert!(expr.contains(r#"#[cfg(target_os = "macos")] let value = "Okay";"#));
    }

    #[test]
    fn urls() {
        assert_eq!(url_join("https://a.org/", "/v1/x"), "https://a.org/v1/x");
        assert_eq!(url_join("https://a.org", "v1"), "https://a.org/v1");
        assert_eq!(url_join("", "/v1"), "/v1");
        assert_eq!(
            url_join("https://a.org", "http://b.org/y"),
            "http://b.org/y"
        );
    }

    #[test]
    fn names() {
        assert_eq!(pascal_case("main_form"), "MainForm");
        assert_eq!(pascal_case("services"), "Services");
        assert_eq!(pascal_case("_x1_y"), "X1Y");
    }

    #[test]
    fn template_syntax() {
        assert!(check_template("{{ rust.url }}").is_ok());
        assert!(check_template("{% if %}").is_err());
    }
}
