//! Code completion like the Lazarus CodeTools (Ctrl+Shift+C), without UI.
//!
//! At the cursor, Freya TDE asks rust-analyzer for its code actions and
//! completes what is missing – nothing is ever deleted:
//!
//! - **method body**: `self.save(&customer, 3)` of a method that does not
//!   exist → `fn save(&mut self, customer: &Customer, arg: i32) {}` in the
//!   user's `impl` block (types from rust-analyzer; `&mut self` like the
//!   handlers; no `-> _`, an empty body unless there is a return type);
//! - **own field**: `self.counter += 1` of a field that does not exist → an
//!   entry in the form's `state` (see [`tdr_core::state`]), not in the
//!   generated struct; the type is suggested from the use ([`suggest_type`]);
//! - **trait members**: `impl Trait for Type` without all required methods →
//!   their bodies (rust-analyzer's "Implement missing members").
//!
//! New methods are placed after the calling method, alphabetically or at the
//! end of the `impl` block ([`MethodOrder`], a setting).
//!
//! *Code-Vervollständigung wie die Lazarus CodeTools: Methodenrumpf,
//! eigenes Formular-Feld, fehlende Trait-Methoden; nichts wird gelöscht.*
//!
//! ```
//! use freya_tde::codetools::{field_at, tidy_method};
//!
//! let method = "\n\n    fn save(&self, n: i32) -> _ {\n        todo!()\n    }";
//! assert_eq!(tidy_method(method), "\n\n    fn save(&mut self, n: i32) {\n    }");
//! let source = "impl F {\n    fn go(&mut self) {\n        self.counter += 1;\n    }\n}\n";
//! assert_eq!(field_at(source, 3, 14), Some(("counter".to_owned(), "i32".to_owned())));
//! ```

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Where new methods go in the `impl` block.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MethodOrder {
    /// Right after the method the completion was started in.
    #[default]
    AfterCaller,
    Alphabetical,
    /// At the end of the `impl` block.
    End,
}

impl MethodOrder {
    pub const ALL: [MethodOrder; 3] = [
        MethodOrder::AfterCaller,
        MethodOrder::Alphabetical,
        MethodOrder::End,
    ];

    pub fn label(self) -> &'static str {
        match self {
            MethodOrder::AfterCaller => "hinter der aufrufenden Methode",
            MethodOrder::Alphabetical => "alphabetisch",
            MethodOrder::End => "am Ende des impl-Blocks",
        }
    }
}

/// A change of a file: replace `line:column`–`end_line:end_column` (lines
/// 1-based, columns in characters, 0-based) with `new_text`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextEdit {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
    pub new_text: String,
}

/// A code action of rust-analyzer with its changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Action {
    pub title: String,
    pub kind: Option<String>,
    pub edits: Vec<TextEdit>,
}

/// What the completion does at the cursor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Completion {
    /// Add a method body (one edit in the user's file).
    Method { name: String, edit: TextEdit },
    /// Add the missing members of a trait implementation.
    TraitMembers { edits: Vec<TextEdit> },
    /// Add an own field to the form (`state`); the type is a suggestion.
    Field { name: String, rust_type: String },
}

/// Why nothing can be completed. Codes are stable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CodetoolsIssue {
    pub code: &'static str,
    pub detail: String,
}

/// Stable codes of [`CodetoolsIssue`].
pub mod codes {
    /// Nothing to complete at the cursor.
    pub const NOTHING: &str = "nothing_to_complete";
    /// The file is generated (`.gen.rs`); completion works in user files.
    pub const GENERATED_FILE: &str = "generated_file";
    /// rust-analyzer is not running (yet).
    pub const ANALYZER_MISSING: &str = "analyzer_missing";
    /// rust-analyzer wants to change a generated file.
    pub const EDITS_GENERATED: &str = "edits_generated_file";
    /// No form is open for the field.
    pub const NO_FORM: &str = "no_form";
}

fn issue(code: &'static str, detail: impl Into<String>) -> CodetoolsIssue {
    CodetoolsIssue {
        code,
        detail: detail.into(),
    }
}

fn generated(path: &Path) -> bool {
    path.to_string_lossy().ends_with(".gen.rs")
}

/// Chooses the completion from rust-analyzer's `actions` at `line:column`
/// of `file` (with its `source`), placing a new method by `order`.
pub fn plan(
    file: &Path,
    source: &str,
    line: usize,
    column: usize,
    actions: &[Action],
    order: MethodOrder,
) -> Result<Completion, CodetoolsIssue> {
    if generated(file) {
        return Err(issue(
            codes::GENERATED_FILE,
            "generierte Dateien (.gen.rs) werden nicht vervollständigt",
        ));
    }
    // Eigene Felder zuerst: rust-analyzer will sie ins generierte Struct schreiben.
    if let Some((name, rust_type)) = field_at(source, line, column)
        && actions.iter().any(|a| a.title.starts_with("Add field"))
    {
        return Ok(Completion::Field { name, rust_type });
    }
    for action in actions {
        if action.edits.iter().any(|e| generated(&e.path)) {
            continue;
        }
        let title = action.title.as_str();
        if let Some(name) = title
            .strip_prefix("Generate ")
            .and_then(|t| t.strip_suffix(" method"))
            && let [edit] = action.edits.as_slice()
            && edit.path == file
        {
            let text = tidy_method(&edit.new_text);
            let edit = place_method(source, edit, name, &text, order);
            return Ok(Completion::Method {
                name: name.trim_matches('`').to_owned(),
                edit,
            });
        }
        if title == "Implement missing members" || title == "Implement default members" {
            return Ok(Completion::TraitMembers {
                edits: action.edits.clone(),
            });
        }
    }
    if actions
        .iter()
        .any(|a| a.edits.iter().any(|e| generated(&e.path)))
    {
        return Err(issue(
            codes::EDITS_GENERATED,
            "rust-analyzer würde eine generierte Datei ändern",
        ));
    }
    Err(issue(
        codes::NOTHING,
        "hier gibt es nichts zu vervollständigen",
    ))
}

/// rust-analyzer's generated method made fit for Freya TDE: `&mut self`,
/// no `-> _` (that does not compile), and then an empty body.
pub fn tidy_method(text: &str) -> String {
    let mut text = text.replacen("(&self", "(&mut self", 1);
    let Some(at) = text.find(" -> _ {") else {
        return text;
    };
    text.replace_range(at..at + " -> _".len(), "");
    // Leerer Rumpf statt todo!() (ohne Rückgabetyp gibt es nichts zu liefern).
    let open = at + text[at..].find('{').unwrap_or(0);
    if let Some(close) = text.rfind('}').filter(|&c| c > open) {
        let line_start = text[..close].rfind('\n').map_or(0, |i| i + 1);
        let indent = text[line_start..close].to_owned();
        if indent.chars().all(|c| c == ' ') {
            text.replace_range(open + 1..close, &format!("\n{indent}"));
        }
    }
    text
}

/// Where the new method goes. `ra` is rust-analyzer's edit (it inserts after
/// the calling method); for the other orders the method is moved within the
/// `impl` block around the cursor.
fn place_method(
    source: &str,
    ra: &TextEdit,
    name: &str,
    text: &str,
    order: MethodOrder,
) -> TextEdit {
    let at = |line: usize, column: usize| TextEdit {
        path: ra.path.clone(),
        line,
        column,
        end_line: line,
        end_column: column,
        new_text: text.to_owned(),
    };
    let fallback = at(ra.line, ra.column);
    if order == MethodOrder::AfterCaller {
        return fallback;
    }
    let Ok(file) = syn::parse_file(source) else {
        return fallback;
    };
    let name = name.trim_matches('`');
    // Der impl-Block, hinter dessen Methode rust-analyzer einfügt.
    let Some(block) = file.items.iter().find_map(|item| match item {
        syn::Item::Impl(block) if block.trait_.is_none() => {
            let span = syn::spanned::Spanned::span(block);
            (span.start().line <= ra.line && ra.line <= span.end().line).then_some(block)
        }
        _ => None,
    }) else {
        return fallback;
    };
    let methods: Vec<(String, usize, usize)> = block
        .items
        .iter()
        .filter_map(|item| match item {
            syn::ImplItem::Fn(f) => {
                let span = syn::spanned::Spanned::span(f);
                Some((f.sig.ident.to_string(), span.start().line, span.end().line))
            }
            _ => None,
        })
        .collect();
    let line_end = |line: usize| {
        source
            .lines()
            .nth(line - 1)
            .map_or(0, |l| l.chars().count())
    };
    let after = match order {
        MethodOrder::Alphabetical => methods
            .iter()
            .filter(|(n, _, _)| n.as_str() < name)
            .max_by(|a, b| a.0.cmp(&b.0)),
        _ => methods.last(),
    };
    match after {
        Some((_, _, end)) => at(*end, line_end(*end)),
        None => {
            // Alphabetisch vor allen anderen: gleich hinter die öffnende Klammer.
            let open = block.brace_token.span.open().end();
            TextEdit {
                new_text: format!("\n{}\n", text.trim_start_matches('\n')),
                ..at(open.line, open.column)
            }
        }
    }
}

/// The field `self.<name>` under the cursor (`line` 1-based, `column`
/// 0-based in characters) with a suggested type.
pub fn field_at(source: &str, line: usize, column: usize) -> Option<(String, String)> {
    let text = source.lines().nth(line.checked_sub(1)?)?;
    let chars: Vec<char> = text.chars().collect();
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    let mut start = column.min(chars.len());
    while start > 0 && ident(chars[start - 1]) {
        start -= 1;
    }
    let mut end = column.min(chars.len());
    while end < chars.len() && ident(chars[end]) {
        end += 1;
    }
    if start == end {
        return None;
    }
    let before: String = chars[..start].iter().collect();
    if !before.trim_end().ends_with("self.") {
        return None;
    }
    let name: String = chars[start..end].iter().collect();
    // Ein Methodenaufruf ist kein Feld.
    let rest: String = chars[end..].iter().collect();
    if rest.trim_start().starts_with('(') || name.chars().next()?.is_ascii_digit() {
        return None;
    }
    Some((name, suggest_type(&rest)))
}

/// A type for a new field from what follows `self.<name>` on its line.
pub fn suggest_type(rest: &str) -> String {
    let rest = rest.trim();
    let value = rest
        .strip_prefix("+=")
        .or_else(|| rest.strip_prefix("-="))
        .or_else(|| rest.strip_prefix("*="))
        .or_else(|| rest.strip_prefix('=').filter(|r| !r.starts_with('=')))
        .map(|v| v.trim().trim_end_matches(';').trim());
    if let Some(value) = value {
        return literal_type(value);
    }
    if let Some(arg) = rest.strip_prefix(".push(") {
        return format!(
            "Vec<{}>",
            literal_type(arg.trim_end_matches([')', ';', ' ']))
        );
    }
    if rest.starts_with(".insert(") {
        return "std::collections::HashMap<String, String>".to_owned();
    }
    if rest.starts_with(".push_str(") {
        return "String".to_owned();
    }
    "i32".to_owned()
}

fn literal_type(value: &str) -> String {
    let value = value.trim();
    if value == "true" || value == "false" {
        "bool"
    } else if value.starts_with('"')
        || value.ends_with(".to_string()")
        || value.ends_with(".to_owned()")
        || value.starts_with("format!")
        || value.starts_with("String::")
    {
        "String"
    } else if value.starts_with('\'') {
        "char"
    } else if value.parse::<i64>().is_ok() {
        "i32"
    } else if value.parse::<f64>().is_ok() {
        "f64"
    } else if value.starts_with("vec!") || value.starts_with("Vec::") {
        "Vec<String>"
    } else {
        "i32"
    }
    .to_owned()
}

/// Applies `edits` (all for one file) to `source`.
pub fn apply(source: &str, edits: &[TextEdit]) -> String {
    // Von hinten nach vorn, damit frühere Stellen gültig bleiben.
    let mut edits: Vec<&TextEdit> = edits.iter().collect();
    edits.sort_by_key(|e| std::cmp::Reverse((e.line, e.column)));
    let mut text = source.to_owned();
    for edit in edits {
        let (Some(start), Some(end)) = (
            offset(&text, edit.line, edit.column),
            offset(&text, edit.end_line, edit.end_column),
        ) else {
            continue;
        };
        text.replace_range(start..end.max(start), &edit.new_text);
    }
    text
}

/// Byte offset of `line:column` (1-based line, character column).
fn offset(text: &str, line: usize, column: usize) -> Option<usize> {
    let mut start = 0;
    for (i, l) in text.split_inclusive('\n').enumerate() {
        if i + 1 == line {
            let content = l.trim_end_matches(['\n', '\r']);
            let byte = content
                .char_indices()
                .nth(column)
                .map_or(content.len(), |(b, _)| b);
            return Some(start + byte);
        }
        start += l.len();
    }
    (line == text.split_inclusive('\n').count() + 1).then_some(text.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "include!(\"form.gen.rs\");

impl MainForm {
    pub fn ok_clicked(&mut self) {
        self.speichern(3);
        self.zaehler += 1;
    }

    pub fn zeta(&mut self) {}
}
";

    fn path() -> PathBuf {
        PathBuf::from("/p/src/main_form.rs")
    }

    fn ra_method() -> Action {
        Action {
            title: "Generate speichern method".into(),
            kind: None,
            edits: vec![TextEdit {
                path: path(),
                line: 7,
                column: 5,
                end_line: 7,
                end_column: 5,
                new_text: "\n\n    fn speichern(&self, arg: i32) -> _ {\n        todo!()\n    }"
                    .into(),
            }],
        }
    }

    #[test]
    fn method_after_caller_alphabetical_or_at_the_end() {
        let completion = plan(
            &path(),
            SOURCE,
            5,
            14,
            &[ra_method()],
            MethodOrder::AfterCaller,
        )
        .unwrap();
        let Completion::Method { name, edit } = completion else {
            panic!("{completion:?}")
        };
        assert_eq!(name, "speichern");
        let result = apply(SOURCE, &[edit]);
        assert!(
            result.contains(
                "    }\n\n    fn speichern(&mut self, arg: i32) {\n    }\n\n    pub fn zeta"
            ),
            "{result}"
        );
        // Alphabetisch: zwischen ok_clicked und zeta – hier dieselbe Stelle; am Ende: hinter zeta.
        let Completion::Method { edit, .. } =
            plan(&path(), SOURCE, 5, 14, &[ra_method()], MethodOrder::End).unwrap()
        else {
            panic!()
        };
        let result = apply(SOURCE, &[edit]);
        assert!(
            result.contains("pub fn zeta(&mut self) {}\n\n    fn speichern"),
            "{result}"
        );
        let Completion::Method { edit, .. } = plan(
            &path(),
            SOURCE,
            5,
            14,
            &[ra_method()],
            MethodOrder::Alphabetical,
        )
        .unwrap() else {
            panic!()
        };
        let result = apply(SOURCE, &[edit]);
        assert!(syn::parse_file(&result).is_ok(), "{result}");
        let ok = result.find("fn ok_clicked").unwrap();
        let new = result.find("fn speichern").unwrap();
        let zeta = result.find("fn zeta").unwrap();
        assert!(ok < new && new < zeta, "{result}");
    }

    #[test]
    fn fields_go_to_the_form_and_generated_files_are_refused() {
        let add_field = Action {
            title: "Add field to Record Struct".into(),
            kind: Some("quickfix".into()),
            edits: vec![TextEdit {
                path: PathBuf::from("/p/src/main_form.gen.rs"),
                line: 2,
                column: 0,
                end_line: 2,
                end_column: 0,
                new_text: "zaehler: &mut _".into(),
            }],
        };
        assert_eq!(
            plan(
                &path(),
                SOURCE,
                6,
                15,
                std::slice::from_ref(&add_field),
                MethodOrder::AfterCaller
            )
            .unwrap(),
            Completion::Field {
                name: "zaehler".into(),
                rust_type: "i32".into()
            }
        );
        // Nur eine Änderung der generierten Datei, ohne Feld unter dem Cursor.
        assert_eq!(
            plan(
                &path(),
                SOURCE,
                5,
                14,
                &[add_field],
                MethodOrder::AfterCaller
            )
            .unwrap_err()
            .code,
            codes::EDITS_GENERATED
        );
        assert_eq!(
            plan(
                Path::new("/p/src/x.gen.rs"),
                SOURCE,
                5,
                14,
                &[],
                MethodOrder::AfterCaller
            )
            .unwrap_err()
            .code,
            codes::GENERATED_FILE
        );
        assert_eq!(
            plan(&path(), SOURCE, 5, 14, &[], MethodOrder::AfterCaller)
                .unwrap_err()
                .code,
            codes::NOTHING
        );
    }

    #[test]
    fn suggested_types() {
        for (rest, ty) in [
            (" += 1;", "i32"),
            (" = 2.5;", "f64"),
            (" = \"x\".to_string();", "String"),
            (" = true;", "bool"),
            (".push(\"a\");", "Vec<String>"),
            (".push(3);", "Vec<i32>"),
            (
                ".insert(k, v);",
                "std::collections::HashMap<String, String>",
            ),
            (" == 3", "i32"),
        ] {
            assert_eq!(suggest_type(rest), ty, "{rest}");
        }
        assert_eq!(field_at("self.go();", 1, 6), None);
        assert_eq!(field_at("other.x = 1;", 1, 6), None);
    }
}
