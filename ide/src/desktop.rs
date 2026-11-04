//! The window arrangement (like Delphi's desktop): position, size and
//! visibility of every IDE window, saved per user and restored on start.
//!
//! Stored as TOML in the user's configuration directory:
//! Linux `$XDG_CONFIG_HOME/freya-tde/desktop.toml` (or `~/.config/…`),
//! macOS `~/Library/Application Support/freya-tde/…`, Windows `%APPDATA%\freya-tde\…`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::files::ScanOptions;

/// Where a window sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Place {
    /// Screen position; `None` = the system decides (e.g. under Wayland).
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub width: i32,
    pub height: i32,
    pub visible: bool,
}

impl Place {
    pub const fn at(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x: Some(x),
            y: Some(y),
            width,
            height,
            visible: true,
        }
    }
}

/// Names of the IDE windows.
pub mod window {
    pub const MAIN: &str = "main";
    pub const DESIGNER: &str = "designer";
    pub const INSPECTOR: &str = "inspector";
    pub const STRUCTURE: &str = "structure";
    pub const MESSAGES: &str = "messages";
    pub const CODE: &str = "code";
    pub const PROJECT: &str = "project";
    pub const GIT: &str = "git";
    pub const DEBUGGER: &str = "debugger";
    pub const SHORTCUTS: &str = "shortcuts";
    pub const DATABASE: &str = "database";
}

/// All window places.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Desktop {
    #[serde(default)]
    pub windows: BTreeMap<String, Place>,
    /// What the project window shows.
    #[serde(default)]
    pub files: ScanOptions,
}

/// The configuration directory of Freya TDE for this user. A folder of the
/// old name (`freya-td`, before 02.11.2026) is taken over.
pub fn config_dir() -> Option<PathBuf> {
    let dir = config_base()?;
    let new = dir.join("freya-tde");
    let old = dir.join("freya-td");
    if !new.exists() && old.is_dir() {
        let _ = std::fs::rename(&old, &new);
    }
    Some(new)
}

fn config_base() -> Option<PathBuf> {
    let env = |name: &str| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    if cfg!(target_os = "windows") {
        env("APPDATA")
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|h| h.join("Library/Application Support"))
    } else {
        env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config")))
    }
}

impl Desktop {
    /// The arrangement on a first start: main window across the top, object
    /// inspector and structure left, designer in the middle, messages below,
    /// project right.
    pub fn standard() -> Self {
        let windows = [
            (window::MAIN, Place::at(40, 40, 1100, 150)),
            (window::STRUCTURE, Place::at(40, 200, 280, 260)),
            (window::INSPECTOR, Place::at(40, 490, 280, 420)),
            (window::DESIGNER, Place::at(340, 200, 800, 560)),
            (window::MESSAGES, Place::at(340, 770, 800, 200)),
            (window::CODE, Place::at(370, 230, 820, 600)),
            (window::PROJECT, Place::at(1160, 200, 300, 330)),
            // Erst auf Wunsch sichtbar (Fenster → Quellcodeverwaltung).
            (
                window::GIT,
                Place {
                    visible: false,
                    ..Place::at(760, 160, 420, 720)
                },
            ),
            // Erscheint beim Debuggen (Start → Starten).
            (
                window::DEBUGGER,
                Place {
                    visible: false,
                    ..Place::at(1120, 200, 420, 700)
                },
            ),
            (
                window::SHORTCUTS,
                Place {
                    visible: false,
                    ..Place::at(300, 120, 720, 640)
                },
            ),
            // Fenster → Datenbank.
            (
                window::DATABASE,
                Place {
                    visible: false,
                    ..Place::at(1160, 200, 380, 600)
                },
            ),
        ];
        Self {
            windows: windows
                .into_iter()
                .map(|(n, p)| (n.to_owned(), p))
                .collect(),
            files: ScanOptions::default(),
        }
    }

    /// Place of a window (standard place if none is stored).
    pub fn place(&self, name: &str) -> Place {
        self.windows.get(name).copied().unwrap_or_else(|| {
            Self::standard()
                .windows
                .get(name)
                .copied()
                .unwrap_or(Place::at(100, 100, 600, 400))
        })
    }

    pub fn set(&mut self, name: &str, place: Place) {
        self.windows.insert(name.to_owned(), place);
    }

    fn path() -> Option<PathBuf> {
        config_dir().map(|d| d.join("desktop.toml"))
    }

    /// The saved arrangement, or the standard one.
    pub fn load() -> Self {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_else(Self::standard)
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::path() else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_defaults() {
        let mut desktop = Desktop::standard();
        desktop.set(window::DESIGNER, Place::at(1, 2, 3, 4));
        let text = toml::to_string_pretty(&desktop).unwrap();
        let back: Desktop = toml::from_str(&text).unwrap();
        assert_eq!(back, desktop);
        assert_eq!(back.place(window::DESIGNER), Place::at(1, 2, 3, 4));
        assert_eq!(Desktop::default().place(window::MAIN).width, 1100);
        assert_eq!(Desktop::default().place(window::MAIN).height, 150);
    }
}
