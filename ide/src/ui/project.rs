//! Project window: the project folder as a tree (file manager). Double
//! click opens a file (a form in the designer together with its `.rs`), the
//! context menu creates, imports, copies, renames and deletes. The logic is
//! [`crate::files`]; the tree is rescanned every few seconds, so changes
//! made outside Freya TDE show up as well.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use rncl::prelude::*;
use ruf::Timer;
use ruf::dialog;

use super::project_manager::ProjectManager;
use super::{Ide, with, with0};
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

/// The CT-Editor program: `FREYA_TDE_CT_EDITOR`, next to Freya TDE, in the
/// repository (development) or in the `PATH`.
fn ct_editor() -> PathBuf {
    if let Some(path) = std::env::var_os("FREYA_TDE_CT_EDITOR").filter(|p| !p.is_empty()) {
        return PathBuf::from(path);
    }
    let file = format!("ct-editor{}", std::env::consts::EXE_SUFFIX);
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf));
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../CT-Editor/target");
    [
        exe_dir.map(|d| d.join(&file)),
        Some(repo.join("release").join(&file)),
        Some(repo.join("debug").join(&file)),
    ]
    .into_iter()
    .flatten()
    .find(|p| p.is_file())
    .unwrap_or_else(|| PathBuf::from(file))
}

pub struct ProjectWindow {
    pub window: RnclWindow,
    /// „Dateien“ and „Projekt-Manager“.
    tabs: RnclTabView,
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
        let window = RnclWindow::new("Projekt");
        let tabs = RnclTabView::new(&window);
        let files = tabs.add_tab("Dateien");
        let manager = ProjectManager::build(&tabs.add_tab("Projekt-Manager"));
        let tree = RnclTreeView::new(&files);
        let show_generated = RnclCheckBox::new(&files, "Generierte");
        show_generated
            .widget()
            .set_tool_tip("Erzeugte Dateien (*.gen.rs) zeigen");
        let show_hidden = RnclCheckBox::new(&files, "Versteckte");
        show_hidden
            .widget()
            .set_tool_tip("Dateien und Ordner mit . am Anfang zeigen");
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
            tabs,
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

    fn layout(&self, width: i32, height: i32) {
        self.tabs.set_geometry(0, 0, width, height);
    }

    /// A project manager entry chosen in another window's menu.
    pub fn menu_from_main(&self, ide: &Ide, id: &str) {
        self.manager.menu(ide, self.window.widget(), id);
    }

    /// Shows the „Projekt-Manager“ tab.
    pub fn show_manager(&self) {
        self.tabs.set_current_index(1);
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
                TreeItem::new("", "(Projekt noch nicht gespeichert)")
            }
        };
        let mut items = vec![root];
        keep_expansion(&mut items, &self.tree.items());
        if items != self.tree.items() {
            self.tree.set_items(items);
        }
        self.window.set_title(&match &dir {
            Some(dir) => format!("Projekt – {}", dir.display()),
            None => "Projekt".to_owned(),
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
            menu = menu.item("files.open", "Öffnen", None);
            if name(id) == "Cargo.toml" {
                menu = menu.item("files.ct_editor", "Mit CT-Editor öffnen", None);
            }
            menu = menu.separator();
        }
        menu = menu
            .item("files.new_file", "Neue Datei …", None)
            .item("files.new_folder", "Neuer Ordner …", None)
            .item("files.import", "Datei einbinden …", None)
            .separator();
        menu = if item {
            menu.item("files.copy", "Kopieren", None)
        } else {
            menu.disabled_item("files.copy", "Kopieren")
        };
        menu = if self.clipboard.borrow().is_some() {
            menu.item("files.paste", "Einfügen", None)
        } else {
            menu.disabled_item("files.paste", "Einfügen")
        };
        menu = if item {
            menu.item("files.rename", "Umbenennen …", None)
                .item("files.delete", "Löschen", None)
        } else {
            menu.disabled_item("files.rename", "Umbenennen …")
                .disabled_item("files.delete", "Löschen")
        };
        menu.separator()
            .item("files.refresh", "Aktualisieren", None)
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
                &format!("Trotzdem fortfahren?{}", warnings(&plan)),
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
            ("Neuer Ordner", "neu")
        } else {
            ("Neue Datei", "neu.rs")
        };
        let Some(name) = self.ask_name(title, "Name (auch mit Unterordnern):", suggestion) else {
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
        if self.run(ide, title, op) {
            self.tree.select(Some(&path));
            if !folder {
                ide.open_project_file(&path);
            }
        }
    }

    fn import(&self, ide: &Ide) {
        let target = self.menu_target.borrow().clone();
        let dir = self.folder_of(&target);
        let Some(source) = dialog::open_file(self.parent_widget(), "Datei einbinden", &[]) else {
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
            "Datei einbinden",
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
            "Einfügen",
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
        let Some(new) = self.ask_name("Umbenennen", "Neuer Name:", name(&from)) else {
            return;
        };
        let to = join(parent(&from), &new);
        if to != from
            && self.run(
                ide,
                "Umbenennen",
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
            dialog::error(self.parent_widget(), "Löschen", &error.detail);
            return;
        }
        let files = plan.changes.first().map_or(0, |c| c.files);
        let what = if self.kind(&path) == Some(EntryKind::Folder) {
            format!("Den Ordner {path} ({files} Dateien)")
        } else {
            path.clone()
        };
        let unsaved = if ide.has_unsaved_below(&path) {
            "\n\nUngespeicherte Änderungen im Code-Editor gehen verloren."
        } else {
            ""
        };
        if !dialog::confirm(
            self.parent_widget(),
            "Löschen",
            &format!(
                "{what} in den Papierkorb verschieben?{unsaved}{}",
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
                    "Löschen",
                    &format!(
                        "Der Papierkorb ist nicht verfügbar ({detail}).\n\n{path} endgültig löschen?"
                    ),
                ) && let Err(e) = ide.file_op(&FileOp::Delete {
                    path,
                    permanent: true,
                }) {
                    dialog::error(self.parent_widget(), "Löschen", &e.to_string());
                }
            }
            Err(e) => dialog::error(self.parent_widget(), "Löschen", &e.to_string()),
        }
    }

    fn open_ct_editor(&self, ide: &Ide) {
        let target = self.menu_target.borrow().clone();
        let Some(root) = ide.session().project_dir() else {
            return;
        };
        let dir = root.join(parent(&target));
        let program = ct_editor();
        match std::process::Command::new(&program)
            .arg("--path")
            .arg(&dir)
            .spawn()
        {
            Ok(_) => ide.set_status(&format!("CT-Editor geöffnet: {}", dir.display())),
            Err(e) => ide.set_status(&format!(
                "CT-Editor nicht gestartet ({}): {e}",
                program.display()
            )),
        }
    }

    fn menu(&self, ide: &Ide, id: &str) {
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
                ide.set_status(&format!("Kopiert: {target} – „Einfügen“ im Zielordner"));
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
    let weak = Rc::downgrade(ide);
    project.window.on_resized(move |w, h| {
        if let Some(ide) = weak.upgrade() {
            ide.project.layout(w, h);
        }
    });
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
    project
        .window
        .on_menu(with(ide, |ide, id: String| ide.project.menu(ide, &id)));
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
