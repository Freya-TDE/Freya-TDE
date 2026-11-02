//! The project manager without UI: the user's global project list (every
//! project with its `.ptdr` and the main → subproject assignment), finding
//! subprojects in a folder, and „Projekt aus Cargo.toml erstellen“ with a
//! preview.
//!
//! The list lives in `<config>/freya-tde/projects.toml` (see
//! [`crate::desktop::config_dir`]); each project keeps its own `.ptdr`.
//! Paths are absolute. Errors and plans are serializable with stable codes.
//!
//! ```
//! use freya_tde::projects::ProjectList;
//!
//! let list = ProjectList::default();
//! assert!(list.tree(&[]).is_empty());
//! ```

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tdr_core::{ProjectFile, is_package_name};

/// Rust version for projects whose `Cargo.toml` names none.
pub const DEFAULT_RUST_VERSION: &str = "1.99";

/// Folders never searched for projects.
const SKIPPED: &[&str] = &["target", ".git", "node_modules"];

/// How deep subprojects are searched below a project folder.
const SEARCH_DEPTH: usize = 6;

/// A project in the global list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectEntry {
    /// The project's `.ptdr` (absolute).
    pub path: PathBuf,
    pub name: String,
    /// The main project's `.ptdr`; `None` for a main project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<PathBuf>,
    /// Hidden by the user (automatic search does not bring it back).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
}

/// A node of the project tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectNode {
    pub path: PathBuf,
    pub name: String,
    /// The `.ptdr` still exists.
    pub exists: bool,
    /// Opened in the IDE (shown with `*`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub open: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<ProjectNode>,
}

/// A failure. Codes are stable: `not_a_project`, `unknown_project`,
/// `cycle`, `not_cargo`, `invalid_cargo`, `io`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectError {
    pub code: &'static str,
    pub path: PathBuf,
    pub detail: String,
}

impl ProjectError {
    fn new(code: &'static str, path: &Path, detail: impl Into<String>) -> Self {
        Self {
            code,
            path: path.to_path_buf(),
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for ProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.detail)
    }
}

impl std::error::Error for ProjectError {}

fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Reads the project file at `ptdr`.
pub fn read_project(ptdr: &Path) -> Result<ProjectFile, ProjectError> {
    let text = std::fs::read_to_string(ptdr).map_err(|e| {
        ProjectError::new("not_a_project", ptdr, format!("{}: {e}", ptdr.display()))
    })?;
    ProjectFile::from_ron(&text)
        .map_err(|e| ProjectError::new("not_a_project", ptdr, format!("{}: {e}", ptdr.display())))
}

/// All `.ptdr` files below `dir` (not `dir` itself's), sorted; `target/`,
/// `.git/` and hidden folders are skipped.
pub fn find_projects(dir: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, depth: usize, top: bool, out: &mut Vec<PathBuf>) {
        let Ok(read) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = read.filter_map(Result::ok).collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                if depth > 0 && !name.starts_with('.') && !SKIPPED.contains(&name.as_str()) {
                    walk(&path, depth - 1, false, out);
                }
            } else if !top
                && path
                    .extension()
                    .is_some_and(|e| e == ProjectFile::EXTENSION)
            {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, SEARCH_DEPTH, true, &mut out);
    out
}

/// The user's list of all projects.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectList {
    #[serde(default)]
    projects: Vec<ProjectEntry>,
}

impl ProjectList {
    /// `<config>/freya-tde/projects.toml`.
    pub fn default_path() -> Option<PathBuf> {
        crate::desktop::config_dir().map(|d| d.join("projects.toml"))
    }

    /// The list stored at `path` (empty if there is none or it is broken).
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), ProjectError> {
        let io = |e: std::io::Error| ProjectError::new("io", path, e.to_string());
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(io)?;
        }
        let text = toml::to_string_pretty(self)
            .map_err(|e| ProjectError::new("io", path, e.to_string()))?;
        std::fs::write(path, text).map_err(io)
    }

    pub fn entries(&self) -> &[ProjectEntry] {
        &self.projects
    }

    pub fn get(&self, ptdr: &Path) -> Option<&ProjectEntry> {
        let ptdr = absolute(ptdr);
        self.projects.iter().find(|p| p.path == ptdr)
    }

    fn index(&self, ptdr: &Path) -> Result<usize, ProjectError> {
        let ptdr = absolute(ptdr);
        self.projects
            .iter()
            .position(|p| p.path == ptdr)
            .ok_or_else(|| {
                ProjectError::new(
                    "unknown_project",
                    &ptdr,
                    format!("{} steht nicht in der Projektliste.", ptdr.display()),
                )
            })
    }

    /// Adds a project (or updates its name and shows it again); `parent`
    /// makes it a subproject.
    pub fn add(&mut self, ptdr: &Path, parent: Option<&Path>) -> Result<(), ProjectError> {
        let ptdr = absolute(ptdr);
        let project = read_project(&ptdr)?;
        match self.projects.iter_mut().find(|p| p.path == ptdr) {
            Some(entry) => {
                entry.name = project.name;
                entry.hidden = false;
            }
            None => self.projects.push(ProjectEntry {
                path: ptdr.clone(),
                name: project.name,
                parent: None,
                hidden: false,
            }),
        }
        if parent.is_some() {
            self.set_parent(&ptdr, parent)?;
        }
        self.sort();
        Ok(())
    }

    /// Takes a project off the list; its subprojects become main projects.
    pub fn remove(&mut self, ptdr: &Path) -> Result<(), ProjectError> {
        let index = self.index(ptdr)?;
        let removed = self.projects.remove(index);
        for entry in &mut self.projects {
            if entry.parent.as_deref() == Some(removed.path.as_path()) {
                entry.parent = None;
            }
        }
        Ok(())
    }

    /// Makes `ptdr` a subproject of `parent` (`None` = main project).
    pub fn set_parent(&mut self, ptdr: &Path, parent: Option<&Path>) -> Result<(), ProjectError> {
        let index = self.index(ptdr)?;
        let parent = match parent {
            Some(parent) => {
                let parent = self.projects[self.index(parent)?].path.clone();
                // Kein Zyklus: `parent` darf nicht unter `ptdr` hängen.
                let mut current = Some(parent.clone());
                while let Some(p) = current {
                    if p == self.projects[index].path {
                        return Err(ProjectError::new(
                            "cycle",
                            &p,
                            format!(
                                "{} kann nicht Unterprojekt seines eigenen Unterprojekts sein.",
                                self.projects[index].name
                            ),
                        ));
                    }
                    current = self.get(&p).and_then(|e| e.parent.clone());
                }
                Some(parent)
            }
            None => None,
        };
        self.projects[index].parent = parent;
        Ok(())
    }

    /// Hides a project (automatic search will not bring it back) or shows it.
    pub fn set_hidden(&mut self, ptdr: &Path, hidden: bool) -> Result<(), ProjectError> {
        let index = self.index(ptdr)?;
        self.projects[index].hidden = hidden;
        Ok(())
    }

    /// Adds the projects found below the folder of `main` as its
    /// subprojects (nested ones under the nearest found project). Known and
    /// hidden ones stay as they are. Returns the new ones.
    pub fn discover(&mut self, main: &Path) -> Vec<PathBuf> {
        let main = absolute(main);
        if self.get(&main).is_none() && self.add(&main, None).is_err() {
            return Vec::new();
        }
        let Some(dir) = main.parent() else {
            return Vec::new();
        };
        let found = find_projects(dir);
        // Erst alle neuen eintragen, dann zuordnen (die Reihenfolge der
        // Suche sagt nichts über die Verschachtelung).
        let mut added = Vec::new();
        for ptdr in &found {
            if self.get(ptdr).is_none() && self.add(ptdr, None).is_ok() {
                added.push(ptdr.clone());
            }
        }
        for ptdr in &added {
            // Das nächste übergeordnete Projekt im Ordnerbaum, sonst `main`.
            let parent = found
                .iter()
                .filter(|p| *p != ptdr && self.get(p).is_some())
                .filter(|p| {
                    p.parent()
                        .is_some_and(|d| ptdr.starts_with(d) && ptdr.parent() != Some(d))
                })
                .max_by_key(|p| p.components().count())
                .cloned()
                .unwrap_or_else(|| main.clone());
            let _ = self.set_parent(ptdr, Some(&parent));
        }
        self.sort();
        added
    }

    fn sort(&mut self) {
        self.projects.sort_by(|a, b| {
            (a.name.to_lowercase(), &a.path).cmp(&(b.name.to_lowercase(), &b.path))
        });
    }

    /// The projects as a tree: main projects with their subprojects, sorted
    /// by name, hidden ones left out; `open` marks opened projects.
    pub fn tree(&self, open: &[PathBuf]) -> Vec<ProjectNode> {
        let open: Vec<PathBuf> = open.iter().map(|p| absolute(p)).collect();
        let visible: Vec<&ProjectEntry> = self.projects.iter().filter(|p| !p.hidden).collect();
        fn children(
            parent: Option<&Path>,
            visible: &[&ProjectEntry],
            open: &[PathBuf],
        ) -> Vec<ProjectNode> {
            visible
                .iter()
                .filter(|p| match parent {
                    Some(parent) => p.parent.as_deref() == Some(parent),
                    // Ohne (sichtbares) Hauptprojekt oben einordnen.
                    None => p
                        .parent
                        .as_ref()
                        .is_none_or(|q| !visible.iter().any(|v| &v.path == q)),
                })
                .map(|p| ProjectNode {
                    path: p.path.clone(),
                    name: p.name.clone(),
                    exists: p.path.is_file(),
                    open: open.contains(&p.path),
                    children: children(Some(&p.path), visible, open),
                })
                .collect()
        }
        children(None, &visible, &open)
    }

    /// The main project of a project (itself for a main project).
    pub fn main_of(&self, ptdr: &Path) -> PathBuf {
        let mut current = absolute(ptdr);
        while let Some(parent) = self.get(&current).and_then(|e| e.parent.clone()) {
            current = parent;
        }
        current
    }
}

// --- Projekt aus Cargo.toml -------------------------------------------------------------

/// A `.ptdr` „Projekt aus Cargo.toml erstellen“ would create.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CargoProject {
    /// The folder with the `Cargo.toml`.
    pub dir: PathBuf,
    pub name: String,
    pub rust_version: String,
    /// The `.ptdr` to create (or the one that is already there).
    pub ptdr: PathBuf,
    /// There is a `.ptdr` already; it is only added to the list.
    pub exists: bool,
    /// A workspace member: subproject of the workspace's project.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member_of: Option<PathBuf>,
}

/// A valid Cargo package name from any name (`Freya TDE` → `Freya TDE`,
/// `my app` → `my_app`, `2d` → `p_2d`).
fn package_name(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if !out.starts_with(|c: char| c.is_ascii_alphabetic()) {
        out.insert_str(0, "p_");
    }
    if is_package_name(&out) {
        out
    } else {
        "project".to_owned()
    }
}

/// The member folders of a workspace (`*` only as a whole last component
/// or name part, like `crates/*`).
fn members(dir: &Path, patterns: &[String], exclude: &[String]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for pattern in patterns {
        let (base, last) = pattern.rsplit_once('/').unwrap_or(("", pattern));
        if let Some((prefix, suffix)) = last.split_once('*') {
            let Ok(read) = std::fs::read_dir(dir.join(base)) else {
                continue;
            };
            let mut found: Vec<PathBuf> = read
                .filter_map(Result::ok)
                .filter(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    name.starts_with(prefix) && name.ends_with(suffix)
                })
                .map(|e| e.path())
                .collect();
            found.sort();
            out.extend(found);
        } else {
            out.push(dir.join(pattern));
        }
    }
    out.retain(|m| {
        m.join("Cargo.toml").is_file() && !exclude.iter().any(|e| dir.join(e) == *m) && m != dir
    });
    out.dedup();
    out
}

/// What „Projekt aus Cargo.toml erstellen“ would do for `dir`: the project
/// of the folder, with `with_members` also one per workspace member
/// (preview – nothing is written).
pub fn plan_from_cargo(dir: &Path, with_members: bool) -> Result<Vec<CargoProject>, ProjectError> {
    let dir = absolute(dir);
    let manifest = dir.join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest).map_err(|_| {
        ProjectError::new(
            "not_cargo",
            &dir,
            format!("{} hat keine Cargo.toml.", dir.display()),
        )
    })?;
    let toml: toml::Table = toml::from_str(&text).map_err(|e| {
        ProjectError::new(
            "invalid_cargo",
            &manifest,
            format!("{}: {e}", manifest.display()),
        )
    })?;
    let workspace = toml.get("workspace").and_then(|w| w.as_table());
    let workspace_version = workspace
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("rust-version"))
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    let project = |dir: &Path, toml: &toml::Table, member_of: Option<PathBuf>| {
        let package = toml.get("package").and_then(|p| p.as_table());
        let folder = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "project".to_owned());
        let name = package_name(
            package
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or(&folder),
        );
        let rust_version = package
            .and_then(|p| p.get("rust-version"))
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .or_else(|| workspace_version.clone())
            .unwrap_or_else(|| DEFAULT_RUST_VERSION.to_owned());
        let existing = find_existing(dir);
        CargoProject {
            dir: dir.to_path_buf(),
            exists: existing.is_some(),
            ptdr: existing
                .unwrap_or_else(|| dir.join(format!("{name}.{}", ProjectFile::EXTENSION))),
            name,
            rust_version,
            member_of,
        }
    };
    let root = project(&dir, &toml, None);
    let mut plan = vec![root.clone()];
    if with_members && let Some(workspace) = workspace {
        let list = |key: &str| -> Vec<String> {
            workspace
                .get(key)
                .and_then(|m| m.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        };
        for member in members(&dir, &list("members"), &list("exclude")) {
            let Ok(text) = std::fs::read_to_string(member.join("Cargo.toml")) else {
                continue;
            };
            let Ok(member_toml) = toml::from_str::<toml::Table>(&text) else {
                continue;
            };
            plan.push(project(&member, &member_toml, Some(root.ptdr.clone())));
        }
    }
    Ok(plan)
}

/// A `.ptdr` directly in `dir`, if there is one.
fn find_existing(dir: &Path) -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == ProjectFile::EXTENSION))
        .collect();
    found.sort();
    found.into_iter().next()
}

/// Performs `plan`: writes the missing `.ptdr` files (only managed:
/// `generate = false`, backend `none`) and puts all of them on `list`
/// (members as subprojects). Returns the written files.
pub fn create_from_cargo(
    plan: &[CargoProject],
    list: &mut ProjectList,
) -> Result<Vec<PathBuf>, ProjectError> {
    let mut written = Vec::new();
    for project in plan {
        if !project.exists {
            let mut file = ProjectFile::new(&project.name, &project.rust_version, "none")
                .map_err(|e| ProjectError::new("not_cargo", &project.dir, e.to_string()))?;
            file.generate = false;
            let text = file
                .to_ron()
                .map_err(|e| ProjectError::new("io", &project.ptdr, e.to_string()))?;
            std::fs::write(&project.ptdr, text)
                .map_err(|e| ProjectError::new("io", &project.ptdr, e.to_string()))?;
            written.push(project.ptdr.clone());
        }
        list.add(&project.ptdr, None)?;
    }
    for project in plan {
        if let Some(parent) = &project.member_of {
            list.set_parent(&project.ptdr, Some(parent))?;
        }
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("freya-projects-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn ptdr(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(format!("{name}.ptdr"));
        write(
            &path,
            &ProjectFile::new(name, "1.99", "none")
                .unwrap()
                .to_ron()
                .unwrap(),
        );
        path
    }

    #[test]
    fn list_tree_and_discovery() {
        let dir = temp("list");
        let main = ptdr(&dir, "main_app");
        let sub = ptdr(&dir.join("tools/ruf"), "ruf");
        let nested = ptdr(&dir.join("tools/ruf/demo"), "demo");
        ptdr(&dir.join("target/debug"), "ignored");
        let mut list = ProjectList::default();
        list.add(&main, None).unwrap();
        let added = list.discover(&main);
        assert_eq!(added, [nested.clone(), sub.clone()]);
        // Verschachtelt: demo hängt unter ruf, ruf unter main_app.
        assert_eq!(
            list.get(&nested).unwrap().parent.as_deref(),
            Some(sub.as_path())
        );
        assert_eq!(
            list.get(&sub).unwrap().parent.as_deref(),
            Some(main.as_path())
        );
        assert_eq!(list.main_of(&nested), main);
        let tree = list.tree(std::slice::from_ref(&sub));
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].name, "main_app");
        assert!(tree[0].children[0].open);
        assert_eq!(tree[0].children[0].children[0].name, "demo");
        // Kein Zyklus; ausblenden bleibt bei der Suche bestehen.
        assert_eq!(
            list.set_parent(&main, Some(&nested)).unwrap_err().code,
            "cycle"
        );
        list.set_hidden(&nested, true).unwrap();
        assert!(list.discover(&main).is_empty());
        assert!(list.tree(&[])[0].children[0].children.is_empty());
        // Speichern und laden.
        let file = dir.join("config/projects.toml");
        list.save(&file).unwrap();
        assert_eq!(ProjectList::load(&file), list);
        // Entfernen: Unterprojekte werden Hauptprojekte.
        list.remove(&main).unwrap();
        assert_eq!(list.get(&sub).unwrap().parent, None);
        let json = serde_json::to_string(&list.tree(&[])).unwrap();
        assert!(json.contains("\"ruf\""), "{json}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn projects_from_cargo() {
        let dir = temp("cargo");
        write(
            &dir.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/*\", \"app\"]\nexclude = [\"crates/skip\"]\n\n[workspace.package]\nrust-version = \"1.90\"\n",
        );
        write(
            &dir.join("crates/core/Cargo.toml"),
            "[package]\nname = \"my-core\"\n",
        );
        write(
            &dir.join("crates/skip/Cargo.toml"),
            "[package]\nname = \"skip\"\n",
        );
        write(
            &dir.join("app/Cargo.toml"),
            "[package]\nname = \"app\"\nrust-version = \"1.95\"\n",
        );
        let existing = ptdr(&dir.join("app"), "app_project");
        assert_eq!(
            plan_from_cargo(&dir.join("nowhere"), false)
                .unwrap_err()
                .code,
            "not_cargo"
        );
        let plan = plan_from_cargo(&dir, true).unwrap();
        let names: Vec<(&str, &str, bool)> = plan
            .iter()
            .map(|p| (p.name.as_str(), p.rust_version.as_str(), p.exists))
            .collect();
        let root = dir.file_name().unwrap().to_string_lossy().replace('.', "_");
        assert_eq!(
            names,
            [
                (root.as_str(), "1.90", false),
                ("my-core", "1.90", false),
                ("app", "1.95", true)
            ]
        );
        assert_eq!(plan[2].ptdr, existing);
        // Probelauf schreibt nichts.
        assert!(!plan[0].ptdr.exists());
        let mut list = ProjectList::default();
        let written = create_from_cargo(&plan, &mut list).unwrap();
        assert_eq!(written.len(), 2);
        let core = read_project(&plan[1].ptdr).unwrap();
        assert!(!core.generate);
        assert_eq!(core.backend, "none");
        assert_eq!(list.tree(&[]).len(), 1);
        assert_eq!(list.tree(&[])[0].children.len(), 2);
        assert_eq!(package_name("my app"), "my_app");
        assert_eq!(package_name("2d"), "p_2d");
        let _ = std::fs::remove_dir_all(dir);
    }
}
