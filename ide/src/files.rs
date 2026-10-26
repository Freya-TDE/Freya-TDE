//! The project folder as data (no UI): the file tree and file operations
//! (new file/folder, rename, copy, import, delete).
//!
//! Every operation is a serializable [`FileOp`]. [`plan`] is a dry run that
//! lists the changes and findings with stable codes; [`apply`] performs it
//! and refuses when the plan has errors. Paths inside the project are
//! relative and use `/` on every system (e.g. `src/main.rs`).
//!
//! ```
//! use freya_td::files::{FileOp, plan};
//!
//! let dir = std::env::temp_dir().join("freya-files-doc");
//! std::fs::create_dir_all(&dir).unwrap();
//! let op = FileOp::NewFolder { path: "../outside".into() };
//! let plan = plan(&dir, &op, &[]);
//! assert_eq!(plan.issues[0].code, "invalid_path");
//! ```

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use tdr_core::Severity;

/// Folders that never show up (build output, Git data).
pub const SKIPPED: &[&str] = &["target", ".git"];

/// What [`scan`] shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanOptions {
    /// Show generated files (`*.gen.rs`).
    #[serde(default)]
    pub show_generated: bool,
    /// Show names starting with `.` (except [`SKIPPED`]).
    #[serde(default)]
    pub show_hidden: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    Folder,
    File,
}

/// A file or folder of the project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// Relative path with `/`.
    pub path: String,
    pub name: String,
    pub kind: EntryKind,
    /// Written by Freya-TD (`*.gen.rs`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub generated: bool,
    /// Folders first, then files, each sorted by name (ignoring case).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Entry>,
}

/// Is this a generated file (`*.gen.rs`)?
pub fn is_generated(name: &str) -> bool {
    name.ends_with(".gen.rs")
}

/// The files and folders below `root` (deterministic order; unreadable
/// folders stay empty).
pub fn scan(root: &Path, options: &ScanOptions) -> Vec<Entry> {
    fn walk(dir: &Path, prefix: &str, options: &ScanOptions) -> Vec<Entry> {
        let Ok(read) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut entries: Vec<Entry> = read
            .filter_map(Result::ok)
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                if SKIPPED.contains(&name.as_str())
                    || (!options.show_hidden && name.starts_with('.'))
                    || (!options.show_generated && is_generated(&name))
                {
                    return None;
                }
                let path = if prefix.is_empty() {
                    name.clone()
                } else {
                    format!("{prefix}/{name}")
                };
                let folder = e.file_type().ok()?.is_dir()
                    || (e.file_type().ok()?.is_symlink() && e.path().is_dir());
                Some(if folder {
                    Entry {
                        children: walk(&e.path(), &path, options),
                        path,
                        name,
                        kind: EntryKind::Folder,
                        generated: false,
                    }
                } else {
                    Entry {
                        generated: is_generated(&name),
                        path,
                        name,
                        kind: EntryKind::File,
                        children: Vec::new(),
                    }
                })
            })
            .collect();
        entries.sort_by_cached_key(|e| {
            (
                e.kind == EntryKind::File,
                e.name.to_lowercase(),
                e.name.clone(),
            )
        });
        entries
    }
    walk(root, "", options)
}

/// A file operation; serialized as `{"op": "rename", "from": …, "to": …}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum FileOp {
    /// A new file (with optional content); missing folders are created.
    NewFile {
        path: String,
        #[serde(default)]
        text: String,
    },
    NewFolder {
        path: String,
    },
    /// Rename or move inside the project.
    Rename {
        from: String,
        to: String,
    },
    /// Copy a file or folder inside the project.
    Copy {
        from: String,
        to: String,
    },
    /// Copy a file or folder from outside into the project.
    Import {
        source: PathBuf,
        to: String,
    },
    /// Delete a file or folder – into the trash, or for good.
    Delete {
        path: String,
        #[serde(default)]
        permanent: bool,
    },
}

/// What happens to a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Create,
    /// Moved here from `from`.
    Move,
    Trash,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Change {
    pub kind: ChangeKind,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// Files affected (a folder counts its files).
    pub files: usize,
}

/// A finding of [`plan`]. Codes are stable: `invalid_path`, `invalid_name`,
/// `not_found`, `already_exists`, `protected`, `into_itself`,
/// `generated` (warning), `form_source` (warning); `Ide::file_op` adds
/// `no_project` (project not saved yet).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileIssue {
    pub severity: Severity,
    pub code: &'static str,
    pub path: String,
    pub detail: String,
}

/// The dry run of a [`FileOp`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Plan {
    pub changes: Vec<Change>,
    pub issues: Vec<FileIssue>,
}

impl Plan {
    pub fn has_errors(&self) -> bool {
        self.issues.iter().any(|i| i.severity == Severity::Error)
    }

    pub fn errors(&self) -> impl Iterator<Item = &FileIssue> {
        self.issues.iter().filter(|i| i.severity == Severity::Error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum FileError {
    /// The plan has errors; nothing was changed.
    Refused { issues: Vec<FileIssue> },
    /// The system refused (e.g. permissions, no trash).
    Io { path: String, detail: String },
}

impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused { issues } => {
                let details: Vec<&str> = issues
                    .iter()
                    .filter(|i| i.severity == Severity::Error)
                    .map(|i| i.detail.as_str())
                    .collect();
                f.write_str(&details.join(" "))
            }
            Self::Io { path, detail } => write!(f, "{path}: {detail}"),
        }
    }
}

impl std::error::Error for FileError {}

/// Characters not allowed in names (they fail on Windows).
const FORBIDDEN: &[char] = &['<', '>', ':', '"', '|', '?', '*', '\\'];

/// Checks a relative project path; returns the absolute path.
fn resolve(root: &Path, path: &str) -> Result<PathBuf, FileIssue> {
    let issue = |code, detail: String| FileIssue {
        severity: Severity::Error,
        code,
        path: path.to_owned(),
        detail,
    };
    let relative = Path::new(path);
    if path.is_empty()
        || relative.is_absolute()
        || !relative
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
    {
        return Err(issue(
            "invalid_path",
            format!("„{path}“ ist kein Pfad innerhalb des Projekts."),
        ));
    }
    for name in path.split('/') {
        if name.is_empty()
            || name.trim() != name
            || name.ends_with('.')
            || name
                .chars()
                .any(|c| c.is_control() || FORBIDDEN.contains(&c))
        {
            return Err(issue(
                "invalid_name",
                format!("„{name}“ ist als Datei- oder Ordnername nicht erlaubt."),
            ));
        }
    }
    Ok(root.join(relative))
}

/// Number of files in a file or folder.
fn count_files(path: &Path) -> usize {
    if path.is_dir() {
        std::fs::read_dir(path)
            .map(|r| {
                r.filter_map(Result::ok)
                    .map(|e| count_files(&e.path()))
                    .sum()
            })
            .unwrap_or(0)
    } else {
        1
    }
}

/// Is `inner` the same as or below `outer` (relative paths with `/`)?
fn is_within(inner: &str, outer: &str) -> bool {
    inner == outer || inner.starts_with(&format!("{outer}/"))
}

/// A free name for a copy of `name` in folder `dir` (relative, `""` = root):
/// `main_copy.rs`, `main_copy2.rs` …
pub fn copy_name(root: &Path, dir: &str, name: &str) -> String {
    let (stem, ext) = match name.split_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (name, String::new()),
    };
    let join = |n: &str| {
        if dir.is_empty() {
            n.to_owned()
        } else {
            format!("{dir}/{n}")
        }
    };
    if !root.join(join(name)).exists() {
        return join(name);
    }
    (1..)
        .map(|n| {
            let suffix = if n == 1 { String::new() } else { n.to_string() };
            join(&format!("{stem}_copy{suffix}{ext}"))
        })
        .find(|p| !root.join(p).exists())
        .expect("free name")
}

/// The dry run: what `op` would change, and what speaks against it.
/// `protected` lists relative paths that must not be renamed or deleted
/// (the project file, the form files).
pub fn plan(root: &Path, op: &FileOp, protected: &[String]) -> Plan {
    let mut changes = Vec::new();
    let mut issues = Vec::new();
    let error = |code, path: &str, detail: String| FileIssue {
        severity: Severity::Error,
        code,
        path: path.to_owned(),
        detail,
    };
    let warn_generated = |path: &str, issues: &mut Vec<FileIssue>| {
        if is_generated(path) {
            issues.push(FileIssue {
                severity: Severity::Warning,
                code: "generated",
                path: path.to_owned(),
                detail: format!("{path} erzeugt Freya-TD beim nächsten „Code erzeugen“ neu."),
            });
        }
    };
    let check_protected = |path: &str, issues: &mut Vec<FileIssue>| {
        if let Some(p) = protected.iter().find(|p| is_within(p, path)) {
            issues.push(error(
                "protected",
                path,
                format!("{p} gehört zum Projekt; Formulare entfernt oder benennt das Projekt um."),
            ));
        }
    };
    let target_free = |path: &str, at: &Path, issues: &mut Vec<FileIssue>| {
        if at.exists() {
            issues.push(error(
                "already_exists",
                path,
                format!("{path} gibt es schon."),
            ));
        }
    };
    let source = |path: &str, issues: &mut Vec<FileIssue>| -> Option<PathBuf> {
        match resolve(root, path) {
            Ok(at) if at.exists() => Some(at),
            Ok(_) => {
                issues.push(error("not_found", path, format!("{path} gibt es nicht.")));
                None
            }
            Err(issue) => {
                issues.push(issue);
                None
            }
        }
    };
    let target = |path: &str, issues: &mut Vec<FileIssue>| -> Option<PathBuf> {
        match resolve(root, path) {
            Ok(at) => {
                target_free(path, &at, issues);
                Some(at)
            }
            Err(issue) => {
                issues.push(issue);
                None
            }
        }
    };

    match op {
        FileOp::NewFile { path, .. } | FileOp::NewFolder { path } => {
            if target(path, &mut issues).is_some() {
                let files = usize::from(matches!(op, FileOp::NewFile { .. }));
                changes.push(Change {
                    kind: ChangeKind::Create,
                    path: path.clone(),
                    from: None,
                    files,
                });
                warn_generated(path, &mut issues);
            }
        }
        FileOp::Rename { from, to } | FileOp::Copy { from, to } => {
            let rename = matches!(op, FileOp::Rename { .. });
            let at = source(from, &mut issues);
            target(to, &mut issues);
            if rename {
                check_protected(from, &mut issues);
            }
            if is_within(to, from) && to != from {
                issues.push(error(
                    "into_itself",
                    to,
                    format!("{from} kann nicht in sich selbst landen."),
                ));
            }
            if let Some(at) = at {
                changes.push(Change {
                    kind: if rename {
                        ChangeKind::Move
                    } else {
                        ChangeKind::Create
                    },
                    path: to.clone(),
                    from: Some(from.clone()),
                    files: count_files(&at),
                });
                if rename {
                    warn_generated(from, &mut issues);
                }
            }
        }
        FileOp::Import {
            source: outside,
            to,
        } => {
            target(to, &mut issues);
            if !outside.exists() {
                issues.push(error(
                    "not_found",
                    &outside.display().to_string(),
                    format!("{} gibt es nicht.", outside.display()),
                ));
            } else {
                changes.push(Change {
                    kind: ChangeKind::Create,
                    path: to.clone(),
                    from: Some(outside.display().to_string()),
                    files: count_files(outside),
                });
            }
        }
        FileOp::Delete { path, permanent } => {
            if let Some(at) = source(path, &mut issues) {
                check_protected(path, &mut issues);
                changes.push(Change {
                    kind: if *permanent {
                        ChangeKind::Delete
                    } else {
                        ChangeKind::Trash
                    },
                    path: path.clone(),
                    from: None,
                    files: count_files(&at),
                });
            }
        }
    }
    // Eine `.rs` neben einer `.ftdr` gehört zum Formular.
    if let FileOp::Rename { from: path, .. } | FileOp::Delete { path, .. } = op
        && let Some(stem) = path.strip_suffix(".rs")
        && !is_generated(path)
        && protected.iter().any(|p| p == &format!("{stem}.ftdr"))
    {
        issues.push(FileIssue {
            severity: Severity::Warning,
            code: "form_source",
            path: path.clone(),
            detail: format!("{path} ist der Code des Formulars {stem}.ftdr."),
        });
    }
    Plan { changes, issues }
}

fn copy_all(from: &Path, to: &Path) -> std::io::Result<()> {
    if from.is_dir() {
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            copy_all(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(from, to).map(|_| ())
    }
}

fn create_parent(path: &Path) -> std::io::Result<()> {
    match path.parent() {
        Some(dir) => std::fs::create_dir_all(dir),
        None => Ok(()),
    }
}

/// Performs `op` (see [`plan`]); refuses when the plan has errors.
pub fn apply(root: &Path, op: &FileOp, protected: &[String]) -> Result<Plan, FileError> {
    let plan = plan(root, op, protected);
    if plan.has_errors() {
        return Err(FileError::Refused {
            issues: plan.errors().cloned().collect(),
        });
    }
    let at = |p: &str| root.join(p);
    let io = |path: &str| {
        let path = path.to_owned();
        move |e: std::io::Error| FileError::Io {
            path,
            detail: e.to_string(),
        }
    };
    match op {
        FileOp::NewFile { path, text } => {
            let file = at(path);
            create_parent(&file)
                .and_then(|()| std::fs::write(&file, text))
                .map_err(io(path))?;
        }
        FileOp::NewFolder { path } => std::fs::create_dir_all(at(path)).map_err(io(path))?,
        FileOp::Rename { from, to } => {
            let target = at(to);
            create_parent(&target)
                .and_then(|()| std::fs::rename(at(from), &target))
                .map_err(io(to))?;
        }
        FileOp::Copy { from, to } => {
            let target = at(to);
            create_parent(&target)
                .and_then(|()| copy_all(&at(from), &target))
                .map_err(io(to))?;
        }
        FileOp::Import { source, to } => {
            let target = at(to);
            create_parent(&target)
                .and_then(|()| copy_all(source, &target))
                .map_err(io(to))?;
        }
        FileOp::Delete { path, permanent } => {
            let target = at(path);
            if *permanent {
                if target.is_dir() {
                    std::fs::remove_dir_all(&target)
                } else {
                    std::fs::remove_file(&target)
                }
                .map_err(io(path))?;
            } else {
                trash::delete(&target).map_err(|e| FileError::Io {
                    path: path.clone(),
                    detail: e.to_string(),
                })?;
            }
        }
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("freya-files-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (path, text) in [
            ("Cargo.toml", "[package]"),
            ("app.ptdr", "()"),
            ("src/main.rs", "fn main() {}"),
            ("src/app.gen.rs", ""),
            ("src/main_form.rs", ""),
            ("src/main_form.ftdr", ""),
            (".gitignore", "/target"),
            ("target/debug/x", ""),
        ] {
            let file = dir.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
        }
        dir
    }

    fn protected() -> Vec<String> {
        vec!["app.ptdr".into(), "src/main_form.ftdr".into()]
    }

    fn paths(entries: &[Entry]) -> Vec<String> {
        entries
            .iter()
            .flat_map(|e| std::iter::once(e.path.clone()).chain(paths(&e.children)))
            .collect()
    }

    #[test]
    fn scan_orders_and_filters() {
        let dir = project("scan");
        let tree = scan(&dir, &ScanOptions::default());
        assert_eq!(
            paths(&tree),
            [
                "src",
                "src/main.rs",
                "src/main_form.ftdr",
                "src/main_form.rs",
                "app.ptdr",
                "Cargo.toml"
            ]
        );
        let all = scan(
            &dir,
            &ScanOptions {
                show_generated: true,
                show_hidden: true,
            },
        );
        let all = paths(&all);
        assert!(all.contains(&"src/app.gen.rs".to_owned()));
        assert!(all.contains(&".gitignore".to_owned()));
        assert!(!all.iter().any(|p| p.starts_with("target")));
        let json = serde_json::to_string(&tree[0]).unwrap();
        assert!(json.contains(r#""kind":"folder""#), "{json}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn plans_find_problems() {
        let dir = project("plan");
        let codes = |op: FileOp| -> Vec<&'static str> {
            plan(&dir, &op, &protected())
                .issues
                .iter()
                .map(|i| i.code)
                .collect()
        };
        assert_eq!(
            codes(FileOp::NewFile {
                path: "src/main.rs".into(),
                text: String::new()
            }),
            ["already_exists"]
        );
        assert_eq!(
            codes(FileOp::NewFolder {
                path: "/etc/x".into()
            }),
            ["invalid_path"]
        );
        assert_eq!(
            codes(FileOp::NewFile {
                path: "src/a?.rs".into(),
                text: String::new()
            }),
            ["invalid_name"]
        );
        assert_eq!(
            codes(FileOp::Delete {
                path: "app.ptdr".into(),
                permanent: true
            }),
            ["protected"]
        );
        assert_eq!(
            codes(FileOp::Delete {
                path: "src".into(),
                permanent: true
            }),
            ["protected"]
        );
        assert_eq!(
            codes(FileOp::Rename {
                from: "src/main_form.rs".into(),
                to: "src/other.rs".into()
            }),
            ["form_source"]
        );
        assert_eq!(
            codes(FileOp::Copy {
                from: "src".into(),
                to: "src/inner".into()
            }),
            ["into_itself"]
        );
        assert_eq!(
            codes(FileOp::Delete {
                path: "nope".into(),
                permanent: false
            }),
            ["not_found"]
        );
        let p = plan(
            &dir,
            &FileOp::Copy {
                from: "src".into(),
                to: "src2".into(),
            },
            &protected(),
        );
        assert_eq!(p.changes[0].files, 4);
        let json = serde_json::to_value(&p).unwrap();
        assert_eq!(json["changes"][0]["kind"], "create");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn apply_changes_files() {
        let dir = project("apply");
        let run = |op: FileOp| apply(&dir, &op, &protected());
        run(FileOp::NewFile {
            path: "src/ui/view.rs".into(),
            text: "// view".into(),
        })
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("src/ui/view.rs")).unwrap(),
            "// view"
        );
        run(FileOp::Rename {
            from: "src/ui".into(),
            to: "src/views".into(),
        })
        .unwrap();
        assert!(dir.join("src/views/view.rs").is_file());
        assert_eq!(copy_name(&dir, "src", "main.rs"), "src/main_copy.rs");
        run(FileOp::Copy {
            from: "src/main.rs".into(),
            to: "src/main_copy.rs".into(),
        })
        .unwrap();
        assert_eq!(copy_name(&dir, "src", "main.rs"), "src/main_copy2.rs");
        assert_eq!(copy_name(&dir, "", "README"), "README");
        run(FileOp::Delete {
            path: "src/views".into(),
            permanent: true,
        })
        .unwrap();
        assert!(!dir.join("src/views").exists());
        let outside = dir.join("target/debug/x");
        run(FileOp::Import {
            source: outside,
            to: "assets/x".into(),
        })
        .unwrap();
        assert!(dir.join("assets/x").is_file());
        let refused = run(FileOp::Delete {
            path: "app.ptdr".into(),
            permanent: true,
        });
        assert!(matches!(refused, Err(FileError::Refused { .. })));
        assert!(dir.join("app.ptdr").exists());
        let op: FileOp = serde_json::from_str(r#"{"op":"rename","from":"a","to":"b"}"#).unwrap();
        assert_eq!(
            op,
            FileOp::Rename {
                from: "a".into(),
                to: "b".into()
            }
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
