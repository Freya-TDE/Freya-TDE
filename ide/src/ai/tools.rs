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
        let root = self
            .root()
            .ok_or("Das Projekt ist noch nicht gespeichert.")?;
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
    std::io::Error::other("Das Projekt ist noch nicht gespeichert.")
}

impl Workspace for Session {
    fn document(&self) -> &Document {
        Session::document(self)
    }

    fn apply_command(&mut self, command: Command) -> Result<String, String> {
        self.apply(command).map_err(|e| e.to_string())?;
        Ok(match self.selection().first() {
            Some(name) => format!("ausgeführt; gewählt: {name}"),
            None => "ausgeführt".to_owned(),
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
        description: "Projekt-Übersicht: Name, Art, UI-Backend, Packs, Zielsysteme, Rust-Version, Formulare mit Elementen. / Project overview.",
        schema: no_input,
    },
    Def {
        name: "list_files",
        access: Access::Read,
        description: "Dateien und Ordner des Projekts (ohne target/, .git/; generierte *.gen.rs mit Kennzeichen). / Project files.",
        schema: || {
            json!({ "type": "object", "properties": {
                "path": { "type": "string", "description": "Unterordner, relativ zum Projekt (leer = alles)" }
            }})
        },
    },
    Def {
        name: "read_file",
        access: Access::Read,
        description: "Liest eine Textdatei des Projekts mit Zeilennummern. / Reads a project file with line numbers.",
        schema: || {
            json!({ "type": "object", "required": ["path"], "properties": {
                "path": { "type": "string", "description": "relativ zum Projekt, z. B. src/main_form.rs" },
                "offset": { "type": "integer", "description": "erste Zeile (ab 1)" },
                "limit": { "type": "integer", "description": "Anzahl Zeilen (Standard 2000)" }
            }})
        },
    },
    Def {
        name: "search_files",
        access: Access::Read,
        description: "Sucht Text (ohne Groß-/Kleinschreibung) in den Dateien des Projekts; liefert Datei:Zeile: Text. / Searches project files.",
        schema: || {
            json!({ "type": "object", "required": ["query"], "properties": {
                "query": { "type": "string" },
                "path": { "type": "string", "description": "nur in diesem Unterordner" }
            }})
        },
    },
    Def {
        name: "read_form",
        access: Access::Read,
        description: "Ein Formular (Designer) als RON: Komponentenbaum mit Eigenschaften, Ereignissen, Platzierung. / A form as RON.",
        schema: || {
            json!({ "type": "object", "required": ["form"], "properties": {
                "form": { "type": "string", "description": "Name des Formulars, z. B. main_form" }
            }})
        },
    },
    Def {
        name: "component_types",
        access: Access::Read,
        description: "Verfügbare Komponenten- und Widget-Typen des Projekts (Kern, Backend, Packs) mit Eigenschaften und Ereignissen. / Available types.",
        schema: || {
            json!({ "type": "object", "properties": {
                "filter": { "type": "string", "description": "nur Typen, deren Name dies enthält" }
            }})
        },
    },
    Def {
        name: "problems",
        access: Access::Read,
        description: "Probleme des Projekts: Formularprüfung, Abhängigkeitsgraph und Codegen-Diagnosen mit Codes. / Project problems.",
        schema: no_input,
    },
    Def {
        name: "generated_code",
        access: Access::Read,
        description: "Probelauf der Codeerzeugung: ohne path die Liste der Dateien (create/update/unchanged), mit path den Inhalt. / Code generation dry run.",
        schema: || {
            json!({ "type": "object", "properties": {
                "path": { "type": "string", "description": "z. B. src/main_form.gen.rs" }
            }})
        },
    },
    Def {
        name: "write_file",
        access: Access::Write,
        description: "Schreibt eine Datei des Projekts (neu oder ganz ersetzt). Nicht für *.gen.rs (generate_code), .ftdr/.ptdr (designer). / Writes a file.",
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
        description: "Ersetzt in einer Datei genau eine Stelle old_text durch new_text (old_text muss genau einmal vorkommen). / Replaces one exact text.",
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
        description: "Ändert Formulare über Designer-Befehle (JSON, mehrere = ein Rückgängig-Schritt). Beispiele: \
{\"command\":\"add_widget\",\"form\":\"main_form\",\"parent\":\"window\",\"type\":\"core/Button\",\"name\":\"ok_button\"}, \
{\"command\":\"set_property\",\"form\":\"main_form\",\"element\":\"ok_button\",\"property\":\"caption\",\"value\":{\"Text\":\"OK\"}}, \
{\"command\":\"set_event\",\"form\":\"main_form\",\"element\":\"ok_button\",\"event\":\"on_click\",\"handler\":\"ok_button_click\"}, \
{\"command\":\"add_component\",\"form\":\"services\",\"type\":\"sqlx/Pool\"}. Weitere: add_form, remove_form, rename_form, remove, rename, move, set_placement, set_layout. \
Werte: {\"Bool\":true}, {\"Int\":5}, {\"Float\":1.5}, {\"Text\":\"…\"}, {\"Enum\":\"…\"}, {\"Reference\":\"services::db_pool\"}. / Designer commands.",
        schema: || {
            json!({ "type": "object", "required": ["commands"], "properties": {
                "commands": { "type": "array", "items": { "type": "object" } }
            }})
        },
    },
    Def {
        name: "generate_code",
        access: Access::Write,
        description: "Speichert das Projekt und erzeugt den Code (*.gen.rs, fehlende Handler-Stubs, Cargo.toml). / Saves and generates code.",
        schema: no_input,
    },
    Def {
        name: "cargo",
        access: Access::Run,
        description: "Führt cargo check, clippy oder test im Projekt aus und liefert die Meldungen. / Runs cargo.",
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
        .ok_or_else(|| format!("Parameter „{key}“ fehlt"))
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
        return Err(format!("{rel}: target/ und .git/ sind gesperrt"));
    }
    let joined = parts.join("/");
    // Symbolische Verknüpfungen dürfen nicht aus dem Projekt führen.
    if let (Ok(full), Ok(base)) = (root.join(&joined).canonicalize(), root.canonicalize())
        && !full.starts_with(&base)
    {
        return Err(format!("{rel}: führt aus dem Projekt"));
    }
    Ok(joined)
}

fn root_of(ws: &dyn Workspace) -> Result<PathBuf, String> {
    ws.root()
        .ok_or_else(|| "Das Projekt ist noch nicht gespeichert.".to_owned())
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
                "(keine Dateien)".into()
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
                return Err("leere Suche".into());
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
                "keine Treffer".into()
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
                .ok_or_else(|| format!("Formular „{name}“ gibt es nicht"))?;
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
                    .ok_or_else(|| format!("{path} wird nicht erzeugt")),
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
            Err(format!("{name} ist kein Lese-Werkzeug"))
        }
        _ => Err(format!("unbekanntes Werkzeug „{name}“")),
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
    let access = access(name).ok_or_else(|| format!("unbekanntes Werkzeug „{name}“"))?;
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
                    "{path} wird erzeugt und ist schreibgeschützt; Formular ändern (designer) und generate_code nutzen"
                ));
            }
            if path.ends_with(".ftdr") || path.ends_with(".ptdr") {
                return Err(format!(
                    "{path}: Formulare und Projektdatei nur über das Werkzeug designer ändern"
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
                    .ok_or_else(|| format!("{path} gibt es nicht"))?;
                match current.matches(old).count() {
                    1 => current.replacen(old, new, 1),
                    0 => return Err(format!("old_text kommt in {path} nicht vor")),
                    n => {
                        return Err(format!(
                            "old_text kommt in {path} {n}-mal vor; mehr Zusammenhang angeben"
                        ));
                    }
                }
            };
            if before.as_deref() == Some(content.as_str()) {
                return Err(format!("{path} bliebe unverändert"));
            }
            let summary = match &before {
                Some(_) => format!("{path} ändern"),
                None => format!("{path} anlegen"),
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
                .ok_or("Parameter „commands“ fehlt (Liste von Befehlen)")?;
            let mut parsed = Vec::new();
            for c in commands {
                parsed.push(
                    serde_json::from_value::<Command>(c.clone())
                        .map_err(|e| format!("Befehl {c}: {e}"))?,
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
                n => format!("Designer: {n} Befehle"),
            };
            let mut preview = commands
                .iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n");
            if !created.is_empty() {
                preview.push_str(&format!("\nneu: {}", created.join(", ")));
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
                preview.push("keine Änderungen am Code".into());
            }
            for d in &generation.diagnostics {
                preview.push(format!("{:?}: {}", d.severity, d.issue));
            }
            Ok(proposal(
                "Projekt speichern und Code erzeugen".into(),
                preview.join("\n"),
                Action::GenerateCode,
            ))
        }
        "cargo" => {
            let tool = match text(input, "tool")? {
                "check" => crate::tools::Tool::Check,
                "clippy" => crate::tools::Tool::Clippy,
                "test" => crate::tools::Tool::Test,
                other => return Err(format!("cargo {other} ist nicht erlaubt")),
            };
            Ok(proposal(
                format!("{} ausführen", tool.command_line()),
                format!("{} im Projektordner", tool.command_line()),
                Action::Cargo { tool },
            ))
        }
        _ => Err(format!("{name} ist ein Lese-Werkzeug")),
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
                    "{path} wurde seit der Vorschau geändert; neu lesen und erneut vorschlagen"
                ));
            }
            ws.write_text(path, content)
                .map_err(|e| format!("{path}: {e}"))?;
            Ok(format!("{path} geschrieben"))
        }
        Action::Designer { command } => ws.apply_command(command.clone()),
        Action::GenerateCode => {
            let written = ws.save_and_generate()?;
            Ok(if written.is_empty() {
                "Code ist aktuell".into()
            } else {
                format!("geschrieben: {}", written.join(", "))
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
