//! Code editor window: one tab per open file (GtkSourceView via RUF).
//! Generated files (`*.gen.rs`) are read-only; unsaved files are marked
//! with •. Switching to the `.rs` of a form shows that form in the designer
//! (a form and its `.rs` belong together); F12 toggles designer and code.
//!
//! rust-analyzer (see [`crate::lsp`]) gets every change after a short
//! typing pause and checks on save; its findings and those of the last
//! build appear as gutter marks. Ctrl+Space (and typing `.` or `::`)
//! completes, Ctrl+Click or Ctrl+F12 jumps to the definition.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use rncl::prelude::*;
use ruf::Timer;
use tdr_core::ComponentName;

use super::{Ide, with, with0};
use crate::desktop::window;

/// Typing pause before rust-analyzer gets the new text.
const CHANGE_PAUSE: i32 = 300;

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

/// The same file always under the same name (rust-analyzer reports
/// canonical paths).
fn key(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// Is `path` a Rust file rust-analyzer should know about?
pub fn is_rust(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some("rs")
}

/// Does the text before the cursor ask for member completion (`.`, `::`)?
fn completion_trigger(text: &str, line: usize, column: usize) -> bool {
    let Some(line_text) = text.lines().nth(line.saturating_sub(1)) else {
        return false;
    };
    let before: Vec<char> = line_text.chars().take(column).collect();
    match before.as_slice() {
        [.., '.', '.'] => false, // Bereich `..`
        [.., '.'] => true,
        [.., ':', ':'] => true,
        _ => false,
    }
}

pub struct CodeWindow {
    pub window: RnclWindow,
    tabs: RnclTabView,
    files: RefCell<Vec<Rc<OpenFile>>>,
    /// Marks per file from the last build and from rust-analyzer (also for
    /// files opened later).
    build_marks: RefCell<BTreeMap<PathBuf, Vec<LineMark>>>,
    lsp_marks: RefCell<BTreeMap<PathBuf, Vec<LineMark>>>,
    /// File waiting to be sent to rust-analyzer after the typing pause.
    pending_change: RefCell<Option<PathBuf>>,
    change_timer: Timer,
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
            "Code".into(),
            Menu::new()
                .item("code.complete", "Vervollständigen", Some("Ctrl+Space"))
                .item("code.definition", "Gehe zu Definition", Some("Ctrl+F12")),
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
        let change_timer = Timer::new();
        change_timer.set_single_shot(true);
        Self {
            window,
            tabs,
            files: RefCell::new(Vec::new()),
            build_marks: RefCell::new(BTreeMap::new()),
            lsp_marks: RefCell::new(BTreeMap::new()),
            pending_change: RefCell::new(None),
            change_timer,
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

    /// Path, text and cursor of the front tab.
    pub fn current_state(&self) -> Option<(PathBuf, String, (usize, usize))> {
        let file = self.current()?;
        Some((file.path.clone(), file.editor.text(), file.editor.cursor()))
    }

    fn file(&self, path: &Path) -> Option<Rc<OpenFile>> {
        let path = key(path);
        self.files.borrow().iter().find(|f| f.path == path).cloned()
    }

    /// Paths and texts of all open Rust files (to announce them to a newly
    /// started rust-analyzer).
    pub fn open_rust_files(&self) -> Vec<(PathBuf, String)> {
        self.files
            .borrow()
            .iter()
            .filter(|f| is_rust(&f.path))
            .map(|f| (f.path.clone(), f.editor.text()))
            .collect()
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
        let path = key(path);
        let path = path.as_path();
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
                let e = editor.clone();
                page.widget()
                    .on_resized(move |w, h| e.set_geometry(0, 0, w, h));
                let p = path.to_path_buf();
                editor.on_text_changed(with(ide, move |ide, text: String| {
                    if ide.is_refreshing() {
                        return;
                    }
                    ide.code.update_titles();
                    ide.code.text_changed(ide, &p, &text);
                }));
                editor.on_mouse(with(ide, |ide, e: MouseEvent| {
                    if e.action == MouseAction::Released && e.button == 1 && e.modifiers.ctrl {
                        ide.goto_definition();
                    }
                }));
                self.files.borrow_mut().push(Rc::new(OpenFile {
                    path: path.to_path_buf(),
                    page,
                    editor,
                    saved: RefCell::new(text.clone()),
                    read_only,
                }));
                self.refresh_marks(path);
                if is_rust(path) {
                    ide.lsp_opened(path, &text);
                }
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

    /// Text of an open file.
    pub fn text(&self, path: &Path) -> Option<String> {
        self.file(path).map(|f| f.editor.text())
    }

    /// Replaces the text of an open file (like typing; not saved).
    pub fn set_text(&self, ide: &Ide, path: &Path, text: &str) -> bool {
        let Some(file) = self.file(path) else {
            return false;
        };
        // Gesetzter Text meldet keine Änderung – wie beim Tippen nachreichen.
        file.editor.set_text(text);
        self.update_titles();
        ide.lsp_changed(&file.path, text);
        true
    }

    /// Gutter marks of an open file (build and rust-analyzer).
    pub fn marks(&self, path: &Path) -> Vec<LineMark> {
        self.file(path)
            .map(|f| f.editor.line_marks())
            .unwrap_or_default()
    }

    /// Moves the cursor of an open file.
    pub fn set_cursor(&self, path: &Path, line: usize, column: usize) {
        if let Some(file) = self.file(path) {
            file.editor.set_cursor(line, column);
        }
    }

    /// Text change: rust-analyzer after a typing pause; `.` and `::` ask
    /// for completions at once.
    fn text_changed(&self, ide: &Rc<Ide>, path: &Path, text: &str) {
        if !is_rust(path) {
            return;
        }
        let Some(file) = self.file(path) else {
            return;
        };
        let (line, column) = file.editor.cursor();
        if completion_trigger(text, line, column) {
            self.change_timer.stop();
            self.pending_change.borrow_mut().take();
            ide.lsp_changed(path, text);
            ide.complete_at_cursor();
            return;
        }
        *self.pending_change.borrow_mut() = Some(path.to_path_buf());
        self.change_timer.start(CHANGE_PAUSE);
    }

    /// Sends a waiting change now.
    pub fn flush_change(&self, ide: &Ide) {
        self.change_timer.stop();
        let pending = self.pending_change.borrow_mut().take();
        if let Some(path) = pending
            && let Some(file) = self.file(&path)
        {
            ide.lsp_changed(&path, &file.editor.text());
        }
    }

    /// Shows completion proposals in the front editor.
    pub fn show_completions(&self, items: &[Completion]) {
        if let Some(file) = self.current() {
            file.editor.show_completions(items);
        }
    }

    /// Writes a file to disk if it has unsaved changes; `true` if written.
    fn save_file(&self, file: &OpenFile) -> Result<bool, String> {
        if !file.is_dirty() {
            return Ok(false);
        }
        let text = file.editor.text();
        std::fs::write(&file.path, &text).map_err(|e| format!("{}: {e}", file.path.display()))?;
        *file.saved.borrow_mut() = text;
        Ok(true)
    }

    /// Saves the front file; returns the saved paths.
    pub fn save_current(&self) -> Result<Vec<PathBuf>, String> {
        let mut saved = Vec::new();
        if let Some(file) = self.current()
            && self.save_file(&file)?
        {
            saved.push(file.path.clone());
        }
        self.update_titles();
        Ok(saved)
    }

    /// Saves every file with unsaved changes; returns the saved paths.
    pub fn save_all(&self) -> Result<Vec<PathBuf>, String> {
        let mut saved = Vec::new();
        for file in self.files.borrow().iter() {
            if self.save_file(file)? {
                saved.push(file.path.clone());
            }
        }
        self.update_titles();
        Ok(saved)
    }

    pub fn has_unsaved(&self) -> bool {
        self.files.borrow().iter().any(|f| f.is_dirty())
    }

    /// Reloads files changed on disk (after code generation) that have no
    /// unsaved changes; returns the reloaded files with their new text.
    pub fn reload_from_disk(&self) -> Vec<(PathBuf, String)> {
        let mut reloaded = Vec::new();
        for file in self.files.borrow().iter() {
            if file.is_dirty() {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&file.path) else {
                continue;
            };
            if text != file.editor.text() {
                file.editor.set_text(&text);
                *file.saved.borrow_mut() = text.clone();
                reloaded.push((file.path.clone(), text));
            }
        }
        self.update_titles();
        reloaded
    }

    /// Closes the front tab (asks about unsaved changes); returns the path.
    pub fn close_current(&self) -> Option<PathBuf> {
        let index = usize::try_from(self.tabs.current_index()).ok()?;
        let file = self.files.borrow().get(index).cloned()?;
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
            return None;
        }
        self.files.borrow_mut().remove(index);
        self.tabs.remove_tab(index as i32);
        let _ = &file.page;
        self.update_titles();
        Some(file.path.clone())
    }

    /// Shows build and rust-analyzer marks of `path` together.
    fn refresh_marks(&self, path: &Path) {
        let Some(file) = self.file(path) else {
            return;
        };
        let path = file.path.as_path();
        let mut marks = self
            .build_marks
            .borrow()
            .get(path)
            .cloned()
            .unwrap_or_default();
        marks.extend(
            self.lsp_marks
                .borrow()
                .get(path)
                .cloned()
                .unwrap_or_default(),
        );
        file.editor.set_line_marks(marks);
    }

    /// Marks of a build: errors and warnings per file (absolute paths).
    pub fn set_build_marks(&self, marks: BTreeMap<PathBuf, Vec<LineMark>>) {
        *self.build_marks.borrow_mut() = marks.into_iter().map(|(p, m)| (key(&p), m)).collect();
        let paths: Vec<PathBuf> = self.files.borrow().iter().map(|f| f.path.clone()).collect();
        for path in paths {
            self.refresh_marks(&path);
        }
    }

    /// Findings of rust-analyzer for one file.
    pub fn set_lsp_marks(&self, path: &Path, marks: Vec<LineMark>) {
        self.lsp_marks.borrow_mut().insert(key(path), marks);
        self.refresh_marks(path);
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
    code.change_timer
        .on_timeout(with0(ide, |ide| ide.code.flush_change(ide)));
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
            "code.save" => ide.code.save_current().map(|saved| ide.lsp_saved(&saved)),
            "code.save_all" => ide.code.save_all().map(|saved| ide.lsp_saved(&saved)),
            "code.close" => {
                if let Some(path) = ide.code.close_current() {
                    ide.lsp_closed(&path);
                }
                Ok(())
            }
            "code.complete" => {
                ide.complete_at_cursor();
                Ok(())
            }
            "code.definition" => {
                ide.goto_definition();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triggers() {
        let text = "fn f() {\n    s.\n    a..b\n    std::\n}\n";
        assert!(completion_trigger(text, 2, 6));
        assert!(!completion_trigger(text, 3, 7));
        assert!(completion_trigger(text, 4, 9));
        assert!(!completion_trigger(text, 1, 3));
    }
}
