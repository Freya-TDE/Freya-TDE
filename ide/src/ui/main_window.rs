//! Main window: menu bar, component palette (one tab per category, like
//! Delphi) and status line.

use std::cell::RefCell;
use std::rc::Rc;

use rncl::prelude::*;
use tdr_core::TypeRef;
use tdr_designer::palette;

use super::{Ide, icons, with, with0};
use crate::desktop::window;

const MARGIN: i32 = 6;
const ROW: i32 = 34;
const STATUS: i32 = 24;
const BRANCH: i32 = 200;

pub struct MainWindow {
    pub window: RnclWindow,
    pointer: RnclButton,
    tabs: RnclTabView,
    status: RnclLabel,
    /// Git branch (right of the status line).
    branch: RnclLabel,
    /// Palette buttons with their type, per tab; rebuilt when the types change.
    buttons: RefCell<Vec<(RnclButton, TypeRef)>>,
    /// Types the palette was built from (rebuilt only when they change).
    built_for: RefCell<Vec<TypeRef>>,
}

pub(super) fn menu_bar() -> MenuBar {
    vec![
        (
            "Datei".into(),
            Menu::new()
                .item("file.new", "Neues Projekt …", Some("Ctrl+N"))
                .item("file.open", "Projekt öffnen …", Some("Ctrl+O"))
                .item("file.save", "Speichern", Some("Ctrl+S"))
                .separator()
                .item("file.projects", "Projekt-Manager", None)
                .item(
                    "file.from_cargo",
                    "Projekt aus Cargo.toml erstellen …",
                    None,
                )
                .separator()
                .item("file.quit", "Beenden", Some("Ctrl+Q")),
        ),
        (
            "Bearbeiten".into(),
            Menu::new()
                .item("edit.undo", "Rückgängig", Some("Ctrl+Z"))
                .item("edit.redo", "Wiederholen", Some("Ctrl+Shift+Z"))
                .separator()
                .item("edit.delete", "Auswahl löschen", None),
        ),
        (
            "Projekt".into(),
            Menu::new()
                .item("project.add_form", "Formular hinzufügen", None)
                .item("project.add_data", "Datenmodul hinzufügen", None)
                .separator()
                .item("project.generate", "Code erzeugen", Some("Ctrl+G"))
                .item("project.build", "Bauen", Some("Ctrl+F9")),
        ),
        (
            // Wie Delphi/Lazarus: Start, Schritte, Haltepunkte.
            "Start".into(),
            Menu::new()
                .item("start.run", "Starten / Weiter", Some("F9"))
                .item(
                    "start.run_plain",
                    "Ohne Debugger starten",
                    Some("Ctrl+Shift+F9"),
                )
                .item("start.pause", "Anhalten", None)
                .item("start.stop", "Programm beenden", Some("Ctrl+F2"))
                .separator()
                .item("start.step_into", "Einzelne Anweisung", Some("F7"))
                .item("start.step_over", "Gesamte Routine", Some("F8"))
                .item("start.step_out", "Bis Rückgabe", Some("Shift+F8"))
                .item("start.run_to", "Bis Cursor", Some("F4"))
                .separator()
                .item("start.toggle_bp", "Haltepunkt umschalten", Some("F5"))
                .item("start.add_watch", "Ausdruck überwachen …", Some("Ctrl+F5"))
                .item("start.remove_all_bp", "Alle Haltepunkte entfernen", None),
        ),
        (
            "Fenster".into(),
            Menu::new()
                .item("window.designer", "Designer", Some("F12"))
                .item("window.inspector", "Objektinspektor", Some("F11"))
                .item("window.structure", "Struktur", None)
                .item("window.messages", "Meldungen", None)
                .item("window.problems", "Probleme", Some("Ctrl+Shift+M"))
                .item("window.output", "Ausgabe", Some("Ctrl+Shift+U"))
                .item("window.terminal", "Terminal", Some("Ctrl+Shift+T"))
                .item("window.code", "Code", None)
                .item("window.project", "Projekt", None)
                .item("window.git", "Quellcodeverwaltung", Some("Ctrl+Shift+G"))
                .item("window.debugger", "Debugger", Some("Ctrl+Shift+D"))
                .item("window.database", "Datenbank", Some("Ctrl+Shift+B"))
                .item(
                    "window.debug_console",
                    "Debug-Konsole",
                    Some("Ctrl+Shift+Y"),
                )
                .separator()
                .item("window.reset", "Standardanordnung", None),
        ),
        (
            "Hilfe".into(),
            Menu::new()
                .item("help.shortcuts", "Tastenkürzel", None)
                .separator()
                .item("help.about", "Über Freya TDE", None),
        ),
    ]
}

impl MainWindow {
    pub fn build() -> Self {
        let window = RnclWindow::new("Freya TDE");
        window.set_menu_bar(menu_bar());
        let pointer = RnclButton::new(&window, "Zeiger");
        pointer
            .widget()
            .set_tool_tip("Auswählen statt platzieren (Esc)");
        pointer.widget().set_image(icons::icon("pointer").as_ref());
        let tabs = RnclTabView::new(&window);
        let status = RnclLabel::new(&window, "");
        let branch = RnclLabel::new(&window, "");
        Self {
            window,
            pointer,
            tabs,
            status,
            branch,
            buttons: RefCell::new(Vec::new()),
            built_for: RefCell::new(Vec::new()),
        }
    }

    fn layout(&self, width: i32, height: i32) {
        self.pointer.set_geometry(MARGIN, MARGIN + 22, 96, ROW);
        self.tabs.set_geometry(
            MARGIN + 102,
            MARGIN,
            width - 2 * MARGIN - 102,
            height - 2 * MARGIN - STATUS,
        );
        self.status.set_geometry(
            MARGIN,
            height - MARGIN - STATUS,
            width - 2 * MARGIN - BRANCH,
            STATUS,
        );
        self.branch.set_geometry(
            width - MARGIN - BRANCH,
            height - MARGIN - STATUS,
            BRANCH,
            STATUS,
        );
    }

    /// Git branch in the status line (empty = no repository).
    pub fn set_branch(&self, text: &str) {
        self.branch.set_text(text);
    }

    pub fn set_status(&self, text: &str) {
        self.status.set_text(text);
    }

    /// Fills the palette: one tab per category, a button per type.
    pub(super) fn build_palette(&self, ide: &Rc<Ide>) {
        let (backend, items) = {
            let session = ide.session();
            let backend = session.document().project().backend.clone();
            let mut items = palette(session.document().registry(), &backend);
            // Wie bei Delphi: die Standard-Steuerelemente zuerst, dann die
            // Palette des UI-Backends, dann die Komponenten-Packs.
            items.sort_by_key(|i| {
                (
                    i.category != "Standard",
                    !matches!(i.type_ref.origin, tdr_core::Origin::Backend(_)),
                )
            });
            (backend, items)
        };
        let _ = backend;
        let types: Vec<TypeRef> = items.iter().map(|i| i.type_ref.clone()).collect();
        if *self.built_for.borrow() == types {
            return;
        }
        while self.tabs.tab_count() > 0 {
            self.tabs.remove_tab(0);
        }
        let mut buttons = Vec::new();
        let mut category = String::new();
        let mut page: Option<RnclPanel> = None;
        let mut on_page: Rc<RefCell<Vec<RnclButton>>> = Rc::default();
        let mut x = 4;
        for item in &items {
            if item.category != category || page.is_none() {
                category = item.category.clone();
                let new_page = self.tabs.add_tab(&category);
                // Knöpfe mittig in der Seite. Ist die Seite zu niedrig (die
                // Reiter sind je nach System verschieden hoch), wächst das
                // Hauptfenster um das Fehlende.
                on_page = Rc::default();
                let page_buttons = on_page.clone();
                let weak = Rc::downgrade(ide);
                new_page.widget().on_resized(move |_, height| {
                    let y = ((height - ROW) / 2).max(2);
                    for button in page_buttons.borrow().iter() {
                        let g = button.geometry();
                        if g.y != y {
                            button.set_geometry(g.x, y, g.width, g.height);
                        }
                    }
                    let missing = ROW + 8 - height;
                    if missing > 0
                        && let Some(ide) = weak.upgrade()
                    {
                        let size = ide.main.window.size();
                        ide.main.window.set_size(size.width, size.height + missing);
                    }
                });
                page = Some(new_page);
                x = 4;
            }
            let page = page.as_ref().expect("page");
            // Wie Lazarus: Symbol, der Name steht im Tooltip.
            let button = RnclButton::new(page, "");
            button
                .widget()
                .set_image(Some(&icons::icon_or_fallback(item.icon.as_deref())));
            let width = ROW + 6;
            button.set_geometry(x, 6, width, ROW);
            on_page.borrow_mut().push(button.clone());
            x += width + 4;
            // Typen aus Komponenten-Packs mit dem Pack: „Runtime (tokio)“.
            let title = match &item.type_ref.origin {
                tdr_core::Origin::Pack(pack) => format!("{} ({pack})", item.type_ref.name),
                _ => item.type_ref.name.clone(),
            };
            let tip = if item.description.is_empty() {
                title
            } else {
                format!("{title} – {}", item.description)
            };
            button.widget().set_tool_tip(&tip);
            let type_ref = item.type_ref.clone();
            button.on_clicked(with0(ide, move |ide| ide.arm(Some(type_ref.clone()))));
            // Auch ziehen und im Designer ablegen.
            button.widget().set_drag_text(&item.type_ref.to_string());
            buttons.push((button, item.type_ref.clone()));
        }
        *self.buttons.borrow_mut() = buttons;
        *self.built_for.borrow_mut() = types;
    }

    pub fn refresh(&self, ide: &Ide) {
        self.window.set_title(&ide.title());
        let session = ide.session();
        let armed = session.armed().cloned();
        drop(session);
        for (button, type_ref) in self.buttons.borrow().iter() {
            // Der gewählte Typ ist hinterlegt (wie ein gedrückter Knopf).
            let css = if Some(type_ref) == armed.as_ref() {
                "background: #9ec3f0"
            } else {
                ""
            };
            if button.widget().style_sheet() != css {
                button.widget().set_style_sheet(css);
            }
        }
        if let Some(armed) = armed {
            self.set_status(&format!(
                "Klicken Sie in den Designer, um {} zu platzieren (Esc bricht ab).",
                armed.name
            ));
        }
    }
}

pub fn connect(ide: &Rc<Ide>) {
    let main = &ide.main;
    main.build_palette(ide);
    main.pointer.on_clicked(with0(ide, |ide| {
        ide.arm(None);
        ide.main.set_status("");
    }));
    let weak = Rc::downgrade(ide);
    main.window.on_resized(move |w, h| {
        if let Some(ide) = weak.upgrade() {
            ide.main.layout(w, h);
        }
    });
    main.window
        .on_menu(with(ide, |ide, id: String| match id.as_str() {
            "file.new" => ide.show_wizard(),
            "file.open" => {
                ide.open_dialog();
                ide.main.build_palette(ide);
            }
            "file.projects" => {
                ide.project.show_manager();
                ide.show_window(window::PROJECT);
            }
            "file.from_cargo" => {
                ide.project.show_manager();
                ide.show_window(window::PROJECT);
                ide.project.menu_from_main(ide, "pm.cargo");
            }
            "file.save" => {
                ide.save();
            }
            "file.quit" => {
                ide.quit();
            }
            "edit.undo" => ide.undo(),
            "edit.redo" => ide.redo(),
            "edit.delete" => ide.delete_selection(),
            "project.add_form" => ide.add_form(false),
            "project.add_data" => ide.add_form(true),
            "project.generate" => {
                ide.generate_code();
            }
            "project.build" => ide.build(false),
            id if id.starts_with("start.") => ide.debug_menu(id),
            "window.debugger" => ide.debug_menu("dbg.show"),
            "window.debug_console" => ide.debug_menu("dbg.console"),
            "help.shortcuts" => ide.show_shortcuts(),
            "help.about" => ide.show_about(),
            "window.designer" => ide.show_window(window::DESIGNER),
            "window.inspector" => ide.show_window(window::INSPECTOR),
            "window.structure" => ide.show_window(window::STRUCTURE),
            "window.messages" => ide.show_window(window::MESSAGES),
            "window.problems" => ide.show_panel(super::panel::Tab::Problems),
            "window.output" => ide.show_panel(super::panel::Tab::Output),
            "window.terminal" => ide.show_terminal(),
            "window.code" => ide.show_code(),
            "window.project" => ide.show_window(window::PROJECT),
            "window.git" => {
                ide.git_refresh(true);
                ide.show_window(window::GIT);
            }
            "window.database" => ide.show_database(),
            "window.reset" => ide.reset_desktop(),
            _ => {}
        }));
    // Schließen des Hauptfensters beendet Freya TDE.
    let weak = Rc::downgrade(ide);
    main.window
        .on_close_requested(move || weak.upgrade().is_none_or(|ide| ide.quit()));
}
