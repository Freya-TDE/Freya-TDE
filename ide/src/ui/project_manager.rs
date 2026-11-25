//! The „Projekt-Manager“ tab of the project window: every project of the
//! user's list as a tree (main projects with their subprojects), open ones
//! marked with `*`, the active one highlighted. The context menu opens,
//! activates and closes projects, assigns and hides them, creates projects
//! from a `Cargo.toml` and edits the Git settings. The logic is
//! [`crate::projects`] and [`Ide`]'s project functions.

use crate::tools::NoConsole;
use crate::tr;
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
                item = item.with_color(MISSING).with_badge(&tr!("pm.missing"));
            } else if active == Some(node.path.as_path()) {
                item = item.with_color(ACTIVE).with_badge(&tr!("pm.active_badge"));
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
            items.push(TreeItem::new("", &tr!("pm.empty")));
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
                (false, _) => menu.item("pm.open", &tr!("menu.pm.open"), None),
                (true, false) => menu.item("pm.activate", &tr!("menu.pm.activate"), None),
                (true, true) => menu.disabled_item("pm.activate", &tr!("pm.active")),
            };
            if open && !main {
                menu = menu.item("pm.close", &tr!("menu.pm.close"), None);
            }
            menu = menu
                .separator()
                .item("pm.add_sub", &tr!("menu.pm.add_sub"), None)
                .item("pm.assign", &tr!("menu.pm.assign"), None)
                .item("pm.discover", &tr!("menu.pm.discover"), None)
                .separator()
                .item("pm.hide", &tr!("menu.pm.hide"), None)
                .item("pm.remove", &tr!("menu.pm.remove"), None)
                .separator();
            menu = if active {
                menu.item("pm.git", &tr!("menu.pm.git"), None)
            } else {
                menu.disabled_item("pm.git", &tr!("pm.git_inactive"))
            };
            menu = menu.separator();
        }
        menu.item("pm.cargo", &tr!("menu.pm.cargo"), None).item(
            "pm.add_main",
            &tr!("menu.pm.add_main"),
            None,
        )
    }

    /// Handles a menu entry `pm.*`; `false` if it is not one.
    pub fn menu(&self, ide: &Ide, window: &Widget, id: &str) -> bool {
        let target = self.menu_target.borrow().clone();
        let report = |result: Result<(), crate::projects::ProjectError>| {
            if let Err(e) = result {
                dialog::error(
                    Some(window),
                    &tr!("menu.window.project_manager"),
                    &e.to_string(),
                );
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
                let filters = [FileFilter::new(&tr!("pm.filter"), &["*.ptdr"])];
                if let Some(file) = dialog::open_file(Some(window), &tr!("pm.add_title"), &filters)
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
                        ide.set_status(&tr!("pm.found", count = added.len()));
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
                        &tr!("pm.remove_title"),
                        &tr!("pm.remove_question", name = path.display()),
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
        let mut menu = Menu::new().item("pm.parent:", &tr!("pm.no_parent"), None);
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
                &tr!("pm.sub_of", name = entry.name),
                None,
            );
        }
        let size = self.tree.size();
        let _ = window;
        self.tree.widget().popup_menu(&menu, size.width / 3, 20);
    }

    /// „Projekt aus Cargo.toml erstellen …“ with a preview.
    fn create_from_cargo(&self, ide: &Ide, window: &Widget) {
        let Some(dir) = dialog::choose_folder(Some(window), &tr!("pm.choose_cargo")) else {
            return;
        };
        let plan = match ide.plan_projects_from_cargo(&dir, true) {
            Ok(plan) => plan,
            Err(e) => {
                dialog::error(Some(window), &tr!("pm.from_cargo"), &e.to_string());
                return;
            }
        };
        let mut plan = plan;
        if plan.len() > 1
            && !dialog::confirm(
                Some(window),
                &tr!("pm.from_cargo"),
                &tr!("pm.workspace_members", count = plan.len() - 1),
            )
        {
            plan.truncate(1);
        }
        let lines: Vec<String> = plan
            .iter()
            .map(|p| {
                let what = if p.exists {
                    tr!("pm.exists")
                } else {
                    tr!("pm.new")
                };
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
            &tr!("pm.from_cargo"),
            &format!("{}\n\n{}", lines.join("\n"), tr!("pm.managed_only")),
        ) {
            return;
        }
        match ide.create_projects_from_cargo(&plan) {
            Ok(written) => {
                ide.set_status(&tr!("pm.written", count = written.len()));
                if dialog::confirm(
                    Some(window),
                    &tr!("pm.from_cargo"),
                    &tr!("pm.open_now", name = plan[0].name),
                ) {
                    ide.open_project(&plan[0].ptdr);
                }
            }
            Err(e) => dialog::error(Some(window), &tr!("pm.from_cargo"), &e.to_string()),
        }
    }

    /// „Git-Einstellungen …“ of the active project: personal ones into
    /// `.git/config`, shared ones into the `.ptdr`.
    fn git_settings(&self, ide: &Ide, window: &Widget) {
        let Some(repo) = ide.repo() else {
            dialog::error(Some(window), &tr!("pm.git_settings"), &tr!("pm.no_repo"));
            return;
        };
        let state = repo.identity().unwrap_or_default();
        let hint = |value: &Option<String>| match value {
            Some(v) => tr!("pm.empty_global", value = v),
            None => tr!("pm.empty_unset"),
        };
        let title = &tr!("pm.git_settings");
        let Some(name) = ask(
            window,
            title,
            &format!("{}{}", tr!("pm.author"), hint(&state.effective.name)),
            state.local.name.as_deref().unwrap_or_default(),
        ) else {
            return;
        };
        let Some(email) = ask(
            window,
            title,
            &format!("{}{}", tr!("pm.email"), hint(&state.effective.email)),
            state.local.email.as_deref().unwrap_or_default(),
        ) else {
            return;
        };
        let Some(signing_key) = ask(
            window,
            title,
            &format!(
                "{}{}",
                tr!("pm.signing_key"),
                hint(&state.effective.signing_key)
            ),
            state.local.signing_key.as_deref().unwrap_or_default(),
        ) else {
            return;
        };
        let sign_now = state
            .local
            .sign_commits
            .map(|b| if b { tr!("pm.yes") } else { tr!("pm.no") });
        let Some(sign) = ask(
            window,
            title,
            &tr!("pm.sign_question"),
            &sign_now.unwrap_or_default(),
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
                        .no_console()
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
            &tr!("pm.remote_question"),
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
            &tr!("pm.branch_question"),
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
            ide.set_status(&tr!("pm.git_saved"));
        }
    }
}
