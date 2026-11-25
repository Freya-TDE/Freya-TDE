//! Code editor window: one tab per open file (GtkSourceView via RUF).
//! Generated files (`*.gen.rs`) are read-only; unsaved files are marked
//! with •. Switching to the `.rs` of a form shows that form in the designer
//! (a form and its `.rs` belong together); F12 toggles designer and code.
//!
//! rust-analyzer (see [`crate::lsp`]) gets every change after a short
//! typing pause and checks on save; its findings and those of the last
//! build appear as gutter marks. Ctrl+Space (and typing `.` or `::`)
//! completes, Ctrl+Click or Ctrl+F12 jumps to the definition. Brace blocks
//! fold with the arrows in the gutter. The leftmost gutter column holds
//! breakpoints (click toggles, right click edits) and the debugger's
//! current line.
//!
//! Tabs close with their „x“ or a middle click. A tab dragged out of the tab
//! bar becomes a window of its own (to look something up without switching
//! tabs); dragging that window's grip onto the tab bar puts it back.

use crate::tr;
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

/// Prefix of the drag text of a detached file's grip.
const DRAG: &str = "freya-code:";
/// Height of the grip of a detached file.
const GRIP: i32 = 22;

/// A file shown in a window of its own instead of a tab.
struct Detached {
    window: RnclWindow,
    grip: RnclLabel,
}

/// An open file.
struct OpenFile {
    path: PathBuf,
    /// Its tab page (replaced when the file goes back into a tab).
    page: RefCell<RnclPanel>,
    /// Shown in a window of its own (see [`CodeWindow::detach`]).
    detached: RefCell<Option<Detached>>,
    editor: RnclCodeEditor,
    /// Text as last loaded or saved.
    saved: RefCell<String>,
    read_only: bool,
    /// Tab title of a text without a file (e.g. a diff); `None` for files.
    label: Option<String>,
    /// Git's version to compare with (the index); `None` = no change marks.
    base: RefCell<Option<String>>,
    /// Text as last seen (to move breakpoints along with edits).
    shown: RefCell<String>,
}

impl OpenFile {
    fn is_dirty(&self) -> bool {
        !self.read_only && self.editor.text() != *self.saved.borrow()
    }

    fn title(&self) -> String {
        if let Some(label) = &self.label {
            return label.clone();
        }
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
pub(super) fn key(path: &Path) -> PathBuf {
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
    /// Breakpoints per file (also for files opened later).
    breakpoints: RefCell<BTreeMap<PathBuf, Vec<BreakpointMark>>>,
    /// Where the debugger stands.
    execution: RefCell<Option<(PathBuf, usize)>>,
    /// A detached file the user works in right now (its menu or a click in
    /// it); `None` = the front tab counts.
    active: RefCell<Option<PathBuf>>,
}

pub(super) fn menu_bar() -> MenuBar {
    vec![
        (
            tr!("menu.title.file"),
            Menu::new()
                .item("code.save", &tr!("menu.code.save"), Some("Ctrl+S"))
                .item("code.save_all", &tr!("menu.code.save_all"), Some("Ctrl+Shift+S"))
                .separator()
                .item("code.close", &tr!("menu.code.close"), Some("Ctrl+W")),
        ),
        (
            tr!("menu.title.code"),
            Menu::new()
                .item("code.complete", &tr!("menu.code.complete"), Some("Ctrl+Space"))
                .item("code.definition", &tr!("menu.code.definition"), Some("Ctrl+F12"))
                .item("code.codetools", &tr!("menu.code.codetools"),
                    Some("Ctrl+Shift+C"),
                )
                .separator()
                .item("code.fold_all", &tr!("menu.code.fold_all"), None)
                .item("code.unfold_all", &tr!("menu.code.unfold_all"), None),
        ),
        (
            tr!("menu.title.view"),
            Menu::new().item("code.designer", &tr!("menu.code.designer"), Some("F12")),
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
            breakpoints: RefCell::new(BTreeMap::new()),
            execution: RefCell::new(None),
            active: RefCell::new(None),
        }
    }

    /// The files in tabs, in tab order.
    fn attached(&self) -> Vec<Rc<OpenFile>> {
        self.files
            .borrow()
            .iter()
            .filter(|f| f.detached.borrow().is_none())
            .cloned()
            .collect()
    }

    /// Titles of the tabs (automation).
    pub fn tab_titles(&self) -> Vec<String> {
        self.tabs.tab_titles()
    }

    /// Files shown in windows of their own (automation).
    pub fn detached_paths(&self) -> Vec<PathBuf> {
        self.files
            .borrow()
            .iter()
            .filter(|f| f.detached.borrow().is_some())
            .map(|f| f.path.clone())
            .collect()
    }

    /// The tab bar (automation: simulate „x“, dragging out, dropping).
    pub fn tabs_widget(&self) -> Widget {
        self.tabs.widget().clone()
    }

    /// Tab index of `path` (not for detached files).
    fn tab_of(&self, path: &Path) -> Option<usize> {
        self.attached().iter().position(|f| f.path == path)
    }

    /// The file being worked in: a detached one the user is in, else the
    /// front tab.
    fn current(&self) -> Option<Rc<OpenFile>> {
        let active = self.active.borrow().clone();
        if let Some(file) = active
            .and_then(|p| self.file(&p))
            .filter(|f| f.detached.borrow().is_some())
        {
            return Some(file);
        }
        let index = usize::try_from(self.tabs.current_index()).ok()?;
        self.attached().get(index).cloned()
    }

    /// Marks `path` as the file being worked in if it is detached (else the
    /// front tab counts again).
    fn set_active(&self, path: &Path) {
        let detached = self
            .file(path)
            .is_some_and(|f| f.detached.borrow().is_some());
        *self.active.borrow_mut() = detached.then(|| path.to_path_buf());
    }

    /// The file shown in the front tab.
    pub fn current_path(&self) -> Option<PathBuf> {
        self.current().map(|f| f.path.clone())
    }

    /// The selected text of the front tab (`None` without selection).
    pub fn selected_text(&self) -> Option<String> {
        self.current()
            .filter(|f| f.label.is_none())?
            .editor
            .selected_text()
            .filter(|t| !t.trim().is_empty())
    }

    /// Path, text and cursor of the front tab.
    pub fn current_state(&self) -> Option<(PathBuf, String, (usize, usize))> {
        let file = self.current().filter(|f| f.label.is_none())?;
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
            .filter(|f| f.label.is_none() && is_rust(&f.path))
            .map(|f| (f.path.clone(), f.editor.text()))
            .collect()
    }

    fn update_titles(&self) {
        for (i, file) in self.attached().iter().enumerate() {
            self.tabs.set_tab_title(i as i32, &file.title());
        }
        for file in self.files.borrow().iter() {
            if let Some(d) = file.detached.borrow().as_ref() {
                let title = file.title();
                d.window.set_title(&format!("Code – {title}"));
                d.grip.set_text(&format!("⠿ {title}"));
            }
        }
        let title = match self.current() {
            Some(file) if file.label.is_some() => format!("Code – {}", file.title()),
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
                    // Haltepunkte wandern mit eingefügten/gelöschten Zeilen.
                    if let Some(file) = ide.code.file(&p) {
                        let old = file.shown.replace(text.clone());
                        ide.code_lines_moved(&p, &old, &text);
                    }
                    ide.code.update_titles();
                    ide.code.text_changed(ide, &p, &text);
                }));
                // Kontextmenü des Editors: markierten Code als Snippet speichern.
                editor.set_extra_menu(&Menu::new().item(
                    "code.save_snippet",
                    &tr!("code.save_snippet"),
                    None,
                ));
                let p = path.to_path_buf();
                let widget = editor.widget().clone();
                editor.on_gutter_clicked(with(ide, move |ide, click: GutterClick| {
                    ide.gutter_clicked(&p, click, &widget);
                }));
                let p = path.to_path_buf();
                editor.on_mouse(with(ide, move |ide, e: MouseEvent| {
                    if e.action == MouseAction::Pressed {
                        ide.code.set_active(&p);
                    }
                    if e.action == MouseAction::Released && e.button == 1 && e.modifiers.ctrl {
                        ide.goto_definition();
                    }
                }));
                self.files.borrow_mut().push(Rc::new(OpenFile {
                    path: path.to_path_buf(),
                    page: RefCell::new(page),
                    detached: RefCell::new(None),
                    editor,
                    saved: RefCell::new(text.clone()),
                    read_only,
                    label: None,
                    base: RefCell::new(None),
                    shown: RefCell::new(text.clone()),
                }));
                self.refresh_marks(path);
                self.refresh_debug_marks(path);
                if is_rust(path) {
                    ide.lsp_opened(path, &text);
                }
                self.files.borrow().len() - 1
            }
        };
        let file = self.files.borrow().get(index).cloned();
        if let Some(file) = file {
            self.show_file(&file);
            file.editor.goto_line(line.max(1));
        }
        self.update_titles();
        Ok(())
    }

    /// Shows a text without a file in a read-only tab (e.g. a diff);
    /// `key` identifies it, a second call with the same key replaces the text.
    pub fn open_text(&self, key: &str, label: &str, text: &str, language: &str) {
        let path = PathBuf::from(format!("freya-tde:{key}"));
        let existing = self.files.borrow().iter().position(|f| f.path == path);
        let index = match existing {
            Some(i) => {
                if let Some(file) = self.files.borrow().get(i) {
                    file.editor.set_read_only(false);
                    file.editor.set_text(text);
                    file.editor.set_read_only(true);
                }
                i
            }
            None => {
                let page = self.tabs.add_tab("");
                let editor = RnclCodeEditor::new(&page);
                editor.set_language(language);
                editor.set_text(text);
                editor.set_read_only(true);
                let e = editor.clone();
                page.widget()
                    .on_resized(move |w, h| e.set_geometry(0, 0, w, h));
                self.files.borrow_mut().push(Rc::new(OpenFile {
                    path,
                    page: RefCell::new(page),
                    detached: RefCell::new(None),
                    editor,
                    saved: RefCell::new(text.to_owned()),
                    read_only: true,
                    label: Some(label.to_owned()),
                    base: RefCell::new(None),
                    shown: RefCell::new(text.to_owned()),
                }));
                self.files.borrow().len() - 1
            }
        };
        let file = self.files.borrow().get(index).cloned();
        if let Some(file) = file {
            self.show_file(&file);
        }
        self.update_titles();
    }

    /// Brings `file` to the front: its own window if detached, else its tab.
    fn show_file(&self, file: &Rc<OpenFile>) {
        if let Some(d) = file.detached.borrow().as_ref() {
            d.window.show();
            *self.active.borrow_mut() = Some(file.path.clone());
            return;
        }
        *self.active.borrow_mut() = None;
        if let Some(tab) = self.tab_of(&file.path) {
            self.tabs.set_current_index(tab as i32);
        }
        self.window.show();
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
        let old = file.shown.replace(text.to_owned());
        ide.code_lines_moved(&file.path, &old, text);
        self.update_titles();
        ide.lsp_changed(&file.path, text);
        true
    }

    /// Someone else (the AI) wrote `text` to `path` on disk: an open file
    /// shows it as its saved state. `false` if it is not open.
    pub fn written(&self, ide: &Ide, path: &Path, text: &str) -> bool {
        let Some(file) = self.file(path) else {
            return false;
        };
        file.editor.set_text(text);
        let old = file.shown.replace(text.to_owned());
        *file.saved.borrow_mut() = text.to_owned();
        ide.code_lines_moved(&file.path, &old, text);
        self.refresh_diff(&file.path);
        self.update_titles();
        ide.lsp_changed(&file.path, text);
        ide.lsp_saved(std::slice::from_ref(&file.path));
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
        let Some(file) = self.file(path) else {
            return;
        };
        let (line, column) = file.editor.cursor();
        if is_rust(path) && completion_trigger(text, line, column) {
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
            self.refresh_diff(&path);
        }
    }

    /// Paths of the open files (not of texts without a file).
    pub fn open_paths(&self) -> Vec<PathBuf> {
        self.files
            .borrow()
            .iter()
            .filter(|f| f.label.is_none())
            .map(|f| f.path.clone())
            .collect()
    }

    /// Sets Git's version of a file to compare with (`None` = no marks).
    pub fn set_git_base(&self, path: &Path, base: Option<String>) {
        if let Some(file) = self.file(path) {
            *file.base.borrow_mut() = base;
        }
        self.refresh_diff(path);
    }

    /// Changed lines of an open file against Git's version.
    pub fn line_changes(&self, path: &Path) -> Vec<crate::git::LineChange> {
        let Some(file) = self.file(path) else {
            return Vec::new();
        };
        let base = file.base.borrow();
        match base.as_deref() {
            Some(base) => crate::git::line_changes(base, &file.editor.text()),
            None => Vec::new(),
        }
    }

    /// Draws the change marks of a file anew.
    fn refresh_diff(&self, path: &Path) {
        let Some(file) = self.file(path) else {
            return;
        };
        let marks = self
            .line_changes(path)
            .into_iter()
            .map(|c| DiffMark {
                kind: match c.kind {
                    crate::git::LineChangeKind::Added => DiffKind::Added,
                    crate::git::LineChangeKind::Modified => DiffKind::Modified,
                    crate::git::LineChangeKind::Deleted => DiffKind::Deleted,
                },
                line: c.line,
                count: c.count,
            })
            .collect();
        file.editor.set_diff_marks(marks);
    }

    /// Shows completion proposals in the front editor.
    pub fn show_completions(&self, items: &[Completion]) {
        if let Some(file) = self.current() {
            file.editor.show_completions(items);
        }
    }

    /// Folds (or unfolds) every region of the front file.
    pub fn set_all_folded(&self, folded: bool) {
        if let Some(file) = self.current() {
            file.editor.set_all_folded(folded);
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

    /// Saves the open files at or below `path` (canonical, see [`key`]).
    pub fn save_below(&self, path: &Path) -> Result<Vec<PathBuf>, String> {
        let mut saved = Vec::new();
        for file in self.files.borrow().iter() {
            if file.path.starts_with(path) && self.save_file(file)? {
                saved.push(file.path.clone());
            }
        }
        self.update_titles();
        Ok(saved)
    }

    /// Unsaved changes in an open file at or below `path` (canonical)?
    pub fn has_unsaved_below(&self, path: &Path) -> bool {
        self.files
            .borrow()
            .iter()
            .any(|f| f.path.starts_with(path) && f.is_dirty())
    }

    /// Closes the files at or below `path` (canonical) without asking –
    /// they were renamed or deleted. Returns their paths and cursor lines.
    pub fn close_below(&self, path: &Path) -> Vec<(PathBuf, usize)> {
        let mut closed = Vec::new();
        let mut index = 0;
        loop {
            let file = self.files.borrow().get(index).cloned();
            let Some(file) = file else {
                break;
            };
            if file.path.starts_with(path) {
                closed.push((file.path.clone(), file.editor.cursor().0));
                self.drop_file(&file);
            } else {
                index += 1;
            }
        }
        self.update_titles();
        closed
    }

    pub fn has_unsaved(&self) -> bool {
        self.files.borrow().iter().any(|f| f.is_dirty())
    }

    /// Reloads files changed on disk (after code generation) that have no
    /// unsaved changes; returns the reloaded files with their new text.
    pub fn reload_from_disk(&self) -> Vec<(PathBuf, String)> {
        let mut reloaded = Vec::new();
        for file in self.files.borrow().iter() {
            if file.is_dirty() || file.label.is_some() {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&file.path) else {
                continue;
            };
            if text != file.editor.text() {
                file.editor.set_text(&text);
                *file.saved.borrow_mut() = text.clone();
                *file.shown.borrow_mut() = text.clone();
                reloaded.push((file.path.clone(), text));
            }
        }
        for (path, _) in &reloaded {
            self.refresh_diff(path);
        }
        self.update_titles();
        reloaded
    }

    /// Shows the file in tab `index` in a window of its own (dragged out of
    /// the tab bar). Its grip, dragged onto the tab bar, puts it back.
    pub fn detach(&self, ide: &Rc<Ide>, index: i32) {
        let Some(file) = usize::try_from(index)
            .ok()
            .and_then(|i| self.attached().get(i).cloned())
        else {
            return;
        };
        let window = RnclWindow::new(&format!("Code – {}", file.title()));
        window.set_menu_bar(menu_bar());
        window.inherit_menu_shortcuts(&ide.main.window);
        let grip = RnclLabel::new(&window, &format!("⠿ {}", file.title()));
        grip.widget().set_tool_tip(
            &tr!("code.grip_tip"),
        );
        grip.widget().set_mouse_cursor("grab");
        grip.widget()
            .set_drag_text(&format!("{DRAG}{}", file.path.display()));
        file.editor.widget().set_parent(window.widget());
        let (g, e) = (grip.clone(), file.editor.clone());
        window.on_resized(move |w, h| {
            g.set_geometry(6, 0, (w - 12).max(0), GRIP);
            e.set_geometry(0, GRIP, w, (h - GRIP).max(0));
        });
        let size = self.window.widget().size();
        window.set_size(size.width.clamp(500, 900), size.height.clamp(400, 900));
        let p = file.path.clone();
        window.on_menu(with(ide, move |ide, id: String| {
            *ide.code.active.borrow_mut() = Some(p.clone());
            code_menu(ide, &id);
        }));
        // Schließen schließt die Datei (mit Rückfrage bei Änderungen).
        let weak = Rc::downgrade(ide);
        let p = file.path.clone();
        window.on_close_requested(move || {
            let Some(ide) = weak.upgrade() else {
                return true;
            };
            let Some(file) = ide.code.file(&p) else {
                return true;
            };
            if let Some(path) = ide.code.close_file(&file) {
                ide.lsp_closed(&path);
            }
            ide.code.update_titles();
            ide.code.file(&p).is_none()
        });
        window.show();
        // Den ursprünglichen Griff aufheben: nur er besitzt das Fenster, mit
        // ihm ginge es samt Editor (RUF zerstört Fenster, deren Besitzer fällt).
        *file.detached.borrow_mut() = Some(Detached { window, grip });
        self.tabs.remove_tab(index);
        *self.active.borrow_mut() = Some(file.path.clone());
        self.update_titles();
    }

    /// Puts a detached file back into a tab (its grip dropped on the tab bar).
    pub fn reattach(&self, path: &Path) {
        let Some(file) = self.file(path) else {
            return;
        };
        let Some(detached) = file.detached.borrow_mut().take() else {
            return;
        };
        // Ans Ende: die Reihenfolge der Reiter folgt der Liste.
        self.files.borrow_mut().retain(|f| !Rc::ptr_eq(f, &file));
        self.files.borrow_mut().push(file.clone());
        let page = self.tabs.add_tab(&file.title());
        file.editor.widget().set_parent(page.widget());
        let e = file.editor.clone();
        page.widget()
            .on_resized(move |w, h| e.set_geometry(0, 0, w, h));
        *file.page.borrow_mut() = page;
        detached.window.widget().set_visible(false);
        ruf::Timer::single_shot(0, move || drop(detached));
        *self.active.borrow_mut() = None;
        if let Some(tab) = self.tab_of(&file.path) {
            self.tabs.set_current_index(tab as i32);
        }
        self.update_titles();
        self.window.show();
    }

    /// Closes the file being worked in (asks about unsaved changes);
    /// returns the path for rust-analyzer.
    pub fn close_current(&self) -> Option<PathBuf> {
        let file = self.current()?;
        self.close_file(&file)
    }

    /// Closes the file in tab `index` („x“ or middle click).
    pub fn close_tab(&self, index: i32) -> Option<PathBuf> {
        let file = self.attached().get(usize::try_from(index).ok()?).cloned()?;
        self.close_file(&file)
    }

    /// Removes `file` with its tab or window, without asking.
    fn drop_file(&self, file: &Rc<OpenFile>) {
        let tab = self.tab_of(&file.path);
        self.files.borrow_mut().retain(|f| !Rc::ptr_eq(f, file));
        let detached = file.detached.borrow_mut().take();
        if let Some(d) = detached {
            // Erst nach dem laufenden Ereignis freigeben (z. B. dem Schließen
            // genau dieses Fensters).
            d.window.widget().set_visible(false);
            ruf::Timer::single_shot(0, move || drop(d));
        } else if let Some(tab) = tab {
            self.tabs.remove_tab(tab as i32);
        }
        if self.active.borrow().as_deref() == Some(file.path.as_path()) {
            *self.active.borrow_mut() = None;
        }
    }

    /// Closes `file` (asks about unsaved changes); returns the path for
    /// rust-analyzer (texts without a file: `None`).
    fn close_file(&self, file: &Rc<OpenFile>) -> Option<PathBuf> {
        let parent = file
            .detached
            .borrow()
            .as_ref()
            .map(|d| d.window.widget().clone())
            .unwrap_or_else(|| self.window.widget().clone());
        if file.is_dirty()
            && !ruf::dialog::confirm(
                Some(&parent),
                &tr!("code.unsaved_title"),
                &tr!("code.unsaved_close", path = file.path.display()),
            )
        {
            return None;
        }
        self.drop_file(file);
        self.update_titles();
        // Texte ohne Datei kennt rust-analyzer nicht.
        file.label.is_none().then(|| file.path.clone())
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

    /// Breakpoints and execution line of `path`.
    fn refresh_debug_marks(&self, path: &Path) {
        let Some(file) = self.file(path) else {
            return;
        };
        let marks = self
            .breakpoints
            .borrow()
            .get(&file.path)
            .cloned()
            .unwrap_or_default();
        if file.editor.breakpoint_marks() != marks {
            file.editor.set_breakpoint_marks(marks);
        }
        let line = self
            .execution
            .borrow()
            .as_ref()
            .filter(|(p, _)| *p == file.path)
            .map(|(_, l)| *l);
        if file.editor.execution_line() != line {
            file.editor.set_execution_line(line);
        }
    }

    fn refresh_all_debug_marks(&self) {
        let paths: Vec<PathBuf> = self.files.borrow().iter().map(|f| f.path.clone()).collect();
        for path in paths {
            self.refresh_debug_marks(&path);
        }
    }

    /// Breakpoints of all files (canonical paths).
    pub fn set_breakpoints(&self, marks: BTreeMap<PathBuf, Vec<BreakpointMark>>) {
        if *self.breakpoints.borrow() != marks {
            *self.breakpoints.borrow_mut() = marks;
            self.refresh_all_debug_marks();
        }
    }

    /// The line the debugger stands on (canonical path); `None` = none.
    pub fn set_execution(&self, at: Option<(&Path, usize)>) {
        let at = at.map(|(p, l)| (p.to_path_buf(), l));
        if *self.execution.borrow() != at {
            *self.execution.borrow_mut() = at;
            self.refresh_all_debug_marks();
        }
    }

    /// Breakpoint marks shown for an open file (automation).
    pub fn breakpoint_marks(&self, path: &Path) -> Vec<BreakpointMark> {
        self.file(path)
            .map(|f| f.editor.breakpoint_marks())
            .unwrap_or_default()
    }

    /// Simulates a click into the gutter.
    pub fn click_gutter(&self, path: &Path, line: usize, button: u8) {
        if let Some(file) = self.file(path) {
            file.editor.widget().simulate_gutter_click(GutterClick {
                line,
                button,
                x: 4,
                y: 4,
            });
        }
    }

    /// The execution line shown in an open file.
    pub fn execution_line(&self, path: &Path) -> Option<usize> {
        self.file(path).and_then(|f| f.editor.execution_line())
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
        *ide.code.active.borrow_mut() = None;
        ide.code.update_titles();
        // Formular und .rs gehören zusammen: der Designer folgt dem Reiter.
        if let Some(form) = ide.code.current_form(ide)
            && ide.session().form_name() != Some(&form)
        {
            ide.select_form(&form);
        }
    }));
    code.window.on_menu(with(ide, |ide, id: String| {
        *ide.code.active.borrow_mut() = None;
        code_menu(ide, &id);
    }));
    // Reiter: „x“/Mittelklick schließt, ins Leere gezogen wird ein eigenes
    // Fenster; dessen Griff auf die Reiterleiste gezogen holt es zurück.
    code.tabs
        .on_tab_close_requested(with(ide, |ide, index: i32| {
            if let Some(path) = ide.code.close_tab(index) {
                ide.lsp_closed(&path);
            }
            ide.code.update_titles();
        }));
    code.tabs
        .on_tab_detach_requested(with(ide, |ide, index: i32| ide.code.detach(ide, index)));
    code.tabs
        .widget()
        .on_drop(with(ide, |ide, event: ruf::DropEvent| {
            if event.action != ruf::DropAction::Drop {
                return;
            }
            if let Some(path) = event.text.as_deref().and_then(|t| t.strip_prefix(DRAG)) {
                ide.code.reattach(Path::new(path));
            }
        }));
}

/// A menu entry of the code window (also of a detached file's window).
fn code_menu(ide: &Rc<Ide>, id: &str) {
    let result = match id {
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
        "code.codetools" => {
            ide.complete_code();
            Ok(())
        }
        "code.save_snippet" => {
            ide.snippet_from_selection();
            Ok(())
        }
        "code.fold_all" | "code.unfold_all" => {
            ide.code.set_all_folded(id == "code.fold_all");
            Ok(())
        }
        "code.designer" => {
            ide.show_window(window::DESIGNER);
            Ok(())
        }
        // Haltepunkt-Menü der Randleiste.
        other if other.starts_with("dbg.") => {
            ide.debug_menu(other);
            Ok(())
        }
        _ => Ok(()),
    };
    if let Err(e) = result {
        ide.main.set_status(&e);
    }
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
