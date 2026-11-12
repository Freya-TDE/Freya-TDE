//! Code snippets, without UI: stored pieces of code with a name, a short
//! description, an optional `use` line and the code. Inserting puts the code
//! at the cursor (following lines indented like the cursor line) and appends
//! the `use` line to the file's `use` block unless it is already there.
//!
//! Kept per user in `<config>/freya-tde/snippets.toml`; without the file a
//! few built-in snippets are offered. `pinned` are the snippets the user put
//! on the palette tab "Snippets" (they stay in the drop-down as well).
//!
//! *Code-Snippets: Name, Kurzbeschreibung, use-Zeile, Code; Einfügen am
//! Cursor, die use-Zeile kommt an den use-Block.*
//!
//! ```
//! use freya_tde::snippets::{Snippet, insert};
//!
//! let snippet = Snippet::new("Datei lesen", "Liest eine Textdatei", "use std::fs;", "let text = fs::read_to_string(\"a.txt\")?;");
//! let source = "use std::io;\n\nfn main() {\n    \n}\n";
//! let done = insert(source, 4, 4, &snippet);
//! assert!(done.text.starts_with("use std::io;\nuse std::fs;\n"));
//! assert!(done.text.contains("    let text = fs::read_to_string(\"a.txt\")?;"));
//! assert_eq!(done.line, 5); // der Cursor rückt mit der neuen use-Zeile
//! ```

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Name of the file in the configuration directory.
pub const FILE: &str = "snippets.toml";

/// One snippet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snippet {
    pub name: String,
    /// Short description (shown when hovering the name).
    #[serde(default)]
    pub description: String,
    /// `use` line for the file's `use` block; empty = none.
    #[serde(default, rename = "use", skip_serializing_if = "String::is_empty")]
    pub use_line: String,
    pub code: String,
}

impl Snippet {
    pub fn new(name: &str, description: &str, use_line: &str, code: &str) -> Self {
        Self {
            name: name.to_owned(),
            description: description.to_owned(),
            use_line: use_line.to_owned(),
            code: code.to_owned(),
        }
    }
}

/// The user's snippets and which of them are on the palette tab.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnippetStore {
    #[serde(default, rename = "snippet")]
    pub snippets: Vec<Snippet>,
    /// Names of the snippets on the palette tab, in order.
    #[serde(default)]
    pub pinned: Vec<String>,
}

impl Default for SnippetStore {
    fn default() -> Self {
        Self {
            snippets: builtin(),
            pinned: Vec::new(),
        }
    }
}

/// Snippets offered before the user has own ones.
pub fn builtin() -> Vec<Snippet> {
    vec![
        Snippet::new(
            "Datei lesen",
            "Liest eine Textdatei vollständig in einen String",
            "use std::fs;",
            "let text = fs::read_to_string(\"datei.txt\")?;",
        ),
        Snippet::new(
            "HashMap",
            "Neue HashMap mit String-Schlüsseln",
            "use std::collections::HashMap;",
            "let mut map: HashMap<String, i32> = HashMap::new();",
        ),
        Snippet::new(
            "Zeilen durchlaufen",
            "Schleife über die Zeilen eines Textes mit Nummer",
            "",
            "for (nummer, zeile) in text.lines().enumerate() {\n    println!(\"{}: {zeile}\", nummer + 1);\n}",
        ),
        Snippet::new(
            "Pause (Tokio)",
            "Wartet asynchron, ohne die Oberfläche zu blockieren",
            "use std::time::Duration;",
            "tokio::time::sleep(Duration::from_millis(500)).await;",
        ),
        Snippet::new(
            "Debug-Ausgabe",
            "Gibt einen Wert mit Datei und Zeile aus (dbg!)",
            "",
            "dbg!(&wert);",
        ),
    ]
}

/// Why the snippets could not be changed. Codes are stable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SnippetIssue {
    pub code: &'static str,
    pub detail: String,
}

/// Stable codes of [`SnippetIssue`].
pub mod codes {
    pub const NAME_EMPTY: &str = "name_empty";
    pub const NAME_TAKEN: &str = "name_taken";
    pub const CODE_EMPTY: &str = "code_empty";
    pub const UNKNOWN: &str = "unknown_snippet";
    pub const WRITE_FAILED: &str = "write_failed";
}

fn issue(code: &'static str, detail: impl Into<String>) -> SnippetIssue {
    SnippetIssue {
        code,
        detail: detail.into(),
    }
}

impl SnippetStore {
    /// The store file (`None` without configuration directory).
    pub fn file() -> Option<PathBuf> {
        crate::desktop::config_dir().map(|d| d.join(FILE))
    }

    /// Reads the snippets; a missing file gives the built-in ones, a broken
    /// one as well.
    pub fn load_from(file: &Path) -> Self {
        std::fs::read_to_string(file)
            .ok()
            .and_then(|t| toml::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save_to(&self, file: &Path) -> Result<(), SnippetIssue> {
        let text =
            toml::to_string_pretty(self).map_err(|e| issue(codes::WRITE_FAILED, e.to_string()))?;
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| issue(codes::WRITE_FAILED, format!("{}: {e}", dir.display())))?;
        }
        std::fs::write(file, text)
            .map_err(|e| issue(codes::WRITE_FAILED, format!("{}: {e}", file.display())))
    }

    pub fn get(&self, name: &str) -> Option<&Snippet> {
        self.snippets.iter().find(|s| s.name == name)
    }

    /// The pinned snippets, in order (unknown names are skipped).
    pub fn pinned_snippets(&self) -> Vec<&Snippet> {
        self.pinned.iter().filter_map(|n| self.get(n)).collect()
    }

    /// Adds `snippet`, or replaces the one named `old` (renaming keeps it
    /// pinned).
    pub fn put(&mut self, old: Option<&str>, snippet: Snippet) -> Result<(), SnippetIssue> {
        let name = snippet.name.trim();
        if name.is_empty() {
            return Err(issue(codes::NAME_EMPTY, "Snippet ohne Namen"));
        }
        if snippet.code.trim().is_empty() {
            return Err(issue(
                codes::CODE_EMPTY,
                format!("Snippet „{name}“ ohne Code"),
            ));
        }
        if old != Some(name) && self.get(name).is_some() {
            return Err(issue(
                codes::NAME_TAKEN,
                format!("Snippet „{name}“ gibt es schon"),
            ));
        }
        let snippet = Snippet {
            name: name.to_owned(),
            ..snippet
        };
        match old.and_then(|o| self.snippets.iter().position(|s| s.name == o)) {
            Some(index) => {
                let old = std::mem::replace(&mut self.snippets[index], snippet);
                for pinned in &mut self.pinned {
                    if *pinned == old.name {
                        pinned.clone_from(&self.snippets[index].name);
                    }
                }
            }
            None if old.is_some() => {
                return Err(issue(
                    codes::UNKNOWN,
                    format!("kein Snippet „{}“", old.unwrap_or_default()),
                ));
            }
            None => self.snippets.push(snippet),
        }
        Ok(())
    }

    /// Removes a snippet (and from the palette tab).
    pub fn remove(&mut self, name: &str) {
        self.snippets.retain(|s| s.name != name);
        self.pinned.retain(|n| n != name);
    }

    /// Puts a snippet on the palette tab or takes it off; returns whether it
    /// is pinned now.
    pub fn toggle_pinned(&mut self, name: &str) -> Result<bool, SnippetIssue> {
        if self.get(name).is_none() {
            return Err(issue(codes::UNKNOWN, format!("kein Snippet „{name}“")));
        }
        if let Some(i) = self.pinned.iter().position(|n| n == name) {
            self.pinned.remove(i);
            Ok(false)
        } else {
            self.pinned.push(name.to_owned());
            Ok(true)
        }
    }
}

/// The text after inserting and where the cursor goes (end of the code).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Inserted {
    pub text: String,
    /// 1-based line, 0-based character column.
    pub line: usize,
    pub column: usize,
}

/// The `use` line as a statement: `std::fs` → `use std::fs;`.
fn use_statement(line: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let mut statement = if line.starts_with("use ") || line.starts_with("pub use ") {
        line.to_owned()
    } else {
        format!("use {line}")
    };
    if !statement.ends_with(';') {
        statement.push(';');
    }
    Some(statement)
}

/// Inserts `snippet` into `source` at `line` (1-based) / `column` (0-based,
/// characters).
pub fn insert(source: &str, line: usize, column: usize, snippet: &Snippet) -> Inserted {
    let mut lines: Vec<String> = source.split('\n').map(str::to_owned).collect();
    let row = line.clamp(1, lines.len()) - 1;
    let indent: String = lines[row]
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect();
    let current: Vec<char> = lines[row].chars().collect();
    let column = column.min(current.len());
    let before: String = current[..column].iter().collect();
    let after: String = current[column..].iter().collect();
    let code: Vec<&str> = snippet.code.trim_end_matches('\n').split('\n').collect();
    let mut new_lines = Vec::new();
    for (i, code_line) in code.iter().enumerate() {
        if i == 0 {
            new_lines.push(format!("{before}{code_line}"));
        } else if code_line.is_empty() {
            new_lines.push(String::new());
        } else {
            new_lines.push(format!("{indent}{code_line}"));
        }
    }
    let last = new_lines.len() - 1;
    let mut cursor = (row + last, new_lines[last].chars().count());
    new_lines[last].push_str(&after);
    lines.splice(row..=row, new_lines);

    // use-Zeile an den use-Block (oder oben in die Datei), wenn nicht schon da.
    if let Some(statement) = use_statement(&snippet.use_line)
        && !lines.iter().any(|l| l.trim() == statement)
    {
        let top_level_use = |l: &str| l.starts_with("use ") || l.starts_with("pub use ");
        let (at, extra) = match lines.iter().rposition(|l| top_level_use(l)) {
            // Hinter das Ende der (ggf. mehrzeiligen) letzten use-Anweisung.
            Some(i) => {
                let end = (i..lines.len())
                    .find(|&k| lines[k].trim_end().ends_with(';'))
                    .unwrap_or(i);
                (end + 1, false)
            }
            None => {
                let skip = lines
                    .iter()
                    .take_while(|l| {
                        let l = l.trim_start();
                        l.starts_with("//!") || l.starts_with("#![") || l.starts_with("include!")
                    })
                    .count();
                (skip, true)
            }
        };
        let mut added = vec![statement];
        if extra && lines.get(at).is_some_and(|l| !l.trim().is_empty()) {
            added.push(String::new());
        }
        let count = added.len();
        lines.splice(at..at, added);
        if at <= cursor.0 {
            cursor.0 += count;
        }
    }
    Inserted {
        text: lines.join("\n"),
        line: cursor.0 + 1,
        column: cursor.1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_indents_and_keeps_the_rest_of_the_line() {
        let snippet = Snippet::new("x", "", "", "if a {\n    b();\n}");
        let source = "fn f() {\n    let x = 1; // end\n}\n";
        let done = insert(source, 2, 15, &snippet);
        assert_eq!(
            done.text,
            "fn f() {\n    let x = 1; if a {\n        b();\n    }// end\n}\n"
        );
        assert_eq!((done.line, done.column), (4, 5));
    }

    #[test]
    fn use_lines_go_to_the_block_once() {
        let snippet = Snippet::new("x", "", "std::fs", "fs::write(\"a\", \"b\")?;");
        // Ohne use-Block: oben (hinter include!), mit Leerzeile.
        let source = "include!(\"f.gen.rs\");\nimpl F {\n    fn g(&mut self) {}\n}\n";
        let done = insert(source, 3, 20, &snippet);
        assert!(
            done.text
                .starts_with("include!(\"f.gen.rs\");\nuse std::fs;\n\nimpl F"),
            "{}",
            done.text
        );
        assert_eq!(done.line, 5);
        // Schon vorhanden: nicht noch einmal.
        let again = insert(&done.text, done.line, done.column, &snippet);
        assert_eq!(again.text.matches("use std::fs;").count(), 1);
        // Mehrzeiliger Block.
        let source = "use std::{\n    io,\n};\n\nfn main() {}\n";
        let done = insert(source, 5, 11, &snippet);
        assert!(
            done.text
                .starts_with("use std::{\n    io,\n};\nuse std::fs;\n"),
            "{}",
            done.text
        );
    }

    #[test]
    fn store_put_rename_pin_remove() {
        let file = std::env::temp_dir().join(format!("freya-snippets-{}.toml", std::process::id()));
        let _ = std::fs::remove_file(&file);
        let mut store = SnippetStore::load_from(&file);
        assert_eq!(store.snippets, builtin());
        let mine = Snippet::new("Mein", "Kurz", "", "todo!();");
        store.put(None, mine.clone()).unwrap();
        assert_eq!(
            store.put(None, mine.clone()).unwrap_err().code,
            codes::NAME_TAKEN
        );
        assert_eq!(
            store
                .put(None, Snippet::new(" ", "", "", "x"))
                .unwrap_err()
                .code,
            codes::NAME_EMPTY
        );
        assert!(store.toggle_pinned("Mein").unwrap());
        store
            .put(Some("Mein"), Snippet::new("Meins", "Kurz", "", "todo!();"))
            .unwrap();
        assert_eq!(store.pinned, ["Meins"]);
        store.save_to(&file).unwrap();
        let loaded = SnippetStore::load_from(&file);
        assert_eq!(loaded, store);
        let mut loaded = loaded;
        loaded.remove("Meins");
        assert!(loaded.pinned.is_empty() && loaded.get("Meins").is_none());
        let _ = std::fs::remove_file(&file);
    }
}
