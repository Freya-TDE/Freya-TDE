//! Help → keyboard shortcuts: every shortcut with what it does, built from
//! the menu definitions (so the page cannot go stale) plus the keys that
//! are not in a menu. [`shortcuts`] returns them as data (e.g. for an AI).

use crate::tr;
use std::rc::Rc;

use rncl::prelude::*;
use ruf::MenuEntry;
use serde::Serialize;

use super::Ide;

/// A keyboard shortcut.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Shortcut {
    /// Where it works: `Alle Fenster`, `Code`, `Designer` …
    pub scope: String,
    /// Menu (and submenu) it is in; empty for keys without a menu.
    pub menu: String,
    /// Keys as written in the menus, e.g. `Ctrl+Shift+F9`.
    pub keys: String,
    pub command: String,
    pub description: String,
}

/// What a menu command does (by menu id; texts `shortcut.<id>`).
fn description(id: &str) -> String {
    let key = format!("shortcut.{id}");
    crate::i18n::ensure();
    if ruf::i18n::has(&key) {
        tr!(&key)
    } else {
        String::new()
    }
}

/// Shortcuts of `menu` (recursively), labelled `path`.
fn collect(menu: &Menu, path: &str, scope: &str, out: &mut Vec<Shortcut>) {
    for entry in &menu.entries {
        match entry {
            MenuEntry::Item {
                id,
                label,
                shortcut: Some(keys),
                ..
            } => out.push(Shortcut {
                scope: scope.to_owned(),
                menu: path.to_owned(),
                keys: keys.clone(),
                command: label.trim_end_matches(" …").to_owned(),
                description: description(id),
            }),
            MenuEntry::Submenu { label, menu } => {
                collect(menu, &format!("{path} › {label}"), scope, out);
            }
            _ => {}
        }
    }
}

/// All shortcuts of Freya TDE, by window.
pub fn shortcuts() -> Vec<Shortcut> {
    let mut out = Vec::new();
    let bars = [
        (tr!("shortcuts.all_windows"), super::main_window::menu_bar()),
        (tr!("menu.title.code"), super::code::menu_bar()),
        (tr!("window.designer"), super::designer::menu_bar()),
        (
            tr!("menu.window.git"),
            vec![("Git".to_owned(), super::git_window::more_actions())],
        ),
        (
            tr!("menu.window.terminal"),
            vec![(tr!("shortcuts.context_menu"), super::panel::terminal_menu())],
        ),
        (
            tr!("menu.window.ai"),
            vec![(tr!("menu.window.ai"), super::ai_window::menu())],
        ),
    ];
    for (scope, bar) in bars {
        for (title, menu) in bar {
            collect(&menu, &title, &scope, &mut out);
        }
    }
    // Tasten ohne Menüeintrag: Bereich, Tasten, Schlüssel des Texts.
    let extra = [
        ("designer", "Esc", "esc"),
        ("designer", "Del", "delete"),
        ("code", "Ctrl+Click", "ctrl_click"),
        ("code", "Gutter", "gutter"),
        ("debug_console", "Enter", "evaluate"),
        ("debugger", "Enter", "watch"),
        ("terminal", "Shift+PageUpDown", "scrollback"),
    ];
    out.extend(extra.into_iter().map(|(scope, keys, key)| Shortcut {
        scope: tr!(&format!("shortcuts.scope.{scope}")),
        menu: String::new(),
        keys: keys.into(),
        command: tr!(&format!("shortcuts.extra.{key}")),
        description: tr!(&format!("shortcuts.extra.{key}_tip")),
    }));
    out
}

/// Keys for display: names of the language, on macOS the symbols (Ctrl = ⌘).
pub fn display_keys(keys: &str) -> String {
    if cfg!(target_os = "macos") {
        keys.split('+')
            .map(|part| match part {
                "Ctrl" => "⌘".to_owned(),
                "Shift" => "⇧".to_owned(),
                "Alt" => "⌥".to_owned(),
                "Del" | "Click" | "Gutter" | "PageUpDown" => {
                    tr!(&format!("keys.{}", part.to_lowercase()))
                }
                other => other.to_owned(),
            })
            .collect::<Vec<_>>()
            .join("")
    } else {
        keys.split('+')
            .map(|part| match part {
                "Ctrl" | "Shift" | "Space" | "Del" | "Click" | "Gutter" | "PageUpDown" => {
                    tr!(&format!("keys.{}", part.to_lowercase()))
                }
                other => other.to_owned(),
            })
            .collect::<Vec<_>>()
            .join("+")
    }
}

/// The page as text: per window, per menu, keys in a column.
pub fn page() -> String {
    let list = shortcuts();
    let width = list
        .iter()
        .map(|s| display_keys(&s.keys).chars().count())
        .max()
        .unwrap_or(10)
        + 3;
    let mut text = tr!("shortcuts.intro");
    let mut scope = "";
    let mut menu = "";
    for s in &list {
        if s.scope != scope {
            scope = &s.scope;
            menu = "";
            text.push_str(&format!(
                "\n{scope}\n{}\n",
                "═".repeat(scope.chars().count())
            ));
        }
        if s.menu != menu {
            menu = &s.menu;
            let no_menu = tr!("shortcuts.no_menu");
            let title = if menu.is_empty() {
                no_menu.as_str()
            } else {
                menu
            };
            text.push_str(&format!("\n  {title}\n"));
        }
        let keys = display_keys(&s.keys);
        let pad = width.saturating_sub(keys.chars().count());
        text.push_str(&format!("    {keys}{}{}", " ".repeat(pad), s.command));
        if !s.description.is_empty() {
            text.push_str(&format!(" – {}", s.description));
        }
        text.push('\n');
    }
    text
}

pub struct ShortcutsWindow {
    pub window: RnclWindow,
    text: RnclTextArea,
}

impl ShortcutsWindow {
    pub fn build() -> Self {
        let window = RnclWindow::new(&tr!("menu.help.shortcuts"));
        let text = RnclTextArea::new(&window);
        text.set_read_only(true);
        text.set_monospace(true);
        text.set_text(&page());
        Self { window, text }
    }
}

pub fn connect(ide: &Rc<Ide>) {
    let weak = Rc::downgrade(ide);
    ide.shortcuts.window.on_resized(move |w, h| {
        if let Some(ide) = weak.upgrade() {
            ide.shortcuts.text.set_geometry(0, 0, w, h);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listed_and_described() {
        // Die Prüfung nennt die deutschen Menünamen.
        crate::i18n::ensure();
        ruf::i18n::set_language("de");
        let list = shortcuts();
        let f9 = list.iter().find(|s| s.keys == "F9").expect("F9");
        assert_eq!(f9.scope, "Alle Fenster");
        assert_eq!(f9.menu, "Start");
        assert!(!f9.description.is_empty());
        // Jedes Kürzel eines Menüs hat eine Erklärung.
        for s in list.iter().filter(|s| !s.menu.is_empty()) {
            assert!(!s.description.is_empty(), "ohne Erklärung: {s:?}");
        }
        // Kein Kürzel doppelt innerhalb eines Fensters.
        let mut seen = std::collections::HashSet::new();
        for s in &list {
            assert!(
                seen.insert((s.scope.clone(), s.keys.clone())),
                "doppelt: {s:?}"
            );
        }
        assert!(page().contains("Bis Rückgabe"));
    }

    #[test]
    fn key_names() {
        crate::i18n::ensure();
        ruf::i18n::set_language("de");
        if !cfg!(target_os = "macos") {
            assert_eq!(display_keys("Ctrl+Shift+F9"), "Strg+Umschalt+F9");
        } else {
            assert_eq!(display_keys("Ctrl+Shift+F9"), "⌘⇧F9");
        }
    }
}
