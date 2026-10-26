//! Code editor window: one tab per open file (GtkSourceView via RUF).
//! Generated files (`*.gen.rs`) are read-only; unsaved files are marked
//! with •. Switching to the `.rs` of a form shows that form in the designer
//! (a form and its `.rs` belong together); F12 toggles designer and code.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use rncl::prelude::*;
use tdr_core::ComponentName;

use super::{Ide, with};
use crate::desktop::window;

/// An open file.
struct OpenFile {
    path: PathBuf,
    page: RnclPanel,
    editor: RnclCodeEditor,
    /// Text as last loaded or saved.
    saved: RefCell<String>,
    read_only: bool,
}

impl OpenFile {
    fn is_dirty(&self) -> bool {
        !self.read_only && self.editor.text() != *self.saved.borrow()
    }

    fn title(&self) -> String {
        let name = self
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if self.is_dirty() {
            format!("{name} •")
        } else {
            name
        }
    }
}

fn language(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("rs") => "rust",
        Some("toml") => "toml",
        Some("json") => "json",
        Some("md") => "markdown",
        Some("ron") => "rust",
        _ => "",
    }
}

fn is_generated(path: &Path) -> bool {
    path.to_string_lossy().ends_with(".gen.rs")
}

pub struct CodeWindow {
    pub window: RnclWindow,
    tabs: RnclTabView,
    files: RefCell<Vec<Rc<OpenFile>>>,
    /// Marks per file (from the last build), also for files opened later.
    marks: RefCell<BTreeMap<PathBuf, Vec<LineMark>>>,
}

fn menu_bar() -> MenuBar {
    vec![
        (
            "Datei".into(),
            Menu::new()
                .item("code.save", "Speichern", Some("Ctrl+S"))
                .item("code.save_all", "Alle speichern", Some("Ctrl+Shift+S"))
                .separator()
                .item("code.close", "Schließen", Some("Ctrl+W")),
        ),
        (
            "Ansicht".into(),
            Menu::new().item("code.designer", "Designer", Some("F12")),
        ),
    ]
}

impl CodeWindow {
    pub fn build() -> Self {
        let window = RnclWindow::new("Code");
        window.set_menu_bar(menu_bar());
        let tabs = RnclTabView::new(&window);
        Self {
            window,
            tabs,
            files: RefCell::new(Vec::new()),
            marks: RefCell::new(BTreeMap::new()),
        }
    }

    fn current(&self) -> Option<Rc<OpenFile>> {
        let index = usize::try_from(self.tabs.current_index()).ok()?;
        self.files.borrow().get(index).cloned()
    }

    /// The file shown in the front tab.
    pub fn current_path(&self) -> Option<PathBuf> {
        self.current().map(|f| f.path.clone())
    }

    fn update_titles(&self) {
        for (i, file) in self.files.borrow().iter().enumerate() {
            self.tabs.set_tab_title(i as i32, &file.title());
        }
        let title = match self.current() {
            Some(file) => format!("Code – {}", file.path.display()),
            None => "Code".to_owned(),
        };
        self.window.set_title(&title);
    }

    /// Opens (or shows) `path` and moves the cursor to `line` (1-based).
    pub fn open(&self, ide: &Rc<Ide>, path: &Path, line: usize) -> Result<(), String> {
        let existing = self.files.borrow().iter().position(|f| f.path == path);
        let index = match existing {
            Some(i) => i,
            None => {
                let text = std::fs::read_to_string(path)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                let page = self.tabs.add_tab("");
                let editor = RnclCodeEditor::new(&page);
                editor.set_language(language(path));
                editor.set_text(&text);
                let read_only = is_generated(path);
                editor.set_read_only(read_only);
                if let Some(marks) = self.marks.borrow().get(path) {
                    editor.set_line_marks(marks.clone());
                }
                let e = editor.clone();
                page.widget()
                    .on_resized(move |w, h| e.set_geometry(0, 0, w, h));
                editor.on_text_changed(with(ide, |ide, _: String| {
                    if !ide.is_refreshing() {
                        ide.code.update_titles();
                    }
                }));
                self.files.borrow_mut().push(Rc::new(OpenFile {
                    path: path.to_path_buf(),
                    page,
                    editor,
                    saved: RefCell::new(text),
                    read_only,
                }));
                self.files.borrow().len() - 1
            }
        };
        self.tabs.set_current_index(index as i32);
        if let Some(file) = self.files.borrow().get(index) {
            file.editor.goto_line(line.max(1));
        }
        self.update_titles();
        self.window.show();
        Ok(())
    }

    /// Writes a file to disk if it has unsaved changes.
    fn save_file(&self, file: &OpenFile) -> Result<(), String> {
        if !file.is_dirty() {
            return Ok(());
        }
        let text = file.editor.text();
        std::fs::write(&file.path, &text).map_err(|e| format!("{}: {e}", file.path.display()))?;
        *file.saved.borrow_mut() = text;
        Ok(())
    }

    pub fn save_current(&self) -> Result<(), String> {
        if let Some(file) = self.current() {
            self.save_file(&file)?;
        }
        self.update_titles();
        Ok(())
    }

    /// Saves every file with unsaved changes.
    pub fn save_all(&self) -> Result<(), String> {
        for file in self.files.borrow().iter() {
            self.save_file(file)?;
        }
        self.update_titles();
        Ok(())
    }

    pub fn has_unsaved(&self) -> bool {
        self.files.borrow().iter().any(|f| f.is_dirty())
    }

    /// Reloads files changed on disk (after code generation) that have no
    /// unsaved changes; generated files always.
    pub fn reload_from_disk(&self) {
        for file in self.files.borrow().iter() {
            if file.is_dirty() {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&file.path) else {
                continue;
            };
            if text != file.editor.text() {
                file.editor.set_text(&text);
                *file.saved.borrow_mut() = text;
            }
        }
        self.update_titles();
    }

    /// Closes the front tab (asks about unsaved changes).
    pub fn close_current(&self) {
        let Some(index) = usize::try_from(self.tabs.current_index()).ok() else {
            return;
        };
        let Some(file) = self.files.borrow().get(index).cloned() else {
            return;
        };
        if file.is_dirty()
            && !ruf::dialog::confirm(
                Some(self.window.widget()),
                "Ungespeicherte Änderungen",
                &format!(
                    "{} schließen und Änderungen verwerfen?",
                    file.path.display()
                ),
            )
        {
            return;
        }
        self.files.borrow_mut().remove(index);
        self.tabs.remove_tab(index as i32);
        let _ = &file.page;
        self.update_titles();
    }

    /// Marks of a build: errors and warnings per file (absolute paths).
    pub fn set_marks(&self, marks: BTreeMap<PathBuf, Vec<LineMark>>) {
        for file in self.files.borrow().iter() {
            file.editor
                .set_line_marks(marks.get(&file.path).cloned().unwrap_or_default());
        }
        *self.marks.borrow_mut() = marks;
    }

    /// The form whose `.rs` is in the front tab.
    fn current_form(&self, ide: &Ide) -> Option<ComponentName> {
        let path = self.current_path()?;
        let dir = ide.session().project_dir()?;
        let relative = path.strip_prefix(dir.join("src")).ok()?;
        let name = relative.to_str()?.strip_suffix(".rs")?;
        let name = name.strip_suffix(".gen").unwrap_or(name);
        let form = ComponentName::new(name).ok()?;
        ide.session().document().form(&form).map(|_| form)
    }
}

pub fn connect(ide: &Rc<Ide>) {
    let code = &ide.code;
    let weak = Rc::downgrade(ide);
    code.window.on_resized(move |w, h| {
        if let Some(ide) = weak.upgrade() {
            ide.code.tabs.set_geometry(0, 0, w, h);
        }
    });
    code.tabs.on_current_index_changed(with(ide, |ide, _: i32| {
        if ide.is_refreshing() {
            return;
        }
        ide.code.update_titles();
        // Formular und .rs gehören zusammen: der Designer folgt dem Reiter.
        if let Some(form) = ide.code.current_form(ide)
            && ide.session().form_name() != Some(&form)
        {
            ide.select_form(&form);
        }
    }));
    code.window.on_menu(with(ide, |ide, id: String| {
        let result = match id.as_str() {
            "code.save" => ide.code.save_current(),
            "code.save_all" => ide.code.save_all(),
            "code.close" => {
                ide.code.close_current();
                Ok(())
            }
            "code.designer" => {
                ide.show_window(window::DESIGNER);
                Ok(())
            }
            _ => Ok(()),
        };
        if let Err(e) = result {
            ide.main.set_status(&e);
        }
    }));
}
