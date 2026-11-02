//! The windows (RUF). Each is a free-floating top-level window; [`Ide`]
//! owns them together with the [`Session`] and refreshes them after every
//! change.

pub mod about;
mod code;
mod convert;
mod debugger;
mod debugging;
mod designer;
mod git_window;
pub mod icons;
mod inspector;
mod main_window;
pub mod panel;
mod project;
mod project_manager;
pub mod shortcuts;
mod structure;
mod terminal_view;

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::sync::Arc;

use rncl::prelude::*;
use ruf::GuiSender;
use ruf::dialog::{self, FileFilter};
use tdr_core::pack::load_all;
use tdr_core::{ComponentName, TypeRef, TypeRegistry};
use tdr_designer::{Command, CommandError};

use crate::desktop::{Desktop, Place, window};
use crate::files::{self, Entry, FileError, FileIssue, FileOp, Plan};
use crate::git::askpass::Askpass;
use crate::git::{After, Git, GitError, GitOp, LogRefs, Preview, Status};
use crate::lsp::{DiagnosticSeverity, LspClient, LspEvent};
use crate::output::{Output, channel};
use crate::problems::{Problem, ProblemSeverity, ProblemSource};
use crate::projects::{CargoProject, ProjectError, ProjectList, ProjectNode};
use crate::session::{Changes, Session};

use about::AboutWindow;
use code::CodeWindow;
use debugger::DebuggerWindow;
use debugging::Debugging;
use designer::DesignerWindow;
use git_window::GitWindow;
use inspector::InspectorWindow;
use main_window::MainWindow;
use panel::{PanelWindow, Tab};
use project::ProjectWindow;
use shortcuts::ShortcutsWindow;
use structure::StructureWindow;

/// Backend of new projects if its pack is loaded (`widgets/gtk4`), else
/// [`NO_BACKEND`].
const DEFAULT_BACKEND: &str = "gtk4";
/// Projects without UI backend (only non-visual components).
const NO_BACKEND: &str = "none";

/// The backend for a new project: GTK 4 if available.
fn default_backend(registry: &TypeRegistry) -> &'static str {
    if registry.backend(DEFAULT_BACKEND).is_some() {
        DEFAULT_BACKEND
    } else {
        NO_BACKEND
    }
}

/// At most this many completion proposals are shown.
const MAX_COMPLETIONS: usize = 100;

/// Directories searched for component and backend packs.
fn pack_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(list) = std::env::var_os("FREYA_TDE_PACKS") {
        dirs.extend(std::env::split_paths(&list));
    }
    let exe = std::env::current_exe().ok();
    let exe_dir = exe.as_deref().and_then(Path::parent);
    for base in [
        exe_dir.map(Path::to_path_buf),
        exe_dir.and_then(Path::parent).map(Path::to_path_buf),
        // Entwicklung: das Repository.
        Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")),
    ]
    .into_iter()
    .flatten()
    {
        for sub in ["components", "widgets"] {
            let dir = base.join(sub);
            if dir.is_dir() && !dirs.contains(&dir) {
                dirs.push(dir);
            }
        }
    }
    dirs
}

/// Core types plus every pack found; returns the registry and the names of
/// the component packs.
fn full_registry(dirs: &[PathBuf]) -> (TypeRegistry, Vec<String>) {
    let mut registry = TypeRegistry::with_builtins();
    let mut names = Vec::new();
    for dir in dirs {
        let (packs, _) = load_all(dir);
        for pack in packs {
            if let Some(dir) = &pack.dir {
                icons::add_pack_dir(dir);
            }
            if pack.register(&mut registry).is_ok()
                && pack.info.kind == tdr_core::pack::PackKind::Components
            {
                names.push(pack.info.name.clone());
            }
        }
    }
    (registry, names)
}

/// Result of a background Git operation: what, clear the commit message,
/// a cloned folder to open, result.
type GitResult = (String, bool, Option<PathBuf>, Result<String, GitError>);

/// Choices „Auto“ and „Alle“ of the graph's branch menu (`*` is not
/// allowed in branch names).
const REFS_AUTO: &str = "*auto";
const REFS_ALL: &str = "*all";

/// A question of git/ssh and where the answer goes.
type Question = (String, std::sync::mpsc::Sender<Option<String>>);

/// The IDE: session, windows and actions.
pub struct Ide {
    /// Weak self reference (windows connect callbacks while refreshing).
    me: Weak<Ide>,
    session: RefCell<Session>,
    /// The `.ptdr` file; `None` while the project is unsaved.
    path: RefCell<Option<PathBuf>>,
    /// The user's global project list and where it is stored.
    projects: RefCell<ProjectList>,
    projects_file: Option<PathBuf>,
    /// The open main project (its `.ptdr`).
    main_project: RefCell<Option<PathBuf>>,
    /// Further open projects (subprojects of the main project) that are
    /// not active; the active one is `session`/`path`.
    parked: RefCell<Vec<(PathBuf, Session)>>,
    pack_dirs: Vec<PathBuf>,
    desktop: RefCell<Desktop>,
    /// Set while windows are refilled (their signals must not edit).
    refreshing: Cell<bool>,
    main: MainWindow,
    designer: DesignerWindow,
    inspector: InspectorWindow,
    structure: StructureWindow,
    panel: PanelWindow,
    /// Output channels (build, program, Git, rust-analyzer).
    output: Arc<Output>,
    /// Problems of the last code generation and build, and of rust-analyzer per file.
    codegen_problems: RefCell<Vec<Problem>>,
    build_problems: RefCell<Vec<Problem>>,
    lsp_problems: RefCell<std::collections::BTreeMap<PathBuf, Vec<Problem>>>,
    code: CodeWindow,
    project: ProjectWindow,
    git_window: GitWindow,
    debugger: DebuggerWindow,
    shortcuts: ShortcutsWindow,
    about: AboutWindow,
    /// Breakpoints, watches and the debug session.
    debugging: RefCell<Debugging>,
    /// Where breakpoints and watches are kept (`debug.toml`).
    debug_store_file: Option<PathBuf>,
    /// The repository of the project folder, checked again when the folder
    /// changes: (project folder, repository).
    repo: RefCell<Option<(PathBuf, Option<Git>)>>,
    git_status: RefCell<Option<Status>>,
    /// HEAD, upstream and ahead/behind of the shown log (reloaded on change).
    git_log_key: RefCell<String>,
    /// Branches the graph shows.
    graph_refs: RefCell<LogRefs>,
    /// A background Git operation (push, pull …) is running.
    git_busy: Cell<bool>,
    /// Delivers results of background Git operations: (what, clear the
    /// commit message, result).
    git_results: RefCell<Option<GuiSender<GitResult>>>,
    /// Questions of git/ssh (password, passphrase) with the answer channel.
    askpass_questions: RefCell<Option<GuiSender<Question>>>,
    askpass: RefCell<Option<Askpass>>,
    /// Debug binary of the last successful build.
    executable: RefCell<Option<PathBuf>>,
    /// A build is running; start the program when it succeeds.
    building: Cell<bool>,
    run_after_build: Cell<bool>,
    /// Delivers build reports from the build thread.
    build_results: RefCell<Option<GuiSender<tdr_core::preview::BuildReport>>>,
    /// rust-analyzer for the project directory (if it has a `Cargo.toml`).
    lsp: RefCell<Option<LspClient>>,
    /// Counts started servers; events of an older one are dropped.
    lsp_generation: Cell<u64>,
    lsp_ready: Cell<bool>,
    lsp_indexed: Cell<bool>,
    /// Delivers rust-analyzer events (with their generation) to the GUI thread.
    lsp_events: RefCell<Option<GuiSender<(u64, LspEvent)>>>,
    /// Open requests; only the latest answer counts.
    completion_request: Cell<Option<u64>>,
    definition_request: Cell<Option<u64>>,
}

/// A callback that runs `f` with the IDE while it exists.
fn with<A>(ide: &Rc<Ide>, f: impl Fn(&Rc<Ide>, A) + 'static) -> impl FnMut(A) + 'static {
    let weak: Weak<Ide> = Rc::downgrade(ide);
    move |arg| {
        if let Some(ide) = weak.upgrade() {
            f(&ide, arg);
        }
    }
}

fn with0(ide: &Rc<Ide>, f: impl Fn(&Rc<Ide>) + 'static) -> impl FnMut() + 'static {
    let mut call = with(ide, move |ide, ()| f(ide));
    move || call(())
}

impl Ide {
    /// Builds all windows, restores their places and opens `project` (or a
    /// new, unsaved one).
    pub fn start(project: Option<&Path>) -> Rc<Self> {
        let pack_dirs = pack_dirs();
        let (registry, packs) = full_registry(&pack_dirs);
        let backend = default_backend(&registry);
        let session =
            Session::untitled("projekt1", backend, packs, registry).expect("valid project name");
        let ide = Rc::new_cyclic(|me| Self {
            me: me.clone(),
            session: RefCell::new(session),
            path: RefCell::new(None),
            projects: RefCell::new(
                ProjectList::default_path()
                    .map(|p| ProjectList::load(&p))
                    .unwrap_or_default(),
            ),
            projects_file: ProjectList::default_path(),
            main_project: RefCell::new(None),
            parked: RefCell::new(Vec::new()),
            pack_dirs,
            desktop: RefCell::new(Desktop::load()),
            refreshing: Cell::new(false),
            main: MainWindow::build(),
            designer: DesignerWindow::build(),
            inspector: InspectorWindow::build(),
            structure: StructureWindow::build(),
            panel: PanelWindow::build(),
            output: Output::new(),
            codegen_problems: RefCell::new(Vec::new()),
            build_problems: RefCell::new(Vec::new()),
            lsp_problems: RefCell::new(Default::default()),
            code: CodeWindow::build(),
            project: ProjectWindow::build(),
            git_window: GitWindow::build(),
            debugger: DebuggerWindow::build(),
            shortcuts: ShortcutsWindow::build(),
            about: AboutWindow::build(),
            debugging: RefCell::new(Debugging::default()),
            debug_store_file: crate::debug::DebugStore::default_path(),
            repo: RefCell::new(None),
            git_status: RefCell::new(None),
            git_log_key: RefCell::new(String::new()),
            graph_refs: RefCell::new(LogRefs::Auto),
            git_busy: Cell::new(false),
            git_results: RefCell::new(None),
            askpass_questions: RefCell::new(None),
            askpass: RefCell::new(None),
            executable: RefCell::new(None),
            building: Cell::new(false),
            run_after_build: Cell::new(false),
            build_results: RefCell::new(None),
            lsp: RefCell::new(None),
            lsp_generation: Cell::new(0),
            lsp_ready: Cell::new(false),
            lsp_indexed: Cell::new(false),
            lsp_events: RefCell::new(None),
            completion_request: Cell::new(None),
            definition_request: Cell::new(None),
        });
        let weak = Rc::downgrade(&ide);
        *ide.build_results.borrow_mut() = Some(GuiSender::new(move |report| {
            if let Some(ide) = weak.upgrade() {
                ide.build_finished(report);
            }
        }));
        let weak = Rc::downgrade(&ide);
        *ide.lsp_events.borrow_mut() = Some(GuiSender::new(move |(generation, event)| {
            if let Some(ide) = weak.upgrade()
                && generation == ide.lsp_generation.get()
            {
                ide.lsp_event(event);
            }
        }));
        let weak = Rc::downgrade(&ide);
        *ide.git_results.borrow_mut() = Some(GuiSender::new(
            move |(what, clear, open, result): GitResult| {
                if let Some(ide) = weak.upgrade() {
                    ide.git_finished(&what, clear, open, result);
                }
            },
        ));
        let weak = Rc::downgrade(&ide);
        *ide.askpass_questions.borrow_mut() =
            Some(GuiSender::new(move |(question, reply): Question| {
                let answer = weak
                    .upgrade()
                    .and_then(|ide| ide.ask_credentials(&question));
                let _ = reply.send(answer);
            }));
        ide.install_debug_wake();
        main_window::connect(&ide);
        designer::connect(&ide);
        inspector::connect(&ide);
        structure::connect(&ide);
        panel::connect(&ide);
        code::connect(&ide);
        project::connect(&ide);
        git_window::connect(&ide);
        debugger::connect(&ide);
        shortcuts::connect(&ide);
        about::connect(&ide);
        // Wie bei Delphi: Strg+G, F9, Strg+Umschalt+G … aus jedem Fenster.
        for (name, window) in ide.windows() {
            if name != window::MAIN {
                window.inherit_menu_shortcuts(&ide.main.window);
            }
        }
        if let Some(path) = project {
            ide.open_path(path);
        }
        ide.refresh(Changes {
            model: true,
            selection: true,
        });
        ide.restore_desktop();
        ide
    }

    pub fn session(&self) -> std::cell::Ref<'_, Session> {
        self.session.borrow()
    }

    fn windows(&self) -> [(&'static str, &RnclWindow); 10] {
        [
            (window::MAIN, &self.main.window),
            (window::STRUCTURE, &self.structure.window),
            (window::INSPECTOR, &self.inspector.window),
            (window::DESIGNER, &self.designer.window),
            (window::MESSAGES, &self.panel.window),
            (window::CODE, &self.code.window),
            (window::PROJECT, &self.project.window),
            (window::GIT, &self.git_window.window),
            (window::DEBUGGER, &self.debugger.window),
            (window::SHORTCUTS, &self.shortcuts.window),
        ]
    }

    fn restore_desktop(&self) {
        let desktop = self.desktop.borrow().clone();
        self.refreshing.set(true);
        self.project.set_options(desktop.files);
        self.refreshing.set(false);
        self.git_refresh(true);
        self.project.refresh(self);
        for (name, window) in self.windows() {
            let place = desktop.place(name);
            window.set_size(place.width, place.height);
            if let (Some(x), Some(y)) = (place.x, place.y) {
                window.set_screen_position(x, y);
            }
            if place.visible || name == window::MAIN {
                window.show();
            }
        }
    }

    fn save_desktop(&self) {
        let mut desktop = self.desktop.borrow_mut();
        desktop.files = self.project.options();
        for (name, window) in self.windows() {
            let size = window.size();
            let position = window.screen_position();
            desktop.set(
                name,
                Place {
                    x: position.map(|p| p.x),
                    y: position.map(|p| p.y),
                    width: size.width,
                    height: size.height,
                    visible: window.widget().is_visible(),
                },
            );
        }
        if let Err(e) = desktop.save() {
            eprintln!("freya-tde: Fensteranordnung nicht gespeichert: {e}");
        }
    }

    /// Applies designer commands (JSON array, as in `tdr_designer::Command`)
    /// – for scripts, tests and an AI driving the IDE.
    pub fn run_commands(&self, json: &str) -> Result<usize, String> {
        let commands: Vec<Command> = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let count = commands.len();
        for command in commands {
            let result = self.session.borrow_mut().apply(command);
            result.map_err(|e| e.to_string())?;
        }
        self.refresh(Changes {
            model: true,
            selection: true,
        });
        Ok(count)
    }

    /// Saves a screenshot of every visible window as `<dir>/<name>.png`.
    pub fn screenshots(&self, dir: &Path) -> Vec<PathBuf> {
        let _ = std::fs::create_dir_all(dir);
        self.windows()
            .into_iter()
            .filter(|(_, w)| w.widget().is_visible())
            .filter_map(|(name, w)| {
                let path = dir.join(format!("{name}.png"));
                w.widget().save_screenshot(&path).then_some(path)
            })
            .collect()
    }

    /// The design surface – for automation (tests, AI): send mouse and key
    /// events with `simulate_mouse` / `simulate_key`.
    pub fn design_surface(&self) -> Widget {
        self.designer.canvas_widget()
    }

    /// The file in the front tab of the code editor (automation).
    pub fn code_file(&self) -> Option<PathBuf> {
        self.code.current_path()
    }

    /// The element the object inspector currently shows.
    /// Text of a file open in the code editor.
    pub fn code_text(&self, path: &Path) -> Option<String> {
        self.code.text(path)
    }

    /// Replaces the text of a file open in the code editor (unsaved, like
    /// typing); `false` if it is not open.
    pub fn set_code_text(&self, path: &Path, text: &str) -> bool {
        self.code.set_text(self, path, text)
    }

    /// Breakpoints shown in the gutter of an open file.
    pub fn code_breakpoint_marks(&self, path: &Path) -> Vec<BreakpointMark> {
        self.code.breakpoint_marks(path)
    }

    /// The debugger's line shown in an open file.
    pub fn code_execution_line(&self, path: &Path) -> Option<usize> {
        self.code.execution_line(path)
    }

    /// The debug console text.
    pub fn debug_console(&self) -> String {
        self.panel.debug_console()
    }

    /// Simulates a click into the gutter of an open file (automation).
    pub fn click_gutter(&self, path: &Path, line: usize, button: u8) {
        self.code.click_gutter(path, line, button);
    }

    /// Gutter marks of a file open in the code editor.
    pub fn code_marks(&self, path: &Path) -> Vec<LineMark> {
        self.code.marks(path)
    }

    pub fn set_status(&self, text: &str) {
        self.main.set_status(text);
    }

    // --- Project files ---------------------------------------------------------------

    /// The project folder as the project window shows it (automation).
    pub fn project_files(&self) -> Vec<Entry> {
        self.project.entries()
    }

    /// Project files that must not be renamed or deleted in the file
    /// manager: the project file and the forms.
    fn protected_files(&self) -> Vec<String> {
        let session = self.session();
        let doc = session.document();
        let mut protected = vec![format!("{}.ptdr", doc.project().name)];
        protected.extend(
            doc.forms()
                .iter()
                .map(|f| tdr_designer::document::form_path(&f.name)),
        );
        protected
    }

    /// The dry run of a file operation; `None` while the project is unsaved.
    pub fn plan_file_op(&self, op: &FileOp) -> Option<Plan> {
        let root = self.session().project_dir()?;
        Some(files::plan(&root, op, &self.protected_files()))
    }

    /// Performs a file operation in the project folder (no questions):
    /// open files are saved before a rename and reopened at the new place;
    /// deleted ones are closed (unsaved changes are lost).
    pub fn file_op(&self, op: &FileOp) -> Result<Plan, FileError> {
        let Some(root) = self.session().project_dir() else {
            return Err(FileError::Refused {
                issues: vec![FileIssue {
                    severity: tdr_core::Severity::Error,
                    code: "no_project",
                    path: String::new(),
                    detail: "Das Projekt ist noch nicht gespeichert.".to_owned(),
                }],
            });
        };
        let (old, new) = match op {
            FileOp::Rename { from, to } => (Some(from), Some(root.join(to))),
            FileOp::Delete { path, .. } => (Some(path), None),
            _ => (None, None),
        };
        // Vorher kanonisch auflösen: danach gibt es den Pfad nicht mehr.
        let old = old.map(|p| code::key(&root.join(p)));
        if let (Some(old), Some(_)) = (&old, &new) {
            match self.code.save_below(old) {
                Ok(saved) => self.lsp_saved(&saved),
                Err(detail) => {
                    return Err(FileError::Io {
                        path: old.display().to_string(),
                        detail,
                    });
                }
            }
        }
        let plan = files::apply(&root, op, &self.protected_files())?;
        if let Some(old) = &old {
            for (path, line) in self.code.close_below(old) {
                self.lsp_closed(&path);
                if let (Some(new), Ok(rest)) = (&new, path.strip_prefix(old)) {
                    let moved = if rest.as_os_str().is_empty() {
                        new.clone()
                    } else {
                        new.join(rest)
                    };
                    self.open_source(&moved, line);
                }
            }
        }
        self.project.refresh(self);
        Ok(plan)
    }

    /// Unsaved changes in the code editor at or below a project path?
    fn has_unsaved_below(&self, relative: &str) -> bool {
        self.session().project_dir().is_some_and(|root| {
            self.code
                .has_unsaved_below(&code::key(&root.join(relative)))
        })
    }

    /// Opens a project file (relative path): a form in the designer together
    /// with its `.rs`, another project file as a project, anything else in
    /// the code editor.
    pub fn open_project_file(&self, relative: &str) {
        let Some(root) = self.session().project_dir() else {
            return;
        };
        let path = root.join(relative);
        if relative.ends_with(".ftdr") {
            let form = self
                .session()
                .document()
                .forms()
                .iter()
                .find(|f| tdr_designer::document::form_path(&f.name) == relative)
                .map(|f| f.name.clone());
            match form {
                Some(form) => {
                    self.select_form(&form);
                    if let Some(source) = self.session().form_source().filter(|s| s.is_file()) {
                        self.open_source(&source, 1);
                    }
                    self.show_window(window::DESIGNER);
                }
                None => self.set_status(&format!(
                    "{relative} gehört nicht zum Projekt (kein Eintrag in der Projektdatei)."
                )),
            }
        } else if relative.ends_with(".ptdr") {
            if self.path.borrow().as_deref() == Some(path.as_path()) {
                self.set_status("Das ist die geöffnete Projektdatei.");
            } else if self.confirm_discard() {
                self.open_path(&path);
            }
        } else {
            self.open_source(&path, 1);
        }
    }

    /// Timer tick: rescan the project folder and reload open files that
    /// changed on disk (and have no unsaved changes).
    fn files_changed_outside(&self) {
        self.git_refresh(false);
        if self.project.window.widget().is_visible() {
            self.project.refresh(self);
        }
        for (path, text) in self.code.reload_from_disk() {
            self.lsp_changed(&path, &text);
            self.lsp_saved(&[path]);
        }
    }

    // --- Git ------------------------------------------------------------------------

    /// Answers git/ssh questions with a dialog (only in the real program:
    /// the helper is `freya-tde` itself).
    fn askpass(&self) -> Option<Askpass> {
        if let Some(askpass) = self.askpass.borrow().clone() {
            return Some(askpass);
        }
        let exe = std::env::current_exe().ok()?;
        if exe.file_stem().and_then(|s| s.to_str()) != Some("freya-tde") {
            return None;
        }
        let sender = self.askpass_questions.borrow().clone()?;
        let askpass = Askpass::start(exe, move |question| {
            let (reply, answer) = std::sync::mpsc::channel();
            sender.send((question.to_owned(), reply));
            answer.recv().ok().flatten()
        })
        .ok()?;
        *self.askpass.borrow_mut() = Some(askpass.clone());
        Some(askpass)
    }

    fn ask_credentials(&self, question: &str) -> Option<String> {
        let parent = Some(self.git_window.window.widget());
        let question = question.trim();
        if question.starts_with("Username") {
            dialog::input(parent, "Git-Anmeldung", question, "")
        } else {
            dialog::password(parent, "Git-Anmeldung", question)
        }
    }

    /// The Git repository of the project folder (`None`: unsaved project,
    /// no repository or no `git`).
    pub fn repo(&self) -> Option<Git> {
        let dir = self.session().project_dir()?;
        if let Some((cached, repo)) = self.repo.borrow().as_ref()
            && *cached == dir
        {
            return repo.clone();
        }
        let repo = Git::open(&dir).ok().map(|git| {
            git.with_askpass(self.askpass()).with_output(Some({
                let output = self.output.clone();
                Arc::new(move |text: &str| output.append(channel::GIT, text))
            }))
        });
        *self.repo.borrow_mut() = Some((dir, repo.clone()));
        repo
    }

    /// Git status of the project's repository (as last read).
    pub fn git_status(&self) -> Option<Status> {
        self.git_status.borrow().clone()
    }

    /// Git changes by project path (for the marks in the project tree).
    pub fn git_changes(&self) -> project::Changes {
        let (Some(status), Some(repo), Some(dir)) = (
            self.git_status.borrow().clone(),
            self.repo(),
            self.session().project_dir(),
        ) else {
            return Default::default();
        };
        let prefix = repo
            .relative(&dir)
            .filter(|p| !p.is_empty())
            .map(|p| format!("{p}/"))
            .unwrap_or_default();
        status
            .files
            .iter()
            .filter_map(|f| {
                let path = f.path.strip_prefix(&prefix)?;
                Some((path.to_owned(), f.unstaged.or(f.staged)?))
            })
            .collect()
    }

    /// Reads the Git state and shows it (window, status line, project
    /// tree); the log only when HEAD or the upstream moved, or with `log`.
    pub fn git_refresh(&self, log: bool) {
        let repo = self.repo();
        let status = repo.as_ref().and_then(|r| r.status().ok());
        let name = repo
            .as_ref()
            .and_then(|r| r.root().file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.git_window.show_status(&name, status.as_ref());
        self.main.set_branch(&match &status {
            Some(s) if s.ahead + s.behind > 0 => {
                format!("⎇ {}  {}↓ {}↑", s.branch_label(), s.behind, s.ahead)
            }
            Some(s) => format!("⎇ {}", s.branch_label()),
            None => String::new(),
        });
        let refs = self.graph_refs.borrow().clone();
        // Neu laden, wenn sich HEAD, Upstream oder die Zweige bewegt haben.
        let key = status
            .as_ref()
            .map(|s| {
                let branches = repo
                    .as_ref()
                    .and_then(|r| r.branches().ok())
                    .map(|b| format!("{b:?}"))
                    .unwrap_or_default();
                format!(
                    "{:?} {:?} {} {} {branches} {refs:?}",
                    s.head, s.upstream, s.ahead, s.behind
                )
            })
            .unwrap_or_default();
        if log || *self.git_log_key.borrow() != key {
            let commits = repo
                .as_ref()
                .and_then(|r| r.log_refs(300, &refs).ok())
                .unwrap_or_default();
            self.git_window.show_log(commits, &refs);
            *self.git_log_key.borrow_mut() = key;
        }
        // Gestagt, committet, ausgecheckt …: Vergleichsstand der Editoren neu.
        let changed = *self.git_status.borrow() != status;
        *self.git_status.borrow_mut() = status;
        if changed {
            self.reload_git_bases();
        }
    }

    /// The dry run of a Git operation.
    pub fn git_preview(&self, op: &GitOp) -> Result<Preview, GitError> {
        self.repo()
            .ok_or_else(|| GitError {
                code: "not_a_repository",
                detail: "Das Projekt liegt in keinem Git-Repository.".to_owned(),
                output: String::new(),
                issues: Vec::new(),
            })?
            .preview(op)
    }

    /// Runs a Git operation now (no questions; for automation). Open files
    /// changed by it are reloaded.
    pub fn git_run(&self, op: &GitOp) -> Result<String, GitError> {
        self.git_preview(op)?;
        let repo = self.repo().expect("checked by the preview");
        let result = repo.run(op);
        self.after_git();
        result
    }

    fn after_git(&self) {
        for (path, text) in self.code.reload_from_disk() {
            self.lsp_changed(&path, &text);
            self.lsp_saved(&[path]);
        }
        self.git_refresh(false);
        self.project.refresh(self);
    }

    /// Shows the findings of a preview: errors stop, warnings ask.
    fn git_confirm(&self, title: &str, op: &GitOp) -> bool {
        let parent = Some(self.git_window.window.widget());
        let preview = match self.git_preview(op) {
            Ok(p) => p,
            Err(e) => {
                dialog::error(parent, title, &e.to_string());
                return false;
            }
        };
        if preview.has_errors() {
            let details: Vec<&str> = preview
                .issues
                .iter()
                .filter(|i| i.severity == tdr_core::Severity::Error)
                .map(|i| i.detail.as_str())
                .collect();
            dialog::error(parent, title, &details.join("\n"));
            return false;
        }
        let warnings: Vec<&str> = preview.issues.iter().map(|i| i.detail.as_str()).collect();
        warnings.is_empty()
            || dialog::confirm(
                parent,
                title,
                &format!("{}\n\nFortfahren?", warnings.join("\n")),
            )
    }

    /// Runs a quick Git operation (stage, checkout …) after the preview.
    pub fn git_now(&self, op: GitOp) {
        if !self.git_confirm("Git", &op) {
            return;
        }
        if let Err(e) = self.git_run(&op) {
            dialog::error(Some(self.git_window.window.widget()), "Git", &e.to_string());
        }
    }

    /// Runs a Git operation in the background (push, pull, commit …).
    pub fn git_start(&self, op: GitOp) {
        if self.git_busy.get() {
            self.set_status("Git arbeitet noch …");
            return;
        }
        if !self.git_confirm("Git", &op) {
            return;
        }
        let (Some(repo), Some(sender)) = (self.repo(), self.git_results.borrow().clone()) else {
            return;
        };
        let what = match &op {
            GitOp::Commit {
                then: After::Push, ..
            } => "Commit & Push",
            GitOp::Commit {
                then: After::Sync, ..
            } => "Commit & Sync",
            GitOp::Commit { .. } => "Commit",
            GitOp::Fetch | GitOp::FetchAll => "Fetch",
            GitOp::Pull | GitOp::PullRebase => "Pull",
            GitOp::Push => "Push",
            GitOp::PushTags => "Tags pushen",
            GitOp::Sync => "Sync",
            _ => "Git",
        }
        .to_owned();
        let clear = matches!(op, GitOp::Commit { .. });
        self.git_busy.set(true);
        self.set_status(&format!("{what} läuft …"));
        std::thread::spawn(move || {
            let result = repo.run(&op);
            sender.send((what, clear, None, result));
        });
    }

    fn git_finished(
        &self,
        what: &str,
        clear: bool,
        open: Option<PathBuf>,
        result: Result<String, GitError>,
    ) {
        self.git_busy.set(false);
        match result {
            Ok(_) => {
                if clear {
                    self.git_window.set_message("");
                }
                self.set_status(&format!("{what} erledigt."));
                if let Some(dir) = open {
                    self.open_cloned(&dir);
                }
            }
            Err(e) => {
                self.set_status(&format!("{what} fehlgeschlagen: {}", e.detail));
                dialog::error(Some(self.git_window.window.widget()), what, &e.to_string());
            }
        }
        self.after_git();
        self.git_refresh(true);
    }

    /// Commit from the window: offers to stage everything when nothing is
    /// staged and to save open files first (like VS Code).
    pub fn git_commit(&self, message: &str, amend: bool, then: After) {
        let parent = Some(self.git_window.window.widget());
        if self.code.has_unsaved()
            && dialog::confirm(
                parent,
                "Commit",
                "Es gibt ungespeicherte Dateien im Code-Editor. Vor dem Commit speichern?",
            )
        {
            match self.code.save_all() {
                Ok(saved) => self.lsp_saved(&saved),
                Err(e) => {
                    dialog::error(parent, "Commit", &e);
                    return;
                }
            }
            self.git_refresh(false);
        }
        let status = self.git_status().unwrap_or_default();
        if !amend && !status.has_staged() && !status.files.is_empty() {
            if !dialog::confirm(
                parent,
                "Commit",
                "Es sind keine Änderungen gestaged. Alle Änderungen stagen und committen?",
            ) {
                return;
            }
            if let Err(e) = self.git_run(&GitOp::Stage { paths: vec![] }) {
                dialog::error(parent, "Commit", &e.to_string());
                return;
            }
        }
        self.git_start(GitOp::Commit {
            message: message.to_owned(),
            amend,
            then,
        });
    }

    /// Discards changes after asking (cannot be undone).
    pub fn git_discard(&self, paths: Vec<String>) {
        let what = if paths.is_empty() {
            "alle Änderungen".to_owned()
        } else {
            paths.join(", ")
        };
        if !dialog::confirm(
            Some(self.git_window.window.widget()),
            "Verwerfen",
            &format!("Änderungen an {what} verwerfen? Das lässt sich nicht rückgängig machen."),
        ) {
            return;
        }
        self.git_now(GitOp::Discard { paths });
    }

    /// „Auschecken nach …“: local branches, remote branches, tags.
    pub fn git_checkout_menu(&self) -> Menu {
        let Some(repo) = self.repo() else {
            return Menu::new();
        };
        let branches = repo.branches().unwrap_or_default();
        let mut menu = Menu::new();
        for b in branches.iter().filter(|b| !b.remote) {
            let id = format!("git.checkout:{}", b.name);
            menu = if b.current {
                menu.disabled_item(&id, &format!("✓ {}", b.name))
            } else {
                menu.item(&id, &b.name, None)
            };
        }
        let mut remote = Menu::new();
        for b in branches.iter().filter(|b| b.remote) {
            remote = remote.item(&format!("git.checkout:{}", b.name), &b.name, None);
        }
        let mut tags = Menu::new();
        for tag in repo.tags().unwrap_or_default() {
            tags = tags.item(&format!("git.checkout:{tag}"), &tag, None);
        }
        if !remote.entries.is_empty() || !tags.entries.is_empty() {
            menu = menu.separator();
        }
        if !remote.entries.is_empty() {
            menu = menu.submenu("Remote-Zweige", remote);
        }
        if !tags.entries.is_empty() {
            menu = menu.submenu("Tags", tags);
        }
        menu
    }

    /// Branches to choose for merge, rebase or delete (`local`: only local
    /// ones; the current branch is never offered).
    pub fn git_branch_pick(&self, prefix: &str, local: bool) -> Menu {
        let branches = self
            .repo()
            .and_then(|r| r.branches().ok())
            .unwrap_or_default();
        let mut menu = Menu::new();
        for b in branches
            .iter()
            .filter(|b| !b.current && !(local && b.remote))
        {
            menu = menu.item(&format!("{prefix}{}", b.name), &b.name, None);
        }
        menu
    }

    /// Branches the graph shows: Auto, All, or single branches (toggled).
    pub fn git_refs_menu(&self) -> Menu {
        let refs = self.graph_refs.borrow().clone();
        let check = |on: bool, label: &str| {
            if on {
                format!("✓ {label}")
            } else {
                label.to_owned()
            }
        };
        let mut menu = Menu::new()
            .item(
                &format!("git.refs:{REFS_AUTO}"),
                &check(refs == LogRefs::Auto, "Auto – aktueller Zweig und Upstream"),
                None,
            )
            .item(
                &format!("git.refs:{REFS_ALL}"),
                &check(refs == LogRefs::All, "Alle Zweige und Tags"),
                None,
            )
            .separator();
        let chosen = match &refs {
            LogRefs::Some(names) => names.clone(),
            _ => Vec::new(),
        };
        for b in self
            .repo()
            .and_then(|r| r.branches().ok())
            .unwrap_or_default()
        {
            menu = menu.item(
                &format!("git.refs:{}", b.name),
                &check(chosen.contains(&b.name), &b.name),
                None,
            );
        }
        menu
    }

    /// Chooses Auto / All or toggles a branch in the graph.
    pub fn git_toggle_refs(&self, choice: &str) {
        let refs = match choice {
            REFS_AUTO => LogRefs::Auto,
            REFS_ALL => LogRefs::All,
            name => {
                let mut names = match self.graph_refs.borrow().clone() {
                    LogRefs::Some(names) => names,
                    _ => Vec::new(),
                };
                match names.iter().position(|n| n == name) {
                    Some(i) => {
                        names.remove(i);
                    }
                    None => names.push(name.to_owned()),
                }
                if names.is_empty() {
                    LogRefs::Auto
                } else {
                    LogRefs::Some(names)
                }
            }
        };
        self.set_graph_refs(refs);
    }

    /// Which branches the graph shows.
    pub fn set_graph_refs(&self, refs: LogRefs) {
        *self.graph_refs.borrow_mut() = refs;
        self.git_refresh(true);
    }

    /// The commands of the Git operations so far with their output.
    pub fn git_output(&self) -> String {
        self.output.text(channel::GIT)
    }

    /// Shows the Git output (channel „Git“ in the panel).
    pub fn git_show_output(&self) {
        self.show_output(channel::GIT);
    }

    /// Clones a repository (in the background) into a chosen folder and
    /// opens its project afterwards (if it has exactly one `.ptdr`).
    pub fn git_clone(&self, url: &str) {
        let parent = Some(self.git_window.window.widget());
        let Some(folder) = dialog::choose_folder(parent, "Ordner für den Klon wählen") else {
            return;
        };
        let name = url
            .trim_end_matches('/')
            .rsplit(['/', ':'])
            .next()
            .unwrap_or_default()
            .trim_end_matches(".git")
            .to_owned();
        let target = folder.join(if name.is_empty() { "repo" } else { &name });
        if target.exists() {
            dialog::error(
                parent,
                "Klonen",
                &format!("{} gibt es schon.", target.display()),
            );
            return;
        }
        let Some(sender) = self.git_results.borrow().clone() else {
            return;
        };
        if self.git_busy.replace(true) {
            self.set_status("Git arbeitet noch …");
            return;
        }
        self.set_status(&format!("Klone {url} …"));
        let askpass = self.askpass();
        let url = url.to_owned();
        std::thread::spawn(move || {
            let result = Git::clone(&url, &target, askpass).map(|_| String::new());
            let open = result.is_ok().then_some(target);
            sender.send(("Klonen".to_owned(), false, open, result));
        });
    }

    /// After a clone: open its project if there is exactly one `.ptdr`.
    fn open_cloned(&self, dir: &Path) {
        let projects: Vec<PathBuf> = std::fs::read_dir(dir)
            .map(|r| {
                r.filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|e| e == "ptdr"))
                    .collect()
            })
            .unwrap_or_default();
        match projects.as_slice() {
            [project] => {
                if self.confirm_discard() {
                    self.open_path(project);
                }
            }
            _ => self.set_status(&format!(
                "Geklont nach {} (kein einzelnes Freya-TDE-Projekt darin).",
                dir.display()
            )),
        }
    }

    /// `git init` in the project folder.
    pub fn git_init(&self) {
        let Some(dir) = self.session().project_dir() else {
            self.set_status("Das Projekt ist noch nicht gespeichert.");
            return;
        };
        if self.repo().is_some() {
            self.set_status("Das Projekt liegt schon in einem Git-Repository.");
            return;
        }
        match Git::init(&dir) {
            Ok(_) => {
                *self.repo.borrow_mut() = None;
                self.after_git();
                self.git_refresh(true);
                self.set_status("Git-Repository angelegt.");
            }
            Err(e) => dialog::error(Some(self.git_window.window.widget()), "Git", &e.to_string()),
        }
    }

    /// Shows the diff of a file in the code editor (read-only tab).
    pub fn git_show_diff(&self, path: &str, staged: bool) {
        let Some(repo) = self.repo() else {
            return;
        };
        match repo.diff(path, staged) {
            Ok(diff) => {
                let name = path.rsplit_once('/').map_or(path, |(_, n)| n);
                let label = if staged {
                    format!("{name} (gestaged)")
                } else {
                    format!("{name} (Änderungen)")
                };
                let text = if diff.text.is_empty() {
                    "(keine Unterschiede)".to_owned()
                } else {
                    diff.text
                };
                let key = format!("diff:{}:{path}", if staged { "s" } else { "u" });
                self.code.open_text(&key, &label, &text, "diff");
            }
            Err(e) => self.set_status(&e.to_string()),
        }
    }

    /// Opens a file of the repository (path relative to its root).
    pub fn git_open_file(&self, path: &str) {
        if let Some(repo) = self.repo() {
            self.open_source(&repo.root().join(path), 1);
        }
    }

    // --- Panel: problems and output -------------------------------------------------

    /// The output channels (build, program, Git, rust-analyzer).
    pub fn output(&self) -> Arc<Output> {
        self.output.clone()
    }

    /// Shows a channel in the panel's output tab.
    pub fn show_output(&self, name: &str) {
        self.show_panel(Tab::Output);
        self.panel.show_channel(self, name);
    }

    /// The folder new terminals start in: the project folder, else home.
    fn terminal_dir(&self) -> PathBuf {
        self.session()
            .project_dir()
            .or_else(std::env::home_dir)
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// Starts the shell of the visible terminal if it is not running; opens
    /// the first terminal if there is none.
    pub fn start_terminal(&self) {
        let dir = self.terminal_dir();
        match self.panel.terminals.active() {
            Some(view) => view.ensure_started(&dir),
            None => {
                self.new_terminal();
            }
        }
    }

    pub fn show_terminal(&self) {
        self.show_panel(Tab::Terminal);
        self.start_terminal();
    }

    /// Opens another terminal (own shell, own tab) in the project folder and
    /// shows it; returns its index.
    pub fn new_terminal(&self) -> usize {
        let Some(ide) = self.me.upgrade() else {
            return 0;
        };
        let view = panel::add_terminal(&ide);
        view.ensure_started(&self.terminal_dir());
        view.canvas.widget().focus();
        self.panel.terminals.count() - 1
    }

    /// Ends the shell of the visible terminal and removes its tab.
    pub fn close_terminal(&self) {
        if let Some(view) = panel::remove_terminal(&self.panel.terminals)
            && let Some(terminal) = view.terminal().as_ref()
        {
            terminal.kill();
        }
    }

    /// Number of open terminals.
    pub fn terminal_count(&self) -> usize {
        self.panel.terminals.count()
    }

    /// Shows terminal `index` (0-based); false if there is none.
    pub fn select_terminal(&self, index: usize) -> bool {
        if index >= self.terminal_count() {
            return false;
        }
        self.show_panel(Tab::Terminal);
        self.panel.terminals.tabs.set_current_index(index as i32);
        true
    }

    fn restart_terminal(&self) {
        let dir = self.terminal_dir();
        if let Some(view) = self.panel.terminals.active() {
            view.restart(&dir);
        }
    }

    fn terminal_paste(&self) {
        let weak = self.me.clone();
        ruf::clipboard::read_text(move |text| {
            if let (Some(ide), Some(text)) = (weak.upgrade(), text)
                && let Some(view) = ide.panel.terminals.active()
            {
                view.paste(&text);
            }
        });
    }

    /// Types `command` and Enter into the visible terminal (starting one if
    /// needed) – for scripts and an AI.
    pub fn terminal_run(&self, command: &str) {
        self.show_terminal();
        if let Some(view) = self.panel.terminals.active()
            && let Some(t) = view.terminal().as_ref()
        {
            t.write_text(&format!("{command}\r"));
        }
    }

    /// The visible text of the visible terminal (empty if none ran).
    pub fn terminal_text(&self) -> String {
        self.panel
            .terminals
            .active()
            .and_then(|view| view.terminal().as_ref().map(|t| t.text()))
            .unwrap_or_default()
    }

    /// Shows the messages window with a tab.
    pub fn show_panel(&self, tab: Tab) {
        self.panel.show_tab(tab);
        self.show_window(window::MESSAGES);
    }

    /// All problems: model findings, code generation, last build and
    /// rust-analyzer – sorted by file and line.
    pub fn problems(&self) -> Vec<Problem> {
        let mut problems = self.model_problems();
        problems.extend(self.codegen_problems.borrow().iter().cloned());
        problems.extend(self.build_problems.borrow().iter().cloned());
        for list in self.lsp_problems.borrow().values() {
            problems.extend(list.iter().cloned());
        }
        crate::problems::sort(&mut problems);
        problems
    }

    /// Findings of form validation and project analysis.
    fn model_problems(&self) -> Vec<Problem> {
        let findings = self.session().document().findings();
        let mut problems = Vec::new();
        for (form, diagnostics) in &findings.forms {
            for d in diagnostics {
                problems.push(Problem {
                    element: d
                        .element
                        .clone()
                        .map(|e| tdr_core::ComponentPath::on(form.clone(), e)),
                    ..Problem::new(
                        ProblemSource::Model,
                        d.severity.into(),
                        &d.issue.to_string(),
                    )
                });
            }
        }
        for d in &findings.project {
            problems.push(Problem {
                element: d.element.clone(),
                ..Problem::new(
                    ProblemSource::Model,
                    d.severity.into(),
                    &d.issue.to_string(),
                )
            });
        }
        problems
    }

    pub fn refresh_problems(&self) {
        let problems = self.problems();
        let was = self.refreshing.replace(true);
        self.panel.show_problems(self, problems);
        self.refreshing.set(was);
    }

    /// Code generation findings (validation findings come from the model).
    fn set_codegen_problems(&self, generation: &tdr_core::codegen::Generation) {
        use tdr_core::codegen::CodegenIssue;
        *self.codegen_problems.borrow_mut() = generation
            .diagnostics
            .iter()
            .filter(|d| !matches!(d.issue, CodegenIssue::Form(_) | CodegenIssue::Project(_)))
            .map(|d| Problem {
                element: d.element.clone(),
                ..Problem::new(
                    ProblemSource::Codegen,
                    d.severity.into(),
                    &d.issue.to_string(),
                )
            })
            .collect();
        self.refresh_problems();
    }

    /// Compiler messages of a build (`dir` makes file names absolute).
    fn set_build_problems(&self, report: &tdr_core::preview::BuildReport, dir: Option<&Path>) {
        *self.build_problems.borrow_mut() = report
            .messages
            .iter()
            .map(|m| Problem {
                code: m.code.clone(),
                // Fehler im erzeugten Code führen zur Komponente im Designer.
                file: match (&m.element, &m.file, dir) {
                    (None, Some(file), Some(dir)) => Some(dir.join(file)),
                    _ => None,
                },
                line: m.element.is_none().then_some(m.line).flatten(),
                column: m
                    .element
                    .is_none()
                    .then_some(m.column.map(|c| c.saturating_sub(1)))
                    .flatten(),
                element: m.element.clone(),
                ..Problem::new(ProblemSource::Build, m.severity.into(), &m.message)
            })
            .collect();
        self.refresh_problems();
    }

    /// Jumps to a problem: the element in the designer or the source line.
    pub fn goto_problem(&self, problem: &Problem) {
        if let Some(path) = &problem.element
            && problem.file.is_none()
        {
            if let Some(form) = &path.form
                && self.session().form_name() != Some(form)
            {
                self.select_form(form);
            }
            self.select(vec![path.name.clone()]);
            return;
        }
        if let Some(file) = &problem.file {
            let line = problem.line.unwrap_or(1);
            self.open_source(file, line);
            if let Some(column) = problem.column {
                self.code.set_cursor(file, line, column);
            }
        }
    }

    pub fn inspected(&self) -> Option<tdr_core::ComponentPath> {
        self.inspector.shown()
    }

    /// Back to the standard arrangement.
    pub fn reset_desktop(&self) {
        *self.desktop.borrow_mut() = Desktop::standard();
        self.restore_desktop();
    }

    /// Help → keyboard shortcuts.
    pub fn show_shortcuts(&self) {
        self.show_window(window::SHORTCUTS);
    }

    /// Help → About: centred over the main window's screen.
    pub fn show_about(&self) {
        let window = &self.about.window;
        if let Some(screen) = Application::screen_rect() {
            let size = window.widget().size();
            window.set_screen_position(
                screen.x + (screen.width - size.width) / 2,
                screen.y + (screen.height - size.height) / 2,
            );
        }
        window.show();
    }

    /// The about window (automation, screenshots).
    pub fn about_window(&self) -> &RnclWindow {
        &self.about.window
    }

    pub fn show_window(&self, name: &str) {
        if let Some((_, window)) = self.windows().into_iter().find(|(n, _)| *n == name) {
            window.show();
        }
    }

    // --- Refresh ------------------------------------------------------------------

    /// Refreshes the windows after `changes`.
    pub fn refresh(&self, changes: Changes) {
        if !changes.any() {
            return;
        }
        self.refreshing.set(true);
        self.main.refresh(self);
        self.designer.refresh(self);
        self.structure.refresh(self);
        if changes.model {
            self.refresh_problems();
            self.git_refresh(false);
            self.project.refresh(self);
        }
        if let Some(me) = self.me.upgrade() {
            self.inspector.refresh(&me, changes.model);
        }
        self.refreshing.set(false);
    }

    pub fn is_refreshing(&self) -> bool {
        self.refreshing.get()
    }

    /// Reports the result of an edit in the status line.
    fn report(&self, result: Result<Changes, CommandError>) {
        match result {
            Ok(changes) => {
                self.main.set_status("");
                self.refresh(changes);
            }
            Err(e) => self.main.set_status(&e.to_string()),
        }
    }

    // --- Actions --------------------------------------------------------------------

    pub fn apply(&self, command: Command) {
        let result = self.session.borrow_mut().apply(command);
        self.report(result);
    }

    /// Like [`Ide::apply`], but leaves the object inspector as it is (its
    /// own edits must not rebuild the field being edited).
    pub fn apply_from_inspector(&self, command: Command) {
        let result = self.session.borrow_mut().apply(command);
        match result {
            Ok(_) => {
                self.main.set_status("");
                self.refreshing.set(true);
                self.main.refresh(self);
                self.designer.refresh(self);
                self.structure.refresh(self);
                self.refreshing.set(false);
                self.refresh_problems();
            }
            Err(e) => self.main.set_status(&e.to_string()),
        }
    }

    pub fn arm(&self, type_ref: Option<TypeRef>) {
        self.session.borrow_mut().arm(type_ref);
        self.main.refresh(self);
    }

    pub fn select(&self, names: Vec<ComponentName>) {
        self.session.borrow_mut().select(names);
        self.refresh(Changes {
            model: false,
            selection: true,
        });
    }

    pub fn select_form(&self, form: &ComponentName) {
        if self.session.borrow_mut().set_form(form) {
            self.refresh(Changes {
                model: false,
                selection: true,
            });
        }
    }

    pub fn undo(&self) {
        let done = self.session.borrow_mut().undo();
        if done {
            self.refresh(Changes {
                model: true,
                selection: true,
            });
        }
    }

    pub fn redo(&self) {
        let done = self.session.borrow_mut().redo();
        if done {
            self.refresh(Changes {
                model: true,
                selection: true,
            });
        }
    }

    pub fn delete_selection(&self) {
        let result = self.session.borrow_mut().delete_selection();
        self.report(result);
    }

    pub fn add_form(&self, data_module: bool) {
        let prefix = if data_module { "data" } else { "form" };
        let name = {
            let session = self.session.borrow();
            let forms = session.document().forms();
            (1..)
                .map(|n| format!("{prefix}{n}"))
                .find(|n| forms.iter().all(|f| f.name.as_str() != n))
                .and_then(|n| ComponentName::new(n).ok())
                .expect("free name")
        };
        self.apply(Command::AddForm { name, data_module });
    }

    fn confirm_discard(&self) -> bool {
        let parked_dirty = self.parked.borrow().iter().any(|(_, s)| s.is_modified());
        !(self.session.borrow().is_modified() || self.code.has_unsaved() || parked_dirty)
            || dialog::confirm(
                Some(self.main.window.widget()),
                "Ungespeicherte Änderungen",
                "Das Projekt hat ungespeicherte Änderungen. Verwerfen?",
            )
    }

    pub fn new_project(&self) {
        if !self.confirm_discard() {
            return;
        }
        let (registry, packs) = full_registry(&self.pack_dirs);
        let backend = default_backend(&registry);
        match Session::untitled("projekt1", backend, packs, registry) {
            Ok(session) => {
                *self.session.borrow_mut() = session;
                *self.path.borrow_mut() = None;
                *self.main_project.borrow_mut() = None;
                self.parked.borrow_mut().clear();
                self.refresh(Changes {
                    model: true,
                    selection: true,
                });
                self.ensure_lsp();
            }
            Err(e) => self.main.set_status(&e),
        }
    }

    pub fn open_dialog(&self) {
        if !self.confirm_discard() {
            return;
        }
        let filters = [FileFilter::new("Freya-TDE-Projekt", &["*.ptdr"])];
        if let Some(path) =
            dialog::open_file(Some(self.main.window.widget()), "Projekt öffnen", &filters)
        {
            self.open_path(&path);
        }
    }

    /// Opens a project without asking (the caller asked already, or it is
    /// the start).
    fn open_path(&self, path: &Path) {
        self.open_project_with(path, false);
    }

    /// Loads a project as the active one (the previous active one is
    /// dropped – callers park or confirm it first).
    fn load_active(&self, path: &Path) -> bool {
        match Session::open(path, &self.pack_dirs) {
            Ok((session, report)) => {
                *self.session.borrow_mut() = session;
                *self.path.borrow_mut() = Some(path.to_path_buf());
                let mut notes: Vec<String> = report
                    .missing_packs
                    .iter()
                    .map(|p| format!("Pack „{p}“ nicht gefunden"))
                    .collect();
                notes.extend(report.pack_errors.iter().map(ToString::to_string));
                self.main.set_status(&notes.join("; "));
                self.refresh(Changes {
                    model: true,
                    selection: true,
                });
                self.ensure_lsp();
                true
            }
            Err(e) => {
                dialog::error(
                    Some(self.main.window.widget()),
                    "Projekt öffnen",
                    &e.to_string(),
                );
                false
            }
        }
    }

    // --- Projects (project manager) -------------------------------------------------

    fn save_projects(&self) {
        if let Some(file) = &self.projects_file
            && let Err(e) = self.projects.borrow().save(file)
        {
            self.set_status(&format!("Projektliste nicht gespeichert: {e}"));
        }
    }

    /// The open projects: the active one first, then the parked ones.
    pub fn open_projects(&self) -> Vec<PathBuf> {
        let mut open: Vec<PathBuf> = self.path.borrow().iter().cloned().collect();
        open.extend(self.parked.borrow().iter().map(|(p, _)| p.clone()));
        open
    }

    /// The active project's `.ptdr` (`None` while it is unsaved).
    pub fn active_project(&self) -> Option<PathBuf> {
        self.path.borrow().clone()
    }

    /// The open main project's `.ptdr`.
    pub fn main_project(&self) -> Option<PathBuf> {
        self.main_project.borrow().clone()
    }

    /// All projects of the list as a tree (open ones marked).
    pub fn project_tree(&self) -> Vec<ProjectNode> {
        self.projects.borrow().tree(&self.open_projects())
    }

    pub fn project_list(&self) -> ProjectList {
        self.projects.borrow().clone()
    }

    /// Opens a project: a main project closes all others (asking about
    /// unsaved changes); a subproject also opens (or keeps) its main
    /// project and becomes the active one.
    pub fn open_project(&self, path: &Path) {
        self.open_project_with(path, true);
    }

    fn open_project_with(&self, path: &Path, ask: bool) {
        // Relativ angegeben (`freya-tde app.ptdr`): der Projektordner wäre leer.
        let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        if self.open_projects().contains(&path) {
            self.activate_project(&path);
            return;
        }
        let main = self.projects.borrow().main_of(&path);
        let main = if main.is_file() { main } else { path.clone() };
        if self.main_project().as_ref() != Some(&main) {
            if ask && !self.confirm_discard() {
                return;
            }
            self.parked.borrow_mut().clear();
            if !self.load_active(&main) {
                return;
            }
            *self.main_project.borrow_mut() = Some(main.clone());
            {
                let mut list = self.projects.borrow_mut();
                if list.get(&main).is_none() {
                    let _ = list.add(&main, None);
                }
                list.discover(&main);
            }
            self.save_projects();
        }
        if path != main {
            self.open_subproject(&path);
        }
        self.project.refresh(self);
    }

    /// Opens a subproject of the open main project as the active one.
    fn open_subproject(&self, path: &Path) {
        let previous = (
            self.path.borrow().clone(),
            std::mem::replace(
                &mut *self.session.borrow_mut(),
                Session::untitled(
                    "park",
                    NO_BACKEND,
                    Vec::new(),
                    TypeRegistry::with_builtins(),
                )
                .expect("valid name"),
            ),
        );
        if self.load_active(path) {
            if let (Some(p), session) = previous {
                self.parked.borrow_mut().push((p, session));
            }
        } else {
            // Zurück zum bisherigen Projekt.
            let (p, session) = previous;
            *self.session.borrow_mut() = session;
            *self.path.borrow_mut() = p;
        }
    }

    /// Makes an open project the active one (designer, code, build, Git).
    pub fn activate_project(&self, path: &Path) {
        if self.path.borrow().as_deref() == Some(path) {
            return;
        }
        let index = self.parked.borrow().iter().position(|(p, _)| p == path);
        let Some(index) = index else {
            return;
        };
        let (target, session) = self.parked.borrow_mut().remove(index);
        let previous_session = std::mem::replace(&mut *self.session.borrow_mut(), session);
        let previous_path = self.path.replace(Some(target));
        if let Some(p) = previous_path {
            self.parked.borrow_mut().push((p, previous_session));
        }
        self.refresh(Changes {
            model: true,
            selection: true,
        });
        self.ensure_lsp();
        self.git_refresh(true);
        self.project.refresh(self);
    }

    /// Closes an open subproject (the main project closes with „Neu“ or by
    /// opening another main project).
    pub fn close_project(&self, path: &Path) {
        if self.main_project().as_deref() == Some(path) {
            self.set_status("Das Hauptprojekt schließt beim Öffnen eines anderen Projekts.");
            return;
        }
        let parent = Some(self.main.window.widget());
        let ask = |dirty: bool| {
            !dirty
                || dialog::confirm(
                    parent,
                    "Projekt schließen",
                    "Das Projekt hat ungespeicherte Änderungen. Verwerfen?",
                )
        };
        if self.path.borrow().as_deref() == Some(path) {
            if !ask(self.session().is_modified()) {
                return;
            }
            if let Some(main) = self.main_project() {
                self.activate_project(&main);
            }
        }
        let index = self.parked.borrow().iter().position(|(p, _)| p == path);
        if let Some(index) = index {
            let dirty = self.parked.borrow()[index].1.is_modified();
            if !ask(dirty) {
                return;
            }
            self.parked.borrow_mut().remove(index);
        }
        self.project.refresh(self);
    }

    /// Changes the project list (add, assign, hide, remove) and saves it.
    pub fn edit_projects(
        &self,
        f: impl FnOnce(&mut ProjectList) -> Result<(), ProjectError>,
    ) -> Result<(), ProjectError> {
        let result = f(&mut self.projects.borrow_mut());
        self.save_projects();
        self.project.refresh(self);
        result
    }

    /// „Projekt aus Cargo.toml erstellen“: what would be created.
    pub fn plan_projects_from_cargo(
        &self,
        dir: &Path,
        with_members: bool,
    ) -> Result<Vec<CargoProject>, ProjectError> {
        crate::projects::plan_from_cargo(dir, with_members)
    }

    /// Creates the planned `.ptdr` files and puts them on the list.
    pub fn create_projects_from_cargo(
        &self,
        plan: &[CargoProject],
    ) -> Result<Vec<PathBuf>, ProjectError> {
        let result = crate::projects::create_from_cargo(plan, &mut self.projects.borrow_mut());
        self.save_projects();
        self.project.refresh(self);
        result
    }

    /// Shared Git settings of the active project (in its `.ptdr`).
    pub fn set_project_git(&self, git: Option<tdr_core::GitSettings>) -> Result<(), String> {
        let dir = self
            .session()
            .project_dir()
            .ok_or("Das Projekt ist noch nicht gespeichert.")?;
        let mut session = self.session.borrow_mut();
        session.document_mut().set_git(git);
        session
            .document_mut()
            .save(Some(&dir))
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// Saves; an unsaved project asks for its folder first.
    pub fn save(&self) -> bool {
        let dir = self
            .path
            .borrow()
            .as_ref()
            .and_then(|p| p.parent().map(Path::to_path_buf));
        let dir = match dir {
            Some(dir) => dir,
            None => match dialog::choose_folder(
                Some(self.main.window.widget()),
                "Ordner für das Projekt wählen",
            ) {
                Some(dir) => dir,
                None => return false,
            },
        };
        let result = self.session.borrow_mut().document_mut().save(Some(&dir));
        match result {
            Ok(_) => {
                let name = self.session.borrow().document().project().name.clone();
                let ptdr = dir.join(format!("{name}.ptdr"));
                *self.path.borrow_mut() = Some(ptdr.clone());
                // Erstmals gespeichert: in die Projektliste (als Hauptprojekt).
                if self.main_project().is_none() {
                    *self.main_project.borrow_mut() = Some(ptdr.clone());
                }
                if self.projects.borrow().get(&ptdr).is_none() {
                    let _ = self.projects.borrow_mut().add(&ptdr, None);
                    self.save_projects();
                }
                self.main.set_status("Gespeichert.");
                self.main.refresh(self);
                self.project.refresh(self);
                true
            }
            Err(e) => {
                dialog::error(Some(self.main.window.widget()), "Speichern", &e.to_string());
                false
            }
        }
    }

    // --- Code, build, run ----------------------------------------------------------

    /// Saves (asking for a folder the first time) and writes the generated
    /// code. Code generation findings go to the problems.
    pub fn generate_code(&self) -> Option<tdr_core::codegen::Generation> {
        if self.path.borrow().is_none() && !self.save() {
            return None;
        }
        // Ungespeicherte Änderungen im Code zuerst sichern (der Generator
        // hängt Handler-Stubs an die Nutzerdateien an).
        match self.code.save_all() {
            Ok(saved) => self.lsp_saved(&saved),
            Err(e) => {
                self.main.set_status(&e);
                return None;
            }
        }
        let result = self.session.borrow_mut().write_code();
        match result {
            Ok((generation, written)) => {
                // Jetzt gibt es eine Cargo.toml: rust-analyzer starten.
                self.ensure_lsp();
                for (path, text) in self.code.reload_from_disk() {
                    self.lsp_changed(&path, &text);
                    self.lsp_saved(&[path]);
                }
                self.set_codegen_problems(&generation);
                self.main.set_status(&if written.is_empty() {
                    "Code ist aktuell.".to_owned()
                } else {
                    format!("Code erzeugt: {}", written.join(", "))
                });
                self.refresh(Changes {
                    model: true,
                    selection: false,
                });
                Some(generation)
            }
            Err(e) => {
                // Bei Fehlern zeigt der Probelauf, was nicht stimmt.
                if let Some(generation) = self.session().generation() {
                    self.set_codegen_problems(&generation);
                }
                self.main.set_status(&format!("Code nicht erzeugt: {e}"));
                self.show_panel(Tab::Problems);
                None
            }
        }
    }

    /// Generates the code and builds the project in debug mode (in the
    /// background); `then_run` starts it when the build succeeds.
    pub fn build(&self, then_run: bool) {
        if self.building.get() {
            self.main.set_status("Es läuft bereits ein Build.");
            return;
        }
        // Nur verwaltete Projekte (aus einer Cargo.toml) bauen ohne Codeerzeugung.
        let generate = self.session().document().project().generate;
        let generation = if generate {
            let Some(generation) = self.generate_code() else {
                return;
            };
            Some(generation)
        } else {
            match self.code.save_all() {
                Ok(saved) => self.lsp_saved(&saved),
                Err(e) => {
                    self.main.set_status(&e);
                    return;
                }
            }
            None
        };
        let Some(dir) = self.session().project_dir() else {
            return;
        };
        let Some(sender) = self.build_results.borrow().clone() else {
            return;
        };
        self.building.set(true);
        self.run_after_build.set(then_run);
        self.main.set_status("Baue …");
        self.output.clear(channel::BUILD);
        self.output.line(
            channel::BUILD,
            &format!("> cargo build ({})", dir.display()),
        );
        std::thread::spawn(move || {
            let report = tdr_core::preview::build(&dir, generation.as_ref()).unwrap_or_else(|e| {
                tdr_core::preview::BuildReport {
                    success: false,
                    messages: vec![tdr_core::preview::BuildMessage {
                        severity: tdr_core::Severity::Error,
                        message: format!("cargo konnte nicht gestartet werden: {e}"),
                        code: None,
                        file: None,
                        line: None,
                        column: None,
                        element: None,
                        rendered: None,
                    }],
                    executable: None,
                }
            });
            sender.send(report);
        });
    }

    fn build_finished(&self, report: tdr_core::preview::BuildReport) {
        self.building.set(false);
        let dir = self.session().project_dir();
        self.set_build_problems(&report, dir.as_deref());
        for m in &report.messages {
            self.output.append(
                channel::BUILD,
                m.rendered.as_deref().unwrap_or(&format!("{}\n", m.message)),
            );
        }
        self.output.line(
            channel::BUILD,
            if report.success {
                "Build erfolgreich."
            } else {
                "Build fehlgeschlagen."
            },
        );
        let mut marks: std::collections::BTreeMap<PathBuf, Vec<LineMark>> = Default::default();
        if let Some(dir) = &dir {
            for m in &report.messages {
                if let (Some(file), Some(line)) = (&m.file, m.line) {
                    let kind = match m.severity {
                        tdr_core::Severity::Error => MarkKind::Error,
                        tdr_core::Severity::Warning => MarkKind::Warning,
                    };
                    marks
                        .entry(dir.join(file))
                        .or_default()
                        .push(LineMark::new(line, kind, &m.message));
                }
            }
        }
        self.code.set_build_marks(marks);
        let errors = report.errors().count();
        if report.success {
            *self.executable.borrow_mut() = report.executable.clone();
            self.main.set_status("Build erfolgreich.");
            if self.run_after_build.replace(false) {
                self.launch_after_build();
            }
        } else {
            self.run_after_build.set(false);
            self.cancel_launch();
            self.main
                .set_status(&format!("Build fehlgeschlagen ({errors} Fehler)."));
            self.show_panel(Tab::Problems);
        }
    }

    /// Builds (if needed) and starts the program as its own process.
    pub fn run(&self) {
        self.build(true);
    }

    fn run_executable(&self) {
        let Some(exe) = self.executable.borrow().clone() else {
            return;
        };
        let dir = self.session().project_dir();
        let mut command = std::process::Command::new(&exe);
        if let Some(dir) = dir {
            command.current_dir(dir);
        }
        // Ausgabe des Programms im Kanal „Programm“.
        command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        match command.spawn() {
            Ok(mut child) => {
                self.output.clear(channel::PROGRAM);
                self.output
                    .line(channel::PROGRAM, &format!("> {}", exe.display()));
                let pipes = [
                    child
                        .stdout
                        .take()
                        .map(|p| Box::new(p) as Box<dyn std::io::Read + Send>),
                    child
                        .stderr
                        .take()
                        .map(|p| Box::new(p) as Box<dyn std::io::Read + Send>),
                ];
                for pipe in pipes.into_iter().flatten() {
                    let output = self.output.clone();
                    std::thread::spawn(move || {
                        use std::io::BufRead;
                        for line in std::io::BufReader::new(pipe).lines().map_while(Result::ok) {
                            output.line(channel::PROGRAM, &line);
                        }
                    });
                }
                let output = self.output.clone();
                std::thread::spawn(move || {
                    let status = child.wait();
                    let text = match status {
                        Ok(s) => format!("Programm beendet ({s})."),
                        Err(e) => format!("Programm: {e}"),
                    };
                    output.line(channel::PROGRAM, &text);
                });
                self.main
                    .set_status(&format!("Gestartet: {}", exe.display()));
                if !self.panel.window.widget().is_visible()
                    || self.panel.current_tab() != Tab::Problems
                {
                    self.show_output(channel::PROGRAM);
                }
            }
            Err(e) => self.main.set_status(&format!("Start fehlgeschlagen: {e}")),
        }
    }

    /// Jumps to the handler of `event` (default: first event) of `element`,
    /// creating it if needed – like Delphi's double click.
    pub fn open_handler(&self, element: &ComponentName, event: Option<&str>) {
        let handler = self.session.borrow_mut().ensure_handler(element, event);
        let handler = match handler {
            Ok(h) => h,
            Err(e) => {
                self.main.set_status(&e);
                return;
            }
        };
        self.refresh(Changes {
            model: true,
            selection: true,
        });
        if self.generate_code().is_none() {
            return;
        }
        let Some(path) = self.session().form_source() else {
            return;
        };
        let line = std::fs::read_to_string(&path)
            .ok()
            .and_then(|source| crate::session::handler_line(&source, &handler))
            .unwrap_or(1);
        self.open_source(&path, line);
    }

    /// Opens a source file at a line (1-based) in the code editor.
    pub fn open_source(&self, path: &Path, line: usize) {
        let Some(me) = self.me.upgrade() else {
            return;
        };
        let new = !self.code.open_paths().contains(&code::key(path));
        match self.code.open(&me, path, line) {
            Ok(()) if new => self.code.set_git_base(path, self.git_base(path)),
            Ok(()) => {}
            Err(e) => self.main.set_status(&e),
        }
    }

    /// Git's version of a file for the change marks: the index; untracked
    /// files compare with nothing (all lines new); `None` outside a
    /// repository and for ignored files.
    fn git_base(&self, path: &Path) -> Option<String> {
        let repo = self.repo()?;
        let relative = repo.relative(path)?;
        repo.index_text(&relative).or_else(|| {
            let untracked = self
                .git_status
                .borrow()
                .as_ref()
                .is_some_and(|s| s.change_of(&relative) == Some(crate::git::Change::Untracked));
            untracked.then(String::new)
        })
    }

    /// Reloads Git's versions of all open files (after staging, commits …).
    fn reload_git_bases(&self) {
        for path in self.code.open_paths() {
            self.code.set_git_base(&path, self.git_base(&path));
        }
    }

    /// Changed lines of an open file against Git's version (the editor's
    /// change marks) – for automation.
    pub fn code_line_changes(&self, path: &Path) -> Vec<crate::git::LineChange> {
        self.code.line_changes(path)
    }

    /// Opens the `.rs` of the current form in the code editor (F12).
    pub fn show_code(&self) {
        match self.session().form_source() {
            Some(path) if path.exists() => self.open_source(&path, 1),
            Some(_) => self
                .main
                .set_status("Noch kein Code – „Code erzeugen“ (Strg+G) legt ihn an."),
            None => self
                .main
                .set_status("Das Projekt ist noch nicht gespeichert."),
        }
    }

    // --- rust-analyzer ------------------------------------------------------------

    /// Starts rust-analyzer for the project directory if it has a
    /// `Cargo.toml` (and stops one for another directory).
    pub fn ensure_lsp(&self) {
        let root = self
            .session()
            .project_dir()
            .filter(|dir| dir.join("Cargo.toml").is_file())
            .and_then(|dir| dir.canonicalize().ok());
        let running = self.lsp.borrow().as_ref().map(|l| l.root().to_path_buf());
        if running == root {
            return;
        }
        self.stop_lsp();
        let Some(root) = root else {
            return;
        };
        let Some(sender) = self.lsp_events.borrow().clone() else {
            return;
        };
        let generation = self.lsp_generation.get() + 1;
        self.lsp_generation.set(generation);
        match LspClient::start(&root, move |event| {
            sender.send((generation, event));
        }) {
            Ok(client) => {
                *self.lsp.borrow_mut() = Some(client);
                self.main.set_status("rust-analyzer startet …");
            }
            Err(e) => self.main.set_status(&format!(
                "rust-analyzer nicht verfügbar ({}): {e}",
                crate::lsp::program().display()
            )),
        }
    }

    /// Stops rust-analyzer (project closed, Freya TDE ends).
    pub fn stop_lsp(&self) {
        // Ereignisse des alten Servers ab hier verwerfen.
        self.lsp_generation.set(self.lsp_generation.get() + 1);
        self.lsp_ready.set(false);
        self.lsp_indexed.set(false);
        self.completion_request.set(None);
        self.definition_request.set(None);
        let client = self.lsp.borrow_mut().take();
        drop(client);
    }

    /// Is rust-analyzer running and initialised?
    pub fn lsp_ready(&self) -> bool {
        self.lsp_ready.get() && self.lsp.borrow().is_some()
    }

    /// Runs `f` with the server once it is ready.
    fn with_lsp<R>(&self, f: impl FnOnce(&LspClient) -> R) -> Option<R> {
        if !self.lsp_ready.get() {
            return None;
        }
        self.lsp.borrow().as_ref().map(f)
    }

    /// A Rust file was opened in the code editor.
    pub fn lsp_opened(&self, path: &Path, text: &str) {
        self.with_lsp(|lsp| lsp.open(path, text));
    }

    /// New text of an open file (after the typing pause).
    pub fn lsp_changed(&self, path: &Path, text: &str) {
        if code::is_rust(path) {
            self.with_lsp(|lsp| lsp.change(path, text));
        }
    }

    /// Files were saved (rust-analyzer runs `cargo check`).
    pub fn lsp_saved(&self, paths: &[PathBuf]) {
        self.code.flush_change(self);
        for path in paths.iter().filter(|p| code::is_rust(p)) {
            self.with_lsp(|lsp| lsp.save(path));
        }
    }

    pub fn lsp_closed(&self, path: &Path) {
        if code::is_rust(path) {
            self.with_lsp(|lsp| lsp.close(path));
        }
    }

    /// Asks rust-analyzer for completions at the cursor (Ctrl+Space).
    pub fn complete_at_cursor(&self) {
        self.code.flush_change(self);
        let Some((path, _, (line, column))) = self.code.current_state() else {
            return;
        };
        if !code::is_rust(&path) {
            return;
        }
        match self.with_lsp(|lsp| lsp.completion(&path, line, column)) {
            Some(id) => self.completion_request.set(Some(id)),
            None => self
                .main
                .set_status("rust-analyzer ist (noch) nicht bereit."),
        }
    }

    /// Jumps to the definition of the symbol at the cursor (Ctrl+Click,
    /// Ctrl+F12).
    pub fn goto_definition(&self) {
        self.code.flush_change(self);
        let Some((path, _, (line, column))) = self.code.current_state() else {
            return;
        };
        if !code::is_rust(&path) {
            return;
        }
        match self.with_lsp(|lsp| lsp.definition(&path, line, column)) {
            Some(id) => self.definition_request.set(Some(id)),
            None => self
                .main
                .set_status("rust-analyzer ist (noch) nicht bereit."),
        }
    }

    fn lsp_event(&self, event: LspEvent) {
        match event {
            LspEvent::Ready => {
                self.lsp_ready.set(true);
                for (path, text) in self.code.open_rust_files() {
                    self.lsp_opened(&path, &text);
                }
                self.main
                    .set_status("rust-analyzer indiziert das Projekt …");
            }
            LspEvent::Status { quiescent, message } => {
                if let Some(message) = message {
                    self.output.line(channel::RUST_ANALYZER, &message);
                }
                if quiescent && !self.lsp_indexed.replace(true) {
                    self.main.set_status("rust-analyzer bereit.");
                }
            }
            LspEvent::Diagnostics { path, items } => {
                let marks = items
                    .iter()
                    .filter_map(|d| {
                        let kind = match d.severity {
                            DiagnosticSeverity::Error => MarkKind::Error,
                            DiagnosticSeverity::Warning => MarkKind::Warning,
                            DiagnosticSeverity::Information => MarkKind::Info,
                            // Hinweise (z. B. „unnötig“) nicht am Rand zeigen.
                            DiagnosticSeverity::Hint => return None,
                        };
                        let code = d
                            .code
                            .as_ref()
                            .map(|c| format!(" [{c}]"))
                            .unwrap_or_default();
                        Some(LineMark::new(d.line, kind, &format!("{}{code}", d.message)))
                    })
                    .collect();
                self.code.set_lsp_marks(&path, marks);
                let problems: Vec<Problem> = items
                    .iter()
                    .filter_map(|d| {
                        let severity = match d.severity {
                            DiagnosticSeverity::Error => ProblemSeverity::Error,
                            DiagnosticSeverity::Warning => ProblemSeverity::Warning,
                            DiagnosticSeverity::Information => ProblemSeverity::Info,
                            DiagnosticSeverity::Hint => return None,
                        };
                        Some(Problem {
                            code: d.code.clone(),
                            file: Some(path.clone()),
                            line: Some(d.line),
                            column: Some(d.column),
                            ..Problem::new(ProblemSource::RustAnalyzer, severity, &d.message)
                        })
                    })
                    .collect();
                if problems.is_empty() {
                    self.lsp_problems.borrow_mut().remove(&path);
                } else {
                    self.lsp_problems.borrow_mut().insert(path, problems);
                }
                self.refresh_problems();
            }
            LspEvent::Log { message } => {
                self.output.line(channel::RUST_ANALYZER, &message);
            }
            LspEvent::Completion { request, items } => {
                if self.completion_request.get() != Some(request) {
                    return;
                }
                self.completion_request.set(None);
                let items: Vec<Completion> = items
                    .into_iter()
                    .take(MAX_COMPLETIONS)
                    .map(|i| Completion {
                        label: i.label,
                        detail: i.detail,
                        insert: i.insert,
                    })
                    .collect();
                self.code.show_completions(&items);
            }
            LspEvent::Definition { request, locations } => {
                if self.definition_request.get() != Some(request) {
                    return;
                }
                self.definition_request.set(None);
                match locations.first() {
                    Some(at) if at.path.is_file() => {
                        self.open_source(&at.path, at.line);
                        self.code.set_cursor(&at.path, at.line, at.column);
                    }
                    _ => self.main.set_status("Keine Definition gefunden."),
                }
            }
            LspEvent::Error { request, message } => {
                if request.is_some() && request == self.definition_request.get() {
                    self.definition_request.set(None);
                    self.main
                        .set_status(&format!("Gehe zu Definition: {message}"));
                } else if request.is_some() && request == self.completion_request.get() {
                    self.completion_request.set(None);
                }
            }
            LspEvent::Exited => {
                self.output
                    .line(channel::RUST_ANALYZER, "rust-analyzer wurde beendet.");
                self.lsp_problems.borrow_mut().clear();
                self.refresh_problems();
                *self.lsp.borrow_mut() = None;
                self.lsp_ready.set(false);
                self.main.set_status("rust-analyzer wurde beendet.");
            }
        }
    }

    /// Ends Freya TDE (asks about unsaved changes).
    pub fn quit(&self) -> bool {
        if !self.confirm_discard() {
            return false;
        }
        self.save_desktop();
        self.stop_lsp();
        self.stop_debugging();
        Application::quit();
        true
    }

    /// Title for the main window: project, path, unsaved mark.
    fn title(&self) -> String {
        let session = self.session.borrow();
        let dirty = if session.is_modified() { " *" } else { "" };
        let path = self
            .path
            .borrow()
            .as_ref()
            .map(|p| format!(" – {}", p.display()))
            .unwrap_or_else(|| " – (nicht gespeichert)".to_owned());
        format!(
            "Freya TDE – {}{dirty}{path}",
            session.document().project().name
        )
    }
}
