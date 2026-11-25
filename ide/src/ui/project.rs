//! Project window: the project folder as a tree (file manager). Double
//! click opens a file (a form in the designer together with its `.rs`), the
//! context menu creates, imports, copies, renames and deletes. The logic is
//! [`crate::files`]; the tree is rescanned every few seconds, so changes
//! made outside Freya TDE show up as well.

use crate::tr;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

use rncl::prelude::*;
use ruf::Timer;
use ruf::dialog;

use super::project_manager::ProjectManager;
use super::{Ide, with, with0};
use crate::desktop::window;
use crate::files::{self, Entry, EntryKind, FileError, FileOp, Plan, ScanOptions};
use crate::git::Change;

/// Git changes per project path (relative, `/`).
pub type Changes = BTreeMap<String, Change>;

/// Id of the project folder itself in the tree.
pub const ROOT: &str = ".";

/// Rescan interval while the window is visible.
const RESCAN: i32 = 2000;

const BOTTOM: i32 = 30;

/// The tree with Git marks: a status letter on changed files, a dot on
/// folders that contain changes (like VS Code).
fn items(entries: &[Entry], changes: &Changes) -> Vec<TreeItem> {
    entries
        .iter()
        .map(|e| {
            let text = if e.kind == EntryKind::Folder {
                format!("{}/", e.name)
            } else {
                e.name.clone()
            };
            let mut item = TreeItem::new(&e.path, &text);
            item.children = items(&e.children, changes);
            // Neue Ordner zugeklappt, nur `src` offen.
            item.expanded = e.path == "src";
            let change = match e.kind {
                EntryKind::File => changes.get(&e.path).copied(),
                EntryKind::Folder => {
                    let prefix = format!("{}/", e.path);
                    changes
                        .range(prefix.clone()..)
                        .take_while(|(p, _)| p.starts_with(&prefix))
                        .map(|(_, c)| *c)
                        .max_by_key(|c| *c == Change::Conflict)
                }
            };
            if let Some(change) = change {
                item.color = Some(super::git_window::change_color(change));
                item.badge = Some(if e.kind == EntryKind::File {
                    change.letter().to_string()
                } else {
                    "•".to_owned()
                });
            }
            item
        })
        .collect()
}

/// Keeps the expansion of nodes that exist before and after a rebuild.
fn keep_expansion(new: &mut [TreeItem], old: &[TreeItem]) {
    fn find<'a>(items: &'a [TreeItem], id: &str) -> Option<&'a TreeItem> {
        items
            .iter()
            .find_map(|i| (i.id == id).then_some(i).or_else(|| find(&i.children, id)))
    }
    for item in new.iter_mut() {
        if let Some(previous) = find(old, &item.id) {
            item.expanded = previous.expanded;
        }
        keep_expansion(&mut item.children, old);
    }
}

fn find<'a>(entries: &'a [Entry], path: &str) -> Option<&'a Entry> {
    entries.iter().find_map(|e| {
        (e.path == path)
            .then_some(e)
            .or_else(|| find(&e.children, path))
    })
}

/// Parent folder of a relative path (`""` = project folder).
fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

fn name(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}

fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_owned()
    } else {
        format!("{dir}/{name}")
    }
}

/// Warnings of a plan as lines for a question.
fn warnings(plan: &Plan) -> String {
    plan.issues
        .iter()
        .map(|i| format!("\n\n{}", i.detail))
        .collect()
}

/// The CT-Editor program: `FREYA_TDE_CT_EDITOR`, next to Freya TDE, installed
/// next to it by the DS-Launcher, in the repository (development: the newer of
/// the release and debug build, so an old build never wins) or in the `PATH`.
fn ct_editor() -> PathBuf {
    if let Some(path) = std::env::var_os("FREYA_TDE_CT_EDITOR").filter(|p| !p.is_empty()) {
        return PathBuf::from(path);
    }
    let file = format!("ct-editor{}", std::env::consts::EXE_SUFFIX);
    let found = std::env::current_exe()
        .ok()
        .and_then(|exe| installed_ct_editor(&exe, &file));
    if let Some(found) = found {
        return found;
    }
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../CT-Editor/target");
    newest(&[
        repo.join("release").join(&file),
        repo.join("debug").join(&file),
    ])
    .unwrap_or_else(|| PathBuf::from(file))
}

/// The CT-Editor next to Freya TDE (`exe`): in the same folder, or as its own
/// program beside Freya's folder – the DS-Launcher installs into
/// `<prefix>/<id>/bin` (Linux: `/opt/dragon-software/ct-editor`) or
/// `<prefix>\<name>\bin` (Windows: `…\Dragon Software\CT-Editor`).
fn installed_ct_editor(exe: &std::path::Path, file: &str) -> Option<PathBuf> {
    let bin = exe.parent()?;
    let beside = bin.join(file);
    if beside.is_file() {
        return Some(beside);
    }
    let prefix = bin.parent()?.parent()?;
    ["ct-editor", "CT-Editor"]
        .iter()
        .map(|dir| prefix.join(dir).join("bin").join(file))
        .find(|p| p.is_file())
}

/// The most recently built of `candidates` that exist.
fn newest(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates
        .iter()
        .filter_map(|p| Some((std::fs::metadata(p).ok()?.modified().ok()?, p)))
        .max_by_key(|(time, _)| *time)
        .map(|(_, p)| p.clone())
}

pub struct ProjectWindow {
    /// „Dateien“.
    pub window: RnclWindow,
    pub pane: super::dock::DockPane,
    /// „Projekt-Manager“: a window of its own, docked into „Dateien“ by
    /// default (see `Desktop::standard`).
    pub manager_window: RnclWindow,
    pub manager_pane: super::dock::DockPane,
    pub manager: ProjectManager,
    tree: RnclTreeView,
    show_generated: RnclCheckBox,
    show_hidden: RnclCheckBox,
    /// The last scan.
    entries: RefCell<Vec<Entry>>,
    /// The node the context menu was opened for.
    menu_target: RefCell<String>,
    /// Copied path (relative), pasted with „Einfügen“.
    clipboard: RefCell<Option<String>>,
    timer: Timer,
}

impl ProjectWindow {
    pub fn build() -> Self {
        let window = RnclWindow::new(&tr!("project.title"));
        let pane =
            super::dock::DockPane::with_body(&window, window::PROJECT, &tr!("menu.window.project"));
        let files = pane.body().clone();
        let manager_window = RnclWindow::new(&tr!("menu.window.project_manager"));
        let manager_pane = super::dock::DockPane::with_body(
            &manager_window,
            window::PROJECT_MANAGER,
            &tr!("menu.window.project_manager"),
        );
        let manager = ProjectManager::build(manager_pane.body());
        let tree = RnclTreeView::new(&files);
        let show_generated = RnclCheckBox::new(&files, &tr!("project.generated"));
        show_generated
            .widget()
            .set_tool_tip(&tr!("project.generated_tip"));
        let show_hidden = RnclCheckBox::new(&files, &tr!("project.hidden"));
        show_hidden
            .widget()
            .set_tool_tip(&tr!("project.hidden_tip"));
        {
            let (t, g, h) = (tree.clone(), show_generated.clone(), show_hidden.clone());
            files.widget().on_resized(move |width, height| {
                t.set_geometry(0, 0, width, height - BOTTOM);
                g.set_geometry(6, height - BOTTOM + 3, 110, BOTTOM - 6);
                h.set_geometry(122, height - BOTTOM + 3, 110, BOTTOM - 6);
            });
        }
        Self {
            window,
            pane,
            manager_window,
            manager_pane,
            manager,
            tree,
            show_generated,
            show_hidden,
            entries: RefCell::new(Vec::new()),
            menu_target: RefCell::new(String::new()),
            clipboard: RefCell::new(None),
            timer: Timer::new(),
        }
    }

    /// A project manager entry chosen in another window's menu.
    pub fn menu_from_main(&self, ide: &Ide, id: &str) {
        self.manager.menu(ide, self.window.widget(), id);
    }

    pub fn options(&self) -> ScanOptions {
        ScanOptions {
            show_generated: self.show_generated.is_checked(),
            show_hidden: self.show_hidden.is_checked(),
        }
    }

    pub fn set_options(&self, options: ScanOptions) {
        self.show_generated.set_checked(options.show_generated);
        self.show_hidden.set_checked(options.show_hidden);
    }

    /// Rescans the project folder and updates the tree if anything changed.
    pub fn refresh(&self, ide: &Ide) {
        self.manager.refresh(ide);
        let dir = ide.session().project_dir();
        let root = match &dir {
            Some(dir) => {
                let entries = files::scan(dir, &self.options());
                let title = dir
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| dir.display().to_string());
                let mut root = TreeItem::new(ROOT, &format!("{title}/"));
                root.children = items(&entries, &ide.git_changes());
                *self.entries.borrow_mut() = entries;
                root
            }
            None => {
                self.entries.borrow_mut().clear();
                TreeItem::new("", &tr!("project.unsaved"))
            }
        };
        let mut items = vec![root];
        keep_expansion(&mut items, &self.tree.items());
        if items != self.tree.items() {
            self.tree.set_items(items);
        }
        self.window.set_title(&match &dir {
            Some(dir) => format!("{} – {}", tr!("project.title"), dir.display()),
            None => tr!("project.title"),
        });
    }

    pub fn entries(&self) -> Vec<Entry> {
        self.entries.borrow().clone()
    }

    fn kind(&self, id: &str) -> Option<EntryKind> {
        if id == ROOT {
            return Some(EntryKind::Folder);
        }
        find(&self.entries.borrow(), id).map(|e| e.kind)
    }

    /// The folder new files go to when `id` is selected.
    fn folder_of(&self, id: &str) -> String {
        match self.kind(id) {
            _ if id == ROOT => String::new(),
            Some(EntryKind::Folder) => id.to_owned(),
            _ => parent(id).to_owned(),
        }
    }

    fn context_menu(&self, id: &str) -> Menu {
        let kind = self.kind(id);
        let file = kind == Some(EntryKind::File);
        let item = id != ROOT;
        let mut menu = Menu::new();
        if file {
            menu = menu.item("files.open", &tr!("menu.files.open"), None);
            if name(id) == "Cargo.toml" {
                menu = menu.item("files.ct_editor", &tr!("menu.files.ct_editor"), None);
            }
            menu = menu.separator();
        }
        menu = menu
            .item("files.new_file", &tr!("menu.files.new_file"), None)
            .item("files.new_folder", &tr!("menu.files.new_folder"), None)
            .item("files.import", &tr!("menu.files.import"), None)
            .separator();
        menu = if item {
            menu.item("files.copy", &tr!("menu.files.copy"), None)
        } else {
            menu.disabled_item("files.copy", &tr!("menu.files.copy"))
        };
        menu = if self.clipboard.borrow().is_some() {
            menu.item("files.paste", &tr!("menu.files.paste"), None)
        } else {
            menu.disabled_item("files.paste", &tr!("menu.files.paste"))
        };
        menu = if item {
            menu.item("files.rename", &tr!("menu.files.rename"), None)
                .item("files.delete", &tr!("menu.files.delete"), None)
        } else {
            menu.disabled_item("files.rename", &tr!("menu.files.rename"))
                .disabled_item("files.delete", &tr!("menu.files.delete"))
        };
        menu.separator()
            .item("files.refresh", &tr!("menu.files.refresh"), None)
    }

    // --- Actions (context menu) ------------------------------------------------------

    fn parent_widget(&self) -> Option<&Widget> {
        Some(self.window.widget())
    }

    fn ask_name(&self, title: &str, label: &str, text: &str) -> Option<String> {
        dialog::input(self.parent_widget(), title, label, text)
            .map(|n| n.trim().to_owned())
            .filter(|n| !n.is_empty())
    }

    /// Runs `op` after showing its warnings (if any) for confirmation.
    fn run(&self, ide: &Ide, title: &str, op: FileOp) -> bool {
        let Some(plan) = ide.plan_file_op(&op) else {
            return false;
        };
        if let Some(error) = plan.errors().next() {
            dialog::error(self.parent_widget(), title, &error.detail);
            return false;
        }
        if !plan.issues.is_empty()
            && !dialog::confirm(
                self.parent_widget(),
                title,
                &format!("{}{}", tr!("project.continue_anyway"), warnings(&plan)),
            )
        {
            return false;
        }
        match ide.file_op(&op) {
            Ok(_) => true,
            Err(e) => {
                dialog::error(self.parent_widget(), title, &e.to_string());
                false
            }
        }
    }

    fn new_entry(&self, ide: &Ide, folder: bool) {
        let target = self.menu_target.borrow().clone();
        let dir = self.folder_of(&target);
        let (title, suggestion) = if folder {
            (tr!("project.new_folder"), tr!("project.new_folder_name"))
        } else {
            (tr!("project.new_file"), tr!("project.new_file_name"))
        };
        let Some(name) = self.ask_name(&title, &tr!("project.name_with_folders"), &suggestion)
        else {
            return;
        };
        let path = join(&dir, &name);
        let op = if folder {
            FileOp::NewFolder { path: path.clone() }
        } else {
            FileOp::NewFile {
                path: path.clone(),
                text: String::new(),
            }
        };
        if self.run(ide, &title, op) {
            self.tree.select(Some(&path));
            if !folder {
                ide.open_project_file(&path);
            }
        }
    }

    fn import(&self, ide: &Ide) {
        let target = self.menu_target.borrow().clone();
        let dir = self.folder_of(&target);
        let Some(source) = dialog::open_file(self.parent_widget(), &tr!("project.import"), &[])
        else {
            return;
        };
        let Some(file) = source.file_name().and_then(|n| n.to_str()) else {
            return;
        };
        let Some(root) = ide.session().project_dir() else {
            return;
        };
        let to = files::copy_name(&root, &dir, file);
        if self.run(
            ide,
            &tr!("project.import"),
            FileOp::Import {
                source,
                to: to.clone(),
            },
        ) {
            self.tree.select(Some(&to));
        }
    }

    fn paste(&self, ide: &Ide) {
        let Some(from) = self.clipboard.borrow().clone() else {
            return;
        };
        let target = self.menu_target.borrow().clone();
        let dir = self.folder_of(&target);
        let Some(root) = ide.session().project_dir() else {
            return;
        };
        let to = files::copy_name(&root, &dir, name(&from));
        if self.run(
            ide,
            &tr!("menu.files.paste"),
            FileOp::Copy {
                from,
                to: to.clone(),
            },
        ) {
            self.tree.select(Some(&to));
        }
    }

    fn rename(&self, ide: &Ide) {
        let from = self.menu_target.borrow().clone();
        let Some(new) = self.ask_name(
            &tr!("project.rename"),
            &tr!("git.ask.new_name"),
            name(&from),
        ) else {
            return;
        };
        let to = join(parent(&from), &new);
        if to != from
            && self.run(
                ide,
                &tr!("project.rename"),
                FileOp::Rename {
                    from,
                    to: to.clone(),
                },
            )
        {
            self.tree.select(Some(&to));
        }
    }

    fn delete(&self, ide: &Ide) {
        let path = self.menu_target.borrow().clone();
        let op = FileOp::Delete {
            path: path.clone(),
            permanent: false,
        };
        let Some(plan) = ide.plan_file_op(&op) else {
            return;
        };
        if let Some(error) = plan.errors().next() {
            dialog::error(self.parent_widget(), &tr!("project.delete"), &error.detail);
            return;
        }
        let files = plan.changes.first().map_or(0, |c| c.files);
        let what = if self.kind(&path) == Some(EntryKind::Folder) {
            tr!("project.the_folder", path = path, files = files)
        } else {
            path.clone()
        };
        let unsaved = if ide.has_unsaved_below(&path) {
            tr!("project.unsaved_lost")
        } else {
            String::new()
        };
        if !dialog::confirm(
            self.parent_widget(),
            &tr!("project.delete"),
            &format!(
                "{}{unsaved}{}",
                tr!("project.to_trash", what = what),
                warnings(&plan)
            ),
        ) {
            return;
        }
        match ide.file_op(&op) {
            Ok(_) => {}
            Err(FileError::Io { detail, .. }) => {
                // Kein Papierkorb (z. B. Netzlaufwerk): endgültig löschen?
                if dialog::confirm(
                    self.parent_widget(),
                    &tr!("project.delete"),
                    &tr!("project.no_trash", detail = detail, path = path),
                ) && let Err(e) = ide.file_op(&FileOp::Delete {
                    path,
                    permanent: true,
                }) {
                    dialog::error(self.parent_widget(), &tr!("project.delete"), &e.to_string());
                }
            }
            Err(e) => dialog::error(self.parent_widget(), &tr!("project.delete"), &e.to_string()),
        }
    }

    fn open_ct_editor(&self, ide: &Ide) {
        let target = self.menu_target.borrow().clone();
        let Some(root) = ide.session().project_dir() else {
            return;
        };
        let dir = root.join(parent(&target));
        let program = ct_editor();
        // Eingebettet: ohne eigenes Menü, Sprache und Darstellung aus den
        // Einstellungen von Freya TDE (Sprache „System“ = LANG).
        let settings = ide.settings();
        let mut command = std::process::Command::new(&program);
        command.arg("--embedded");
        if let Some(lang) = settings.language.code() {
            command.args(["--lang", lang]);
        }
        match command
            .args(["--theme", settings.theme.arg()])
            .arg("--path")
            .arg(&dir)
            .spawn()
        {
            Ok(_) => ide.set_status(&tr!("project.ct_opened", path = dir.display())),
            Err(e) => ide.set_status(&tr!(
                "project.ct_failed",
                program = program.display(),
                error = e
            )),
        }
    }

    pub(super) fn menu(&self, ide: &Ide, id: &str) {
        if self.manager.menu(ide, self.window.widget(), id) {
            return;
        }
        match id {
            "files.open" => {
                let target = self.menu_target.borrow().clone();
                ide.open_project_file(&target);
            }
            "files.ct_editor" => self.open_ct_editor(ide),
            "files.new_file" => self.new_entry(ide, false),
            "files.new_folder" => self.new_entry(ide, true),
            "files.import" => self.import(ide),
            "files.copy" => {
                let target = self.menu_target.borrow().clone();
                ide.set_status(&tr!("project.copied", target = target));
                *self.clipboard.borrow_mut() = Some(target);
            }
            "files.paste" => self.paste(ide),
            "files.rename" => self.rename(ide),
            "files.delete" => self.delete(ide),
            "files.refresh" => self.refresh(ide),
            _ => {}
        }
    }
}

pub fn connect(ide: &Rc<Ide>) {
    let project = &ide.project;
    for check in [&project.show_generated, &project.show_hidden] {
        check.on_toggled(with(ide, |ide, _: bool| ide.project.refresh(ide)));
    }
    project
        .tree
        .on_item_activated(with(ide, |ide, id: String| match ide.project.kind(&id) {
            Some(EntryKind::File) => ide.open_project_file(&id),
            Some(EntryKind::Folder) => {
                ide.project.tree.toggle(&id);
            }
            None => {}
        }));
    let mut context = with(ide, |ide, (id, x, y): (String, i32, i32)| {
        if id.is_empty() {
            return;
        }
        let menu = ide.project.context_menu(&id);
        *ide.project.menu_target.borrow_mut() = id;
        ide.project.tree.widget().popup_menu(&menu, x, y);
    });
    project
        .tree
        .on_context_menu(move |id, x, y| context((id, x, y)));
    // Projekt-Manager: Doppelklick öffnet bzw. aktiviert, Rechtsklick: Menü.
    let manager = &project.manager;
    manager.tree.on_item_activated(with(ide, |ide, id: String| {
        if !id.is_empty() {
            ide.open_project(std::path::Path::new(&id));
        }
    }));
    let mut manager_context = with(ide, |ide, (id, x, y): (String, i32, i32)| {
        let menu = ide.project.manager.context_menu(ide, &id);
        ide.project.manager.tree.widget().popup_menu(&menu, x, y);
    });
    manager
        .tree
        .on_context_menu(move |id, x, y| manager_context((id, x, y)));
    // Änderungen außerhalb von Freya TDE (CT-Editor, Git, Dateimanager).
    project
        .timer
        .on_timeout(with0(ide, |ide| ide.files_changed_outside()));
    project.timer.start(RESCAN);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_ct_editor_beside_or_installed_next_to_freya() {
        let base = std::env::temp_dir().join(format!("freya-ct-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let exe = base.join("freya-tde/bin/freya-tde");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        assert_eq!(installed_ct_editor(&exe, "ct-editor"), None);

        // Vom DS-Launcher daneben installiert (Linux: <id>, Windows: <Name>).
        for dir in ["ct-editor", "CT-Editor"] {
            let ct = base.join(dir).join("bin/ct-editor");
            std::fs::create_dir_all(ct.parent().unwrap()).unwrap();
            std::fs::write(&ct, "").unwrap();
            assert_eq!(installed_ct_editor(&exe, "ct-editor"), Some(ct));
            std::fs::remove_dir_all(base.join(dir)).unwrap();
        }

        // Im selben Ordner geht vor.
        let beside = base.join("freya-tde/bin/ct-editor");
        std::fs::write(&beside, "").unwrap();
        assert_eq!(installed_ct_editor(&exe, "ct-editor"), Some(beside));
        let _ = std::fs::remove_dir_all(&base);
    }
}
