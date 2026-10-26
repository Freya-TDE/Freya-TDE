//! Source control window (like VS Code, see `images/vsc-git*.png`): the
//! repository with branch and ↓/↑, the commit message with the commit
//! button and its variants, the staged and unstaged changes with their
//! status letters, and the commit graph. The logic is [`crate::git`];
//! the window shows [`Ide`]'s Git state and forwards input.

use std::cell::RefCell;
use std::rc::Rc;

use rncl::prelude::*;
use ruf::{Application, Color, ColorScheme, Point, Rect, Shape};

use super::{Ide, with, with0};
use crate::git::{After, Change, Commit, GitOp, GraphRow, RowPos, Status};

const MARGIN: i32 = 6;
const HEADER: i32 = 24;
const MESSAGE: i32 = 64;
const BUTTON: i32 = 30;
const DROPDOWN: i32 = 30;

/// Height of a graph row and width of a lane.
const ROW: i32 = 22;
const LANE: i32 = 12;
const GRAPH_LEFT: i32 = 10;

/// Tree ids of the two groups; files are `s:<path>` and `u:<path>`.
const STAGED: &str = "staged";
const UNSTAGED: &str = "unstaged";

/// Colors of the status letters (as in VS Code).
pub fn change_color(change: Change) -> Color {
    match change {
        Change::Modified | Change::TypeChanged => Color::rgb(0xd1, 0x9a, 0x3c),
        Change::Added | Change::Copied | Change::Untracked | Change::Renamed => {
            Color::rgb(0x4e, 0xa8, 0x5a)
        }
        Change::Deleted => Color::rgb(0xc7, 0x4e, 0x39),
        Change::Conflict => Color::rgb(0xd0, 0x45, 0x8a),
    }
}

/// Lane colors of the graph.
const LANE_COLORS: [Color; 6] = [
    Color::rgb(0x3d, 0x8b, 0xd9),
    Color::rgb(0xb0, 0x6c, 0xd8),
    Color::rgb(0xd1, 0x9a, 0x3c),
    Color::rgb(0x4e, 0xa8, 0x5a),
    Color::rgb(0xd0, 0x45, 0x8a),
    Color::rgb(0x3a, 0xb0, 0xb0),
];

fn lane_color(lane: usize) -> Color {
    LANE_COLORS[lane % LANE_COLORS.len()]
}

fn text_color() -> Color {
    match Application::color_scheme() {
        ColorScheme::Dark => Color::rgb(0xdd, 0xdd, 0xdd),
        _ => Color::rgb(0x22, 0x22, 0x22),
    }
}

fn dim_color() -> Color {
    Color::rgb(0x88, 0x88, 0x88)
}

fn file_name(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, n)| n)
}

fn file_item(prefix: &str, path: &str, change: Change) -> TreeItem {
    let dir = path.rsplit_once('/').map_or("", |(d, _)| d);
    let text = if dir.is_empty() {
        file_name(path).to_owned()
    } else {
        format!("{}   {dir}", file_name(path))
    };
    TreeItem::new(&format!("{prefix}:{path}"), &text)
        .with_color(change_color(change))
        .with_badge(&change.letter().to_string())
}

/// The change tree: „Gestaged“ and „Änderungen“ with counters.
fn change_items(status: &Status) -> Vec<TreeItem> {
    let mut items = Vec::new();
    let staged: Vec<TreeItem> = status
        .staged()
        .map(|(f, c)| file_item("s", &f.path, c))
        .collect();
    if !staged.is_empty() {
        let mut group = TreeItem::new(STAGED, "Gestaged").with_badge(&staged.len().to_string());
        group.children = staged;
        items.push(group);
    }
    let unstaged: Vec<TreeItem> = status
        .unstaged()
        .map(|(f, c)| file_item("u", &f.path, c))
        .collect();
    let mut group = TreeItem::new(UNSTAGED, "Änderungen").with_badge(&unstaged.len().to_string());
    group.children = unstaged;
    items.push(group);
    items
}

/// Keeps the expansion of the groups.
fn keep_expansion(new: &mut [TreeItem], old: &[TreeItem]) {
    for item in new.iter_mut() {
        if let Some(previous) = old.iter().find(|o| o.id == item.id) {
            item.expanded = previous.expanded;
        }
    }
}

fn lane_x(lane: usize) -> i32 {
    GRAPH_LEFT + lane as i32 * LANE
}

/// The graph as shapes: lines, dots, refs and subject per row.
fn graph_shapes(commits: &[Commit], rows: &[GraphRow]) -> Vec<Shape> {
    let mut shapes = Vec::new();
    let fg = text_color();
    for (i, (commit, row)) in commits.iter().zip(rows).enumerate() {
        let top = i as i32 * ROW;
        let y = |pos: RowPos| match pos {
            RowPos::Top => top,
            RowPos::Middle => top + ROW / 2,
            RowPos::Bottom => top + ROW,
        };
        for line in &row.lines {
            shapes.push(Shape::Line {
                from: Point::new(lane_x(line.from.0), y(line.from.1)),
                to: Point::new(lane_x(line.to.0), y(line.to.1)),
                color: lane_color(line.lane),
            });
        }
        let (x, mid) = (lane_x(row.lane), top + ROW / 2);
        let color = lane_color(row.lane);
        shapes.push(Shape::Rect {
            rect: Rect::new(x - 4, mid - 4, 9, 9),
            fill: (!commit.head).then_some(color),
            stroke: Some(color),
            radius: 5,
            dashed: commit.outgoing,
        });
        let lanes = row
            .lines
            .iter()
            .map(|l| l.from.0.max(l.to.0))
            .max()
            .unwrap_or(0)
            .max(row.lane);
        let mut text_x = lane_x(lanes) + 14;
        let subject_color = if commit.incoming { dim_color() } else { fg };
        shapes.push(Shape::Text {
            x: text_x,
            y: top + 3,
            text: commit.subject.clone(),
            color: subject_color,
            size: 13,
            bold: commit.head,
        });
        // Geschätzte Textbreite (13 px ≈ 8 px je Zeichen), danach Autor und Zweige.
        text_x += commit.subject.chars().count() as i32 * 8 + 10;
        shapes.push(Shape::Text {
            x: text_x,
            y: top + 4,
            text: commit.author.clone(),
            color: dim_color(),
            size: 11,
            bold: false,
        });
        text_x += commit.author.chars().count() as i32 * 7 + 10;
        for name in &commit.refs {
            let w = name.chars().count() as i32 * 7 + 14;
            let remote = name.contains('/') && !name.starts_with("tag: ");
            let color = if remote {
                Color::rgb(0xb0, 0x6c, 0xd8)
            } else {
                Color::rgb(0x3d, 0x8b, 0xd9)
            };
            shapes.push(Shape::Rect {
                rect: Rect::new(text_x, top + 3, w, ROW - 6),
                fill: Some(color),
                stroke: None,
                radius: 8,
                dashed: false,
            });
            shapes.push(Shape::Text {
                x: text_x + 6,
                y: top + 4,
                text: name.clone(),
                color: Color::rgb(0xff, 0xff, 0xff),
                size: 11,
                bold: false,
            });
            text_x += w + 4;
        }
    }
    shapes
}

pub struct GitWindow {
    pub window: RnclWindow,
    header: RnclLabel,
    message: RnclTextArea,
    commit: RnclButton,
    variants: RnclButton,
    changes: RnclTreeView,
    graph_title: RnclLabel,
    graph_scroll: RnclScrollView,
    graph: RnclCanvas,
    /// Commits shown in the graph.
    commits: RefCell<Vec<Commit>>,
    /// The tree node the context menu was opened for.
    menu_target: RefCell<String>,
}

fn menu_bar() -> MenuBar {
    vec![(
        "Git".into(),
        Menu::new()
            .item("git.commit", "Commit", Some("Ctrl+Enter"))
            .item("git.commit_amend", "Commit (Amend)", None)
            .item("git.commit_push", "Commit & Push", None)
            .item("git.commit_sync", "Commit & Sync", None)
            .separator()
            .item("git.stage_all", "Alle Änderungen stagen", None)
            .item("git.unstage_all", "Alle Änderungen unstagen", None)
            .separator()
            .item("git.fetch", "Fetch", None)
            .item("git.pull", "Pull", None)
            .item("git.push", "Push", None)
            .item("git.sync", "Sync (Pull und Push)", None)
            .separator()
            .item("git.switch", "Zweig wechseln …", None)
            .item("git.branch", "Neuer Zweig …", None)
            .separator()
            .item("git.refresh", "Aktualisieren", Some("F5"))
            .item("git.init", "Repository anlegen", None),
    )]
}

fn commit_menu() -> Menu {
    Menu::new()
        .item("git.commit", "Commit", None)
        .item("git.commit_amend", "Commit (Amend)", None)
        .separator()
        .item("git.commit_push", "Commit & Push", None)
        .item("git.commit_sync", "Commit & Sync", None)
}

impl GitWindow {
    pub fn build() -> Self {
        let window = RnclWindow::new("Quellcodeverwaltung");
        window.set_menu_bar(menu_bar());
        let header = RnclLabel::new(&window, "");
        let message = RnclTextArea::new(&window);
        message.set_placeholder("Nachricht (Strg+Enter für Commit)");
        let commit = RnclButton::new(&window, "✓ Commit");
        let variants = RnclButton::new(&window, "▾");
        variants
            .widget()
            .set_tool_tip("Commit (Amend), Commit & Push, Commit & Sync");
        let changes = RnclTreeView::new(&window);
        let graph_title = RnclLabel::new(&window, "<b>Graph</b>");
        let graph_scroll = RnclScrollView::new(&window);
        let graph = RnclCanvas::new(graph_scroll.content());
        Self {
            window,
            header,
            message,
            commit,
            variants,
            changes,
            graph_title,
            graph_scroll,
            graph,
            commits: RefCell::new(Vec::new()),
            menu_target: RefCell::new(String::new()),
        }
    }

    fn layout(&self, width: i32, height: i32) {
        let inner = width - 2 * MARGIN;
        let mut y = MARGIN;
        self.header.set_geometry(MARGIN, y, inner, HEADER);
        y += HEADER + 4;
        self.message.set_geometry(MARGIN, y, inner, MESSAGE);
        y += MESSAGE + 4;
        self.commit
            .set_geometry(MARGIN, y, inner - DROPDOWN - 2, BUTTON);
        self.variants
            .set_geometry(width - MARGIN - DROPDOWN, y, DROPDOWN, BUTTON);
        y += BUTTON + 6;
        let rest = (height - y - MARGIN).max(0);
        let changes = rest * 45 / 100;
        self.changes.set_geometry(MARGIN, y, inner, changes);
        y += changes + 4;
        self.graph_title.set_geometry(MARGIN, y, inner, 20);
        y += 22;
        self.graph_scroll
            .set_geometry(MARGIN, y, inner, (height - y - MARGIN).max(0));
        self.layout_graph();
    }

    fn layout_graph(&self) {
        let width = self.graph_scroll.size().width.max(200);
        let rows = self.commits.borrow().len() as i32;
        let height = (rows * ROW).max(ROW);
        self.graph_scroll.set_content_size(width - 4, height);
        self.graph.set_geometry(0, 0, width - 4, height);
    }

    /// Shows the Git state (`None` = no repository).
    pub fn show_status(&self, name: &str, status: Option<&Status>) {
        let Some(status) = status else {
            self.header
                .set_text("Kein Git-Repository – Git → Repository anlegen");
            self.changes.set_items(Vec::new());
            self.commit.set_text("✓ Commit");
            self.commit.widget().set_enabled(false);
            self.variants.widget().set_enabled(false);
            self.window.set_title("Quellcodeverwaltung");
            return;
        };
        let sync = match (status.behind, status.ahead) {
            (0, 0) => String::new(),
            (behind, ahead) => format!("   {behind}↓ {ahead}↑"),
        };
        self.header.set_text(&format!(
            "<b>{name}</b>   ⎇ {}{sync}",
            status.branch_label()
        ));
        let mut items = change_items(status);
        keep_expansion(&mut items, &self.changes.items());
        if items != self.changes.items() {
            self.changes.set_items(items);
        }
        // Wie VS Code: ohne Änderungen, aber mit ausstehenden Commits → Sync.
        let changes = status.files.len();
        let label = if changes == 0 && (status.ahead > 0 || status.behind > 0) {
            format!("⟳ Sync {}↓ {}↑", status.behind, status.ahead)
        } else {
            "✓ Commit".to_owned()
        };
        self.commit.set_text(&label);
        self.commit
            .widget()
            .set_enabled(changes > 0 || status.ahead > 0 || status.behind > 0);
        self.variants.widget().set_enabled(true);
        self.window.set_title(&if changes > 0 {
            format!("Quellcodeverwaltung ({changes})")
        } else {
            "Quellcodeverwaltung".to_owned()
        });
        self.message.set_placeholder(&format!(
            "Nachricht (Strg+Enter für Commit auf „{}“)",
            status.branch.as_deref().unwrap_or("HEAD")
        ));
    }

    pub fn show_log(&self, commits: Vec<Commit>) {
        let rows = crate::git::graph(&commits);
        self.graph.set_shapes(graph_shapes(&commits, &rows));
        *self.commits.borrow_mut() = commits;
        self.layout_graph();
    }

    pub fn message(&self) -> String {
        self.message.text()
    }

    pub fn set_message(&self, text: &str) {
        self.message.set_text(text);
    }

    fn context_menu(&self, id: &str) -> Menu {
        match id.split_once(':') {
            Some(("s", _)) => Menu::new()
                .item("git.file_diff", "Änderungen ansehen", None)
                .item("git.file_open", "Datei öffnen", None)
                .separator()
                .item("git.file_unstage", "Unstagen", None),
            Some(("u", _)) => Menu::new()
                .item("git.file_diff", "Änderungen ansehen", None)
                .item("git.file_open", "Datei öffnen", None)
                .separator()
                .item("git.file_stage", "Stagen", None)
                .item("git.file_discard", "Änderungen verwerfen …", None),
            _ if id == STAGED => Menu::new().item("git.unstage_all", "Alle unstagen", None),
            _ => Menu::new().item("git.stage_all", "Alle stagen", None).item(
                "git.discard_all",
                "Alle verwerfen …",
                None,
            ),
        }
    }

    fn target(&self) -> Option<(bool, String)> {
        let id = self.menu_target.borrow().clone();
        let (group, path) = id.split_once(':')?;
        Some((group == "s", path.to_owned()))
    }

    fn menu(&self, ide: &Ide, id: &str) {
        let commit = |then, amend| {
            ide.git_commit(&self.message(), amend, then);
        };
        match id {
            "git.commit" => {
                let sync = self.commit.text().contains("Sync");
                if sync && self.message().trim().is_empty() {
                    ide.git_start(GitOp::Sync);
                } else {
                    commit(After::Nothing, false);
                }
            }
            "git.commit_amend" => commit(After::Nothing, true),
            "git.commit_push" => commit(After::Push, false),
            "git.commit_sync" => commit(After::Sync, false),
            "git.stage_all" => ide.git_now(GitOp::Stage { paths: vec![] }),
            "git.unstage_all" => ide.git_now(GitOp::Unstage { paths: vec![] }),
            "git.discard_all" => ide.git_discard(Vec::new()),
            "git.fetch" => ide.git_start(GitOp::Fetch),
            "git.pull" => ide.git_start(GitOp::Pull),
            "git.push" => ide.git_start(GitOp::Push),
            "git.sync" => ide.git_start(GitOp::Sync),
            "git.switch" => {
                let menu = ide.git_branch_menu();
                let size = self.header.size();
                self.header.widget().popup_menu(&menu, 40, size.height);
            }
            "git.branch" => {
                if let Some(name) = ruf::dialog::input(
                    Some(self.window.widget()),
                    "Neuer Zweig",
                    "Name des Zweigs:",
                    "",
                ) {
                    ide.git_now(GitOp::CreateBranch {
                        name: name.trim().to_owned(),
                        checkout: true,
                    });
                }
            }
            "git.refresh" => ide.git_refresh(true),
            "git.init" => ide.git_init(),
            "git.file_diff" => {
                if let Some((staged, path)) = self.target() {
                    ide.git_show_diff(&path, staged);
                }
            }
            "git.file_open" => {
                if let Some((_, path)) = self.target() {
                    ide.git_open_file(&path);
                }
            }
            "git.file_stage" => {
                if let Some((_, path)) = self.target() {
                    ide.git_now(GitOp::Stage { paths: vec![path] });
                }
            }
            "git.file_unstage" => {
                if let Some((_, path)) = self.target() {
                    ide.git_now(GitOp::Unstage { paths: vec![path] });
                }
            }
            "git.file_discard" => {
                if let Some((_, path)) = self.target() {
                    ide.git_discard(vec![path]);
                }
            }
            other => {
                if let Some(branch) = other.strip_prefix("git.checkout:") {
                    ide.git_now(GitOp::Checkout {
                        branch: branch.to_owned(),
                    });
                }
            }
        }
    }
}

pub fn connect(ide: &Rc<Ide>) {
    let git = &ide.git_window;
    let weak = Rc::downgrade(ide);
    git.window.on_resized(move |w, h| {
        if let Some(ide) = weak.upgrade() {
            ide.git_window.layout(w, h);
        }
    });
    git.commit
        .on_clicked(with0(ide, |ide| ide.git_window.menu(ide, "git.commit")));
    git.variants.on_clicked(with0(ide, |ide| {
        let w = &ide.git_window;
        let size = w.variants.size();
        w.variants
            .widget()
            .popup_menu(&commit_menu(), 0, size.height);
    }));
    git.changes.on_item_activated(with(ide, |ide, id: String| {
        if let Some((group, path)) = id.split_once(':') {
            ide.git_show_diff(path, group == "s");
        } else {
            ide.git_window.changes.toggle(&id);
        }
    }));
    let mut context = with(ide, |ide, (id, x, y): (String, i32, i32)| {
        let w = &ide.git_window;
        let menu = w.context_menu(&id);
        *w.menu_target.borrow_mut() = id;
        w.changes.widget().popup_menu(&menu, x, y);
    });
    git.changes
        .on_context_menu(move |id, x, y| context((id, x, y)));
    git.window
        .on_menu(with(ide, |ide, id: String| ide.git_window.menu(ide, &id)));
}
