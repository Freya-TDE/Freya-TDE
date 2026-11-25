//! Source control window (like VS Code, see `images/vsc-git*.png`): the
//! repository row with branch, sync and more actions; the commit message
//! (grows with the text) with the commit button and its variants; the
//! staged and unstaged changes (list or tree) with their status letters;
//! the commit graph with its toolbar (branches shown, current commit,
//! fetch, pull, push, refresh). The logic is [`crate::git`]; the window
//! shows [`Ide`]'s Git state and forwards input.

use crate::tr;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use rncl::prelude::*;
use ruf::{Application, Color, ColorScheme, Point, Rect, Shape};

use rncl::SPLITTER_SIZE;

use super::{Ide, with, with0};
use crate::git::{After, Change, Commit, GitOp, GraphRow, LogRefs, RowPos, Status};

const MARGIN: i32 = 6;
const BAR: i32 = 28;
const ICON: i32 = 30;
const BUTTON: i32 = 30;
const DROPDOWN: i32 = 30;
/// The commit message shows 2 to 12 lines.
const MESSAGE_MIN_LINES: i32 = 2;
const MESSAGE_MAX_LINES: i32 = 12;
/// Frame and inner margins of the commit message.
const MESSAGE_PADDING: i32 = 10;
/// Smallest height of the changes and of the graph when dragging the splitter.
const MIN_PART: i32 = 60;

/// The commit shortcut as the user presses it (RUF maps Ctrl to Cmd on macOS).
/// Keys for a commit (Ctrl+Enter, on macOS ⌘+Enter) in the language.
fn commit_key() -> String {
    if cfg!(target_os = "macos") {
        "⌘+Enter".to_owned()
    } else {
        format!("{}+Enter", tr!("keys.ctrl"))
    }
}

/// Height of a graph row and width of a lane.
const ROW: i32 = 22;
const LANE: i32 = 12;
const GRAPH_LEFT: i32 = 10;

/// Tree ids of the two groups; files are `s:<path>` / `u:<path>`, folders
/// (tree view) `sd:<path>` / `ud:<path>`.
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

fn split(path: &str) -> (&str, &str) {
    path.rsplit_once('/').unwrap_or(("", path))
}

fn file_item(prefix: &str, path: &str, change: Change, show_dir: bool) -> TreeItem {
    let (dir, name) = split(path);
    let text = if show_dir && !dir.is_empty() {
        format!("{name}   {dir}")
    } else {
        name.to_owned()
    };
    TreeItem::new(&format!("{prefix}:{path}"), &text)
        .with_color(change_color(change))
        .with_badge(&change.letter().to_string())
}

/// Files of a group as a folder tree (VS Code's „View as Tree“).
fn folder_tree(prefix: &str, files: &[(String, Change)]) -> Vec<TreeItem> {
    #[derive(Default)]
    struct Node {
        folders: BTreeMap<String, Node>,
        files: Vec<(String, Change)>,
    }
    let mut root = Node::default();
    for (path, change) in files {
        let (dir, _) = split(path);
        let mut node = &mut root;
        if !dir.is_empty() {
            for part in dir.split('/') {
                node = node.folders.entry(part.to_owned()).or_default();
            }
        }
        node.files.push((path.clone(), *change));
    }
    fn items(prefix: &str, base: &str, node: &Node) -> Vec<TreeItem> {
        let mut out = Vec::new();
        for (name, child) in &node.folders {
            let path = if base.is_empty() {
                name.clone()
            } else {
                format!("{base}/{name}")
            };
            let mut item = TreeItem::new(&format!("{prefix}d:{path}"), &format!("{name}/"));
            item.children = items(prefix, &path, child);
            out.push(item);
        }
        out.extend(
            node.files
                .iter()
                .map(|(path, change)| file_item(prefix, path, *change, false)),
        );
        out
    }
    items(prefix, "", &root)
}

/// The change tree: „Gestaged“ and „Änderungen“ with counters.
fn change_items(status: &Status, as_tree: bool) -> Vec<TreeItem> {
    let group = |id: &str, label: &str, prefix: &str, files: Vec<(String, Change)>| {
        let mut group = TreeItem::new(id, label).with_badge(&files.len().to_string());
        group.children = if as_tree {
            folder_tree(prefix, &files)
        } else {
            files
                .iter()
                .map(|(p, c)| file_item(prefix, p, *c, true))
                .collect()
        };
        group
    };
    let staged: Vec<(String, Change)> = status.staged().map(|(f, c)| (f.path.clone(), c)).collect();
    let unstaged: Vec<(String, Change)> = status
        .unstaged()
        .map(|(f, c)| (f.path.clone(), c))
        .collect();
    let mut items = Vec::new();
    if !staged.is_empty() {
        items.push(group(STAGED, &tr!("git.staged"), "s", staged));
    }
    items.push(group(UNSTAGED, &tr!("git.sub.changes"), "u", unstaged));
    items
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

fn lane_x(lane: usize) -> i32 {
    GRAPH_LEFT + lane as i32 * LANE
}

/// The graph as shapes: lines, dots, subject, author and refs per row.
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
            italic: false,
            family: None,
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
            italic: false,
            family: None,
        });
        text_x += commit.author.chars().count() as i32 * 7 + 10;
        for name in &commit.refs {
            let w = name.chars().count() as i32 * 7 + 14;
            let color = if name.starts_with("tag: ") {
                Color::rgb(0x4e, 0xa8, 0x5a)
            } else if name.contains('/') {
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
                italic: false,
                family: None,
            });
            text_x += w + 4;
        }
    }
    shapes
}

/// „Weitere Aktionen“ (VS Code's „…“ at the repository); also the
/// window's menu bar.
pub(super) fn more_actions() -> Menu {
    Menu::new()
        .item("git.pull", &tr!("menu.git.pull"), None)
        .item("git.push", &tr!("menu.git.push"), None)
        .item("git.clone", &tr!("menu.git.clone"), None)
        .item("git.checkout_to", &tr!("menu.git.checkout_to"), None)
        .item("git.fetch", &tr!("menu.git.fetch"), None)
        .separator()
        .submenu(
            &tr!("git.sub.commit"),
            Menu::new()
                .item("git.commit", &tr!("menu.git.commit"), Some("Ctrl+Enter"))
                .item("git.commit_amend", &tr!("menu.git.commit_amend"), None)
                .item("git.commit_push", &tr!("menu.git.commit_push"), None)
                .item("git.commit_sync", &tr!("menu.git.commit_sync"), None)
                .item("git.commit_pr", &tr!("menu.git.commit_pr"), None)
                .separator()
                .item("git.undo_commit", &tr!("menu.git.undo_commit"), None),
        )
        .submenu(
            &tr!("git.sub.changes"),
            Menu::new()
                .item("git.stage_all", &tr!("menu.git.stage_all"), None)
                .item("git.unstage_all", &tr!("menu.git.unstage_all"), None)
                .item("git.discard_all", &tr!("menu.git.discard_all"), None),
        )
        .submenu(
            &tr!("git.sub.pull_push"),
            Menu::new()
                .item("git.sync", &tr!("menu.git.sync"), None)
                .separator()
                .item("git.pull", &tr!("menu.git.pull"), None)
                .item("git.pull_rebase", &tr!("menu.git.pull_rebase"), None)
                .item("git.push", &tr!("menu.git.push"), None)
                .separator()
                .item("git.fetch", &tr!("menu.git.fetch"), None)
                .item("git.fetch_all", &tr!("menu.git.fetch_all"), None),
        )
        .submenu(
            &tr!("git.sub.branch"),
            Menu::new()
                .item("git.merge", &tr!("menu.git.merge"), None)
                .item("git.rebase", &tr!("menu.git.rebase"), None)
                .separator()
                .item("git.branch", &tr!("menu.git.branch"), None)
                .item("git.rename_branch", &tr!("menu.git.rename_branch"), None)
                .item("git.delete_branch", &tr!("menu.git.delete_branch"), None)
                .separator()
                .item("git.publish", &tr!("menu.git.publish"), None),
        )
        .submenu(
            &tr!("git.sub.remote"),
            Menu::new()
                .item("git.add_remote", &tr!("menu.git.add_remote"), None)
                .item("git.remove_remote", &tr!("menu.git.remove_remote"), None),
        )
        .submenu(
            &tr!("git.sub.stash"),
            Menu::new()
                .item("git.stash", &tr!("menu.git.stash"), None)
                .item(
                    "git.stash_untracked",
                    &tr!("menu.git.stash_untracked"),
                    None,
                )
                .separator()
                .item(
                    "git.stash_pop_latest",
                    &tr!("menu.git.stash_pop_latest"),
                    None,
                )
                .item(
                    "git.stash_apply_pick",
                    &tr!("menu.git.stash_apply_pick"),
                    None,
                )
                .item("git.stash_pop_pick", &tr!("menu.git.stash_pop_pick"), None)
                .item(
                    "git.stash_drop_pick",
                    &tr!("menu.git.stash_drop_pick"),
                    None,
                ),
        )
        .submenu(
            &tr!("git.sub.tags"),
            Menu::new()
                .item("git.create_tag", &tr!("menu.git.create_tag"), None)
                .item("git.delete_tag", &tr!("menu.git.delete_tag"), None)
                .separator()
                .item("git.push_tags", &tr!("menu.git.push_tags"), None),
        )
        .separator()
        .item("git.output", &tr!("menu.git.output"), None)
        .item("git.init", &tr!("menu.git.init"), None)
        .item("git.refresh", &tr!("menu.git.refresh"), Some("F5"))
}

fn commit_menu() -> Menu {
    Menu::new()
        .item("git.commit", &tr!("menu.git.commit"), None)
        .item("git.commit_amend", &tr!("menu.git.commit_amend"), None)
        .separator()
        .item("git.commit_push", &tr!("menu.git.commit_push"), None)
        .item("git.commit_sync", &tr!("menu.git.commit_sync"), None)
        .item("git.commit_pr", &tr!("menu.git.commit_pr"), None)
}

fn icon_button(window: &RnclPanel, text: &str, tip: &str) -> RnclButton {
    let button = RnclButton::new(window, text);
    button.widget().set_tool_tip(tip);
    button
}

pub struct GitWindow {
    pub window: RnclWindow,
    /// Grip and content (dockable, see `dock`).
    pub pane: super::dock::DockPane,
    repo_name: RnclLabel,
    branch: RnclButton,
    sync: RnclButton,
    more: RnclButton,
    message: RnclTextArea,
    commit: RnclButton,
    variants: RnclButton,
    changes: RnclTreeView,
    /// Between the changes and the graph.
    splitter: RnclSplitter,
    /// Height of the changes set by dragging the splitter.
    split: Cell<Option<i32>>,
    graph_title: RnclLabel,
    graph_refs: RnclButton,
    /// Graph toolbar: current commit, fetch all, pull, push, refresh, view.
    graph_tools: Vec<(RnclButton, &'static str)>,
    graph_scroll: RnclScrollView,
    graph: RnclCanvas,
    /// Commits shown in the graph.
    commits: RefCell<Vec<Commit>>,
    /// The tree node the context menu was opened for.
    menu_target: RefCell<String>,
    /// Changes as a folder tree instead of a list.
    as_tree: Cell<bool>,
    /// Last status (to rebuild the changes when the view changes).
    status: RefCell<Option<Status>>,
}

impl GitWindow {
    pub fn build() -> Self {
        let window = RnclWindow::new(&tr!("menu.window.git"));
        let pane = super::dock::DockPane::with_body(
            &window,
            crate::desktop::window::GIT,
            &tr!("menu.window.git"),
        );
        let body = pane.body().clone();
        window.set_menu_bar(vec![("Git".into(), more_actions())]);
        let repo_name = RnclLabel::new(&body, "");
        let branch = icon_button(&body, "⎇", &tr!("git.checkout_tip"));
        let sync = icon_button(&body, "⟳", &tr!("git.sync_tip"));
        let more = icon_button(&body, "⋯", &tr!("git.more_tip"));
        let message = RnclTextArea::new(&body);
        message.set_placeholder(&tr!("git.message_placeholder", key = commit_key()));
        message.set_word_wrap(true);
        let commit = RnclButton::new(&body, "✓ Commit");
        let variants = icon_button(&body, "▾", &tr!("git.variants_tip"));
        let changes = RnclTreeView::new(&body);
        let splitter = RnclSplitter::new(&body, true);
        let graph_title = RnclLabel::new(&body, "<b>Graph</b>");
        let graph_refs = icon_button(&body, "⎇ Auto", &tr!("git.refs_tip"));
        let graph_tools = [
            // ⊙ statt ◎: ◎ fehlt in den macOS-Schriften.
            ("⊙", "Zum aktuellen Commit", "git.goto_head"),
            ("⤓", "Fetch (alle Remotes)", "git.fetch_all"),
            ("↓", "Pull", "git.pull"),
            ("↑", "Push", "git.push"),
            ("↻", "Aktualisieren", "git.refresh"),
            ("⋯", "Ansicht …", "git.view_menu"),
        ]
        .into_iter()
        .map(|(text, tip, id)| (icon_button(&body, text, tip), id))
        .collect();
        let graph_scroll = RnclScrollView::new(&body);
        let graph = RnclCanvas::new(graph_scroll.content());
        Self {
            window,
            pane,
            repo_name,
            branch,
            sync,
            more,
            message,
            commit,
            variants,
            changes,
            splitter,
            split: Cell::new(None),
            graph_title,
            graph_refs,
            graph_tools,
            graph_scroll,
            graph,
            commits: RefCell::new(Vec::new()),
            menu_target: RefCell::new(String::new()),
            as_tree: Cell::new(false),
            status: RefCell::new(None),
        }
    }

    /// Lines the commit message needs (wrapped text estimated), 2 to 12.
    /// Height of the commit message: the measured text (with wrapping),
    /// 2 to 12 lines.
    fn message_height(&self, width: i32) -> i32 {
        let (text, line) = self
            .message
            .widget()
            .text_height(width)
            .unwrap_or((2 * 18, 18));
        let line = line.max(1);
        let lines = ((text + line - 1) / line).clamp(MESSAGE_MIN_LINES, MESSAGE_MAX_LINES);
        lines * line + MESSAGE_PADDING
    }

    fn layout(&self, width: i32, height: i32) {
        let inner = width - 2 * MARGIN;
        let right = width - MARGIN;
        let mut y = MARGIN;
        // Repository: Name, Zweig, Sync, weitere Aktionen.
        let branch_width = 130.min(inner / 3);
        self.more.set_geometry(right - ICON, y, ICON, BAR);
        self.sync.set_geometry(right - 2 * ICON - 2, y, ICON, BAR);
        self.branch
            .set_geometry(right - 2 * ICON - 4 - branch_width, y, branch_width, BAR);
        self.repo_name
            .set_geometry(MARGIN, y, inner - 2 * ICON - 8 - branch_width, BAR);
        y += BAR + 6;
        let message = self.message_height(inner);
        self.message.set_geometry(MARGIN, y, inner, message);
        y += message + 4;
        self.commit
            .set_geometry(MARGIN, y, inner - DROPDOWN - 2, BUTTON);
        self.variants
            .set_geometry(right - DROPDOWN, y, DROPDOWN, BUTTON);
        y += BUTTON + 6;
        // Änderungen und Graph teilen sich den Rest; der Teiler verschiebt die Grenze.
        let rest = (height - y - MARGIN - SPLITTER_SIZE - BAR - 2).max(0);
        let changes = self.split.get().unwrap_or(rest * 45 / 100).clamp(
            MIN_PART.min(rest),
            (rest - MIN_PART).max(MIN_PART.min(rest)),
        );
        self.changes.set_geometry(MARGIN, y, inner, changes);
        y += changes;
        self.splitter.set_geometry(MARGIN, y, inner, SPLITTER_SIZE);
        y += SPLITTER_SIZE;
        // Graph-Leiste: Titel, Zweigauswahl, Werkzeuge rechts.
        let tools = self.graph_tools.len() as i32;
        for (i, (button, _)) in self.graph_tools.iter().enumerate() {
            let x = right - (tools - i as i32) * (ICON + 2) + 2;
            button.set_geometry(x, y, ICON, BAR);
        }
        let refs_width = 90;
        let refs_x = right - tools * (ICON + 2) - refs_width;
        self.graph_refs.set_geometry(refs_x, y, refs_width, BAR);
        self.graph_title
            .set_geometry(MARGIN, y, (refs_x - MARGIN - 4).max(0), BAR);
        y += BAR + 2;
        self.graph_scroll
            .set_geometry(MARGIN, y, inner, (height - y - MARGIN).max(0));
        self.layout_graph();
    }

    fn relayout(&self) {
        let size = self.pane.body().size();
        self.layout(size.width, size.height);
    }

    fn layout_graph(&self) {
        let width = self.graph_scroll.size().width.max(200);
        let rows = self.commits.borrow().len() as i32;
        let height = (rows * ROW).max(ROW);
        self.graph_scroll.set_content_size(width - 4, height);
        self.graph.set_geometry(0, 0, width - 4, height);
    }

    fn show_changes(&self) {
        let status = self.status.borrow();
        let Some(status) = status.as_ref() else {
            self.changes.set_items(Vec::new());
            return;
        };
        let mut items = change_items(status, self.as_tree.get());
        keep_expansion(&mut items, &self.changes.items());
        if items != self.changes.items() {
            self.changes.set_items(items);
        }
    }

    /// Shows the Git state (`None` = no repository).
    pub fn show_status(&self, name: &str, status: Option<&Status>) {
        *self.status.borrow_mut() = status.cloned();
        self.show_changes();
        let Some(status) = status else {
            self.repo_name.set_text(&tr!("git.no_repo"));
            self.branch.set_text("⎇");
            self.commit.set_text("✓ Commit");
            for button in [&self.commit, &self.variants, &self.branch, &self.sync] {
                button.widget().set_enabled(false);
            }
            self.window.set_title(&tr!("menu.window.git"));
            return;
        };
        self.repo_name.set_text(&format!("<b>{name}</b>"));
        self.branch
            .set_text(&format!("⎇ {}", status.branch_label()));
        let (behind, ahead) = (status.behind, status.ahead);
        self.sync
            .set_text(if behind + ahead > 0 { "⟳•" } else { "⟳" });
        self.sync.widget().set_tool_tip(&match &status.upstream {
            Some(up) => tr!(
                "git.sync_with",
                behind = behind,
                ahead = ahead,
                upstream = up
            ),
            None => tr!("git.publish_branch"),
        });
        // Wie VS Code: ohne Änderungen, aber mit ausstehenden Commits → Sync.
        let changes = status.files.len();
        let label = if changes == 0 && behind + ahead > 0 {
            format!("⟳ Sync {behind}↓ {ahead}↑")
        } else {
            "✓ Commit".to_owned()
        };
        self.commit.set_text(&label);
        let branch = status.branch.as_deref().unwrap_or("HEAD");
        self.commit
            .widget()
            .set_tool_tip(&tr!("git.commit_tip", branch = branch));
        self.commit
            .widget()
            .set_enabled(changes > 0 || ahead > 0 || behind > 0);
        for button in [&self.variants, &self.branch, &self.sync] {
            button.widget().set_enabled(true);
        }
        self.window.set_title(&if changes > 0 {
            format!("{} ({changes})", tr!("menu.window.git"))
        } else {
            tr!("menu.window.git")
        });
        self.message.set_placeholder(&tr!(
            "git.message_placeholder_branch",
            key = commit_key(),
            branch = branch
        ));
    }

    /// Draws the graph again with the colors of the current scheme.
    pub fn repaint_graph(&self) {
        let commits = self.commits.borrow();
        let rows = crate::git::graph(&commits);
        self.graph.set_shapes(graph_shapes(&commits, &rows));
    }

    pub fn show_log(&self, commits: Vec<Commit>, refs: &LogRefs) {
        let rows = crate::git::graph(&commits);
        self.graph.set_shapes(graph_shapes(&commits, &rows));
        *self.commits.borrow_mut() = commits;
        self.graph_refs.set_text(&match refs {
            LogRefs::Auto => "⎇ Auto".to_owned(),
            LogRefs::All => format!("⎇ {}", tr!("git.refs_all")),
            LogRefs::Some(_) => format!("⎇ {}", tr!("git.refs_some")),
        });
        self.layout_graph();
    }

    /// Scrolls the graph to the commit HEAD points to.
    fn goto_head(&self) {
        if let Some(row) = self.commits.borrow().iter().position(|c| c.head) {
            self.graph_scroll
                .scroll_to(0, ((row as i32) - 2).max(0) * ROW);
        }
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
                .item("git.file_diff", &tr!("git.ctx.diff"), None)
                .item("git.file_open", &tr!("git.ctx.open"), None)
                .separator()
                .item("git.file_unstage", &tr!("git.ctx.unstage"), None),
            Some(("u", _)) => Menu::new()
                .item("git.file_diff", &tr!("git.ctx.diff"), None)
                .item("git.file_open", &tr!("git.ctx.open"), None)
                .separator()
                .item("git.file_stage", &tr!("git.ctx.stage"), None)
                .item("git.file_discard", &tr!("git.ctx.discard"), None),
            Some(("sd", _)) => {
                Menu::new().item("git.file_unstage", &tr!("git.ctx.unstage_folder"), None)
            }
            Some(("ud", _)) => Menu::new()
                .item("git.file_stage", &tr!("git.ctx.stage_folder"), None)
                .item("git.file_discard", &tr!("git.ctx.discard_folder"), None),
            _ if id == STAGED => {
                Menu::new().item("git.unstage_all", &tr!("menu.git.unstage_all"), None)
            }
            _ => Menu::new()
                .item("git.stage_all", &tr!("menu.git.stage_all"), None)
                .item("git.discard_all", &tr!("menu.git.discard_all"), None),
        }
    }

    /// (staged group, path) of the node the context menu belongs to.
    fn target(&self) -> Option<(bool, String)> {
        let id = self.menu_target.borrow().clone();
        let (group, path) = id.split_once(':')?;
        Some((group.starts_with('s'), path.to_owned()))
    }

    /// A popup under the repository row (choices: branches, stashes …).
    fn pick(&self, menu: &Menu) {
        if menu.entries.is_empty() {
            return;
        }
        let size = self.branch.size();
        self.branch.widget().popup_menu(menu, 0, size.height);
    }

    fn ask(&self, title: &str, label: &str, text: &str) -> Option<String> {
        ruf::dialog::input(Some(self.window.widget()), title, label, text)
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty())
    }

    fn view_menu(&self) -> Menu {
        let (list, tree) = (tr!("git.view_list"), tr!("git.view_tree"));
        let (list, tree) = if self.as_tree.get() {
            (list, format!("✓ {tree}"))
        } else {
            (format!("✓ {list}"), tree)
        };
        Menu::new()
            .item("git.view_list", &list, None)
            .item("git.view_tree", &tree, None)
    }

    pub(super) fn menu(&self, ide: &Ide, id: &str) {
        let commit = |then, amend| {
            ide.git_commit(&self.message(), amend, then);
        };
        let repo = || ide.repo();
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
            "git.commit_pr" => commit(After::PullRequest, false),
            "git.undo_commit" => ide.git_now(GitOp::UndoLastCommit),
            "git.stage_all" => ide.git_now(GitOp::Stage { paths: vec![] }),
            "git.unstage_all" => ide.git_now(GitOp::Unstage { paths: vec![] }),
            "git.discard_all" => ide.git_discard(Vec::new()),
            "git.fetch" => ide.git_start(GitOp::Fetch),
            "git.fetch_all" => ide.git_start(GitOp::FetchAll),
            "git.pull" => ide.git_start(GitOp::Pull),
            "git.pull_rebase" => ide.git_start(GitOp::PullRebase),
            "git.push" | "git.publish" => ide.git_start(GitOp::Push),
            "git.sync" => ide.git_start(GitOp::Sync),
            "git.clone" => {
                if let Some(url) = self.ask(&tr!("git.ask.clone"), &tr!("git.ask.clone_url"), "") {
                    ide.git_clone(&url);
                }
            }
            "git.checkout_to" => self.pick(&ide.git_checkout_menu()),
            "git.merge" => self.pick(&ide.git_branch_pick("git.merge:", false)),
            "git.rebase" => self.pick(&ide.git_branch_pick("git.rebase:", false)),
            "git.delete_branch" => self.pick(&ide.git_branch_pick("git.delete_branch:", true)),
            "git.branch" => {
                if let Some(name) =
                    self.ask(&tr!("git.ask.new_branch"), &tr!("git.ask.branch_name"), "")
                {
                    ide.git_now(GitOp::CreateBranch {
                        name,
                        checkout: true,
                    });
                }
            }
            "git.rename_branch" => {
                let current = ide.git_status().and_then(|s| s.branch).unwrap_or_default();
                if let Some(name) = self.ask(
                    &tr!("git.ask.rename_branch"),
                    &tr!("git.ask.new_name"),
                    &current,
                ) {
                    ide.git_now(GitOp::RenameBranch { name });
                }
            }
            "git.add_remote" => {
                if let Some(url) = self.ask(&tr!("git.ask.add_remote"), &tr!("git.ask.address"), "")
                    && let Some(name) =
                        self.ask(&tr!("git.ask.add_remote"), &tr!("git.ask.name"), "origin")
                {
                    ide.git_now(GitOp::AddRemote { name, url });
                }
            }
            "git.remove_remote" => {
                let mut menu = Menu::new();
                for name in repo().and_then(|r| r.remotes().ok()).unwrap_or_default() {
                    menu = menu.item(&format!("git.remove_remote:{name}"), &name, None);
                }
                self.pick(&menu);
            }
            "git.stash" | "git.stash_untracked" => {
                let message =
                    self.ask(&tr!("menu.git.stash"), &tr!("git.ask.message_optional"), "");
                ide.git_now(GitOp::Stash {
                    message: message.unwrap_or_default(),
                    include_untracked: id == "git.stash_untracked",
                });
            }
            "git.stash_pop_latest" => ide.git_now(GitOp::StashApply {
                index: 0,
                pop: true,
            }),
            "git.stash_apply_pick" | "git.stash_pop_pick" | "git.stash_drop_pick" => {
                let prefix = id.trim_end_matches("_pick");
                let mut menu = Menu::new();
                for stash in repo().and_then(|r| r.stashes().ok()).unwrap_or_default() {
                    menu = menu.item(
                        &format!("{prefix}:{}", stash.index),
                        &format!("{}: {}", stash.index, stash.message),
                        None,
                    );
                }
                self.pick(&menu);
            }
            "git.create_tag" => {
                if let Some(name) =
                    self.ask(&tr!("git.ask.create_tag"), &tr!("git.ask.tag_name"), "")
                {
                    let message = self
                        .ask(&tr!("git.ask.create_tag"), &tr!("git.ask.tag_message"), "")
                        .unwrap_or_default();
                    ide.git_now(GitOp::CreateTag { name, message });
                }
            }
            "git.delete_tag" => {
                let mut menu = Menu::new();
                for tag in repo().and_then(|r| r.tags().ok()).unwrap_or_default() {
                    menu = menu.item(&format!("git.delete_tag:{tag}"), &tag, None);
                }
                self.pick(&menu);
            }
            "git.push_tags" => ide.git_start(GitOp::PushTags),
            "git.output" => ide.git_show_output(),
            "git.refresh" => ide.git_refresh(true),
            "git.init" => ide.git_init(),
            "git.goto_head" => self.goto_head(),
            "git.view_menu" => {
                let (button, _) = &self.graph_tools[self.graph_tools.len() - 1];
                let size = button.size();
                button
                    .widget()
                    .popup_menu(&self.view_menu(), 0, size.height);
            }
            "git.view_list" | "git.view_tree" => {
                self.as_tree.set(id == "git.view_tree");
                self.show_changes();
            }
            "git.refs_menu" => {
                let size = self.graph_refs.size();
                self.graph_refs
                    .widget()
                    .popup_menu(&ide.git_refs_menu(), 0, size.height);
            }
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
            other => self.pick_chosen(ide, other),
        }
    }

    /// Entries of the choice popups: `git.<action>:<value>`.
    fn pick_chosen(&self, ide: &Ide, id: &str) {
        let Some((action, value)) = id.split_once(':') else {
            return;
        };
        let value = value.to_owned();
        let index = || value.parse().unwrap_or(usize::MAX);
        match action {
            "git.checkout" => ide.git_now(GitOp::Checkout { branch: value }),
            "git.merge" => ide.git_now(GitOp::Merge { branch: value }),
            "git.rebase" => ide.git_now(GitOp::Rebase { branch: value }),
            "git.delete_branch" => self.delete_branch(ide, value),
            "git.remove_remote" => ide.git_now(GitOp::RemoveRemote { name: value }),
            "git.stash_apply" => ide.git_now(GitOp::StashApply {
                index: index(),
                pop: false,
            }),
            "git.stash_pop" => ide.git_now(GitOp::StashApply {
                index: index(),
                pop: true,
            }),
            "git.stash_drop" => ide.git_now(GitOp::StashDrop { index: index() }),
            "git.delete_tag" => ide.git_now(GitOp::DeleteTag { name: value }),
            "git.refs" => ide.git_toggle_refs(&value),
            _ => {}
        }
    }

    /// Deletes a branch; a not merged one only after a second question.
    fn delete_branch(&self, ide: &Ide, name: String) {
        let parent = Some(self.window.widget());
        if !ruf::dialog::confirm(
            parent,
            &tr!("git.delete_branch"),
            &tr!("git.delete_question", name = name),
        ) {
            return;
        }
        let op = GitOp::DeleteBranch {
            name: name.clone(),
            force: false,
        };
        match ide.git_run(&op) {
            Ok(_) => {}
            Err(e) if e.code == "not_merged" => {
                if ruf::dialog::confirm(
                    parent,
                    &tr!("git.delete_branch"),
                    &tr!("git.delete_anyway", detail = e.detail),
                ) {
                    ide.git_now(GitOp::DeleteBranch { name, force: true });
                }
            }
            Err(e) => ruf::dialog::error(parent, &tr!("git.delete_branch"), &e.to_string()),
        }
    }
}

pub fn connect(ide: &Rc<Ide>) {
    let git = &ide.git_window;
    let weak = Rc::downgrade(ide);
    git.pane.body().widget().on_resized(move |w, h| {
        if let Some(ide) = weak.upgrade() {
            ide.git_window.layout(w, h);
        }
    });
    let click = |button: &RnclButton, id: &'static str| {
        button.on_clicked(with0(ide, move |ide| ide.git_window.menu(ide, id)));
    };
    click(&git.commit, "git.commit");
    click(&git.branch, "git.checkout_to");
    click(&git.sync, "git.sync");
    click(&git.graph_refs, "git.refs_menu");
    for (button, id) in &git.graph_tools {
        click(button, id);
    }
    git.more.on_clicked(with0(ide, |ide| {
        let w = &ide.git_window;
        let size = w.more.size();
        w.more.widget().popup_menu(&more_actions(), 0, size.height);
    }));
    git.variants.on_clicked(with0(ide, |ide| {
        let w = &ide.git_window;
        let size = w.variants.size();
        w.variants
            .widget()
            .popup_menu(&commit_menu(), 0, size.height);
    }));
    // Das Nachrichtenfeld wächst mit dem Text (wie in VS Code).
    git.message.on_text_changed(with(ide, |ide, _: String| {
        let w = &ide.git_window;
        if w.message_height(w.pane.body().size().width - 2 * MARGIN) != w.message.size().height {
            w.relayout();
        }
    }));
    git.splitter.on_dragged(with(ide, |ide, delta: i32| {
        let w = &ide.git_window;
        w.split.set(Some(w.changes.size().height + delta));
        w.relayout();
    }));
    git.changes
        .on_item_activated(with(ide, |ide, id: String| match id.split_once(':') {
            Some(("s" | "u", path)) => ide.git_show_diff(path, id.starts_with("s:")),
            _ => {
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
}
