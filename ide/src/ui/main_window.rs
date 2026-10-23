//! Main window: menu bar, component palette (one tab per category, like
//! Delphi) and status line.

use std::cell::RefCell;
use std::rc::Rc;

use rncl::prelude::*;
use tdr_core::TypeRef;
use tdr_designer::palette;

use super::{Ide, with, with0};
use crate::desktop::window;

const MARGIN: i32 = 6;
const ROW: i32 = 30;
const STATUS: i32 = 24;

pub struct MainWindow {
    pub window: RnclWindow,
    pointer: RnclButton,
    tabs: RnclTabView,
    status: RnclLabel,
    /// Palette buttons with their type, per tab; rebuilt when the types change.
    buttons: RefCell<Vec<(RnclButton, TypeRef)>>,
    /// Types the palette was built from (rebuilt only when they change).
    built_for: RefCell<Vec<TypeRef>>,
}

fn menu_bar() -> MenuBar {
    vec![
        (
            "Datei".into(),
            Menu::new()
                .item("file.new", "Neues Projekt", Some("Ctrl+N"))
                .item("file.open", "Projekt öffnen …", Some("Ctrl+O"))
                .item("file.save", "Speichern", Some("Ctrl+S"))
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
                .item("project.add_data", "Datenmodul hinzufügen", None),
        ),
        (
            "Fenster".into(),
            Menu::new()
                .item("window.designer", "Designer", Some("F12"))
                .item("window.inspector", "Objektinspektor", Some("F11"))
                .item("window.structure", "Struktur", None)
                .item("window.messages", "Meldungen", None)
                .separator()
                .item("window.reset", "Standardanordnung", None),
        ),
    ]
}

impl MainWindow {
    pub fn build() -> Self {
        let window = RnclWindow::new("Freya-TD");
        window.set_menu_bar(menu_bar());
        let pointer = RnclButton::new(&window, "Zeiger");
        pointer
            .widget()
            .set_tool_tip("Auswählen statt platzieren (Esc)");
        let tabs = RnclTabView::new(&window);
        let status = RnclLabel::new(&window, "");
        Self {
            window,
            pointer,
            tabs,
            status,
            buttons: RefCell::new(Vec::new()),
            built_for: RefCell::new(Vec::new()),
        }
    }

    fn layout(&self, width: i32, height: i32) {
        self.pointer.set_geometry(MARGIN, MARGIN + 22, 80, ROW);
        self.tabs.set_geometry(
            MARGIN + 86,
            MARGIN,
            width - 2 * MARGIN - 86,
            height - 2 * MARGIN - STATUS,
        );
        self.status
            .set_geometry(MARGIN, height - MARGIN - STATUS, width - 2 * MARGIN, STATUS);
    }

    pub fn set_status(&self, text: &str) {
        self.status.set_text(text);
    }

    /// Fills the palette: one tab per category, a button per type.
    fn build_palette(&self, ide: &Rc<Ide>) {
        let (backend, items) = {
            let session = ide.session();
            let backend = session.document().project().backend.clone();
            let mut items = palette(session.document().registry(), &backend);
            // Wie bei Delphi: die Standard-Steuerelemente zuerst.
            items.sort_by_key(|i| i.category != "Standard");
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
        let mut x = 4;
        for item in &items {
            if item.category != category || page.is_none() {
                category = item.category.clone();
                page = Some(self.tabs.add_tab(&category));
                x = 4;
            }
            let page = page.as_ref().expect("page");
            let button = RnclButton::new(page, &item.type_ref.name);
            button.widget().adjust_size();
            let width = button.size().width.max(70);
            button.set_geometry(x, 6, width, ROW);
            x += width + 4;
            let tip = if item.description.is_empty() {
                item.type_ref.to_string()
            } else {
                format!("{} – {}", item.type_ref, item.description)
            };
            button.widget().set_tool_tip(&tip);
            let type_ref = item.type_ref.clone();
            button.on_clicked(with0(ide, move |ide| ide.arm(Some(type_ref.clone()))));
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
            let label = if Some(type_ref) == armed.as_ref() {
                format!("▶ {}", type_ref.name)
            } else {
                type_ref.name.clone()
            };
            button.set_text(&label);
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
            "file.new" => {
                ide.new_project();
                ide.main.build_palette(ide);
            }
            "file.open" => {
                ide.open_dialog();
                ide.main.build_palette(ide);
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
            "window.designer" => ide.show_window(window::DESIGNER),
            "window.inspector" => ide.show_window(window::INSPECTOR),
            "window.structure" => ide.show_window(window::STRUCTURE),
            "window.messages" => ide.show_window(window::MESSAGES),
            "window.reset" => ide.reset_desktop(),
            _ => {}
        }));
    // Schließen des Hauptfensters beendet Freya-TD.
    let weak = Rc::downgrade(ide);
    main.window
        .on_close_requested(move || weak.upgrade().is_none_or(|ide| ide.quit()));
}
