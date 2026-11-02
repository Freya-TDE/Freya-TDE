//! Breakpoints and watch expressions of a project – plain data, kept per
//! user in `<config>/freya-td/debug.toml` (personal like the window
//! layout; nothing goes into the shared `.ptdr`).
//!
//! ```
//! use freya_td::debug::{Breakpoint, DebugSettings};
//!
//! let mut settings = DebugSettings::default();
//! assert!(settings.toggle("src/main.rs".as_ref(), 12));
//! settings.set(Breakpoint {
//!     condition: Some("i == 3".into()),
//!     ..Breakpoint::new("src/main.rs", 20)
//! });
//! assert_eq!(settings.in_file("src/main.rs".as_ref()).len(), 2);
//! assert!(!settings.toggle("src/main.rs".as_ref(), 12));
//! ```

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

/// A breakpoint on a source line. With `log_message` it is a logpoint: it
/// prints the message (`{ausdruck}` is replaced) instead of stopping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Breakpoint {
    pub path: PathBuf,
    /// 1-based.
    pub line: usize,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub enabled: bool,
    /// Stops only when this expression is true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    /// Stops only on the n-th hit, e.g. `5` (adapter syntax).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hit_condition: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_message: Option<String>,
}

/// How a breakpoint is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BreakpointKind {
    Normal,
    Conditional,
    Log,
    Disabled,
}

impl Breakpoint {
    pub fn new(path: impl Into<PathBuf>, line: usize) -> Self {
        Self {
            path: path.into(),
            line,
            enabled: true,
            condition: None,
            hit_condition: None,
            log_message: None,
        }
    }

    pub fn kind(&self) -> BreakpointKind {
        if !self.enabled {
            BreakpointKind::Disabled
        } else if self.log_message.is_some() {
            BreakpointKind::Log
        } else if self.condition.is_some() || self.hit_condition.is_some() {
            BreakpointKind::Conditional
        } else {
            BreakpointKind::Normal
        }
    }
}

/// The debugger settings of one project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DebugSettings {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breakpoints: Vec<Breakpoint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub watches: Vec<String>,
    /// Stop when the program panics (breakpoint on `rust_panic`).
    #[serde(default = "yes")]
    pub stop_on_panic: bool,
}

impl Default for DebugSettings {
    fn default() -> Self {
        Self {
            breakpoints: Vec::new(),
            watches: Vec::new(),
            stop_on_panic: true,
        }
    }
}

impl DebugSettings {
    pub fn get(&self, path: &Path, line: usize) -> Option<&Breakpoint> {
        self.breakpoints
            .iter()
            .find(|b| b.path == path && b.line == line)
    }

    /// Adds a breakpoint or removes the one on that line; true = now set.
    pub fn toggle(&mut self, path: &Path, line: usize) -> bool {
        if self.remove(path, line) {
            false
        } else {
            self.set(Breakpoint::new(path, line));
            true
        }
    }

    /// Adds `breakpoint` or replaces the one on its line.
    pub fn set(&mut self, breakpoint: Breakpoint) {
        match self
            .breakpoints
            .iter_mut()
            .find(|b| b.path == breakpoint.path && b.line == breakpoint.line)
        {
            Some(b) => *b = breakpoint,
            None => {
                self.breakpoints.push(breakpoint);
                self.breakpoints
                    .sort_by(|a, b| (&a.path, a.line).cmp(&(&b.path, b.line)));
            }
        }
    }

    /// True if there was one.
    pub fn remove(&mut self, path: &Path, line: usize) -> bool {
        let before = self.breakpoints.len();
        self.breakpoints
            .retain(|b| !(b.path == path && b.line == line));
        self.breakpoints.len() != before
    }

    pub fn set_all_enabled(&mut self, enabled: bool) {
        for b in &mut self.breakpoints {
            b.enabled = enabled;
        }
    }

    /// The breakpoints of a file, by line.
    pub fn in_file(&self, path: &Path) -> Vec<&Breakpoint> {
        self.breakpoints.iter().filter(|b| b.path == path).collect()
    }

    pub fn files(&self) -> BTreeSet<PathBuf> {
        self.breakpoints.iter().map(|b| b.path.clone()).collect()
    }

    /// Keeps breakpoints on their code when lines are inserted (`delta` >
    /// 0) or removed (`delta` < 0) after line `after` of `path`. Breakpoints
    /// in removed lines go to the first line after the cut.
    pub fn shift_lines(&mut self, path: &Path, after: usize, delta: isize) {
        if delta == 0 {
            return;
        }
        for b in self.breakpoints.iter_mut().filter(|b| b.path == path) {
            if b.line > after {
                b.line = if delta < 0 {
                    b.line.saturating_sub(delta.unsigned_abs()).max(after + 1)
                } else {
                    b.line + delta as usize
                };
            }
        }
        // Zusammengerückte Haltepunkte auf derselben Zeile: einer bleibt.
        let mut seen = BTreeSet::new();
        self.breakpoints
            .retain(|b| seen.insert((b.path.clone(), b.line)));
    }
}

/// How an edit moved lines from `old` to `new`: lines after `after` (1-based)
/// moved by `delta` – for [`DebugSettings::shift_lines`]. `None` if the
/// number of lines stayed the same.
///
/// ```
/// use freya_td::debug::line_shift;
///
/// // Neue Zeile nach Zeile 2.
/// assert_eq!(line_shift("a\nb\nc", "a\nb\nneu\nc"), Some((2, 1)));
/// // Zeile 2 gelöscht.
/// assert_eq!(line_shift("a\nb\nc", "a\nc"), Some((1, -1)));
/// assert_eq!(line_shift("a\nb", "a\nB"), None);
/// ```
pub fn line_shift(old: &str, new: &str) -> Option<(usize, isize)> {
    let a: Vec<&str> = old.split('\n').collect();
    let b: Vec<&str> = new.split('\n').collect();
    if a.len() == b.len() {
        return None;
    }
    // Bis zur ersten abweichenden Zeile bleibt alles stehen, dahinter
    // verschiebt sich der Rest um die Differenz.
    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    Some((prefix, b.len() as isize - a.len() as isize))
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct ProjectEntry {
    /// The `.ptdr` (or project folder).
    project: PathBuf,
    #[serde(flatten)]
    settings: DebugSettings,
}

/// Debugger settings of all projects of the user (`debug.toml`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DebugStore {
    #[serde(default, rename = "project")]
    projects: Vec<ProjectEntry>,
}

impl DebugStore {
    pub fn default_path() -> Option<PathBuf> {
        crate::desktop::config_dir().map(|d| d.join("debug.toml"))
    }

    /// Reads the store; a missing or broken file gives an empty one.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }

    /// The settings of `project` (defaults if it has none yet).
    pub fn get(&self, project: &Path) -> DebugSettings {
        self.projects
            .iter()
            .find(|p| p.project == project)
            .map(|p| p.settings.clone())
            .unwrap_or_default()
    }

    pub fn set(&mut self, project: &Path, settings: DebugSettings) {
        match self.projects.iter_mut().find(|p| p.project == project) {
            Some(p) => p.settings = settings,
            None => {
                self.projects.push(ProjectEntry {
                    project: project.to_path_buf(),
                    settings,
                });
                self.projects.sort_by(|a, b| a.project.cmp(&b.project));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_and_kinds() {
        let mut s = DebugSettings::default();
        let file = Path::new("/p/src/main.rs");
        assert!(s.toggle(file, 5));
        assert_eq!(s.get(file, 5).unwrap().kind(), BreakpointKind::Normal);
        let mut b = s.get(file, 5).unwrap().clone();
        b.condition = Some("x > 1".into());
        s.set(b);
        assert_eq!(s.get(file, 5).unwrap().kind(), BreakpointKind::Conditional);
        s.set(Breakpoint {
            log_message: Some("x = {x}".into()),
            ..Breakpoint::new(file, 3)
        });
        assert_eq!(s.in_file(file)[0].line, 3);
        assert_eq!(s.in_file(file)[0].kind(), BreakpointKind::Log);
        s.set_all_enabled(false);
        assert!(
            s.breakpoints
                .iter()
                .all(|b| b.kind() == BreakpointKind::Disabled)
        );
        assert!(!s.toggle(file, 5));
        assert_eq!(s.breakpoints.len(), 1);
    }

    #[test]
    fn shift_lines() {
        let mut s = DebugSettings::default();
        let file = Path::new("a.rs");
        for line in [2, 5, 6, 9] {
            s.toggle(file, line);
        }
        s.shift_lines(file, 3, 2);
        let lines = |s: &DebugSettings| s.in_file(file).iter().map(|b| b.line).collect::<Vec<_>>();
        assert_eq!(lines(&s), [2, 7, 8, 11]);
        // Zeilen 4–8 gelöscht: 7 und 8 landen auf 4, nur einer bleibt.
        s.shift_lines(file, 3, -5);
        assert_eq!(lines(&s), [2, 4, 6]);
    }

    #[test]
    fn store_round_trip() {
        let dir = std::env::temp_dir().join(format!("freya-debug-store-{}", std::process::id()));
        let path = dir.join("debug.toml");
        let mut store = DebugStore::default();
        let mut settings = DebugSettings::default();
        settings.toggle(Path::new("/p/src/main.rs"), 7);
        settings.watches.push("x + 1".into());
        settings.stop_on_panic = false;
        store.set(Path::new("/p/p.ptdr"), settings.clone());
        store.save(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("[[project]]"), "{text}");
        let loaded = DebugStore::load(&path);
        assert_eq!(loaded.get(Path::new("/p/p.ptdr")), settings);
        assert_eq!(
            loaded.get(Path::new("/other.ptdr")),
            DebugSettings::default()
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
