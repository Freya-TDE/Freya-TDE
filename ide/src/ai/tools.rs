//! The tools the AI may call: the library API of Freya TDE.
//!
//! Reading tools ([`Access::Read`]) always run. Tools that change something
//! ([`Access::Write`]) or run programs ([`Access::Run`]) follow the
//! project's [`AiPermission`]: not offered with "nur lesen", a [`Proposal`]
//! with preview first with "mit Vorschau", applied at once with
//! "selbstständig". A proposal holds the parsed [`Action`], so what is
//! applied is exactly what was shown.
//!
//! *Die Werkzeuge der KI: Lesen immer, Schreiben und Ausführen nach den
//! Rechten des Projekts, mit Vorschau als [`Proposal`].*

use crate::tr;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tdr_designer::{Command, Document};

use super::message::ToolSpec;
use crate::session::Session;
use crate::settings::AiPermission;

/// What a tool does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    /// Reads the project; always allowed.
    Read,
    /// Changes files or forms.
    Write,
    /// Runs a program (cargo).
    Run,
}

impl Access {
    /// May a tool of this kind be offered under `permission`?
    pub fn offered(self, permission: AiPermission) -> bool {
        self == Access::Read || permission != AiPermission::ReadOnly
    }
}

/// The project as the tools see it. The IDE implements it on its
/// [`Session`] (so changes go through the same commands and undo as the
/// designer); a wrapper can read and write open editor buffers instead of
/// the disk.
pub trait Workspace {
    fn document(&self) -> &Document;

    /// Executes a designer command (one undo step). Returns a short report.
    fn apply_command(&mut self, command: Command) -> Result<String, String>;

    /// Saves the project and writes the generated code; returns the
    /// written files.
    fn save_and_generate(&mut self) -> Result<Vec<String>, String>;

    /// Project folder.
    fn root(&self) -> Option<PathBuf> {
        self.document().dir().map(Path::to_path_buf)
    }

    /// Reads a project file (`rel` with `/`, already checked).
    fn read_text(&self, rel: &str) -> std::io::Result<String> {
        let root = self.root().ok_or_else(no_project)?;
        std::fs::read_to_string(root.join(rel))
    }

    /// Writes a project file (`rel` already checked); folders are created.
    fn write_text(&mut self, rel: &str, text: &str) -> std::io::Result<()> {
        let root = self.root().ok_or_else(no_project)?;
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, text)
    }

    /// Runs a cargo tool in the project folder (blocking).
    fn run_cargo(&mut self, tool: crate::tools::Tool) -> Result<String, String> {
        let root = self.root().ok_or("The project is not saved yet.")?;
        let generation = tdr_core::codegen::generate(
            self.document().project(),
            self.document().forms(),
            self.document().registry(),
            &root,
        );
        let report = crate::tools::run(tool, &root, Some(&generation), &[])
            .map_err(|e| format!("{}: {e}", tool.command_line()))?;
        Ok(cargo_summary(tool, &report))
    }
}

fn no_project() -> std::io::Error {
    std::io::Error::other("The project is not saved yet.")
}

impl Workspace for Session {
    fn document(&self) -> &Document {
        Session::document(self)
    }

    fn apply_command(&mut self, command: Command) -> Result<String, String> {
        self.apply(command).map_err(|e| e.to_string())?;
        Ok(match self.selection().first() {
            Some(name) => format!("done; selected: {name}"),
            None => "done".to_owned(),
        })
    }

    fn save_and_generate(&mut self) -> Result<Vec<String>, String> {
        self.write_code().map(|(_, written)| written)
    }
}

/// A change that waits for approval (or was applied at once).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Proposal {
    /// The tool call it answers.
    pub call_id: String,
    pub tool: String,
    pub access: Access,
    /// One line: what will happen.
    pub summary: String,
    /// Details for the preview: a diff, the command, the files.
    pub preview: String,
    pub action: Action,
}

/// What a proposal does when applied.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    /// Writes a file. `before` is the content the preview was made from
    /// (`None` = new file); applying refuses if the file changed since.
    WriteFile {
        path: String,
        before: Option<String>,
        content: String,
    },
    /// A designer command.
    Designer { command: Command },
    /// Save the project and write the generated code.
    GenerateCode,
    /// Run a cargo tool.
    Cargo { tool: crate::tools::Tool },
}

struct Def {
    name: &'static str,
    access: Access,
    description: &'static str,
    schema: fn() -> Value,
}

fn no_input() -> Value {
    json!({ "type": "object", "properties": {} })
}

const DEFS: &[Def] = &[
    Def {
        name: "project_info",
        access: Access::Read,
        description: "Project overview: name, kind, UI backend, packs, target platforms, Rust version, forms with their elements.",
        schema: no_input,
    },
    Def {
        name: "list_files",
        access: Access::Read,
        description: "Files and folders of the project (without target/, .git/; generated *.gen.rs marked).",
        schema: || {
            json!({ "type": "object", "properties": {
                "path": { "type": "string", "description": "subfolder, relative to the project (empty = everything)" }
            }})
        },
    },
    Def {
        name: "read_file",
        access: Access::Read,
        description: "Reads a text file of the project with line numbers.",
        schema: || {
            json!({ "type": "object", "required": ["path"], "properties": {
                "path": { "type": "string", "description": "relative to the project, e.g. src/main_form.rs" },
                "offset": { "type": "integer", "description": "first line (from 1)" },
                "limit": { "type": "integer", "description": "number of lines (default 2000)" }
            }})
        },
    },
    Def {
        name: "search_files",
        access: Access::Read,
        description: "Searches text (case-insensitive) in the project files; returns file:line: text.",
        schema: || {
            json!({ "type": "object", "required": ["query"], "properties": {
                "query": { "type": "string" },
                "path": { "type": "string", "description": "only in this subfolder" }
            }})
        },
    },
    Def {
        name: "read_form",
        access: Access::Read,
        description: "A form (designer) as RON: component tree with properties, events, placement.",
        schema: || {
            json!({ "type": "object", "required": ["form"], "properties": {
                "form": { "type": "string", "description": "name of the form, e.g. main_form" }
            }})
        },
    },
    Def {
        name: "component_types",
        access: Access::Read,
        description: "Component and widget types available in the project (core, backend, packs) with properties and events.",
        schema: || {
            json!({ "type": "object", "properties": {
                "filter": { "type": "string", "description": "only types whose name contains this" }
            }})
        },
    },
    Def {
        name: "problems",
        access: Access::Read,
        description: "Problems of the project: form validation, dependency graph and code generation diagnostics with codes.",
        schema: no_input,
    },
    Def {
        name: "generated_code",
        access: Access::Read,
        description: "Dry run of the code generation: without path the list of files (create/update/unchanged), with path the content.",
        schema: || {
            json!({ "type": "object", "properties": {
                "path": { "type": "string", "description": "e.g. src/main_form.gen.rs" }
            }})
        },
    },
    Def {
        name: "write_file",
        access: Access::Write,
        description: "Writes a file of the project (new or fully replaced). Not for *.gen.rs (generate_code) or .ftdr/.ptdr (designer).",
        schema: || {
            json!({ "type": "object", "required": ["path", "content"], "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" }
            }})
        },
    },
    Def {
        name: "edit_file",
        access: Access::Write,
        description: "Replaces exactly one occurrence of old_text with new_text in a file (old_text must occur exactly once).",
        schema: || {
            json!({ "type": "object", "required": ["path", "old_text", "new_text"], "properties": {
                "path": { "type": "string" },
                "old_text": { "type": "string" },
                "new_text": { "type": "string" }
            }})
        },
    },
    Def {
        name: "designer",
        access: Access::Write,
        description: "Changes forms through designer commands (JSON; several = one undo step). Examples: \
{\"command\":\"add_widget\",\"form\":\"main_form\",\"parent\":\"window\",\"type\":\"core/Button\",\"name\":\"ok_button\"}, \
{\"command\":\"set_property\",\"form\":\"main_form\",\"element\":\"ok_button\",\"property\":\"caption\",\"value\":{\"Text\":\"OK\"}}, \
{\"command\":\"set_event\",\"form\":\"main_form\",\"element\":\"ok_button\",\"event\":\"on_click\",\"handler\":\"ok_button_click\"}, \
{\"command\":\"add_component\",\"form\":\"services\",\"type\":\"sqlx/Pool\"}. More: add_form, remove_form, rename_form, remove, rename, move, set_placement, set_layout. \
Values: {\"Bool\":true}, {\"Int\":5}, {\"Float\":1.5}, {\"Text\":\"…\"}, {\"Enum\":\"…\"}, {\"Reference\":\"services::db_pool\"}.",
        schema: || {
            json!({ "type": "object", "required": ["commands"], "properties": {
                "commands": { "type": "array", "items": { "type": "object" } }
            }})
        },
    },
    Def {
        name: "generate_code",
        access: Access::Write,
        description: "Saves the project and generates the code (*.gen.rs, missing handler stubs, Cargo.toml).",
        schema: no_input,
    },
    Def {
        name: "cargo",
        access: Access::Run,
        description: "Runs cargo check, clippy or test in the project and returns the messages.",
        schema: || {
            json!({ "type": "object", "required": ["tool"], "properties": {
                "tool": { "type": "string", "enum": ["check", "clippy", "test"] }
            }})
        },
    },
];

/// The tools offered under `permission`.
pub fn specs(permission: AiPermission) -> Vec<ToolSpec> {
    DEFS.iter()
        .filter(|d| d.access.offered(permission))
        .map(|d| ToolSpec {
            name: d.name.to_owned(),
            description: d.description.to_owned(),
            input_schema: (d.schema)(),
        })
        .collect()
}

/// The kind of tool `name` (`None` = unknown).
pub fn access(name: &str) -> Option<Access> {
    DEFS.iter().find(|d| d.name == name).map(|d| d.access)
}

fn text<'a>(input: &'a Value, key: &str) -> Result<&'a str, String> {
    input[key]
        .as_str()
        .ok_or_else(|| format!("parameter `{key}` is missing"))
}

/// Checks a path given by the AI: relative, inside the project, not into
/// `target/` or `.git/`. Returns it with `/`.
pub fn project_path(root: &Path, rel: &str) -> Result<String, String> {
    let rel = rel.trim().replace('\\', "/");
    let rel = rel.trim_start_matches("./");
    if rel.is_empty() {
        return Err("leerer Pfad".into());
    }
    let mut parts = Vec::new();
    for c in Path::new(rel).components() {
        match c {
            Component::Normal(p) => parts.push(p.to_string_lossy().into_owned()),
            Component::CurDir => {}
            _ => return Err(format!("{rel}: nur Pfade innerhalb des Projekts")),
        }
    }
    if parts
        .first()
        .is_some_and(|p| crate::files::SKIPPED.contains(&p.as_str()))
    {
        return Err(format!("{rel}: target/ and .git/ are off limits"));
    }
    let joined = parts.join("/");
    // Symbolische Verknüpfungen dürfen nicht aus dem Projekt führen.
    if let (Ok(full), Ok(base)) = (root.join(&joined).canonicalize(), root.canonicalize())
        && !full.starts_with(&base)
    {
        return Err(format!("{rel}: leads out of the project"));
    }
    Ok(joined)
}

fn root_of(ws: &dyn Workspace) -> Result<PathBuf, String> {
    ws.root()
        .ok_or_else(|| "The project is not saved yet.".to_owned())
}

/// Runs a reading tool.
pub fn read(ws: &dyn Workspace, name: &str, input: &Value) -> Result<String, String> {
    let doc = ws.document();
    match name {
        "project_info" => {
            let p = doc.project();
            let forms: Vec<Value> = doc
                .forms()
                .iter()
                .map(|f| {
                    let elements: Vec<String> = f
                        .elements()
                        .iter()
                        .map(|e| {
                            let c = e.component();
                            format!("{}: {}", c.name, c.type_ref)
                        })
                        .collect();
                    json!({ "name": f.name.to_string(), "elements": elements })
                })
                .collect();
            pretty(&json!({
                "name": p.name,
                "kind": p.kind,
                "backend": p.backend,
                "packs": p.packs,
                "platforms": p.platforms,
                "rust_version": p.rust_version,
                "generate": p.generate,
                "dir": doc.dir().map(|d| d.display().to_string()),
                "forms": forms,
            }))
        }
        "list_files" => {
            let root = root_of(ws)?;
            let entries = crate::files::scan(
                &root,
                &crate::files::ScanOptions {
                    show_generated: true,
                    show_hidden: false,
                },
            );
            let prefix = match input["path"].as_str().filter(|p| !p.trim().is_empty()) {
                Some(p) => Some(project_path(&root, p)?),
                None => None,
            };
            let mut lines = Vec::new();
            list(&entries, prefix.as_deref(), &mut lines);
            Ok(if lines.is_empty() {
                "(no files)".into()
            } else {
                lines.join("\n")
            })
        }
        "read_file" => {
            let root = root_of(ws)?;
            let path = project_path(&root, text(input, "path")?)?;
            let content = ws.read_text(&path).map_err(|e| format!("{path}: {e}"))?;
            let offset = input["offset"].as_u64().unwrap_or(1).max(1) as usize;
            let limit = input["limit"].as_u64().unwrap_or(2000).clamp(1, 5000) as usize;
            let total = content.lines().count();
            let mut out: String = content
                .lines()
                .enumerate()
                .skip(offset - 1)
                .take(limit)
                .map(|(i, l)| format!("{:>5}\t{l}\n", i + 1))
                .collect();
            if offset - 1 + limit < total {
                out.push_str(&format!("… ({total} Zeilen insgesamt)\n"));
            }
            Ok(out)
        }
        "search_files" => {
            let root = root_of(ws)?;
            let query = text(input, "query")?.to_lowercase();
            if query.is_empty() {
                return Err("empty search".into());
            }
            let prefix = match input["path"].as_str().filter(|p| !p.trim().is_empty()) {
                Some(p) => Some(project_path(&root, p)?),
                None => None,
            };
            let entries = crate::files::scan(
                &root,
                &crate::files::ScanOptions {
                    show_generated: true,
                    show_hidden: false,
                },
            );
            let mut files = Vec::new();
            list(&entries, prefix.as_deref(), &mut files);
            let mut hits = Vec::new();
            'files: for path in files.iter().filter(|p| !p.ends_with('/')) {
                let Ok(content) = ws.read_text(path) else {
                    continue;
                };
                for (i, line) in content.lines().enumerate() {
                    if line.to_lowercase().contains(&query) {
                        hits.push(format!("{path}:{}: {}", i + 1, line.trim()));
                        if hits.len() >= 200 {
                            hits.push("… (mehr als 200 Treffer)".into());
                            break 'files;
                        }
                    }
                }
            }
            Ok(if hits.is_empty() {
                "no matches".into()
            } else {
                hits.join("\n")
            })
        }
        "read_form" => {
            let name = text(input, "form")?;
            let form = doc
                .forms()
                .iter()
                .find(|f| f.name.as_str() == name)
                .ok_or_else(|| format!("form `{name}` does not exist"))?;
            form.to_ron().map_err(|e| e.to_string())
        }
        "component_types" => {
            let filter = input["filter"].as_str().unwrap_or_default().to_lowercase();
            let backend = &doc.project().backend;
            let mut types: Vec<Value> = doc
                .registry()
                .types()
                .filter(|t| t.type_ref.to_string().to_lowercase().contains(&filter))
                .map(|t| {
                    let props: Vec<Value> = doc
                        .registry()
                        .property_defs(&t.type_ref, backend)
                        .iter()
                        .map(|p| json!({ "name": p.name, "type": p.ty }))
                        .collect();
                    let events: Vec<&str> = t.events.iter().map(|e| e.name.as_str()).collect();
                    json!({
                        "type": t.type_ref.to_string(),
                        "kind": t.kind,
                        "category": t.category,
                        "description": t.description,
                        "properties": props,
                        "events": events,
                    })
                })
                .collect();
            types.sort_by(|a, b| a["type"].as_str().cmp(&b["type"].as_str()));
            pretty(&Value::Array(types))
        }
        "problems" => {
            let mut out = json!({ "findings": doc.findings() });
            if let Some(root) = ws.root() {
                let generation =
                    tdr_core::codegen::generate(doc.project(), doc.forms(), doc.registry(), &root);
                out["codegen"] = json!(generation.diagnostics);
            }
            pretty(&out)
        }
        "generated_code" => {
            let root = root_of(ws)?;
            let generation =
                tdr_core::codegen::generate(doc.project(), doc.forms(), doc.registry(), &root);
            match input["path"].as_str().filter(|p| !p.is_empty()) {
                Some(path) => generation
                    .file(path)
                    .map(|f| f.content.clone())
                    .ok_or_else(|| format!("{path} is not generated")),
                None => pretty(&json!(
                    generation
                        .files
                        .iter()
                        .map(|f| json!({ "path": f.path, "change": f.change, "kind": f.kind }))
                        .collect::<Vec<_>>()
                )),
            }
        }
        "cargo" | "write_file" | "edit_file" | "designer" | "generate_code" => {
            Err(format!("{name} is not a read tool"))
        }
        _ => Err(format!("unknown tool `{name}`")),
    }
}

fn list(entries: &[crate::files::Entry], prefix: Option<&str>, out: &mut Vec<String>) {
    for e in entries {
        let inside = prefix.is_none_or(|p| e.path == p || e.path.starts_with(&format!("{p}/")));
        let above = prefix.is_some_and(|p| p.starts_with(&format!("{}/", e.path)));
        match e.kind {
            crate::files::EntryKind::Folder => {
                if inside {
                    out.push(format!("{}/", e.path));
                }
                if inside || above {
                    list(&e.children, prefix, out);
                }
            }
            crate::files::EntryKind::File if inside => out.push(if e.generated {
                format!("{} (generiert)", e.path)
            } else {
                e.path.clone()
            }),
            crate::files::EntryKind::File => {}
        }
    }
}

fn pretty(v: &Value) -> Result<String, String> {
    serde_json::to_string_pretty(v).map_err(|e| e.to_string())
}

/// Prepares a writing or running tool: checks the input and builds the
/// preview, without changing anything.
pub fn plan(
    ws: &dyn Workspace,
    call_id: &str,
    name: &str,
    input: &Value,
) -> Result<Proposal, String> {
    let access = access(name).ok_or_else(|| format!("unknown tool `{name}`"))?;
    let proposal = |summary: String, preview: String, action: Action| Proposal {
        call_id: call_id.to_owned(),
        tool: name.to_owned(),
        access,
        summary,
        preview,
        action,
    };
    match name {
        "write_file" | "edit_file" => {
            let root = root_of(ws)?;
            let path = project_path(&root, text(input, "path")?)?;
            if crate::files::is_generated(&path) {
                return Err(format!(
                    "{path} is generated and read-only; change the form (designer) and use generate_code"
                ));
            }
            if path.ends_with(".ftdr") || path.ends_with(".ptdr") {
                return Err(format!(
                    "{path}: change forms and the project file only through the tool designer"
                ));
            }
            let before = ws.read_text(&path).ok();
            let content = if name == "write_file" {
                text(input, "content")?.to_owned()
            } else {
                let old = text(input, "old_text")?;
                let new = text(input, "new_text")?;
                let current = before
                    .as_deref()
                    .ok_or_else(|| format!("{path} does not exist"))?;
                match current.matches(old).count() {
                    1 => current.replacen(old, new, 1),
                    0 => return Err(format!("old_text does not occur in {path}")),
                    n => {
                        return Err(format!(
                            "old_text kommt in {path} {n}-mal vor; mehr Zusammenhang angeben"
                        ));
                    }
                }
            };
            if before.as_deref() == Some(content.as_str()) {
                return Err(format!("{path} would stay unchanged"));
            }
            let summary = match &before {
                Some(_) => tr!("ai.proposal.change", path = path),
                None => tr!("ai.proposal.create", path = path),
            };
            let preview = line_diff(&path, before.as_deref().unwrap_or_default(), &content);
            Ok(proposal(
                summary,
                preview,
                Action::WriteFile {
                    path,
                    before,
                    content,
                },
            ))
        }
        "designer" => {
            let commands = input["commands"]
                .as_array()
                .filter(|c| !c.is_empty())
                .ok_or("parameter `commands` is missing (list of commands)")?;
            let mut parsed = Vec::new();
            for c in commands {
                parsed.push(
                    serde_json::from_value::<Command>(c.clone())
                        .map_err(|e| format!("command {c}: {e}"))?,
                );
            }
            let command = if parsed.len() == 1 {
                parsed.remove(0)
            } else {
                Command::Batch { commands: parsed }
            };
            // Probelauf an einer Kopie: Fehler zeigen sich vor dem Übernehmen.
            let mut trial = ws.document().clone();
            let outcome = trial.apply(command.clone()).map_err(|e| e.to_string())?;
            let created: Vec<String> = outcome.created.iter().map(ToString::to_string).collect();
            let summary = match commands.len() {
                1 => format!(
                    "Designer: {}",
                    commands[0]["command"].as_str().unwrap_or("?")
                ),
                n => tr!("ai.proposal.commands", count = n),
            };
            let mut preview = commands
                .iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n");
            if !created.is_empty() {
                preview.push_str(&tr!("ai.proposal.created", names = created.join(", ")));
            }
            Ok(proposal(summary, preview, Action::Designer { command }))
        }
        "generate_code" => {
            let root = root_of(ws)?;
            let doc = ws.document();
            let generation =
                tdr_core::codegen::generate(doc.project(), doc.forms(), doc.registry(), &root);
            let mut preview: Vec<String> = generation
                .changed()
                .map(|f| format!("{:?}: {}", f.change, f.path))
                .collect();
            if preview.is_empty() {
                preview.push(tr!("ai.proposal.no_changes"));
            }
            for d in &generation.diagnostics {
                preview.push(format!("{:?}: {}", d.severity, d.issue));
            }
            Ok(proposal(
                tr!("ai.proposal.generate"),
                preview.join("\n"),
                Action::GenerateCode,
            ))
        }
        "cargo" => {
            let tool = match text(input, "tool")? {
                "check" => crate::tools::Tool::Check,
                "clippy" => crate::tools::Tool::Clippy,
                "test" => crate::tools::Tool::Test,
                other => return Err(format!("cargo {other} is not allowed")),
            };
            Ok(proposal(
                tr!("ai.proposal.run", command = tool.command_line()),
                tr!("ai.proposal.in_project", command = tool.command_line()),
                Action::Cargo { tool },
            ))
        }
        _ => Err(format!("{name} is a read tool")),
    }
}

/// Applies a proposal. Returns the report for the AI.
pub fn apply(ws: &mut dyn Workspace, proposal: &Proposal) -> Result<String, String> {
    match &proposal.action {
        Action::WriteFile {
            path,
            before,
            content,
        } => {
            let now = ws.read_text(path).ok();
            if now != *before {
                return Err(format!(
                    "{path} was changed since the preview; read it again and propose again"
                ));
            }
            ws.write_text(path, content)
                .map_err(|e| format!("{path}: {e}"))?;
            Ok(format!("{path} written"))
        }
        Action::Designer { command } => ws.apply_command(command.clone()),
        Action::GenerateCode => {
            let written = ws.save_and_generate()?;
            Ok(if written.is_empty() {
                "code is up to date".into()
            } else {
                format!("written: {}", written.join(", "))
            })
        }
        Action::Cargo { tool } => ws.run_cargo(*tool),
    }
}

/// Short text of a cargo run for the AI: success and the messages.
pub fn cargo_summary(tool: crate::tools::Tool, report: &crate::tools::ToolReport) -> String {
    let mut out = format!(
        "{}: {}\n",
        tool.command_line(),
        if report.success {
            "ok"
        } else {
            "fehlgeschlagen"
        }
    );
    if let Some(build) = &report.build {
        for m in build.messages.iter().take(50) {
            out.push_str(&serde_json::to_string(m).unwrap_or_default());
            out.push('\n');
        }
    }
    let rest: String = report.output.chars().rev().take(4000).collect();
    out.extend(rest.chars().rev());
    out
}

/// A compact diff for the preview: common start and end are skipped, the
/// changed block is shown with `-`/`+` and three lines of context.
pub fn line_diff(path: &str, old: &str, new: &str) -> String {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    let start = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let tail = a[start..]
        .iter()
        .rev()
        .zip(b[start..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let ctx = 3;
    let from = start.saturating_sub(ctx);
    let mut out = format!("--- {path}\n+++ {path}\n@@ Zeile {} @@\n", from + 1);
    for l in &a[from..start] {
        out.push_str(&format!(" {l}\n"));
    }
    for l in &a[start..a.len() - tail] {
        out.push_str(&format!("-{l}\n"));
    }
    for l in &b[start..b.len() - tail] {
        out.push_str(&format!("+{l}\n"));
    }
    for l in a[a.len() - tail..].iter().take(ctx) {
        out.push_str(&format!(" {l}\n"));
    }
    out
}
