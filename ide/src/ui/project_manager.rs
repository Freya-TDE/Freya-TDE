//! The „Projekt-Manager“ tab of the project window: every project of the
//! user's list as a tree (main projects with their subprojects), open ones
//! marked with `*`, the active one highlighted. The context menu opens,
//! activates and closes projects, assigns and hides them, creates projects
//! from a `Cargo.toml` and edits the Git settings. The logic is
//! [`crate::projects`] and [`Ide`]'s project functions.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use rncl::prelude::*;
use ruf::Color;
use ruf::dialog::{self, FileFilter};

use super::Ide;
use crate::git::{GitOp, Identity};
use crate::projects::ProjectNode;

const ACTIVE: Color = Color::rgb(0x3d, 0x8b, 0xd9);
const MISSING: Color = Color::rgb(0x88, 0x88, 0x88);

pub struct ProjectManager {
    pub tree: RnclTreeView,
    /// The project the context menu was opened for.
    menu_target: RefCell<Option<PathBuf>>,
}

fn items(nodes: &[ProjectNode], active: Option<&Path>) -> Vec<TreeItem> {
    nodes
        .iter()
        .map(|node| {
            let text = if node.open {
                format!("{} *", node.name)
            } else {
                node.name.clone()
            };
            let mut item = TreeItem::new(&node.path.to_string_lossy(), &text);
            if !node.exists {
                item = item.with_color(MISSING).with_badge("fehlt");
            } else if active == Some(node.path.as_path()) {
                item = item.with_color(ACTIVE).with_badge("aktiv");
            }
            item.children = items(&node.children, active);
            item
        })
        .collect()
}

/// `None` for an empty answer, `Some(None)` when cancelled.
fn ask(parent: &Widget, title: &str, label: &str, text: &str) -> Option<Option<String>> {
    dialog::input(Some(parent), title, label, text).map(|t| {
        let t = t.trim().to_owned();
        (!t.is_empty()).then_some(t)
    })
}

impl ProjectManager {
    pub fn build(page: &RnclPanel) -> Self {
        let tree = RnclTreeView::new(page);
        let t = tree.clone();
        page.widget()
            .on_resized(move |w, h| t.set_geometry(0, 0, w, h));
        Self {
            tree,
            menu_target: RefCell::new(None),
        }
    }

    pub fn refresh(&self, ide: &Ide) {
        let mut items = items(&ide.project_tree(), ide.active_project().as_deref());
        if items.is_empty() {
            items.push(TreeItem::new(
                "",
                "Noch keine Projekte – Rechtsklick: aus Cargo.toml erstellen",
            ));
        }
        // Aufgeklappt bleibt, was der Nutzer nicht zugeklappt hat.
        let old = self.tree.items();
        fn keep(new: &mut [TreeItem], old: &[TreeItem]) {
            for item in new.iter_mut() {
                if let Some(previous) = old.iter().find(|o| o.id == item.id) {
                    item.expanded = previous.expanded;
                    keep(&mut item.children, &previous.children);
                }
            }
        }
        keep(&mut items, &old);
        if items != old {
            self.tree.set_items(items);
        }
    }

    pub fn context_menu(&self, ide: &Ide, id: &str) -> Menu {
        *self.menu_target.borrow_mut() = (!id.is_empty()).then(|| PathBuf::from(id));
        let mut menu = Menu::new();
        if !id.is_empty() {
            let path = PathBuf::from(id);
            let open = ide.open_projects().contains(&path);
            let active = ide.active_project().as_ref() == Some(&path);
            let main = ide.main_project().as_ref() == Some(&path);
            menu = match (open, active) {
                (false, _) => menu.item("pm.open", "Öffnen", None),
                (true, false) => menu.item("pm.activate", "Aktivieren", None),
                (true, true) => menu.disabled_item("pm.activate", "Aktiv"),
            };
            if open && !main {
                menu = menu.item("pm.close", "Schließen", None);
            }
            menu = menu
                .separator()
                .item("pm.add_sub", "Unterprojekt hinzufügen …", None)
                .item("pm.assign", "Zuordnen …", None)
                .item("pm.discover", "Unterprojekte suchen", None)
                .separator()
                .item("pm.hide", "Ausblenden", None)
                .item("pm.remove", "Aus der Liste entfernen", None)
                .separator();
            menu = if active {
                menu.item("pm.git", "Git-Einstellungen …", None)
            } else {
                menu.disabled_item("pm.git", "Git-Einstellungen (nur aktives Projekt)")
            };
            menu = menu.separator();
        }
        menu.item("pm.cargo", "Projekt aus Cargo.toml erstellen …", None)
            .item("pm.add_main", "Projekt hinzufügen …", None)
    }

    /// Handles a menu entry `pm.*`; `false` if it is not one.
    pub fn menu(&self, ide: &Ide, window: &Widget, id: &str) -> bool {
        let target = self.menu_target.borrow().clone();
        let report = |result: Result<(), crate::projects::ProjectError>| {
            if let Err(e) = result {
                dialog::error(Some(window), "Projekt-Manager", &e.to_string());
            }
        };
        match id {
            "pm.open" => {
                if let Some(path) = target {
                    ide.open_project(&path);
                }
            }
            "pm.activate" => {
                if let Some(path) = target {
                    ide.activate_project(&path);
                }
            }
            "pm.close" => {
                if let Some(path) = target {
                    ide.close_project(&path);
                }
            }
            "pm.add_sub" | "pm.add_main" => {
                let filters = [FileFilter::new("Freya-TDE-Projekt", &["*.ptdr"])];
                if let Some(file) = dialog::open_file(Some(window), "Projekt hinzufügen", &filters)
                {
                    let parent = (id == "pm.add_sub").then_some(target).flatten();
                    report(ide.edit_projects(|list| list.add(&file, parent.as_deref())));
                }
            }
            "pm.assign" => {
                if let Some(path) = target {
                    self.assign_menu(ide, window, &path);
                }
            }
            "pm.discover" => {
                if let Some(path) = target {
                    report(ide.edit_projects(|list| {
                        let main = list.main_of(&path);
                        let added = list.discover(&main);
                        ide.set_status(&format!("{} Unterprojekte gefunden.", added.len()));
                        Ok(())
                    }));
                }
            }
            "pm.hide" => {
                if let Some(path) = target {
                    report(ide.edit_projects(|list| list.set_hidden(&path, true)));
                }
            }
            "pm.remove" => {
                if let Some(path) = target
                    && dialog::confirm(
                        Some(window),
                        "Aus der Liste entfernen",
                        &format!(
                            "{} aus der Projektliste entfernen? Die Dateien bleiben erhalten.",
                            path.display()
                        ),
                    )
                {
                    report(ide.edit_projects(|list| list.remove(&path)));
                }
            }
            "pm.cargo" => self.create_from_cargo(ide, window),
            "pm.git" => self.git_settings(ide, window),
            other => {
                let Some(parent) = other.strip_prefix("pm.parent:") else {
                    return false;
                };
                if let Some(path) = target {
                    let parent = (!parent.is_empty()).then(|| PathBuf::from(parent));
                    report(ide.edit_projects(|list| list.set_parent(&path, parent.as_deref())));
                }
            }
        }
        true
    }

    /// „Zuordnen …“: choose the main project (or none).
    fn assign_menu(&self, ide: &Ide, window: &Widget, path: &Path) {
        let list = ide.project_list();
        let mut menu = Menu::new().item("pm.parent:", "Kein Unterprojekt (Hauptprojekt)", None);
        // Nicht sich selbst oder eigene Unterprojekte (das wäre ein Zyklus).
        let below = |candidate: &Path| {
            let mut current = Some(candidate.to_path_buf());
            while let Some(p) = current {
                if p == path {
                    return true;
                }
                current = list.get(&p).and_then(|e| e.parent.clone());
            }
            false
        };
        let mut any = false;
        for entry in list
            .entries()
            .iter()
            .filter(|e| !e.hidden && !below(&e.path))
        {
            if !any {
                menu = menu.separator();
                any = true;
            }
            menu = menu.item(
                &format!("pm.parent:{}", entry.path.display()),
                &format!("Unterprojekt von {}", entry.name),
                None,
            );
        }
        let size = self.tree.size();
        let _ = window;
        self.tree.widget().popup_menu(&menu, size.width / 3, 20);
    }

    /// „Projekt aus Cargo.toml erstellen …“ with a preview.
    fn create_from_cargo(&self, ide: &Ide, window: &Widget) {
        let Some(dir) = dialog::choose_folder(Some(window), "Ordner mit Cargo.toml wählen") else {
            return;
        };
        let plan = match ide.plan_projects_from_cargo(&dir, true) {
            Ok(plan) => plan,
            Err(e) => {
                dialog::error(Some(window), "Projekt aus Cargo.toml", &e.to_string());
                return;
            }
        };
        let mut plan = plan;
        if plan.len() > 1
            && !dialog::confirm(
                Some(window),
                "Projekt aus Cargo.toml",
                &format!(
                    "Der Workspace hat {} Mitglieder. Für sie ebenfalls Projekte anlegen (als Unterprojekte)?",
                    plan.len() - 1
                ),
            )
        {
            plan.truncate(1);
        }
        let lines: Vec<String> = plan
            .iter()
            .map(|p| {
                let what = if p.exists { "vorhanden" } else { "neu" };
                format!(
                    "• {} (Rust {}, {what}): {}",
                    p.name,
                    p.rust_version,
                    p.ptdr.display()
                )
            })
            .collect();
        if !dialog::confirm(
            Some(window),
            "Projekt aus Cargo.toml",
            &format!(
                "{}\n\nDiese Projekte werden nur verwaltet: Freya TDE erzeugt keinen Code darin.\nAnlegen?",
                lines.join("\n")
            ),
        ) {
            return;
        }
        match ide.create_projects_from_cargo(&plan) {
            Ok(written) => {
                ide.set_status(&format!("{} Projektdatei(en) angelegt.", written.len()));
                if dialog::confirm(
                    Some(window),
                    "Projekt aus Cargo.toml",
                    &format!("{} jetzt öffnen?", plan[0].name),
                ) {
                    ide.open_project(&plan[0].ptdr);
                }
            }
            Err(e) => dialog::error(Some(window), "Projekt aus Cargo.toml", &e.to_string()),
        }
    }

    /// „Git-Einstellungen …“ of the active project: personal ones into
    /// `.git/config`, shared ones into the `.ptdr`.
    fn git_settings(&self, ide: &Ide, window: &Widget) {
        let Some(repo) = ide.repo() else {
            dialog::error(
                Some(window),
                "Git-Einstellungen",
                "Das Projekt liegt in keinem Git-Repository.",
            );
            return;
        };
        let state = repo.identity().unwrap_or_default();
        let hint = |value: &Option<String>| match value {
            Some(v) => format!(" (leer = global: {v})"),
            None => " (leer = nicht gesetzt)".to_owned(),
        };
        let title = "Git-Einstellungen";
        let Some(name) = ask(
            window,
            title,
            &format!("Autor{}", hint(&state.effective.name)),
            state.local.name.as_deref().unwrap_or_default(),
        ) else {
            return;
        };
        let Some(email) = ask(
            window,
            title,
            &format!("E-Mail{}", hint(&state.effective.email)),
            state.local.email.as_deref().unwrap_or_default(),
        ) else {
            return;
        };
        let Some(signing_key) = ask(
            window,
            title,
            &format!(
                "Signierschlüssel (GPG-ID){}",
                hint(&state.effective.signing_key)
            ),
            state.local.signing_key.as_deref().unwrap_or_default(),
        ) else {
            return;
        };
        let sign_now = state
            .local
            .sign_commits
            .map(|b| if b { "ja" } else { "nein" });
        let Some(sign) = ask(
            window,
            title,
            "Commits signieren? ja / nein (leer = globale Einstellung)",
            sign_now.unwrap_or_default(),
        ) else {
            return;
        };
        let sign_commits =
            sign.map(|s| matches!(s.to_lowercase().as_str(), "ja" | "j" | "yes" | "y" | "true"));
        // Geteilt (steht in der .ptdr): Remote und Hauptzweig.
        let shared = ide
            .session()
            .document()
            .project()
            .git
            .clone()
            .unwrap_or_default();
        let remote_now = shared.remote.clone().or_else(|| {
            repo.remotes()
                .ok()
                .and_then(|r| r.first().cloned())
                .and_then(|name| {
                    std::process::Command::new(crate::git::program())
                        .arg("-C")
                        .arg(repo.root())
                        .args(["remote", "get-url", &name])
                        .output()
                        .ok()
                        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
                })
        });
        let Some(remote) = ask(
            window,
            title,
            "Remote des Projekts (geteilt, steht in der .ptdr)",
            remote_now.as_deref().unwrap_or_default(),
        ) else {
            return;
        };
        let branch_now = shared
            .branch
            .clone()
            .or_else(|| ide.git_status().and_then(|s| s.branch));
        let Some(branch) = ask(
            window,
            title,
            "Hauptzweig (geteilt, steht in der .ptdr)",
            branch_now.as_deref().unwrap_or_default(),
        ) else {
            return;
        };
        ide.git_now(GitOp::SetIdentity(Identity {
            name,
            email,
            signing_key,
            sign_commits,
        }));
        let git = (remote.is_some() || branch.is_some())
            .then_some(tdr_core::GitSettings { remote, branch });
        if let Err(e) = ide.set_project_git(git) {
            dialog::error(Some(window), title, &e);
        } else {
            ide.set_status("Git-Einstellungen gespeichert.");
        }
    }
}
