//! Git via the `git` command line, like VS Code: the user's credentials,
//! SSH keys, hooks and settings just work. No UI here – status, diffs and
//! the log are data; changes are serializable [`GitOp`]s with a
//! [`Git::preview`] (the git commands and findings) before [`Git::run`].
//!
//! Passwords and SSH passphrases: with an [`askpass::Askpass`], `git` and
//! `ssh` ask the running program (which shows a dialog); nothing is stored.
//! Without one, a needed login fails with the code `auth_required` instead
//! of waiting for a terminal.
//!
//! Paths are relative to the repository root and use `/`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tdr_core::Severity;

pub mod askpass;

/// The `git` program: `FREYA_TD_GIT` or `git` from the `PATH`.
pub fn program() -> PathBuf {
    std::env::var_os("FREYA_TD_GIT")
        .filter(|p| !p.is_empty())
        .map_or_else(|| PathBuf::from("git"), PathBuf::from)
}

// --- Data ---------------------------------------------------------------------------

/// The kind of a change, as VS Code shows it with a letter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Change {
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Untracked,
    /// Unresolved merge conflict.
    Conflict,
}

impl Change {
    /// `M`, `A`, `D`, `R`, `U` (untracked), `C` (conflict) …
    pub fn letter(self) -> char {
        match self {
            Self::Modified | Self::TypeChanged => 'M',
            Self::Added | Self::Copied => 'A',
            Self::Deleted => 'D',
            Self::Renamed => 'R',
            Self::Untracked => 'U',
            Self::Conflict => 'C',
        }
    }

    fn from_code(code: char) -> Option<Self> {
        Some(match code {
            'M' => Self::Modified,
            'A' => Self::Added,
            'D' => Self::Deleted,
            'R' => Self::Renamed,
            'C' => Self::Copied,
            'T' => Self::TypeChanged,
            _ => return None,
        })
    }
}

/// A changed file. A file can be staged and changed again afterwards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileStatus {
    pub path: String,
    /// Old path of a rename or copy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orig_path: Option<String>,
    /// Change in the index („Gestaged“).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub staged: Option<Change>,
    /// Change in the working tree („Änderungen“); untracked files and
    /// conflicts are listed here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unstaged: Option<Change>,
}

/// State of the repository.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    /// Current branch; `None` with a detached HEAD.
    pub branch: Option<String>,
    /// Commit id of HEAD; `None` before the first commit.
    pub head: Option<String>,
    /// Upstream branch, e.g. `origin/main`.
    pub upstream: Option<String>,
    /// Commits to push.
    pub ahead: u32,
    /// Commits to pull.
    pub behind: u32,
    pub files: Vec<FileStatus>,
}

impl Status {
    pub fn staged(&self) -> impl Iterator<Item = (&FileStatus, Change)> {
        self.files.iter().filter_map(|f| f.staged.map(|c| (f, c)))
    }

    pub fn unstaged(&self) -> impl Iterator<Item = (&FileStatus, Change)> {
        self.files.iter().filter_map(|f| f.unstaged.map(|c| (f, c)))
    }

    pub fn has_staged(&self) -> bool {
        self.staged().next().is_some()
    }

    pub fn has_conflicts(&self) -> bool {
        self.files
            .iter()
            .any(|f| f.unstaged == Some(Change::Conflict))
    }

    /// The branch as VS Code shows it: `main+` with staged changes, a
    /// short commit id with a detached HEAD.
    pub fn branch_label(&self) -> String {
        let name = match (&self.branch, &self.head) {
            (Some(b), _) => b.clone(),
            (None, Some(h)) => h.chars().take(8).collect(),
            (None, None) => "(kein Commit)".to_owned(),
        };
        if self.has_staged() {
            format!("{name}+")
        } else {
            name
        }
    }

    /// The change of a file (staged or not), for marks in a file tree:
    /// conflict before working tree before index.
    pub fn change_of(&self, path: &str) -> Option<Change> {
        let file = self.files.iter().find(|f| f.path == path)?;
        file.unstaged.or(file.staged)
    }
}

/// A commit of the log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commit {
    pub id: String,
    pub parents: Vec<String>,
    pub author: String,
    pub email: String,
    /// Unix time (seconds).
    pub time: i64,
    pub subject: String,
    /// Branches and tags at this commit (`main`, `origin/main`, `tag: v1`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<String>,
    /// HEAD points here.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub head: bool,
    /// Not pushed yet (in HEAD, not in the upstream).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub outgoing: bool,
    /// Not pulled yet (in the upstream, not in HEAD).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub incoming: bool,
}

/// A local or remote branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Branch {
    /// `main` or `origin/main`.
    pub name: String,
    pub remote: bool,
    pub current: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineKind {
    Context,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: LineKind,
    pub text: String,
}

/// A hunk of a unified diff (line numbers 1-based).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hunk {
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    pub lines: Vec<DiffLine>,
}

/// The diff of one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diff {
    pub path: String,
    /// The unified diff as git prints it.
    pub text: String,
    pub hunks: Vec<Hunk>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub binary: bool,
}

/// Parses the hunks of a unified diff.
pub fn parse_hunks(diff: &str) -> Vec<Hunk> {
    fn range(text: &str) -> (usize, usize) {
        let text = &text[1..];
        match text.split_once(',') {
            Some((start, lines)) => (start.parse().unwrap_or(0), lines.parse().unwrap_or(0)),
            None => (text.parse().unwrap_or(0), 1),
        }
    }
    let mut hunks: Vec<Hunk> = Vec::new();
    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("@@ ") {
            let mut parts = rest.split(' ');
            let (old_start, old_lines) = parts.next().map_or((0, 0), range);
            let (new_start, new_lines) = parts.next().map_or((0, 0), range);
            hunks.push(Hunk {
                old_start,
                old_lines,
                new_start,
                new_lines,
                lines: Vec::new(),
            });
            continue;
        }
        let Some(hunk) = hunks.last_mut() else {
            continue;
        };
        let (kind, text) = match line.chars().next() {
            Some('+') => (LineKind::Added, &line[1..]),
            Some('-') => (LineKind::Removed, &line[1..]),
            Some(' ') => (LineKind::Context, &line[1..]),
            // `\ No newline at end of file` und Kopfzeilen der nächsten Datei.
            _ => continue,
        };
        hunk.lines.push(DiffLine {
            kind,
            text: text.to_owned(),
        });
    }
    hunks
}

// --- Graph --------------------------------------------------------------------------

/// Where a graph line starts or ends within a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowPos {
    Top,
    /// The commit's dot.
    Middle,
    Bottom,
}

/// A line of the graph inside one row: from lane/position to lane/position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphLine {
    pub from: (usize, RowPos),
    pub to: (usize, RowPos),
    /// Lane whose color the line takes.
    pub lane: usize,
}

/// One row of the commit graph (same order as the commits).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphRow {
    /// Lane of the commit's dot.
    pub lane: usize,
    pub lines: Vec<GraphLine>,
}

/// Lays out the commit graph: one lane per line of development, merges
/// and forks as diagonal lines (like VS Code's graph view).
pub fn graph(commits: &[Commit]) -> Vec<GraphRow> {
    // Je Spur: der Commit, den sie als Nächstes erwartet.
    let mut lanes: Vec<Option<String>> = Vec::new();
    let mut rows = Vec::with_capacity(commits.len());
    for commit in commits {
        let lane = lanes
            .iter()
            .position(|l| l.as_deref() == Some(commit.id.as_str()))
            .or_else(|| lanes.iter().position(Option::is_none))
            .unwrap_or_else(|| {
                lanes.push(None);
                lanes.len() - 1
            });
        let mut lines = Vec::new();
        // Von oben: durchlaufende Spuren und Spuren, die hier enden.
        for (i, expected) in lanes.iter().enumerate() {
            match expected {
                Some(id) if *id == commit.id => lines.push(GraphLine {
                    from: (i, RowPos::Top),
                    to: (lane, RowPos::Middle),
                    lane: i,
                }),
                Some(_) => lines.push(GraphLine {
                    from: (i, RowPos::Top),
                    to: (i, RowPos::Bottom),
                    lane: i,
                }),
                None => {}
            }
        }
        for expected in lanes.iter_mut() {
            if expected.as_deref() == Some(commit.id.as_str()) {
                *expected = None;
            }
        }
        // Nach unten: zu den Eltern.
        for (n, parent) in commit.parents.iter().enumerate() {
            let target = if n == 0 && lanes[lane].is_none() {
                lane
            } else if let Some(j) = lanes.iter().position(|l| l.as_ref() == Some(parent)) {
                j
            } else {
                match lanes.iter().position(Option::is_none) {
                    Some(free) => free,
                    None => {
                        lanes.push(None);
                        lanes.len() - 1
                    }
                }
            };
            lanes[target] = Some(parent.clone());
            lines.push(GraphLine {
                from: (lane, RowPos::Middle),
                to: (target, RowPos::Bottom),
                lane: target,
            });
        }
        while lanes.last().is_some_and(Option::is_none) {
            lanes.pop();
        }
        rows.push(GraphRow { lane, lines });
    }
    rows
}

// --- Operations ---------------------------------------------------------------------

/// What follows a commit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum After {
    #[default]
    Nothing,
    Push,
    /// Pull, then push.
    Sync,
}

/// A change to the repository; serialized as `{"op": "commit", …}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum GitOp {
    /// Stage files (empty = all changes).
    Stage {
        #[serde(default)]
        paths: Vec<String>,
    },
    /// Take files out of the index (empty = all).
    Unstage {
        #[serde(default)]
        paths: Vec<String>,
    },
    /// Throw away changes in the working tree (empty = all; a path can be
    /// a folder); untracked files go to the trash.
    Discard {
        #[serde(default)]
        paths: Vec<String>,
    },
    Commit {
        message: String,
        /// Replace the last commit.
        #[serde(default)]
        amend: bool,
        #[serde(default)]
        then: After,
    },
    /// Take back the last commit; its changes stay staged.
    UndoLastCommit,
    Fetch,
    /// Fetch from all remotes.
    FetchAll,
    Pull,
    /// Pull with rebase instead of a merge.
    PullRebase,
    /// Push; without an upstream the branch is published (`-u`).
    Push,
    /// Pull, then push.
    Sync,
    /// Switch to a branch (a remote one gets a local tracking branch) or
    /// to a tag (detached HEAD).
    Checkout {
        branch: String,
    },
    CreateBranch {
        name: String,
        #[serde(default = "yes")]
        checkout: bool,
    },
    /// Rename the current branch.
    RenameBranch {
        name: String,
    },
    DeleteBranch {
        name: String,
        /// Also when it is not merged.
        #[serde(default)]
        force: bool,
    },
    /// Merge a branch into the current one.
    Merge {
        branch: String,
    },
    /// Rebase the current branch onto another.
    Rebase {
        branch: String,
    },
    AddRemote {
        name: String,
        url: String,
    },
    RemoveRemote {
        name: String,
    },
    /// Put the changes aside.
    Stash {
        #[serde(default)]
        message: String,
        #[serde(default)]
        include_untracked: bool,
    },
    /// Bring back a stash (0 = newest); `pop` also removes it.
    StashApply {
        #[serde(default)]
        index: usize,
        #[serde(default)]
        pop: bool,
    },
    StashDrop {
        #[serde(default)]
        index: usize,
    },
    /// A tag at HEAD; with a message an annotated tag.
    CreateTag {
        name: String,
        #[serde(default)]
        message: String,
    },
    DeleteTag {
        name: String,
    },
    /// Push all tags.
    PushTags,
}

/// Which branches the log (graph) shows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "refs", content = "names", rename_all = "snake_case")]
pub enum LogRefs {
    /// HEAD and its upstream (VS Code's „Auto“).
    #[default]
    Auto,
    /// All branches, remote branches and tags.
    All,
    /// These branches or tags.
    Some(Vec<String>),
}

/// A stash entry (0 = newest).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StashEntry {
    pub index: usize,
    pub message: String,
}

fn yes() -> bool {
    true
}

/// A finding of [`Git::preview`]. Codes are stable: `empty_message`,
/// `nothing_staged`, `conflicts`, `amend_pushed` (warning), `no_remote`,
/// `no_upstream` (warning: push publishes the branch), `detached_head`,
/// `invalid_branch_name`, `branch_exists`, `unknown_branch`,
/// `nothing_to_discard`, `untracked_deleted` (warning), `no_commit`,
/// `undo_pushed` (warning), `current_branch`, `remote_exists`,
/// `unknown_remote`, `invalid_remote`, `nothing_to_stash`, `unknown_stash`,
/// `invalid_tag_name`, `tag_exists`, `unknown_tag`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitIssue {
    pub severity: Severity,
    pub code: &'static str,
    pub detail: String,
}

/// The dry run of a [`GitOp`]: the git commands in order and the findings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Preview {
    pub commands: Vec<Vec<String>>,
    /// Untracked files [`GitOp::Discard`] deletes (into the trash).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub delete: Vec<String>,
    pub issues: Vec<GitIssue>,
}

impl Preview {
    pub fn has_errors(&self) -> bool {
        self.issues.iter().any(|i| i.severity == Severity::Error)
    }
}

/// A failure. Codes are stable: `git_missing`, `not_a_repository`,
/// `refused` (the preview has errors), `auth_required`, `rejected` (push
/// refused, pull first), `diverged`, `merge_conflict`, `local_changes`,
/// `not_merged`, `signing_failed`, `network`, `failed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitError {
    pub code: &'static str,
    pub detail: String,
    /// What git printed (English).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub output: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<GitIssue>,
}

impl GitError {
    fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
            output: String::new(),
            issues: Vec::new(),
        }
    }

    /// Sorts a failed git run by what it printed.
    fn from_output(output: String) -> Self {
        let has = |s: &str| output.contains(s);
        let (code, detail) = if has("Permission denied")
            || has("Authentication failed")
            || has("could not read Username")
            || has("could not read Password")
            || has("terminal prompts disabled")
            || has("passphrase")
            || has("Host key verification failed")
        {
            (
                "auth_required",
                "Anmeldung fehlgeschlagen oder abgebrochen.",
            )
        } else if has("[rejected]") || has("non-fast-forward") || has("fetch first") {
            (
                "rejected",
                "Push abgelehnt: Das Remote hat neuere Commits – zuerst pullen.",
            )
        } else if has("CONFLICT") || has("Automatic merge failed") {
            (
                "merge_conflict",
                "Konflikte beim Zusammenführen – bitte auflösen und committen.",
            )
        } else if has("divergent branches") || has("Not possible to fast-forward") {
            (
                "diverged",
                "Lokaler und entfernter Zweig sind auseinandergelaufen (pull.rebase einstellen oder von Hand zusammenführen).",
            )
        } else if has("gpg failed to sign") || has("failed to write commit object") {
            (
                "signing_failed",
                "Signieren fehlgeschlagen (GPG-Passphrase abgebrochen oder gpg-agent nicht erreichbar).",
            )
        } else if has("not fully merged") {
            (
                "not_merged",
                "Der Zweig ist noch nicht zusammengeführt – „Löschen (erzwingen)“ verwirft seine Commits.",
            )
        } else if has("would be overwritten") {
            (
                "local_changes",
                "Lokale Änderungen würden überschrieben – erst committen oder verwerfen.",
            )
        } else if has("Could not resolve host")
            || has("unable to access")
            || has("Connection timed out")
            || has("Connection refused")
            || has("Could not read from remote repository")
        {
            ("network", "Das Remote ist nicht erreichbar.")
        } else {
            ("failed", "git meldet einen Fehler.")
        };
        Self {
            code,
            detail: detail.to_owned(),
            output,
            issues: Vec::new(),
        }
    }
}

impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.detail)?;
        for issue in &self.issues {
            write!(f, " {}", issue.detail)?;
        }
        let output = self.output.trim();
        if !output.is_empty() {
            write!(f, "\n\n{output}")?;
        }
        Ok(())
    }
}

impl std::error::Error for GitError {}

// --- Repository ---------------------------------------------------------------------

/// A Git repository (its root folder).
#[derive(Debug, Clone)]
pub struct Git {
    root: PathBuf,
    askpass: Option<askpass::Askpass>,
    /// Commands of [`Git::run`] with their output (VS Code's „Git Output“).
    output: Option<Arc<Mutex<String>>>,
}

impl Git {
    /// The repository `dir` belongs to.
    pub fn open(dir: &Path) -> Result<Self, GitError> {
        let output = run_git(dir, &["rev-parse", "--show-toplevel"], None, None)?;
        if !output.success {
            return Err(GitError::new(
                "not_a_repository",
                format!("{} liegt in keinem Git-Repository.", dir.display()),
            ));
        }
        let root = PathBuf::from(output.stdout.trim());
        Ok(Self {
            root: root.canonicalize().unwrap_or(root),
            askpass: None,
            output: None,
        })
    }

    /// Creates a repository in `dir` (`git init`).
    pub fn init(dir: &Path) -> Result<Self, GitError> {
        std::fs::create_dir_all(dir).map_err(|e| GitError::new("failed", e.to_string()))?;
        run_git(dir, &["init", "-q"], None, None)?.check()?;
        Self::open(dir)
    }

    /// Clones `url` into `dir` (which must not exist or be empty).
    pub fn clone(
        url: &str,
        dir: &Path,
        askpass: Option<askpass::Askpass>,
    ) -> Result<Self, GitError> {
        let parent = dir.parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(parent).map_err(|e| GitError::new("failed", e.to_string()))?;
        let target = dir.to_string_lossy();
        let output = run_git(
            parent,
            &["clone", "--progress", "--", url, &target],
            None,
            askpass.as_ref(),
        )?;
        if !output.success {
            return Err(GitError::from_output(output.stderr));
        }
        Ok(Self::open(dir)?.with_askpass(askpass))
    }

    /// Collects the commands of [`Git::run`] with their output in `sink`.
    pub fn with_output(mut self, sink: Option<Arc<Mutex<String>>>) -> Self {
        self.output = sink;
        self
    }

    /// Lets `git`/`ssh` ask for passwords and passphrases.
    pub fn with_askpass(mut self, askpass: Option<askpass::Askpass>) -> Self {
        self.askpass = askpass;
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Path of a file relative to the repository (`None` if outside).
    pub fn relative(&self, path: &Path) -> Option<String> {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let relative = path.strip_prefix(&self.root).ok()?;
        Some(
            relative
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/"),
        )
    }

    fn git(&self, args: &[&str], stdin: Option<&str>) -> Result<GitOutput, GitError> {
        run_git(&self.root, args, stdin, self.askpass.as_ref())
    }

    /// Branch, upstream, ahead/behind and the changed files.
    pub fn status(&self) -> Result<Status, GitError> {
        let output = self.git(
            &[
                "--no-optional-locks",
                "status",
                "--porcelain=v2",
                "--branch",
                "-z",
                "--untracked-files=all",
            ],
            None,
        )?;
        let output = output.check()?;
        Ok(parse_status(&output))
    }

    /// The last `limit` commits of HEAD and its upstream, newest first.
    pub fn log(&self, limit: usize) -> Result<Vec<Commit>, GitError> {
        self.log_refs(limit, &LogRefs::Auto)
    }

    /// The last `limit` commits of the chosen branches, newest first.
    pub fn log_refs(&self, limit: usize, refs: &LogRefs) -> Result<Vec<Commit>, GitError> {
        let status = self.status()?;
        if status.head.is_none() {
            return Ok(Vec::new());
        }
        let limit = format!("-n{limit}");
        let mut args = vec![
            "log",
            "--topo-order",
            "--format=%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%D%x1f%s%x1e",
            &limit,
        ];
        match refs {
            LogRefs::Auto => {
                args.push("HEAD");
                if let Some(upstream) = &status.upstream {
                    args.push(upstream);
                }
            }
            LogRefs::All => args.extend(["HEAD", "--branches", "--remotes", "--tags"]),
            LogRefs::Some(names) if names.is_empty() => args.push("HEAD"),
            LogRefs::Some(names) => args.extend(names.iter().map(String::as_str)),
        }
        args.push("--");
        let text = self.git(&args, None)?.check()?;
        let set = |range: String| -> Vec<String> {
            self.git(&["rev-list", &range, "--"], None)
                .ok()
                .filter(|o| o.success)
                .map(|o| o.stdout.lines().map(str::to_owned).collect())
                .unwrap_or_default()
        };
        let (outgoing, incoming) = match &status.upstream {
            Some(up) => (set(format!("{up}..HEAD")), set(format!("HEAD..{up}"))),
            None => (Vec::new(), Vec::new()),
        };
        Ok(text
            .split('\x1e')
            .filter_map(|record| {
                let record = record.trim_start_matches('\n');
                let mut f = record.split('\x1f');
                let id = f.next()?.to_owned();
                if id.is_empty() {
                    return None;
                }
                let parents = f.next()?.split_whitespace().map(str::to_owned).collect();
                let author = f.next()?.to_owned();
                let email = f.next()?.to_owned();
                let time = f.next()?.parse().unwrap_or(0);
                let mut head = false;
                let refs = f
                    .next()?
                    .split(", ")
                    .filter(|r| !r.is_empty())
                    .filter_map(|r| match r.strip_prefix("HEAD -> ") {
                        Some(branch) => {
                            head = true;
                            Some(branch.to_owned())
                        }
                        None if r == "HEAD" => {
                            head = true;
                            None
                        }
                        None => Some(r.to_owned()),
                    })
                    .collect();
                let subject = f.next().unwrap_or_default().to_owned();
                Some(Commit {
                    outgoing: outgoing.contains(&id),
                    incoming: incoming.contains(&id),
                    id,
                    parents,
                    author,
                    email,
                    time,
                    subject,
                    refs,
                    head,
                })
            })
            .collect())
    }

    /// Local and remote branches.
    pub fn branches(&self) -> Result<Vec<Branch>, GitError> {
        let text = self
            .git(
                &[
                    "for-each-ref",
                    "--format=%(refname)%1f%(HEAD)%1f%(upstream:short)",
                    "refs/heads",
                    "refs/remotes",
                ],
                None,
            )?
            .check()?;
        Ok(text
            .lines()
            .filter_map(|line| {
                let mut f = line.split('\x1f');
                let refname = f.next()?;
                let current = f.next()? == "*";
                let upstream = f.next().filter(|u| !u.is_empty()).map(str::to_owned);
                let (name, remote) = if let Some(n) = refname.strip_prefix("refs/heads/") {
                    (n, false)
                } else {
                    let n = refname.strip_prefix("refs/remotes/")?;
                    if n.ends_with("/HEAD") {
                        return None;
                    }
                    (n, true)
                };
                Some(Branch {
                    name: name.to_owned(),
                    remote,
                    current,
                    upstream,
                })
            })
            .collect())
    }

    /// The stashes, newest first.
    pub fn stashes(&self) -> Result<Vec<StashEntry>, GitError> {
        let text = self
            .git(&["stash", "list", "--format=%gs"], None)?
            .check()?;
        Ok(text
            .lines()
            .enumerate()
            .map(|(index, message)| StashEntry {
                index,
                message: message.to_owned(),
            })
            .collect())
    }

    /// The tags, newest first.
    pub fn tags(&self) -> Result<Vec<String>, GitError> {
        let text = self.git(&["tag", "--sort=-creatordate"], None)?.check()?;
        Ok(text.lines().map(str::to_owned).collect())
    }

    /// Names of the remotes (`origin` …).
    pub fn remotes(&self) -> Result<Vec<String>, GitError> {
        let text = self.git(&["remote"], None)?.check()?;
        Ok(text.lines().map(str::to_owned).collect())
    }

    /// The diff of a file: staged (index against HEAD) or in the working
    /// tree (against the index; untracked files against nothing).
    pub fn diff(&self, path: &str, staged: bool) -> Result<Diff, GitError> {
        let status = self.status()?;
        let untracked = !staged
            && status
                .files
                .iter()
                .any(|f| f.path == path && f.unstaged == Some(Change::Untracked));
        let output = if untracked {
            // Unterschied zu „nichts“: Exit-Code 1 heißt hier „verschieden“.
            let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
            self.git(
                &["diff", "--no-index", "--no-color", "--", null, path],
                None,
            )?
            .stdout
        } else if staged {
            self.git(&["diff", "--cached", "--no-color", "--", path], None)?
                .check()?
        } else {
            self.git(&["diff", "--no-color", "--", path], None)?
                .check()?
        };
        Ok(Diff {
            path: path.to_owned(),
            binary: output.contains("\nBinary files ") || output.starts_with("Binary files "),
            hunks: parse_hunks(&output),
            text: output,
        })
    }

    /// The text of a file in HEAD (`None` if it is not committed).
    pub fn head_text(&self, path: &str) -> Option<String> {
        let output = self.git(&["show", &format!("HEAD:{path}")], None).ok()?;
        output.success.then_some(output.stdout)
    }

    // --- Changes ------------------------------------------------------------------

    /// What `op` would do: the git commands in order and the findings.
    pub fn preview(&self, op: &GitOp) -> Result<Preview, GitError> {
        let status = self.status()?;
        let mut commands: Vec<Vec<String>> = Vec::new();
        let mut delete = Vec::new();
        let mut issues = Vec::new();
        let error = |code, detail: String| GitIssue {
            severity: Severity::Error,
            code,
            detail,
        };
        let warning = |code, detail: String| GitIssue {
            severity: Severity::Warning,
            code,
            detail,
        };
        let cmd = |args: &[&str], paths: &[String]| -> Vec<String> {
            let mut c: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
            if !paths.is_empty() {
                c.push("--".to_owned());
                c.extend(paths.iter().cloned());
            }
            c
        };
        let remote = || -> Result<Option<String>, GitError> {
            let remotes = self.remotes()?;
            Ok(remotes
                .iter()
                .find(|r| *r == "origin")
                .or(remotes.first())
                .cloned())
        };
        let push = |commands: &mut Vec<Vec<String>>,
                    issues: &mut Vec<GitIssue>|
         -> Result<(), GitError> {
            let Some(branch) = &status.branch else {
                issues.push(error(
                    "detached_head",
                    "Kein Zweig ausgecheckt (losgelöster HEAD).".into(),
                ));
                return Ok(());
            };
            if status.upstream.is_some() {
                commands.push(cmd(&["push"], &[]));
                return Ok(());
            }
            match remote()? {
                Some(remote) => {
                    issues.push(warning(
                        "no_upstream",
                        format!("{branch} wird auf {remote} veröffentlicht."),
                    ));
                    commands.push(cmd(&["push", "-u", &remote, branch], &[]));
                }
                None => issues.push(error("no_remote", "Das Repository hat kein Remote.".into())),
            }
            Ok(())
        };
        let pull = |commands: &mut Vec<Vec<String>>, issues: &mut Vec<GitIssue>| {
            if status.branch.is_none() {
                issues.push(error(
                    "detached_head",
                    "Kein Zweig ausgecheckt (losgelöster HEAD).".into(),
                ));
            } else if status.upstream.is_none() {
                issues.push(error(
                    "no_upstream",
                    "Der Zweig hat noch keinen Upstream – zuerst pushen.".into(),
                ));
            } else {
                commands.push(cmd(&["pull"], &[]));
            }
        };
        match op {
            GitOp::Stage { paths } => {
                commands.push(cmd(&["add", "-A"], paths));
            }
            GitOp::Unstage { paths } => {
                let paths: Vec<String> = if paths.is_empty() {
                    status.staged().map(|(f, _)| f.path.clone()).collect()
                } else {
                    paths.clone()
                };
                if status.head.is_some() {
                    commands.push(cmd(&["restore", "--staged"], &paths));
                } else if !paths.is_empty() {
                    commands.push(cmd(&["rm", "--cached", "-r", "-q"], &paths));
                }
            }
            GitOp::Discard { paths } => {
                // Ein Pfad kann auch ein Ordner sein.
                let selected = |f: &FileStatus| {
                    paths.is_empty()
                        || paths
                            .iter()
                            .any(|p| f.path == *p || f.path.starts_with(&format!("{p}/")))
                };
                let mut restore = Vec::new();
                for (file, change) in status.unstaged().filter(|(f, _)| selected(f)) {
                    match change {
                        Change::Untracked => delete.push(file.path.clone()),
                        Change::Conflict => {}
                        _ => restore.push(file.path.clone()),
                    }
                }
                if !restore.is_empty() {
                    commands.push(cmd(&["restore", "--worktree"], &restore));
                }
                if !delete.is_empty() {
                    issues.push(warning(
                        "untracked_deleted",
                        format!(
                            "Neue Dateien werden gelöscht (Papierkorb): {}",
                            delete.join(", ")
                        ),
                    ));
                }
                if commands.is_empty() && delete.is_empty() {
                    issues.push(error(
                        "nothing_to_discard",
                        "Keine Änderungen zum Verwerfen.".into(),
                    ));
                }
            }
            GitOp::Commit {
                message,
                amend,
                then,
            } => {
                if message.trim().is_empty() {
                    issues.push(error(
                        "empty_message",
                        "Die Commit-Nachricht ist leer.".into(),
                    ));
                }
                if status.has_conflicts() {
                    issues.push(error(
                        "conflicts",
                        "Es gibt noch ungelöste Konflikte.".into(),
                    ));
                }
                if *amend {
                    if status.head.is_none() {
                        issues.push(error(
                            "no_commit",
                            "Es gibt noch keinen Commit zum Ändern.".into(),
                        ));
                    } else if status.upstream.is_some() && status.ahead == 0 {
                        issues.push(warning(
                            "amend_pushed",
                            "Der letzte Commit ist schon gepusht; danach ist ein erzwungener Push nötig."
                                .into(),
                        ));
                    }
                } else if !status.has_staged() {
                    issues.push(error("nothing_staged", "Es ist nichts gestaged.".into()));
                }
                let mut commit = vec!["commit", "-F", "-"];
                if *amend {
                    commit.insert(1, "--amend");
                }
                commands.push(cmd(&commit, &[]));
                match then {
                    After::Nothing => {}
                    After::Push => push(&mut commands, &mut issues)?,
                    After::Sync => {
                        if status.upstream.is_some() {
                            pull(&mut commands, &mut issues);
                        }
                        push(&mut commands, &mut issues)?;
                    }
                }
            }
            GitOp::Fetch => {
                if remote()?.is_none() {
                    issues.push(error("no_remote", "Das Repository hat kein Remote.".into()));
                }
                commands.push(cmd(&["fetch", "--prune"], &[]));
            }
            GitOp::Pull => pull(&mut commands, &mut issues),
            GitOp::Push => push(&mut commands, &mut issues)?,
            GitOp::Sync => {
                if status.upstream.is_some() {
                    pull(&mut commands, &mut issues);
                }
                push(&mut commands, &mut issues)?;
            }
            GitOp::Checkout { branch } => {
                let branches = self.branches()?;
                match branches.iter().find(|b| &b.name == branch) {
                    None if self.tags()?.contains(branch) => {
                        commands.push(cmd(&["switch", "--detach", branch], &[]));
                    }
                    None => issues.push(error(
                        "unknown_branch",
                        format!("Den Zweig {branch} gibt es nicht."),
                    )),
                    Some(b) if b.remote => {
                        // origin/feature → lokaler Zweig feature mit Upstream.
                        let local = branch.split_once('/').map_or(branch.as_str(), |(_, n)| n);
                        if branches.iter().any(|b| !b.remote && b.name == local) {
                            commands.push(cmd(&["switch", local], &[]));
                        } else {
                            commands.push(cmd(&["switch", "--track", branch], &[]));
                        }
                    }
                    Some(_) => commands.push(cmd(&["switch", branch], &[])),
                }
            }
            GitOp::CreateBranch { name, checkout } => {
                self.check_branch_name(name, &mut issues)?;
                if *checkout {
                    commands.push(cmd(&["switch", "-c", name], &[]));
                } else {
                    commands.push(cmd(&["branch", name], &[]));
                }
            }
            GitOp::UndoLastCommit => {
                if status.head.is_none() {
                    issues.push(error("no_commit", "Es gibt noch keinen Commit.".into()));
                } else {
                    if status.upstream.is_some() && status.ahead == 0 {
                        issues.push(warning(
                            "undo_pushed",
                            "Der letzte Commit ist schon gepusht; danach ist ein erzwungener Push nötig."
                                .into(),
                        ));
                    }
                    let has_parent = self
                        .git(&["rev-parse", "--verify", "-q", "HEAD~1"], None)
                        .is_ok_and(|o| o.success);
                    if has_parent {
                        commands.push(cmd(&["reset", "--soft", "HEAD~1"], &[]));
                    } else {
                        // Der erste Commit: HEAD löschen, der Index bleibt.
                        commands.push(cmd(&["update-ref", "-d", "HEAD"], &[]));
                    }
                }
            }
            GitOp::FetchAll => {
                if remote()?.is_none() {
                    issues.push(error("no_remote", "Das Repository hat kein Remote.".into()));
                }
                commands.push(cmd(&["fetch", "--all", "--prune"], &[]));
            }
            GitOp::PullRebase => {
                pull(&mut commands, &mut issues);
                if let Some(c) = commands.last_mut() {
                    c.push("--rebase".to_owned());
                }
            }
            GitOp::RenameBranch { name } => {
                if status.branch.is_none() {
                    issues.push(error(
                        "detached_head",
                        "Kein Zweig ausgecheckt (losgelöster HEAD).".into(),
                    ));
                }
                self.check_branch_name(name, &mut issues)?;
                commands.push(cmd(&["branch", "-m", name], &[]));
            }
            GitOp::DeleteBranch { name, force } => {
                match self
                    .branches()?
                    .iter()
                    .find(|b| !b.remote && &b.name == name)
                {
                    None => issues.push(error(
                        "unknown_branch",
                        format!("Den Zweig {name} gibt es nicht."),
                    )),
                    Some(b) if b.current => issues.push(error(
                        "current_branch",
                        format!("{name} ist ausgecheckt – erst einen anderen Zweig wählen."),
                    )),
                    Some(_) => {}
                }
                commands.push(cmd(
                    &["branch", if *force { "-D" } else { "-d" }, name],
                    &[],
                ));
            }
            GitOp::Merge { branch } | GitOp::Rebase { branch } => {
                let known = self.branches()?.iter().any(|b| &b.name == branch)
                    || self.tags()?.contains(branch);
                if !known {
                    issues.push(error(
                        "unknown_branch",
                        format!("Den Zweig {branch} gibt es nicht."),
                    ));
                }
                if status.has_conflicts() {
                    issues.push(error(
                        "conflicts",
                        "Es gibt noch ungelöste Konflikte.".into(),
                    ));
                }
                if matches!(op, GitOp::Merge { .. }) {
                    commands.push(cmd(&["merge", "--no-edit", branch], &[]));
                } else {
                    commands.push(cmd(&["rebase", branch], &[]));
                }
            }
            GitOp::AddRemote { name, url } => {
                let valid = !name.is_empty()
                    && !name.starts_with('-')
                    && self
                        .git(
                            &["check-ref-format", &format!("refs/remotes/{name}/x")],
                            None,
                        )
                        .is_ok_and(|o| o.success);
                if !valid || url.trim().is_empty() {
                    issues.push(error(
                        "invalid_remote",
                        format!("„{name}“ mit „{url}“ ist kein gültiges Remote."),
                    ));
                } else if self.remotes()?.contains(name) {
                    issues.push(error(
                        "remote_exists",
                        format!("Das Remote {name} gibt es schon."),
                    ));
                }
                commands.push(cmd(&["remote", "add", name, url.trim()], &[]));
            }
            GitOp::RemoveRemote { name } => {
                if !self.remotes()?.contains(name) {
                    issues.push(error(
                        "unknown_remote",
                        format!("Das Remote {name} gibt es nicht."),
                    ));
                }
                commands.push(cmd(&["remote", "remove", name], &[]));
            }
            GitOp::Stash {
                message,
                include_untracked,
            } => {
                let changes = status
                    .files
                    .iter()
                    .any(|f| *include_untracked || f.unstaged != Some(Change::Untracked));
                if !changes {
                    issues.push(error(
                        "nothing_to_stash",
                        "Keine Änderungen zum Zurücklegen.".into(),
                    ));
                }
                let mut stash = vec!["stash", "push"];
                if *include_untracked {
                    stash.push("--include-untracked");
                }
                if !message.trim().is_empty() {
                    stash.extend(["-m", message.trim()]);
                }
                commands.push(cmd(&stash, &[]));
            }
            GitOp::StashApply { index, pop } => {
                self.check_stash(*index, &mut issues)?;
                let name = format!("stash@{{{index}}}");
                commands.push(cmd(
                    &["stash", if *pop { "pop" } else { "apply" }, &name],
                    &[],
                ));
            }
            GitOp::StashDrop { index } => {
                self.check_stash(*index, &mut issues)?;
                commands.push(cmd(&["stash", "drop", &format!("stash@{{{index}}}")], &[]));
            }
            GitOp::CreateTag { name, message } => {
                let valid = !name.starts_with('-')
                    && self
                        .git(&["check-ref-format", &format!("refs/tags/{name}")], None)
                        .is_ok_and(|o| o.success);
                if !valid {
                    issues.push(error(
                        "invalid_tag_name",
                        format!("„{name}“ ist kein gültiger Tag-Name."),
                    ));
                } else if self.tags()?.contains(name) {
                    issues.push(error(
                        "tag_exists",
                        format!("Den Tag {name} gibt es schon."),
                    ));
                }
                if status.head.is_none() {
                    issues.push(error("no_commit", "Es gibt noch keinen Commit.".into()));
                }
                if message.trim().is_empty() {
                    commands.push(cmd(&["tag", name], &[]));
                } else {
                    commands.push(cmd(&["tag", "-a", name, "-F", "-"], &[]));
                }
            }
            GitOp::DeleteTag { name } => {
                if !self.tags()?.contains(name) {
                    issues.push(error(
                        "unknown_tag",
                        format!("Den Tag {name} gibt es nicht."),
                    ));
                }
                commands.push(cmd(&["tag", "-d", name], &[]));
            }
            GitOp::PushTags => match remote()? {
                Some(remote) => commands.push(cmd(&["push", &remote, "--tags"], &[])),
                None => issues.push(error("no_remote", "Das Repository hat kein Remote.".into())),
            },
        }
        Ok(Preview {
            commands,
            delete,
            issues,
        })
    }

    /// Performs `op` (see [`Git::preview`]); refuses when the preview has
    /// errors. Returns what git printed.
    pub fn run(&self, op: &GitOp) -> Result<String, GitError> {
        let preview = self.preview(op)?;
        if preview.has_errors() {
            return Err(GitError {
                code: "refused",
                detail: "Nicht ausgeführt:".to_owned(),
                output: String::new(),
                issues: preview
                    .issues
                    .into_iter()
                    .filter(|i| i.severity == Severity::Error)
                    .collect(),
            });
        }
        let stdin = match op {
            GitOp::Commit { message, .. } | GitOp::CreateTag { message, .. } => {
                Some(message.as_str())
            }
            _ => None,
        };
        let mut log = String::new();
        for path in &preview.delete {
            trash::delete(self.root.join(path))
                .map_err(|e| GitError::new("failed", format!("{path}: {e}")))?;
        }
        for command in &preview.commands {
            let args: Vec<&str> = command.iter().map(String::as_str).collect();
            // Nachrichten kommen über stdin (`-F -`).
            let input = args
                .windows(2)
                .any(|w| w == ["-F", "-"])
                .then_some(stdin)
                .flatten();
            let output = self.git(&args, input)?;
            log.push_str(&output.stdout);
            log.push_str(&output.stderr);
            if let Some(sink) = &self.output
                && let Ok(mut sink) = sink.lock()
            {
                sink.push_str(&format!(
                    "> git {}\n{}{}",
                    args.join(" "),
                    output.stdout,
                    output.stderr
                ));
                if !sink.ends_with('\n') {
                    sink.push('\n');
                }
            }
            if !output.success {
                return Err(GitError::from_output(format!(
                    "{}{}",
                    output.stdout, output.stderr
                )));
            }
        }
        Ok(log)
    }
}

impl Git {
    fn check_branch_name(&self, name: &str, issues: &mut Vec<GitIssue>) -> Result<(), GitError> {
        let valid = !name.starts_with('-')
            && self
                .git(&["check-ref-format", "--branch", name], None)
                .is_ok_and(|o| o.success);
        if !valid {
            issues.push(GitIssue {
                severity: Severity::Error,
                code: "invalid_branch_name",
                detail: format!("„{name}“ ist kein gültiger Zweigname."),
            });
        } else if self.branches()?.iter().any(|b| !b.remote && b.name == name) {
            issues.push(GitIssue {
                severity: Severity::Error,
                code: "branch_exists",
                detail: format!("Den Zweig {name} gibt es schon."),
            });
        }
        Ok(())
    }

    fn check_stash(&self, index: usize, issues: &mut Vec<GitIssue>) -> Result<(), GitError> {
        if index >= self.stashes()?.len() {
            issues.push(GitIssue {
                severity: Severity::Error,
                code: "unknown_stash",
                detail: format!("Den Stash {index} gibt es nicht."),
            });
        }
        Ok(())
    }
}

// --- Running git ----------------------------------------------------------------------

struct GitOutput {
    success: bool,
    stdout: String,
    stderr: String,
}

impl GitOutput {
    fn check(self) -> Result<String, GitError> {
        if self.success {
            Ok(self.stdout)
        } else {
            Err(GitError::from_output(self.stderr))
        }
    }
}

fn run_git(
    dir: &Path,
    args: &[&str],
    stdin: Option<&str>,
    askpass: Option<&askpass::Askpass>,
) -> Result<GitOutput, GitError> {
    let program = program();
    let mut command = Command::new(&program);
    command
        .arg("-C")
        .arg(dir)
        .args(args)
        // Meldungen auf Englisch: daran erkennt `GitError` die Ursache.
        .env("LC_ALL", "C")
        .env("LANGUAGE", "C")
        // Nie auf ein Terminal oder einen Editor warten.
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_EDITOR", "true")
        .env("GIT_MERGE_AUTOEDIT", "no")
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    match askpass {
        Some(askpass) => askpass.apply(&mut command),
        None => askpass::disable(&mut command),
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Kein Konsolenfenster aufblitzen lassen.
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn().map_err(|e| {
        GitError::new(
            "git_missing",
            format!("git nicht gefunden ({}): {e}", program.display()),
        )
    })?;
    if let (Some(text), Some(mut pipe)) = (stdin, child.stdin.take()) {
        use std::io::Write;
        let _ = pipe.write_all(text.as_bytes());
    }
    let output = child
        .wait_with_output()
        .map_err(|e| GitError::new("failed", e.to_string()))?;
    Ok(GitOutput {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

/// Parses `git status --porcelain=v2 --branch -z`.
fn parse_status(text: &str) -> Status {
    let mut status = Status::default();
    let mut fields = text.split('\0');
    let xy = |code: &str| -> (Option<Change>, Option<Change>) {
        let mut chars = code.chars();
        let x = chars.next().and_then(Change::from_code);
        let y = chars.next().and_then(Change::from_code);
        (x, y)
    };
    while let Some(record) = fields.next() {
        if let Some(header) = record.strip_prefix("# ") {
            let (key, value) = header.split_once(' ').unwrap_or((header, ""));
            match key {
                "branch.oid" if value != "(initial)" => status.head = Some(value.to_owned()),
                "branch.head" if value != "(detached)" => status.branch = Some(value.to_owned()),
                "branch.upstream" => status.upstream = Some(value.to_owned()),
                "branch.ab" => {
                    for part in value.split(' ') {
                        if let Some(n) = part.strip_prefix('+') {
                            status.ahead = n.parse().unwrap_or(0);
                        } else if let Some(n) = part.strip_prefix('-') {
                            status.behind = n.parse().unwrap_or(0);
                        }
                    }
                }
                _ => {}
            }
            continue;
        }
        let kind = record.chars().next();
        let file = match kind {
            Some('1') => {
                let parts: Vec<&str> = record.splitn(9, ' ').collect();
                let Some(path) = parts.get(8) else { continue };
                let (staged, unstaged) = xy(parts[1]);
                FileStatus {
                    path: (*path).to_owned(),
                    orig_path: None,
                    staged,
                    unstaged,
                }
            }
            Some('2') => {
                let parts: Vec<&str> = record.splitn(10, ' ').collect();
                let Some(path) = parts.get(9) else { continue };
                let (staged, unstaged) = xy(parts[1]);
                FileStatus {
                    path: (*path).to_owned(),
                    orig_path: fields.next().map(str::to_owned),
                    staged,
                    unstaged,
                }
            }
            Some('u') => {
                let parts: Vec<&str> = record.splitn(11, ' ').collect();
                let Some(path) = parts.get(10) else { continue };
                FileStatus {
                    path: (*path).to_owned(),
                    orig_path: None,
                    staged: None,
                    unstaged: Some(Change::Conflict),
                }
            }
            Some('?') => FileStatus {
                path: record[2..].to_owned(),
                orig_path: None,
                staged: None,
                unstaged: Some(Change::Untracked),
            },
            _ => continue,
        };
        status.files.push(file);
    }
    status.files.sort_by(|a, b| a.path.cmp(&b.path));
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_parsing() {
        let text = "# branch.oid 1234567890abcdef\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +2 -1\0\
1 M. N... 100644 100644 100644 aaa bbb src/main.rs\0\
1 .M N... 100644 100644 100644 aaa bbb src/with space.rs\0\
2 R. N... 100644 100644 100644 aaa bbb R100 src/new.rs\0src/old.rs\0\
u UU N... 100644 100644 100644 100644 a b c conflict.rs\0\
? notes.txt\0";
        let status = parse_status(text);
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!(status.upstream.as_deref(), Some("origin/main"));
        assert_eq!((status.ahead, status.behind), (2, 1));
        assert_eq!(status.files.len(), 5);
        let get = |p: &str| status.files.iter().find(|f| f.path == p).unwrap();
        assert_eq!(get("src/main.rs").staged, Some(Change::Modified));
        assert_eq!(get("src/with space.rs").unstaged, Some(Change::Modified));
        assert_eq!(get("src/new.rs").orig_path.as_deref(), Some("src/old.rs"));
        assert_eq!(get("src/new.rs").staged.map(Change::letter), Some('R'));
        assert_eq!(status.change_of("conflict.rs"), Some(Change::Conflict));
        assert_eq!(status.change_of("notes.txt").map(Change::letter), Some('U'));
        assert_eq!(status.branch_label(), "main+");
        assert!(status.has_conflicts());

        let empty = parse_status("# branch.oid (initial)\0# branch.head main\0");
        assert_eq!(empty.head, None);
        assert_eq!(empty.branch_label(), "main");
    }

    #[test]
    fn hunks() {
        let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1,3 +1,3 @@ fn f()\n a\n-b\n+B\n c\n@@ -10 +10,2 @@\n x\n+y\n\\ No newline at end of file\n";
        let hunks = parse_hunks(diff);
        assert_eq!(hunks.len(), 2);
        assert_eq!((hunks[0].old_start, hunks[0].old_lines), (1, 3));
        assert_eq!(hunks[0].lines[1].kind, LineKind::Removed);
        assert_eq!(hunks[0].lines[2].text, "B");
        assert_eq!((hunks[1].old_start, hunks[1].old_lines), (10, 1));
        assert_eq!((hunks[1].new_start, hunks[1].new_lines), (10, 2));
        assert_eq!(hunks[1].lines.len(), 2);
    }

    #[test]
    fn graph_lanes() {
        let c = |id: &str, parents: &[&str]| Commit {
            id: id.into(),
            parents: parents.iter().map(|p| (*p).to_owned()).collect(),
            author: String::new(),
            email: String::new(),
            time: 0,
            subject: String::new(),
            refs: Vec::new(),
            head: false,
            outgoing: false,
            incoming: false,
        };
        // m = Merge von b (Spur 0) und f (Seitenzweig); a ist die Wurzel.
        let commits = [
            c("m", &["b", "f"]),
            c("f", &["a"]),
            c("b", &["a"]),
            c("a", &[]),
        ];
        let rows = graph(&commits);
        assert_eq!(
            rows.iter().map(|r| r.lane).collect::<Vec<_>>(),
            [0, 1, 0, 0]
        );
        // Der Merge öffnet die zweite Spur.
        assert!(rows[0].lines.contains(&GraphLine {
            from: (0, RowPos::Middle),
            to: (1, RowPos::Bottom),
            lane: 1
        }));
        // f läuft in a zusammen: Spur 1 endet bei a (Zeile 3) in Spur 0.
        assert!(rows[3].lines.contains(&GraphLine {
            from: (1, RowPos::Top),
            to: (0, RowPos::Middle),
            lane: 1
        }));
        // b: Spur 1 läuft daneben durch.
        assert!(rows[2].lines.contains(&GraphLine {
            from: (1, RowPos::Top),
            to: (1, RowPos::Bottom),
            lane: 1
        }));
    }

    #[test]
    fn errors_are_sorted() {
        let code = |s: &str| GitError::from_output(s.to_owned()).code;
        assert_eq!(
            code(
                "git@github.com: Permission denied (publickey).\nfatal: Could not read from remote repository."
            ),
            "auth_required"
        );
        assert_eq!(
            code(" ! [rejected]        main -> main (fetch first)"),
            "rejected"
        );
        assert_eq!(
            code("CONFLICT (content): Merge conflict in a.rs"),
            "merge_conflict"
        );
        assert_eq!(
            code("fatal: unable to access 'https://x/': Could not resolve host: x"),
            "network"
        );
        assert_eq!(code("fatal: something else"), "failed");
        let op: GitOp =
            serde_json::from_str(r#"{"op":"commit","message":"m","then":"push"}"#).unwrap();
        assert_eq!(
            op,
            GitOp::Commit {
                message: "m".into(),
                amend: false,
                then: After::Push
            }
        );
    }
}
