//! Help → keyboard shortcuts: every shortcut with what it does, built from
//! the menu definitions (so the page cannot go stale) plus the keys that
//! are not in a menu. [`shortcuts`] returns them as data (e.g. for an AI).

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

/// What a menu command does (by menu id).
fn description(id: &str) -> &'static str {
    match id {
        "file.new" => {
            "Öffnet den Assistenten für ein neues Projekt (Programm oder Bibliothek, Oberfläche, Komponenten, Git)."
        }
        "file.open" => "Öffnet eine Projektdatei (.ptdr).",
        "file.save" => "Speichert Projekt und Formulare.",
        "file.quit" => "Beendet Freya TDE (fragt bei ungespeicherten Änderungen).",
        "edit.undo" => "Nimmt die letzte Änderung im Designer zurück.",
        "edit.redo" => "Stellt die zurückgenommene Änderung wieder her.",
        "project.generate" => "Erzeugt den Rust-Code aus den Formularen.",
        "project.build" => "Erzeugt den Code und baut das Projekt (cargo build).",
        "start.run" => "Baut und startet das Programm im Debugger; angehalten: weiter.",
        "start.run_plain" => "Baut und startet das Programm ohne Debugger.",
        "start.stop" => "Beendet das Programm im Debugger.",
        "start.step_into" => "Führt eine Zeile aus und springt in aufgerufene Funktionen.",
        "start.step_over" => "Führt eine Zeile aus, Aufrufe laufen ganz durch.",
        "start.step_out" => "Läuft bis zum Rücksprung in die aufrufende Funktion.",
        "start.run_to" => "Läuft bis zur Zeile mit dem Cursor (startet bei Bedarf).",
        "start.toggle_bp" => "Setzt oder entfernt den Haltepunkt in der Cursorzeile.",
        "start.add_watch" => "Überwacht einen Ausdruck (vorbelegt mit dem Wort am Cursor).",
        "window.designer" => {
            "Zeigt den Designer (im Code-Fenster: zwischen Code und Designer wechseln)."
        }
        "window.inspector" => "Zeigt den Objektinspektor.",
        "window.problems" => "Zeigt die Probleme (Modell, Code, Build, rust-analyzer).",
        "window.output" => "Zeigt die Ausgabe (Build, Programm, Git, rust-analyzer).",
        "window.terminal" => "Zeigt das Terminal und startet die Shell im Projektordner.",
        "window.git" => "Zeigt die Quellcodeverwaltung.",
        "window.debugger" => "Zeigt das Debugger-Fenster (Variablen, Aufrufstapel …).",
        "window.database" => {
            "Zeigt das Fenster Datenbank (Tabellen, Spalten, Routinen, Redis-Schlüssel)."
        }
        "window.debug_console" => "Zeigt die Debug-Konsole.",
        "code.save" => "Speichert die Datei im vorderen Reiter.",
        "code.save_all" => "Speichert alle geänderten Dateien.",
        "code.close" => "Schließt den vorderen Reiter.",
        "code.complete" => "Zeigt Vorschläge (rust-analyzer).",
        "code.definition" => "Springt zur Definition des Worts am Cursor.",
        "code.designer" | "designer.code" => "Wechselt zwischen Designer und Code.",
        "git.commit" => "Committet die gestagten Änderungen mit der Nachricht.",
        "git.refresh" => "Liest Status und Graph neu ein.",
        "term.copy" => "Kopiert die Auswahl im Terminal.",
        "term.paste" => "Fügt die Zwischenablage ins Terminal ein.",
        _ => "",
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
                description: description(id).to_owned(),
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
        ("Alle Fenster", super::main_window::menu_bar()),
        ("Code", super::code::menu_bar()),
        ("Designer", super::designer::menu_bar()),
        (
            "Quellcodeverwaltung",
            vec![("Git".to_owned(), super::git_window::more_actions())],
        ),
        (
            "Terminal",
            vec![("Kontextmenü".to_owned(), super::panel::terminal_menu())],
        ),
    ];
    for (scope, bar) in bars {
        for (title, menu) in bar {
            collect(&menu, &title, scope, &mut out);
        }
    }
    // Tasten ohne Menüeintrag.
    let extra = [
        (
            "Designer",
            "Esc",
            "Abbrechen",
            "Bricht das Platzieren ab und hebt die Auswahl auf.",
        ),
        (
            "Designer",
            "Entf",
            "Löschen",
            "Löscht die gewählten Elemente.",
        ),
        (
            "Code",
            "Ctrl+Klick",
            "Gehe zu Definition",
            "Springt zur Definition des angeklickten Worts.",
        ),
        (
            "Code",
            "Klick in den Rand",
            "Haltepunkt",
            "Setzt bzw. entfernt einen Haltepunkt; Rechtsklick: Bedingung, Logpoint, bis hier ausführen.",
        ),
        (
            "Debug-Konsole",
            "Enter",
            "Auswerten",
            "Wertet den Ausdruck im gewählten Rahmen aus; ` am Anfang schickt einen Befehl an den Debugger.",
        ),
        (
            "Debugger",
            "Enter",
            "Überwachen",
            "Fügt den Ausdruck im Feld „Überwacht“ hinzu.",
        ),
        (
            "Terminal",
            "Shift+Bild↑/↓",
            "Rückschau",
            "Blättert in der Ausgabe (auch mit dem Mausrad).",
        ),
    ];
    out.extend(
        extra
            .into_iter()
            .map(|(scope, keys, command, description)| Shortcut {
                scope: scope.into(),
                menu: String::new(),
                keys: keys.into(),
                command: command.into(),
                description: description.into(),
            }),
    );
    out
}

/// Keys for display: German names, on macOS the symbols (Ctrl = ⌘).
pub fn display_keys(keys: &str) -> String {
    if cfg!(target_os = "macos") {
        keys.split('+')
            .map(|part| match part {
                "Ctrl" => "⌘",
                "Shift" => "⇧",
                "Alt" => "⌥",
                other => other,
            })
            .collect::<Vec<_>>()
            .join("")
    } else {
        keys.split('+')
            .map(|part| match part {
                "Ctrl" => "Strg",
                "Shift" => "Umschalt",
                "Space" => "Leertaste",
                other => other,
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
    let mut text = String::from(
        "Tastenkürzel von Freya TDE\n\
         Die Kürzel des Hauptmenüs gelten in allen Fenstern; das eigene Menü eines\n\
         Fensters geht vor (z. B. F5 = Aktualisieren in der Quellcodeverwaltung).\n",
    );
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
            let title = if menu.is_empty() { "Ohne Menü" } else { menu };
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
        let window = RnclWindow::new("Tastenkürzel");
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
        if !cfg!(target_os = "macos") {
            assert_eq!(display_keys("Ctrl+Shift+F9"), "Strg+Umschalt+F9");
        } else {
            assert_eq!(display_keys("Ctrl+Shift+F9"), "⌘⇧F9");
        }
    }
}
