//! Actions of the main window's tool bar (Rust tools) and of the palette tab
//! "Snippets"; the logic is in [`crate::tools`] and [`crate::snippets`].

use std::path::PathBuf;

use rncl::prelude::*;
use ruf::dialog;

use super::Ide;
use super::panel::Tab;
use crate::output::channel;
use crate::snippets::{Inserted, Snippet, SnippetIssue, SnippetStore};
use crate::tools::{Tool, ToolReport};

/// What a background tool run delivers.
pub enum ToolDone {
    Report(Tool, ToolReport),
    /// What `cargo update` would change (asked before updating).
    UpdatePreview(String),
    Failed(Tool, String),
}

impl Ide {
    // --- Snippets ---------------------------------------------------------

    /// All snippets and which are on the palette tab.
    pub fn snippets(&self) -> SnippetStore {
        self.snippets.borrow().clone()
    }

    fn snippets_changed(&self) {
        if let Some(file) = &self.snippets_file
            && let Err(e) = self.snippets.borrow().save_to(file)
        {
            self.main
                .set_status(&format!("Snippets: {} ({})", e.detail, e.code));
        }
        if let Some(ide) = self.me.upgrade() {
            self.main.build_snippets(&ide);
        }
    }

    /// Adds a snippet or replaces the one named `old`, then saves.
    pub fn put_snippet(&self, old: Option<&str>, snippet: Snippet) -> Result<(), SnippetIssue> {
        self.snippets.borrow_mut().put(old, snippet)?;
        self.snippets_changed();
        Ok(())
    }

    pub fn remove_snippet(&self, name: &str) {
        self.snippets.borrow_mut().remove(name);
        self.snippets_changed();
    }

    /// Puts a snippet on the palette tab or takes it off.
    pub fn toggle_snippet_pinned(&self, name: &str) {
        let result = self.snippets.borrow_mut().toggle_pinned(name);
        match result {
            Ok(_) => self.snippets_changed(),
            Err(e) => self.main.set_status(&format!("{} ({})", e.detail, e.code)),
        }
    }

    /// Inserts a snippet at the cursor of the front code editor (unsaved,
    /// undoable there).
    pub fn insert_snippet(&self, name: &str) -> Result<Inserted, String> {
        let snippet = self
            .snippets
            .borrow()
            .get(name)
            .cloned()
            .ok_or_else(|| format!("kein Snippet „{name}“"))?;
        let (path, text, (line, column)) = self
            .code
            .current_state()
            .ok_or("Kein Code geöffnet: erst eine Datei im Code-Fenster öffnen.")?;
        let done = crate::snippets::insert(&text, line, column, &snippet);
        self.set_code_text(&path, &done.text);
        self.code.set_cursor(&path, done.line, done.column);
        Ok(done)
    }

    pub(super) fn insert_snippet_reported(&self, name: &str) {
        match self.insert_snippet(name) {
            Ok(_) => {
                self.main
                    .set_status(&format!("Snippet „{name}“ eingefügt."));
                self.show_code();
            }
            Err(e) => self.main.set_status(&e),
        }
    }

    // --- Rust-Werkzeuge ---------------------------------------------------

    /// Runs a tool of the tool bar in the background; the result goes to the
    /// output channel "Build" (check/clippy also to the problems).
    pub fn run_tool(&self, tool: Tool) {
        if self.tool_busy.get() || self.building.get() {
            self.main
                .set_status("Es läuft schon ein Werkzeug oder Build.");
            return;
        }
        let Some(dir) = self.session().project_dir() else {
            self.main.set_status("Erst das Projekt speichern.");
            return;
        };
        if !tool.applies(&dir) {
            self.main.set_status(&match tool {
                Tool::SqlxPrepare => "SQLx prepare: das Projekt nutzt kein SQLx.".to_owned(),
                _ => "Das Projekt hat noch keine Cargo.toml (Code erzeugen).".to_owned(),
            });
            return;
        }
        // Prüfen braucht den aktuellen Code; sonst wenigstens alles gespeichert.
        let generate = self.session().document().project().generate;
        let generation = if matches!(tool, Tool::Check | Tool::Clippy) && generate {
            let Some(generation) = self.generate_code() else {
                return;
            };
            Some(generation)
        } else {
            if let Err(e) = self.save_all_code() {
                self.main.set_status(&e);
                return;
            }
            None
        };
        let env = match tool {
            Tool::SqlxPrepare => self
                .database_url()
                .map(|url| vec![("DATABASE_URL".to_owned(), url)])
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let Some(sender) = self.tool_results.borrow().clone() else {
            return;
        };
        self.tool_busy.set(true);
        self.main.set_status(&format!("{} …", tool.command_line()));
        self.output.clear(channel::BUILD);
        self.output.line(
            channel::BUILD,
            &format!("> {} ({})", tool.command_line(), dir.display()),
        );
        std::thread::spawn(move || {
            let done = if tool == Tool::Update {
                match crate::tools::update_preview(&dir) {
                    Ok(text) => ToolDone::UpdatePreview(text),
                    Err(e) => ToolDone::Failed(tool, e.to_string()),
                }
            } else {
                match crate::tools::run(tool, &dir, generation.as_ref(), &env) {
                    Ok(report) => ToolDone::Report(tool, report),
                    Err(e) => ToolDone::Failed(tool, e.to_string()),
                }
            };
            sender.send(done);
        });
    }

    /// Is a tool of the tool bar running (automation, tests)?
    pub fn tool_running(&self) -> bool {
        self.tool_busy.get()
    }

    /// The URL of the first SQLx pool of the project (for `sqlx prepare`).
    fn database_url(&self) -> Option<String> {
        let session = self.session();
        let pool = tdr_core::TypeRef::pack("sqlx", "Pool");
        session.document().forms().iter().find_map(|form| {
            form.components.iter().find_map(|component| {
                match (
                    component.type_ref == pool,
                    component.get("url").map(|v| &v.value),
                ) {
                    (true, Some(tdr_core::Value::Text(url))) if !url.is_empty() => {
                        Some(url.clone())
                    }
                    _ => None,
                }
            })
        })
    }

    pub(super) fn tool_finished(&self, done: ToolDone) {
        self.tool_busy.set(false);
        match done {
            ToolDone::Failed(tool, error) => {
                self.output.line(
                    channel::BUILD,
                    &format!("{} konnte nicht starten: {error}", tool.command_line()),
                );
                self.main.set_status(&format!("{}: {error}", tool.label()));
                self.show_panel(Tab::Output);
            }
            ToolDone::UpdatePreview(text) => {
                self.output.append(channel::BUILD, &text);
                let changes: Vec<&str> = text
                    .lines()
                    .map(str::trim)
                    .filter(|l| {
                        l.starts_with("Updating")
                            || l.starts_with("Adding")
                            || l.starts_with("Removing")
                            || l.starts_with("Downgrading")
                    })
                    .collect();
                if changes.is_empty() {
                    self.main.set_status("cargo update: alles ist aktuell.");
                    return;
                }
                let question = format!(
                    "cargo update würde ändern:\n\n{}\n\nCargo.lock jetzt aktualisieren?",
                    changes.join("\n")
                );
                if dialog::confirm(Some(self.main.window.widget()), "cargo update", &question) {
                    self.run_update_now();
                } else {
                    self.main.set_status("cargo update abgebrochen.");
                }
            }
            ToolDone::Report(tool, report) => {
                let dir = self.session().project_dir();
                if let Some(build) = &report.build {
                    self.set_build_problems(build, dir.as_deref());
                    for m in &build.messages {
                        self.output.append(
                            channel::BUILD,
                            m.rendered.as_deref().unwrap_or(&format!("{}\n", m.message)),
                        );
                    }
                    self.set_build_marks(build, dir.as_deref());
                }
                self.output.append(channel::BUILD, &report.output);
                if tool == Tool::Fmt {
                    for (path, text) in self.code.reload_from_disk() {
                        self.lsp_changed(&path, &text);
                        self.lsp_saved(&[path]);
                    }
                }
                if let Some(page) = &report.open
                    && report.success
                {
                    let _ = crate::tools::open_externally(page);
                }
                let problems = report.build.as_ref().map_or(0, |b| b.messages.len());
                let status = match (report.success, tool) {
                    (true, Tool::Check | Tool::Clippy) if problems == 0 => {
                        format!("{}: keine Meldungen.", tool.label())
                    }
                    (true, Tool::Check | Tool::Clippy) => {
                        format!("{}: {problems} Meldungen (Probleme).", tool.label())
                    }
                    (true, Tool::Fmt) => format!("Formatiert: {} Datei(en).", report.changed.len()),
                    (true, _) => format!("{}: fertig.", tool.label()),
                    (false, _) => format!("{}: fehlgeschlagen (Ausgabe).", tool.label()),
                };
                self.main.set_status(&status);
                if !report.success || problems > 0 {
                    self.show_panel(if problems > 0 {
                        Tab::Problems
                    } else {
                        Tab::Output
                    });
                } else if !matches!(tool, Tool::Check | Tool::Clippy | Tool::Fmt | Tool::Doc) {
                    self.show_panel(Tab::Output);
                }
            }
        }
    }

    /// `cargo update` after the preview was confirmed.
    fn run_update_now(&self) {
        let (Some(dir), Some(sender)) = (
            self.session().project_dir(),
            self.tool_results.borrow().clone(),
        ) else {
            return;
        };
        self.tool_busy.set(true);
        self.main.set_status("cargo update …");
        std::thread::spawn(move || {
            let done = match crate::tools::run(Tool::Update, &dir, None, &[]) {
                Ok(report) => ToolDone::Report(Tool::Update, report),
                Err(e) => ToolDone::Failed(Tool::Update, e.to_string()),
            };
            sender.send(done);
        });
    }
}

/// The snippets file (`None` without configuration directory).
pub(super) fn snippets_file() -> Option<PathBuf> {
    SnippetStore::file()
}
