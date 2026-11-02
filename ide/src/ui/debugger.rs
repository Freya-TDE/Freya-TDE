//! Debugger window (like VS Code's „Run and Debug“ view, as a free-floating
//! window): toolbar (continue, pause, step over/into/out, stop), state,
//! variables, and tabs for watch expressions, call stack, threads and
//! breakpoints. It shows a [`crate::debug::Snapshot`] plus the project's
//! [`DebugSettings`]; all actions go to the [`Ide`].
//!
//! Variables and watch values with children load when expanded; what is
//! expanded stays expanded across stops (by name path).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use rncl::prelude::*;
use ruf::Color;

use super::{Ide, with, with0};
use crate::debug::{DebugCommand, DebugSettings, Snapshot, State, Variable};

const MARGIN: i32 = 6;
const BAR: i32 = 28;
const ICON: i32 = 34;
const SPLITTER_SIZE: i32 = 6;
const MIN_PART: i32 = 60;
/// Separates the names in tree ids of variables.
const SEP: char = '\u{1f}';

const GRAY: Color = Color::rgb(0x8b, 0x8b, 0x8b);
const RED: Color = Color::rgb(0xe5, 0x48, 0x4d);
const ACCENT: Color = Color::rgb(0x3d, 0x8b, 0xd9);

/// The tabs below the variables, in order.
const TAB_WATCH: i32 = 0;

/// `alloc::vec::Vec<i32, alloc::alloc::Global>` → `Vec<i32, Global>`.
fn short_type(name: &str) -> String {
    let mut out = String::new();
    let mut word = String::new();
    let mut chars = name.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_alphanumeric() || c == '_' {
            word.push(c);
        } else if c == ':' && chars.peek() == Some(&':') {
            chars.next();
            word.clear(); // Modulpfad weglassen
        } else {
            out.push_str(&word);
            word.clear();
            out.push(c);
        }
    }
    out.push_str(&word);
    out
}

/// What a variable node stands for (for loading children and the menu).
#[derive(Debug, Clone)]
struct Node {
    reference: i64,
    name: String,
    value: String,
    evaluate_name: Option<String>,
}

pub struct DebuggerWindow {
    pub window: RnclWindow,
    /// Toolbar: button and debugger action.
    tools: Vec<(RnclButton, &'static str)>,
    panic: RnclCheckBox,
    status: RnclLabel,
    variables_title: RnclLabel,
    variables: RnclTreeView,
    splitter: RnclSplitter,
    split: Cell<Option<i32>>,
    tabs: RnclTabView,
    watch_input: RnclTextField,
    watches: RnclTreeView,
    stack: RnclTreeView,
    threads: RnclTreeView,
    breakpoints: RnclTreeView,
    /// Expanded (true) or collapsed nodes; others use their default.
    open: RefCell<HashMap<String, bool>>,
    /// Variable nodes by tree id.
    nodes: RefCell<HashMap<String, Node>>,
    /// The node a context menu was opened for.
    pub menu_target: RefCell<String>,
}

fn tool(window: &RnclWindow, text: &str, tip: &str) -> RnclButton {
    let button = RnclButton::new(window, text);
    button.widget().set_tool_tip(tip);
    button
}

impl DebuggerWindow {
    pub fn build() -> Self {
        let window = RnclWindow::new("Debugger");
        let tools = [
            ("▶", "Starten bzw. Weiter (F9)", "dbg.run"),
            ("❚❚", "Anhalten", "dbg.pause"),
            ("↷", "Gesamte Routine – über Aufrufe (F8)", "dbg.step_over"),
            ("↓", "Einzelne Anweisung – in Aufrufe (F7)", "dbg.step_into"),
            (
                "↑",
                "Bis Rückgabe – zum Aufrufer (Umschalt+F8)",
                "dbg.step_out",
            ),
            ("■", "Programm beenden (Strg+F2)", "dbg.stop"),
        ]
        .into_iter()
        .map(|(text, tip, id)| (tool(&window, text, tip), id))
        .collect();
        let panic = RnclCheckBox::new(&window, "Bei Panic anhalten");
        let status = RnclLabel::new(&window, "");
        let variables_title = RnclLabel::new(&window, "<b>Variablen</b>");
        let variables = RnclTreeView::new(&window);
        let splitter = RnclSplitter::new(&window, true);
        let tabs = RnclTabView::new(&window);
        let watch_page = tabs.add_tab("Überwacht");
        let stack_page = tabs.add_tab("Aufrufstapel");
        let threads_page = tabs.add_tab("Threads");
        let breakpoints_page = tabs.add_tab("Haltepunkte");
        let watch_input = RnclTextField::new(&watch_page);
        watch_input.set_placeholder("Ausdruck überwachen (Enter)");
        let watches = RnclTreeView::new(&watch_page);
        let stack = RnclTreeView::new(&stack_page);
        let threads = RnclTreeView::new(&threads_page);
        let breakpoints = RnclTreeView::new(&breakpoints_page);
        {
            let (input, tree) = (watch_input.clone(), watches.clone());
            watch_page.widget().on_resized(move |w, h| {
                input.set_geometry(2, 2, (w - 4).max(0), BAR);
                tree.set_geometry(0, BAR + 4, w, (h - BAR - 4).max(0));
            });
        }
        for (page, tree) in [
            (&stack_page, &stack),
            (&threads_page, &threads),
            (&breakpoints_page, &breakpoints),
        ] {
            let tree = tree.clone();
            page.widget()
                .on_resized(move |w, h| tree.set_geometry(0, 0, w, h));
        }
        Self {
            window,
            tools,
            panic,
            status,
            variables_title,
            variables,
            splitter,
            split: Cell::new(None),
            tabs,
            watch_input,
            watches,
            stack,
            threads,
            breakpoints,
            open: RefCell::new(HashMap::new()),
            nodes: RefCell::new(HashMap::new()),
            menu_target: RefCell::new(String::new()),
        }
    }

    fn layout(&self, width: i32, height: i32) {
        let inner = width - 2 * MARGIN;
        let mut y = MARGIN;
        let mut x = MARGIN;
        for (button, _) in &self.tools {
            button.set_geometry(x, y, ICON, BAR);
            x += ICON + 2;
        }
        self.panic
            .set_geometry(x + 6, y, (width - x - 6 - MARGIN).max(0), BAR);
        y += BAR + 4;
        self.status.set_geometry(MARGIN, y, inner, 22);
        y += 24;
        self.variables_title.set_geometry(MARGIN, y, inner, 20);
        y += 22;
        let rest = (height - y - MARGIN - SPLITTER_SIZE).max(0);
        let top = self.split.get().unwrap_or(rest * 55 / 100).clamp(
            MIN_PART.min(rest),
            (rest - MIN_PART).max(MIN_PART.min(rest)),
        );
        self.variables.set_geometry(MARGIN, y, inner, top);
        y += top;
        self.splitter.set_geometry(MARGIN, y, inner, SPLITTER_SIZE);
        y += SPLITTER_SIZE;
        self.tabs
            .set_geometry(MARGIN, y, inner, (height - y - MARGIN).max(0));
    }

    fn relayout(&self) {
        let size = self.window.size();
        self.layout(size.width, size.height);
    }

    fn is_open(&self, id: &str, default: bool) -> bool {
        self.open.borrow().get(id).copied().unwrap_or(default)
    }

    /// Tree nodes for the children of `reference`; asks for references that
    /// are expanded but not loaded yet (`wanted`).
    fn variable_items(
        &self,
        snap: &Snapshot,
        variables: &[Variable],
        parent: &str,
        nodes: &mut HashMap<String, Node>,
        wanted: &mut Vec<i64>,
    ) -> Vec<TreeItem> {
        variables
            .iter()
            .map(|v| {
                let id = format!("{parent}{SEP}{}", v.name);
                nodes.insert(
                    id.clone(),
                    Node {
                        reference: v.reference,
                        name: v.name.clone(),
                        value: v.value.clone(),
                        evaluate_name: v.evaluate_name.clone(),
                    },
                );
                let mut item = TreeItem::new(&id, &format!("{} = {}", v.name, v.value));
                if let Some(t) = &v.type_name {
                    let t = short_type(t);
                    if t.chars().count() <= 28 {
                        item = item.with_badge(&t);
                    }
                }
                self.with_children(item, snap, &id, v.reference, nodes, wanted)
            })
            .collect()
    }

    /// Children of a node with `reference` (0 = none): loaded ones, a
    /// placeholder while loading, or a placeholder to make it expandable.
    fn with_children(
        &self,
        mut item: TreeItem,
        snap: &Snapshot,
        id: &str,
        reference: i64,
        nodes: &mut HashMap<String, Node>,
        wanted: &mut Vec<i64>,
    ) -> TreeItem {
        if reference <= 0 {
            return item;
        }
        let open = self.is_open(id, false);
        match snap.variables.get(&reference) {
            Some(children) if open => {
                item.children = self.variable_items(snap, children, id, nodes, wanted);
            }
            _ => {
                if open {
                    wanted.push(reference);
                }
                item.children = vec![TreeItem::new(&format!("{id}{SEP}…"), "…").with_color(GRAY)];
            }
        }
        item.expanded = open;
        item
    }

    fn set_tree(tree: &RnclTreeView, items: Vec<TreeItem>) {
        if tree.items() != items {
            tree.set_items(items);
        }
    }

    /// Shows the session (`None`: none yet) and the settings. Returns
    /// references to load (expanded but not loaded).
    pub fn refresh(&self, snap: Option<&Snapshot>, settings: &DebugSettings) -> Vec<i64> {
        let state = snap.map(|s| &s.state);
        let stopped = snap.is_some_and(Snapshot::is_stopped);
        let running = matches!(state, Some(State::Running | State::Starting));
        for (button, id) in &self.tools {
            let enabled = match *id {
                "dbg.run" => !running,
                "dbg.pause" => running,
                "dbg.step_over" | "dbg.step_into" | "dbg.step_out" => stopped,
                "dbg.stop" => running || stopped,
                _ => true,
            };
            button.widget().set_enabled(enabled);
        }
        if self.panic.is_checked() != settings.stop_on_panic {
            self.panic.set_checked(settings.stop_on_panic);
        }
        self.status.set_text(&status_text(snap));

        let mut nodes = HashMap::new();
        let mut wanted = Vec::new();
        let empty = Snapshot::default();
        let s = snap.unwrap_or(&empty);

        // Variablen: Bereiche (Lokal aufgeklappt, teure zu).
        let scopes: Vec<TreeItem> = s
            .scopes
            .iter()
            .map(|scope| {
                let id = format!("v{SEP}{}", scope.name);
                let open = self.is_open(&id, !scope.expensive);
                let mut item = TreeItem::new(&id, &scope.name).with_color(ACCENT);
                match s.variables.get(&scope.reference) {
                    Some(vars) if open => {
                        item.children = self.variable_items(s, vars, &id, &mut nodes, &mut wanted);
                    }
                    _ => {
                        if open && scope.reference > 0 {
                            wanted.push(scope.reference);
                        }
                        item.children =
                            vec![TreeItem::new(&format!("{id}{SEP}…"), "…").with_color(GRAY)];
                    }
                }
                nodes.insert(
                    id,
                    Node {
                        reference: scope.reference,
                        name: scope.name.clone(),
                        value: String::new(),
                        evaluate_name: None,
                    },
                );
                item.expanded = open;
                item
            })
            .collect();
        Self::set_tree(&self.variables, scopes);

        // Überwachte Ausdrücke (aus den Einstellungen; Werte aus der Sitzung).
        let watches: Vec<TreeItem> = settings
            .watches
            .iter()
            .enumerate()
            .map(|(i, expression)| {
                let id = format!("w{i}");
                let watch = s.watches.iter().find(|w| &w.expression == expression);
                let (text, color, reference) = match watch {
                    Some(w) if w.value.is_some() => (
                        format!("{expression} = {}", w.value.as_deref().unwrap_or_default()),
                        None,
                        w.reference,
                    ),
                    Some(w) if w.error.is_some() => (
                        format!("{expression}: {}", w.error.as_deref().unwrap_or_default()),
                        Some(RED),
                        0,
                    ),
                    _ => (expression.clone(), Some(GRAY), 0),
                };
                nodes.insert(
                    id.clone(),
                    Node {
                        reference,
                        name: expression.clone(),
                        value: watch.and_then(|w| w.value.clone()).unwrap_or_default(),
                        evaluate_name: Some(expression.clone()),
                    },
                );
                let mut item = TreeItem::new(&id, &text);
                if let Some(color) = color {
                    item = item.with_color(color);
                }
                self.with_children(item, s, &id, reference, &mut nodes, &mut wanted)
            })
            .collect();
        Self::set_tree(&self.watches, watches);

        // Aufrufstapel: der gewählte Rahmen mit ▶, Bibliothekscode grau.
        let frames: Vec<TreeItem> = s
            .frames
            .iter()
            .map(|f| {
                let marker = if Some(f.id) == s.frame { "▶ " } else { "" };
                let mut item = TreeItem::new(&format!("f{}", f.id), &format!("{marker}{}", f.name));
                if let Some(path) = &f.path {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    item = item.with_badge(&format!("{name}:{}", f.line));
                }
                if !f.has_source {
                    item = item.with_color(GRAY);
                }
                item
            })
            .collect();
        Self::set_tree(&self.stack, frames);

        let threads: Vec<TreeItem> = s
            .threads
            .iter()
            .map(|t| {
                let marker = if Some(t.id) == s.thread { "▶ " } else { "" };
                TreeItem::new(&format!("t{}", t.id), &format!("{marker}{}", t.name))
                    .with_badge(&format!("#{}", t.id))
            })
            .collect();
        Self::set_tree(&self.threads, threads);

        let breakpoints: Vec<TreeItem> = settings
            .breakpoints
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let name = b
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let mut text = format!("{name}:{}", b.line);
                if let Some(c) = &b.condition {
                    text.push_str(&format!(" – wenn {c}"));
                }
                if let Some(h) = &b.hit_condition {
                    text.push_str(&format!(" – Treffer {h}"));
                }
                if let Some(m) = &b.log_message {
                    text.push_str(&format!(" – Log: {m}"));
                }
                let mut item = TreeItem::new(&format!("b{i}"), &text);
                if !b.enabled {
                    item = item.with_color(GRAY).with_badge("aus");
                } else if let Some(status) = s
                    .breakpoints
                    .iter()
                    .find(|st| st.path == b.path && st.line == b.line)
                    && !status.verified
                {
                    item = item.with_badge("nicht aufgelöst");
                }
                item
            })
            .collect();
        Self::set_tree(&self.breakpoints, breakpoints);

        *self.nodes.borrow_mut() = nodes;
        wanted.sort_unstable();
        wanted.dedup();
        wanted
    }

    fn node(&self, id: &str) -> Option<Node> {
        self.nodes.borrow().get(id).cloned()
    }

    /// Shows the watch tab.
    pub fn show_watches(&self) {
        self.tabs.set_current_index(TAB_WATCH);
    }
}

fn status_text(snap: Option<&Snapshot>) -> String {
    let Some(snap) = snap else {
        return "Kein Programm. Start mit F9.".into();
    };
    let place = |snap: &Snapshot| {
        snap.location()
            .map(|(path, line)| {
                let name = Path::new(path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                format!(" – {name}:{line}")
            })
            .unwrap_or_default()
    };
    match &snap.state {
        State::Starting => format!("Startet ({}) …", snap.adapter),
        State::Running => "Läuft …".into(),
        State::Stopped {
            reason,
            description,
        } => {
            let why = match reason.as_str() {
                "breakpoint" | "function breakpoint" => "Haltepunkt".to_owned(),
                "step" => "Schritt".to_owned(),
                "pause" => "Angehalten".to_owned(),
                "panic" => "Panic".to_owned(),
                "exception" => description.clone().unwrap_or_else(|| "Ausnahme".into()),
                other => other.to_owned(),
            };
            format!("⏸ {why}{}", place(snap))
        }
        State::Ended {
            exit_code: Some(code),
        } => format!("Beendet (Exit-Code {code})."),
        State::Ended { exit_code: None } => "Beendet.".into(),
    }
}

fn breakpoint_menu() -> Menu {
    Menu::new()
        .item("dbg.bp.toggle", "Aktivieren/Deaktivieren", None)
        .item("dbg.bp.condition", "Bedingung …", None)
        .item("dbg.bp.hit", "Trefferzahl …", None)
        .item("dbg.bp.log", "Logmeldung …", None)
        .item("dbg.bp.goto", "Gehe zur Zeile", None)
        .separator()
        .item("dbg.bp.remove", "Entfernen", None)
        .separator()
        .item("dbg.bp.enable_all", "Alle aktivieren", None)
        .item("dbg.bp.disable_all", "Alle deaktivieren", None)
        .item("dbg.bp.remove_all", "Alle entfernen", None)
}

fn variable_menu() -> Menu {
    Menu::new().item("dbg.var.watch", "Überwachen", None).item(
        "dbg.var.copy",
        "Wert kopieren",
        None,
    )
}

fn watch_menu() -> Menu {
    Menu::new()
        .item("dbg.watch.copy", "Wert kopieren", None)
        .item("dbg.watch.remove", "Entfernen", None)
        .separator()
        .item("dbg.watch.remove_all", "Alle entfernen", None)
}

pub fn connect(ide: &Rc<Ide>) {
    let w = &ide.debugger;
    let weak = Rc::downgrade(ide);
    w.window.on_resized(move |width, height| {
        if let Some(ide) = weak.upgrade() {
            ide.debugger.layout(width, height);
        }
    });
    w.splitter.on_dragged(with(ide, |ide, delta: i32| {
        let w = &ide.debugger;
        w.split.set(Some(w.variables.size().height + delta));
        w.relayout();
    }));
    for (button, id) in &w.tools {
        let id = *id;
        button.on_clicked(with0(ide, move |ide| ide.debug_menu(id)));
    }
    w.panic.on_toggled(with(ide, |ide, on: bool| {
        if !ide.is_refreshing() {
            ide.set_stop_on_panic(on);
        }
    }));
    // Aufklappen merken; Kinder erst jetzt laden.
    for tree in [&w.variables, &w.watches] {
        let mut toggled = with(ide, |ide, (id, open): (String, bool)| {
            ide.debugger.open.borrow_mut().insert(id.clone(), open);
            if open
                && let Some(node) = ide.debugger.node(&id)
                && node.reference > 0
            {
                let _ = ide.debug_command(&DebugCommand::LoadVariables {
                    reference: node.reference,
                });
            }
        });
        tree.on_item_toggled(move |id, open| toggled((id, open)));
    }
    w.watch_input.on_return_pressed(with0(ide, |ide| {
        let text = ide.debugger.watch_input.text();
        if !text.trim().is_empty() {
            ide.add_watch(text.trim());
            ide.debugger.watch_input.set_text("");
        }
    }));
    w.stack.on_selection_changed(with(ide, |ide, id: String| {
        if ide.is_refreshing() {
            return;
        }
        if let Some(frame) = id.strip_prefix('f').and_then(|f| f.parse().ok()) {
            ide.debug_select_frame(frame);
        }
    }));
    w.threads.on_selection_changed(with(ide, |ide, id: String| {
        if ide.is_refreshing() {
            return;
        }
        if let Some(thread) = id.strip_prefix('t').and_then(|t| t.parse().ok()) {
            let _ = ide.debug_command(&DebugCommand::SelectThread { id: thread });
        }
    }));
    w.breakpoints
        .on_item_activated(with(ide, |ide, id: String| {
            ide.breakpoint_context(&id);
            ide.debug_menu("dbg.bp.goto");
        }));
    let context = |tree: &RnclTreeView, menu: fn() -> Menu| {
        let t = tree.clone();
        let mut open = with(ide, move |ide, (id, x, y): (String, i32, i32)| {
            if id.starts_with('b') {
                ide.breakpoint_context(&id);
            }
            ide.debugger.menu_target.replace(id);
            t.widget().popup_menu(&menu(), x, y);
        });
        tree.on_context_menu(move |id, x, y| open((id, x, y)));
    };
    context(&w.breakpoints, breakpoint_menu);
    context(&w.variables, variable_menu);
    context(&w.watches, watch_menu);
    w.window
        .on_menu(with(ide, |ide, id: String| ide.debug_menu(&id)));
}

impl Ide {
    /// Value, name and expression of the variable or watch node the context
    /// menu was opened for.
    pub(super) fn debugger_target(&self) -> Option<(String, String, Option<String>)> {
        let id = self.debugger.menu_target.borrow().clone();
        self.debugger
            .node(&id)
            .map(|n| (n.name, n.value, n.evaluate_name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_types() {
        assert_eq!(
            short_type("alloc::vec::Vec<i32, alloc::alloc::Global>"),
            "Vec<i32, Global>"
        );
        assert_eq!(short_type("&str"), "&str");
        assert_eq!(
            short_type("core::option::Option<alloc::string::String>"),
            "Option<String>"
        );
    }
}
