//! Messages window as a panel with tabs (like VS Code's panel): problems
//! (model, code generation, build, rust-analyzer – grouped by file, with a
//! filter), output (channels: build, program, Git, rust-analyzer), the
//! terminal ([`super::terminal_view`]) and the debug console (still to
//! come). The data are [`crate::problems`], [`crate::output`] and
//! [`crate::terminal`]; the window shows them.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use rncl::prelude::*;
use ruf::{Color, Timer};

use super::terminal_view::TerminalView;
use super::{Ide, with, with0};
use crate::problems::{Group, Problem, ProblemSeverity, counts};

const ROW: i32 = 30;
const MARGIN: i32 = 4;

/// How often the output and terminal tabs look for new text.
const POLL: i32 = 40;

/// The tabs, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Problems = 0,
    Output = 1,
    Terminal = 2,
    Debug = 3,
}

fn severity_color(severity: ProblemSeverity) -> Color {
    match severity {
        ProblemSeverity::Error => Color::rgb(0xe5, 0x48, 0x4d),
        ProblemSeverity::Warning => Color::rgb(0xd1, 0x9a, 0x3c),
        ProblemSeverity::Info => Color::rgb(0x3d, 0x8b, 0xd9),
    }
}

pub struct PanelWindow {
    pub window: RnclWindow,
    tabs: RnclTabView,
    // Probleme
    filter: RnclTextField,
    show_errors: RnclCheckBox,
    show_warnings: RnclCheckBox,
    show_infos: RnclCheckBox,
    tree: RnclTreeView,
    /// The problems behind the tree (`p:<index>`).
    shown: RefCell<Vec<Problem>>,
    // Ausgabe
    channel: RnclComboBox,
    clear: RnclButton,
    text: RnclTextArea,
    /// Output version and channel last shown.
    output_seen: Cell<u64>,
    output_channel: RefCell<String>,
    /// Set while the channel list is refilled (its signal must not switch).
    updating: Cell<bool>,
    pub terminal: TerminalView,
    timer: Timer,
}

fn terminal_menu() -> Menu {
    Menu::new()
        .item("term.copy", "Kopieren", Some("Ctrl+Shift+C"))
        .item("term.paste", "Einfügen", Some("Ctrl+Shift+V"))
        .separator()
        .item("term.clear", "Leeren", None)
        .item("term.restart", "Neu starten", None)
}

fn placeholder(page: &RnclPanel, text: &str) -> RnclLabel {
    let label = RnclLabel::new(page, text);
    let l = label.clone();
    page.widget().on_resized(move |w, _| {
        l.set_geometry(12, 12, (w - 24).max(0), 24);
    });
    label
}

impl PanelWindow {
    pub fn build() -> Self {
        let window = RnclWindow::new("Meldungen");
        let tabs = RnclTabView::new(&window);
        let problems = tabs.add_tab("Probleme");
        let output = tabs.add_tab("Ausgabe");
        let terminal = tabs.add_tab("Terminal");
        let debug = tabs.add_tab("Debug-Konsole");

        let filter = RnclTextField::new(&problems);
        filter.set_placeholder("Filter (Text, Datei oder Code)");
        let show_errors = RnclCheckBox::new(&problems, "Fehler");
        let show_warnings = RnclCheckBox::new(&problems, "Warnungen");
        let show_infos = RnclCheckBox::new(&problems, "Hinweise");
        for check in [&show_errors, &show_warnings, &show_infos] {
            check.set_checked(true);
        }
        let tree = RnclTreeView::new(&problems);

        let channel = RnclComboBox::new(&output);
        let clear = RnclButton::new(&output, "Leeren");
        let text = RnclTextArea::new(&output);
        text.set_read_only(true);
        text.set_monospace(true);

        let terminal_view = TerminalView::build(&terminal);
        placeholder(
            &debug,
            "Die Debug-Konsole folgt zusammen mit dem Debugger (Haltepunkte, Variablen).",
        );

        // Seiteninhalte folgen der Größe der Seite.
        {
            let (filter, e, w, i, tree) = (
                filter.clone(),
                show_errors.clone(),
                show_warnings.clone(),
                show_infos.clone(),
                tree.clone(),
            );
            problems.widget().on_resized(move |width, height| {
                let widths = [80, 115, 95];
                let checks: i32 = widths.iter().sum();
                filter.set_geometry(
                    MARGIN,
                    MARGIN,
                    (width - checks - 3 * MARGIN).max(80),
                    ROW - 4,
                );
                let mut x = width - checks - MARGIN;
                for (check, cw) in [&e, &w, &i].into_iter().zip(widths) {
                    check.set_geometry(x, MARGIN, cw, ROW - 4);
                    x += cw;
                }
                tree.set_geometry(0, ROW + MARGIN, width, (height - ROW - MARGIN).max(0));
            });
        }
        {
            let (channel, clear, text) = (channel.clone(), clear.clone(), text.clone());
            output.widget().on_resized(move |width, height| {
                channel.set_geometry(MARGIN, MARGIN, 200, ROW - 4);
                clear.set_geometry(200 + 2 * MARGIN, MARGIN, 80, ROW - 4);
                text.set_geometry(0, ROW + MARGIN, width, (height - ROW - MARGIN).max(0));
            });
        }
        {
            let canvas = terminal_view.canvas.clone();
            terminal.widget().on_resized(move |width, height| {
                canvas.set_geometry(0, 0, width, height);
            });
        }
        Self {
            window,
            tabs,
            filter,
            show_errors,
            show_warnings,
            show_infos,
            tree,
            shown: RefCell::new(Vec::new()),
            channel,
            clear,
            text,
            output_seen: Cell::new(u64::MAX),
            output_channel: RefCell::new(String::new()),
            updating: Cell::new(false),
            terminal: terminal_view,
            timer: Timer::new(),
        }
    }

    pub fn show_tab(&self, tab: Tab) {
        self.tabs.set_current_index(tab as i32);
    }

    pub fn current_tab(&self) -> Tab {
        match self.tabs.current_index() {
            1 => Tab::Output,
            2 => Tab::Terminal,
            3 => Tab::Debug,
            _ => Tab::Problems,
        }
    }

    fn visible(&self, problem: &Problem) -> bool {
        let severity = match problem.severity {
            ProblemSeverity::Error => self.show_errors.is_checked(),
            ProblemSeverity::Warning => self.show_warnings.is_checked(),
            ProblemSeverity::Info => self.show_infos.is_checked(),
        };
        let filter = self.filter.text().to_lowercase();
        let text = || {
            format!(
                "{} {} {}",
                problem.message,
                problem.code.as_deref().unwrap_or_default(),
                problem
                    .file
                    .as_ref()
                    .map(|f| f.display().to_string())
                    .unwrap_or_default()
            )
            .to_lowercase()
        };
        severity && (filter.is_empty() || text().contains(&filter))
    }

    /// Shows the problems (grouped by file, form or project).
    pub fn show_problems(&self, ide: &Ide, problems: Vec<Problem>) {
        let all = counts(&problems);
        let dir = ide.session().project_dir();
        let shown: Vec<Problem> = problems.into_iter().filter(|p| self.visible(p)).collect();
        let mut items: Vec<TreeItem> = Vec::new();
        let mut current: Option<Group> = None;
        for (index, problem) in shown.iter().enumerate() {
            let group = problem.group();
            if current.as_ref() != Some(&group) {
                let label = match &group {
                    Group::Project => "Projekt".to_owned(),
                    Group::Form(form) => format!("Formular {form}"),
                    Group::File(file) => {
                        let name = file
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        let folder = file
                            .parent()
                            .and_then(|p| match &dir {
                                Some(dir) => {
                                    p.strip_prefix(dir).ok().map(|r| r.display().to_string())
                                }
                                None => Some(p.display().to_string()),
                            })
                            .unwrap_or_default();
                        if folder.is_empty() {
                            name
                        } else {
                            format!("{name}   {folder}")
                        }
                    }
                };
                items.push(TreeItem::new(&format!("g:{index}"), &label));
                current = Some(group);
            }
            let code = problem
                .code
                .as_ref()
                .map(|c| format!("  [{c}]"))
                .unwrap_or_default();
            let at = match (problem.line, problem.column) {
                (Some(line), Some(column)) => format!("  Z. {line}:{}", column + 1),
                (Some(line), None) => format!("  Z. {line}"),
                _ => problem
                    .element
                    .as_ref()
                    .map(|e| format!("  {e}"))
                    .unwrap_or_default(),
            };
            let item = TreeItem::new(
                &format!("p:{index}"),
                &format!(
                    "{}{code}  ({}){at}",
                    problem.message,
                    problem.source.label()
                ),
            )
            .with_color(severity_color(problem.severity))
            .with_badge(problem.severity.label());
            if let Some(group) = items.last_mut() {
                group.children.push(item);
            }
        }
        for group in &mut items {
            group.badge = Some(group.children.len().to_string());
        }
        if items.is_empty() {
            items.push(TreeItem::new("", "Keine Probleme."));
        }
        // Aufgeklappt bleibt, was der Nutzer nicht zugeklappt hat.
        let old = self.tree.items();
        for item in &mut items {
            if let Some(previous) = old.iter().find(|o| o.text == item.text) {
                item.expanded = previous.expanded;
            }
        }
        if items != old {
            self.tree.set_items(items);
        }
        *self.shown.borrow_mut() = shown;
        let total = all.errors + all.warnings + all.infos;
        self.tabs.set_tab_title(
            Tab::Problems as i32,
            &if total == 0 {
                "Probleme".to_owned()
            } else {
                format!("Probleme ({total})")
            },
        );
        self.window.set_title(&format!(
            "Meldungen – {} Fehler, {} Warnungen",
            all.errors, all.warnings
        ));
    }

    /// Shows a channel of the output (and selects it).
    pub fn show_channel(&self, ide: &Ide, channel: &str) {
        *self.output_channel.borrow_mut() = channel.to_owned();
        self.output_seen.set(u64::MAX);
        self.refresh_output(ide);
    }

    /// Updates the output tab when the text changed.
    fn refresh_output(&self, ide: &Ide) {
        let output = ide.output();
        let channels = output.channels();
        self.updating.set(true);
        if self.channel.items() != channels {
            self.channel.set_items(&channels);
        }
        let mut name = self.output_channel.borrow().clone();
        if !channels.contains(&name) {
            name = channels[0].clone();
            *self.output_channel.borrow_mut() = name.clone();
        }
        if self.channel.current_text() != name {
            self.channel.set_current_text(&name);
        }
        self.updating.set(false);
        let version = output.version();
        if version == self.output_seen.get() {
            return;
        }
        self.output_seen.set(version);
        let text = output.text(&name);
        if text != self.text.text() {
            self.text.set_text(&text);
            // Ans Ende: das Neueste sehen.
            self.text.goto_line(text.lines().count() as i32);
        }
    }

    fn problem(&self, id: &str) -> Option<Problem> {
        let index: usize = id.strip_prefix("p:")?.parse().ok()?;
        self.shown.borrow().get(index).cloned()
    }
}

pub fn connect(ide: &Rc<Ide>) {
    let panel = &ide.panel;
    let weak = Rc::downgrade(ide);
    panel.window.on_resized(move |w, h| {
        if let Some(ide) = weak.upgrade() {
            ide.panel.tabs.set_geometry(0, 0, w, h);
        }
    });
    panel
        .filter
        .on_text_changed(with(ide, |ide, _: String| ide.refresh_problems()));
    for check in [&panel.show_errors, &panel.show_warnings, &panel.show_infos] {
        check.on_toggled(with(ide, |ide, _: bool| ide.refresh_problems()));
    }
    panel
        .tree
        .on_selection_changed(with(ide, |ide, id: String| {
            if ide.is_refreshing() {
                return;
            }
            if let Some(problem) = ide.panel.problem(&id) {
                ide.goto_problem(&problem);
            }
        }));
    panel.tree.on_item_activated(with(ide, |ide, id: String| {
        if id.starts_with("g:") {
            ide.panel.tree.toggle(&id);
        }
    }));
    panel
        .channel
        .on_current_index_changed(with(ide, |ide, _: i32| {
            if ide.is_refreshing() || ide.panel.updating.get() {
                return;
            }
            let name = ide.panel.channel.current_text();
            if !name.is_empty() && name != *ide.panel.output_channel.borrow() {
                ide.panel.show_channel(ide, &name);
            }
        }));
    panel.clear.on_clicked(with0(ide, |ide| {
        let name = ide.panel.output_channel.borrow().clone();
        ide.output().clear(&name);
        ide.panel.refresh_output(ide);
    }));
    panel.timer.on_timeout(with0(ide, |ide| {
        if !ide.panel.window.widget().is_visible() {
            return;
        }
        match ide.panel.current_tab() {
            Tab::Output => ide.panel.refresh_output(ide),
            Tab::Terminal => ide.panel.terminal.redraw(),
            _ => {}
        }
    }));
    panel.timer.start(POLL);
    // Terminal: startet beim ersten Zeigen des Reiters im Projektordner.
    panel
        .tabs
        .on_current_index_changed(with(ide, |ide, index: i32| {
            if index == Tab::Terminal as i32 {
                ide.start_terminal();
            }
        }));
    let canvas = &panel.terminal.canvas;
    let weak = Rc::downgrade(ide);
    canvas.widget().on_resized(move |_, _| {
        if let Some(ide) = weak.upgrade() {
            ide.panel.terminal.resized();
        }
    });
    canvas.on_mouse(with(ide, |ide, event: MouseEvent| {
        if ide.panel.terminal.mouse(event) {
            ide.panel
                .terminal
                .canvas
                .widget()
                .popup_menu(&terminal_menu(), event.x, event.y);
        }
    }));
    canvas.on_key(with(ide, |ide, event: KeyEvent| {
        match ide.panel.terminal.key(event) {
            Some(true) => ide.terminal_paste(),
            Some(false) => ide.restart_terminal(),
            None => {}
        }
        ide.panel.terminal.redraw();
    }));
    panel
        .window
        .on_menu(with(ide, |ide, id: String| match id.as_str() {
            "term.copy" => ide.panel.terminal.copy(),
            "term.paste" => ide.terminal_paste(),
            "term.clear" => ide.panel.terminal.clear(),
            "term.restart" => ide.restart_terminal(),
            _ => {}
        }));
}
