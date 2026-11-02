//! Debugging in the IDE: starting the program under the debugger (F9),
//! breakpoints in the code editor, the debugger window and the debug
//! console. The work is done by [`crate::debug`]; this keeps the project's
//! [`DebugSettings`] (saved per user) and shows the session's snapshot.
//!
//! Keys like Delphi/Lazarus: F9 start/continue, Ctrl+Shift+F9 start without
//! debugger, Ctrl+F2 stop, F7 step into, F8 step over, Shift+F8 step out,
//! F4 run to cursor, F5 toggle breakpoint, Ctrl+F5 watch expression.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rncl::prelude::*;
use ruf::{BreakpointKind as Kind, GuiSender};

use super::code::key;
use super::panel::Tab;
use super::{Ide, Menu};
use crate::debug::{
    Adapter, Breakpoint, DebugCommand, DebugError, DebugSession, DebugSettings, DebugStore,
    LaunchConfig, Snapshot, State,
};
use crate::desktop::window;
use crate::output::channel;

/// Debugger state of the IDE.
#[derive(Default)]
pub(super) struct Debugging {
    /// The project the settings belong to (its `.ptdr` or folder).
    key: Option<PathBuf>,
    settings: DebugSettings,
    session: Option<DebugSession>,
    /// Wakes the GUI thread after a change; set while a wake-up is queued.
    wake: Option<GuiSender<()>>,
    queued: Arc<AtomicBool>,
    /// Snapshot version last shown.
    seen: u64,
    /// Where the execution line was last shown.
    location: Option<(PathBuf, usize)>,
    /// Start the debugger (not the plain program) after the build.
    after_build: bool,
    /// F4 without a session: run to this line, then drop the temporary breakpoint.
    run_to: Option<(PathBuf, usize)>,
    /// The breakpoint line a context menu is about.
    target: Option<(PathBuf, usize)>,
}

fn kind(breakpoint: &Breakpoint) -> Kind {
    match breakpoint.kind() {
        crate::debug::BreakpointKind::Normal => Kind::Normal,
        crate::debug::BreakpointKind::Conditional => Kind::Conditional,
        crate::debug::BreakpointKind::Log => Kind::Log,
        crate::debug::BreakpointKind::Disabled => Kind::Disabled,
    }
}

/// The identifier (with `.` and `::` paths) at `column` of `line` in `text`.
pub(super) fn word_at(text: &str, line: usize, column: usize) -> Option<String> {
    let chars: Vec<char> = text.lines().nth(line.checked_sub(1)?)?.chars().collect();
    let part = |c: char| c.is_alphanumeric() || c == '_' || c == '.' || c == ':';
    let mut start = column.min(chars.len());
    while start > 0 && part(chars[start - 1]) {
        start -= 1;
    }
    let mut end = column.min(chars.len());
    while end < chars.len() && part(chars[end]) {
        end += 1;
    }
    let word: String = chars[start..end].iter().collect();
    let word = word.trim_matches(|c| c == '.' || c == ':');
    (!word.is_empty()).then(|| word.to_owned())
}

impl Ide {
    // --- Settings (breakpoints, watches) -------------------------------------------

    fn debug_key(&self) -> Option<PathBuf> {
        self.path
            .borrow()
            .clone()
            .or_else(|| self.session().project_dir())
    }

    /// Loads the settings of the active project if it changed.
    fn sync_debug_settings(&self) {
        let key = self.debug_key();
        if self.debugging.borrow().key == key {
            return;
        }
        let settings = match (&key, &self.debug_store_file) {
            (Some(key), Some(file)) => DebugStore::load(file).get(key),
            _ => DebugSettings::default(),
        };
        let mut d = self.debugging.borrow_mut();
        d.key = key;
        d.settings = settings;
    }

    /// Breakpoints and watches of the active project.
    pub fn debug_settings(&self) -> DebugSettings {
        self.sync_debug_settings();
        self.debugging.borrow().settings.clone()
    }

    /// Changes the settings, saves them and passes them to a running session.
    fn change_debug_settings(&self, change: impl FnOnce(&mut DebugSettings)) {
        self.sync_debug_settings();
        let (key, settings) = {
            let mut d = self.debugging.borrow_mut();
            change(&mut d.settings);
            (d.key.clone(), d.settings.clone())
        };
        if let (Some(key), Some(file)) = (key, &self.debug_store_file) {
            let mut store = DebugStore::load(file);
            store.set(&key, settings.clone());
            if let Err(e) = store.save(file) {
                self.main
                    .set_status(&format!("Haltepunkte nicht gespeichert: {e}"));
            }
        }
        if let Some(session) = self.debugging.borrow().session.as_ref()
            && !session.is_ended()
        {
            let _ = session.execute(&DebugCommand::SetBreakpoints {
                breakpoints: settings.breakpoints.clone(),
            });
            let _ = session.execute(&DebugCommand::SetWatches {
                watches: settings.watches.clone(),
            });
        }
        self.debug_refresh(true);
    }

    /// Sets or removes a breakpoint; true = now set.
    pub fn toggle_breakpoint(&self, path: &Path, line: usize) -> bool {
        let path = key(path);
        let mut set = false;
        self.change_debug_settings(|s| set = s.toggle(&path, line));
        set
    }

    /// Adds a breakpoint or replaces the one on its line.
    pub fn set_breakpoint(&self, mut breakpoint: Breakpoint) {
        breakpoint.path = key(&breakpoint.path);
        self.change_debug_settings(|s| s.set(breakpoint));
    }

    pub fn remove_breakpoint(&self, path: &Path, line: usize) {
        let path = key(path);
        self.change_debug_settings(|s| {
            s.remove(&path, line);
        });
    }

    pub fn add_watch(&self, expression: &str) {
        let expression = expression.trim().to_owned();
        if expression.is_empty() {
            return;
        }
        self.change_debug_settings(|s| {
            if !s.watches.contains(&expression) {
                s.watches.push(expression);
            }
        });
        self.show_window(window::DEBUGGER);
        self.debugger.show_watches();
    }

    pub fn remove_watch(&self, index: usize) {
        self.change_debug_settings(|s| {
            if index < s.watches.len() {
                s.watches.remove(index);
            }
        });
    }

    pub fn set_stop_on_panic(&self, enabled: bool) {
        self.change_debug_settings(|s| s.stop_on_panic = enabled);
        if let Some(session) = self.debugging.borrow().session.as_ref() {
            let _ = session.execute(&DebugCommand::SetStopOnPanic { enabled });
        }
    }

    /// Breakpoint marks for the code editor, by file.
    fn breakpoint_marks(&self) -> BTreeMap<PathBuf, Vec<BreakpointMark>> {
        let d = self.debugging.borrow();
        let snap = d.session.as_ref().map(DebugSession::snapshot);
        let mut marks: BTreeMap<PathBuf, Vec<BreakpointMark>> = BTreeMap::new();
        for b in &d.settings.breakpoints {
            let unverified = b.enabled
                && snap.as_ref().is_some_and(|s| {
                    !s.is_ended()
                        && s.breakpoints
                            .iter()
                            .any(|st| st.path == b.path && st.line == b.line && !st.verified)
                });
            marks
                .entry(b.path.clone())
                .or_default()
                .push(BreakpointMark {
                    line: b.line,
                    kind: if unverified {
                        Kind::Unverified
                    } else {
                        kind(b)
                    },
                });
        }
        marks
    }

    /// Lines of an open file moved while typing: breakpoints move along.
    pub(super) fn code_lines_moved(&self, path: &Path, old: &str, new: &str) {
        let Some((after, delta)) = crate::debug::line_shift(old, new) else {
            return;
        };
        self.sync_debug_settings();
        let path = key(path);
        if !self
            .debugging
            .borrow()
            .settings
            .breakpoints
            .iter()
            .any(|b| b.path == path && b.line > after)
        {
            return;
        }
        self.change_debug_settings(|s| s.shift_lines(&path, after, delta));
    }

    // --- Session ------------------------------------------------------------------

    /// The running (or last) session's state.
    pub fn debug_snapshot(&self) -> Option<Snapshot> {
        self.debugging
            .borrow()
            .session
            .as_ref()
            .map(DebugSession::snapshot)
    }

    /// Runs a debugger command on the session.
    pub fn debug_command(&self, command: &DebugCommand) -> Result<(), DebugError> {
        let result = match self.debugging.borrow().session.as_ref() {
            Some(session) => session.execute(command),
            None => Err(DebugError {
                code: "not_running",
                detail: "Es läuft keine Debug-Sitzung.".into(),
            }),
        };
        if let Err(e) = &result
            && e.code != "not_stopped"
        {
            self.main.set_status(&e.detail);
        }
        self.debug_refresh(false);
        result
    }

    fn debug_active(&self) -> bool {
        self.debugging
            .borrow()
            .session
            .as_ref()
            .is_some_and(|s| !s.is_ended())
    }

    fn debug_stopped(&self) -> bool {
        self.debug_snapshot().is_some_and(|s| s.is_stopped())
    }

    /// Ends a running debug session (on quit).
    pub(super) fn stop_debugging(&self) {
        let session = self.debugging.borrow_mut().session.take();
        drop(session);
    }

    /// F9: continue when stopped, else build and start under the debugger.
    pub fn start_debugging(&self) {
        if self.debug_stopped() {
            let _ = self.debug_command(&DebugCommand::Continue);
            return;
        }
        if self.debug_active() {
            self.main
                .set_status("Das Programm läuft bereits im Debugger.");
            return;
        }
        self.debugging.borrow_mut().after_build = true;
        self.build(true);
        if !self.building.get() {
            self.debugging.borrow_mut().after_build = false;
        }
    }

    /// Called after a successful build that should run: debugger or plain.
    pub(super) fn launch_after_build(&self) {
        let debug = std::mem::take(&mut self.debugging.borrow_mut().after_build);
        if debug {
            self.debug_launch();
        } else {
            self.run_executable();
        }
    }

    pub(super) fn cancel_launch(&self) {
        let mut d = self.debugging.borrow_mut();
        d.after_build = false;
        d.run_to = None;
    }

    fn debug_launch(&self) {
        let Some(exe) = self.executable.borrow().clone() else {
            return;
        };
        let Some(dir) = self.session().project_dir() else {
            return;
        };
        let adapter = match Adapter::find() {
            Ok(adapter) => adapter,
            Err(e) => {
                self.output.line(channel::PROGRAM, &e.detail);
                self.main
                    .set_status(&format!("{} Das Programm startet ohne Debugger.", e.detail));
                self.run_executable();
                return;
            }
        };
        let mut settings = self.debug_settings();
        // F4 ohne Sitzung: vorübergehender Haltepunkt am Ziel.
        if let Some((path, line)) = self.debugging.borrow().run_to.clone()
            && settings.get(&path, line).is_none()
        {
            settings.set(Breakpoint::new(path, line));
        }
        // Ältere Sitzung beenden.
        if let Some(old) = self.debugging.borrow_mut().session.take()
            && !old.is_ended()
        {
            let _ = old.execute(&DebugCommand::Stop);
        }
        let Some(wake) = self.debugging.borrow().wake.clone() else {
            return;
        };
        let queued = self.debugging.borrow().queued.clone();
        let started = DebugSession::start(
            &adapter,
            LaunchConfig::new(&exe, &dir),
            &settings,
            move || {
                // Viele Änderungen hintereinander: nur ein Auftrag an die Oberfläche.
                if !queued.swap(true, Ordering::AcqRel) {
                    wake.send(());
                }
            },
        );
        match started {
            Ok(session) => {
                {
                    let mut d = self.debugging.borrow_mut();
                    d.session = Some(session);
                    d.seen = u64::MAX;
                    d.location = None;
                }
                self.main.set_status(&format!(
                    "Debuggen: {} ({})",
                    exe.display(),
                    adapter.program.display()
                ));
                self.show_window(window::DEBUGGER);
                self.show_panel(Tab::Debug);
                self.debug_refresh(true);
            }
            Err(e) => {
                self.main.set_status(&e.detail);
                self.debugging.borrow_mut().run_to = None;
            }
        }
    }

    /// The session changed (from the GUI sender).
    pub(super) fn debug_woken(&self) {
        let queued = self.debugging.borrow().queued.clone();
        queued.store(false, Ordering::Release);
        self.debug_refresh(false);
    }

    pub(super) fn install_debug_wake(&self) {
        let weak = self.me.clone();
        self.debugging.borrow_mut().wake = Some(GuiSender::new(move |()| {
            if let Some(ide) = weak.upgrade() {
                ide.debug_woken();
            }
        }));
    }

    /// Shows the session in the debugger window, the debug console and the
    /// code editor; `force` also without a new snapshot version.
    pub(super) fn debug_refresh(&self, force: bool) {
        self.sync_debug_settings();
        let snap = self.debug_snapshot();
        let version = snap.as_ref().map_or(0, |s| s.version);
        if !force && self.debugging.borrow().seen == version {
            return;
        }
        self.debugging.borrow_mut().seen = version;

        // F4 ohne Sitzung: am Ziel angekommen, vorübergehenden Haltepunkt weg.
        let arrived = snap
            .as_ref()
            .is_some_and(|s| s.is_stopped() || s.is_ended());
        let reached = arrived && self.debugging.borrow_mut().run_to.take().is_some();
        if reached {
            let d = self.debugging.borrow();
            if let Some(session) = d.session.as_ref() {
                let _ = session.execute(&DebugCommand::SetBreakpoints {
                    breakpoints: d.settings.breakpoints.clone(),
                });
            }
        }

        let settings = self.debugging.borrow().settings.clone();
        self.refreshing.set(true);
        let wanted = self.debugger.refresh(snap.as_ref(), &settings);
        self.refreshing.set(false);
        if let Some(session) = self.debugging.borrow().session.as_ref() {
            for reference in wanted {
                let _ = session.execute(&DebugCommand::LoadVariables { reference });
            }
        }
        self.panel.set_debug_console(
            snap.as_ref()
                .map(Snapshot::console_text)
                .unwrap_or_default(),
        );

        // Ausführungszeile: beim Anhalten die Datei zeigen.
        let location = snap
            .as_ref()
            .filter(|s| s.is_stopped())
            .and_then(|s| s.location().map(|(p, l)| (key(p), l)));
        let moved = self.debugging.borrow().location != location;
        if moved {
            self.debugging.borrow_mut().location = location.clone();
            if let Some((path, line)) = &location
                && let Some(ide) = self.me.upgrade()
                && path.is_file()
            {
                let _ = self.code.open(&ide, path, *line);
            }
        }
        self.code
            .set_execution(location.as_ref().map(|(p, l)| (p.as_path(), *l)));
        self.code.set_breakpoints(self.breakpoint_marks());
        if moved
            && let Some(State::Stopped { reason, .. }) = snap.as_ref().map(|s| &s.state)
            && reason == "panic"
        {
            self.main.set_status("Panic – das Programm ist angehalten.");
        }
    }

    pub(super) fn debug_select_frame(&self, frame: i64) {
        let _ = self.debug_command(&DebugCommand::SelectFrame { id: frame });
        if let Some(snap) = self.debug_snapshot()
            && let Some(f) = snap.frames.iter().find(|f| f.id == frame)
            && let Some(path) = &f.path
            && path.is_file()
            && let Some(ide) = self.me.upgrade()
        {
            let path = key(path);
            let _ = self.code.open(&ide, &path, f.line);
            self.code.set_execution(Some((&path, f.line)));
            self.debugging.borrow_mut().location = Some((path, f.line));
        }
    }

    /// Debug console input.
    pub fn debug_evaluate(&self, expression: &str) {
        let _ = self.debug_command(&DebugCommand::Evaluate {
            expression: expression.to_owned(),
        });
    }

    // --- Code editor ----------------------------------------------------------------

    /// Click into the gutter: left toggles, right opens the breakpoint menu.
    pub(super) fn gutter_clicked(&self, path: &Path, click: GutterClick, editor: &Widget) {
        let path = key(path);
        if click.button == 1 {
            self.toggle_breakpoint(&path, click.line);
            return;
        }
        if click.button != 3 {
            return;
        }
        self.sync_debug_settings();
        let existing = self
            .debugging
            .borrow()
            .settings
            .get(&path, click.line)
            .cloned();
        self.debugging.borrow_mut().target = Some((path, click.line));
        let menu = match existing {
            Some(b) => Menu::new()
                .item(
                    "dbg.bp.toggle",
                    if b.enabled {
                        "Haltepunkt deaktivieren"
                    } else {
                        "Haltepunkt aktivieren"
                    },
                    None,
                )
                .item("dbg.bp.condition", "Bedingung …", None)
                .item("dbg.bp.hit", "Trefferzahl …", None)
                .item("dbg.bp.log", "Logmeldung …", None)
                .item("dbg.bp.remove", "Haltepunkt entfernen", None),
            None => Menu::new()
                .item("dbg.bp.add", "Haltepunkt setzen", Some("F5"))
                .item("dbg.bp.condition", "Bedingten Haltepunkt …", None)
                .item("dbg.bp.log", "Logpoint …", None),
        }
        .separator()
        .item("dbg.bp.run_to", "Bis hier ausführen", Some("F4"));
        editor.popup_menu(&menu, click.x, click.y);
    }

    /// Context menu of a breakpoint in the debugger window.
    pub(super) fn breakpoint_context(&self, id: &str) {
        let index: Option<usize> = id.strip_prefix('b').and_then(|i| i.parse().ok());
        let target = index.and_then(|i| {
            self.debugging
                .borrow()
                .settings
                .breakpoints
                .get(i)
                .map(|b| (b.path.clone(), b.line))
        });
        self.debugging.borrow_mut().target = target;
    }

    /// The front file and cursor line.
    fn cursor_line(&self) -> Option<(PathBuf, usize, String, usize)> {
        let (path, text, (line, column)) = self.code.current_state()?;
        Some((key(&path), line, text, column))
    }

    fn edit_breakpoint(
        &self,
        title: &str,
        label: &str,
        field: fn(&mut Breakpoint) -> &mut Option<String>,
    ) {
        let Some((path, line)) = self.debugging.borrow().target.clone() else {
            return;
        };
        let mut breakpoint = self
            .debug_settings()
            .get(&path, line)
            .cloned()
            .unwrap_or_else(|| Breakpoint::new(&path, line));
        let current = field(&mut breakpoint).clone().unwrap_or_default();
        let Some(text) =
            ruf::dialog::input(Some(self.code.window.widget()), title, label, &current)
        else {
            return;
        };
        let text = text.trim();
        *field(&mut breakpoint) = (!text.is_empty()).then(|| text.to_owned());
        self.set_breakpoint(breakpoint);
    }

    // --- Menu actions ---------------------------------------------------------------

    /// Debugger actions from the menu „Start“, the debugger window and the
    /// breakpoint menus (`start.…` / `dbg.…`).
    pub fn debug_menu(&self, id: &str) {
        let action = id
            .strip_prefix("start.")
            .or_else(|| id.strip_prefix("dbg."))
            .unwrap_or(id);
        let step = |command: DebugCommand| {
            if self.debug_stopped() {
                let _ = self.debug_command(&command);
            } else if !self.debug_active() {
                self.main
                    .set_status("Erst mit F9 im Debugger starten (oder F4: bis Cursor).");
            }
        };
        match action {
            "run" => self.start_debugging(),
            "run_plain" => self.run(),
            "pause" => {
                let _ = self.debug_command(&DebugCommand::Pause);
            }
            "stop" => {
                if self.debug_active() {
                    let _ = self.debug_command(&DebugCommand::Stop);
                }
            }
            "step_over" => step(DebugCommand::StepOver),
            "step_into" => step(DebugCommand::StepInto),
            "step_out" => step(DebugCommand::StepOut),
            "run_to" => {
                if let Some((path, line, ..)) = self.cursor_line() {
                    self.run_to(path, line);
                }
            }
            "toggle_bp" => {
                if let Some((path, line, ..)) = self.cursor_line() {
                    self.toggle_breakpoint(&path, line);
                }
            }
            "add_watch" => {
                let word = self
                    .cursor_line()
                    .and_then(|(_, line, text, column)| word_at(&text, line, column))
                    .unwrap_or_default();
                if let Some(expression) = ruf::dialog::input(
                    Some(self.main.window.widget()),
                    "Ausdruck überwachen",
                    "Ausdruck:",
                    &word,
                ) {
                    self.add_watch(&expression);
                }
            }
            "remove_all_bp" | "bp.remove_all" => {
                self.change_debug_settings(|s| s.breakpoints.clear())
            }
            "bp.enable_all" => self.change_debug_settings(|s| s.set_all_enabled(true)),
            "bp.disable_all" => self.change_debug_settings(|s| s.set_all_enabled(false)),
            "show" => {
                self.show_window(window::DEBUGGER);
                self.debug_refresh(true);
            }
            "console" => self.show_panel(Tab::Debug),
            // Haltepunkt aus einem Kontextmenü (Randleiste oder Debugger-Fenster).
            "bp.add" | "bp.remove" | "bp.toggle" | "bp.goto" | "bp.run_to" | "bp.condition"
            | "bp.hit" | "bp.log" => {
                if self.debugging.borrow().target.is_none() {
                    let id = self.debugger.menu_target.borrow().clone();
                    self.breakpoint_context(&id);
                }
                let Some((path, line)) = self.debugging.borrow().target.clone() else {
                    return;
                };
                match action {
                    "bp.add" => self.set_breakpoint(Breakpoint::new(&path, line)),
                    "bp.remove" => self.remove_breakpoint(&path, line),
                    "bp.toggle" => {
                        if let Some(mut b) = self.debug_settings().get(&path, line).cloned() {
                            b.enabled = !b.enabled;
                            self.set_breakpoint(b);
                        }
                    }
                    "bp.goto" => {
                        if let Some(ide) = self.me.upgrade() {
                            let _ = self.code.open(&ide, &path, line);
                        }
                    }
                    "bp.run_to" => self.run_to(path, line),
                    "bp.condition" => self.edit_breakpoint(
                        "Bedingung",
                        "Anhalten, wenn dieser Ausdruck wahr ist (leer = immer):",
                        |b| &mut b.condition,
                    ),
                    "bp.hit" => self.edit_breakpoint(
                        "Trefferzahl",
                        "Anhalten beim n-ten Erreichen (z. B. 5; leer = immer):",
                        |b| &mut b.hit_condition,
                    ),
                    _ => self.edit_breakpoint(
                        "Logmeldung",
                        "Statt anzuhalten ausgeben; {ausdruck} wird ersetzt (leer = anhalten):",
                        |b| &mut b.log_message,
                    ),
                }
                self.debugging.borrow_mut().target = None;
            }
            "var.watch" => {
                if let Some((name, _, expression)) = self.debugger_target() {
                    self.add_watch(&expression.unwrap_or(name));
                }
            }
            "var.copy" | "watch.copy" => {
                if let Some((_, value, _)) = self.debugger_target() {
                    ruf::clipboard::set_text(&value);
                }
            }
            "watch.remove" => {
                let id = self.debugger.menu_target.borrow().clone();
                // Nur Ausdrücke selbst (w3), nicht ihre Kinder.
                if let Some(index) = id.strip_prefix('w').and_then(|i| i.parse().ok()) {
                    self.remove_watch(index);
                }
            }
            "watch.remove_all" => self.change_debug_settings(|s| s.watches.clear()),
            _ => {}
        }
    }

    /// F4: continue to `line` (or start and run there).
    fn run_to(&self, path: PathBuf, line: usize) {
        if self.debug_stopped() {
            let _ = self.debug_command(&DebugCommand::RunTo { path, line });
        } else if !self.debug_active() {
            self.debugging.borrow_mut().run_to = Some((path, line));
            self.start_debugging();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words() {
        let text = "let x = self.name.len();\nfoo::bar(a)";
        assert_eq!(word_at(text, 1, 5).as_deref(), Some("x"));
        assert_eq!(word_at(text, 1, 12).as_deref(), Some("self.name.len"));
        assert_eq!(word_at(text, 2, 4).as_deref(), Some("foo::bar"));
        assert_eq!(word_at(text, 1, 6), None);
    }
}
