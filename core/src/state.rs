//! Own fields of a form (like the private fields of a Delphi `TForm`): the
//! property `state` of `core/Form`, one entry per field – `name: Type` or
//! `name: Type = expression`. The generator adds them to the form's struct
//! and initialises them with the expression or `Default::default()`; the
//! code completion adds a field here when the code uses one that is missing.
//!
//! ```
//! use tdr_core::state::parse_state;
//!
//! let fields = parse_state(&["zaehler: i32".into(), "titel: String = \"Neu\".into()".into()]).unwrap();
//! assert_eq!(fields[0].rust_type, "i32");
//! assert_eq!(fields[1].init.as_deref(), Some("\"Neu\".into()"));
//! assert!(parse_state(&["zaehler".into()]).is_err());
//! ```

use serde::Serialize;

/// The property of `core/Form` with the own fields.
pub const STATE: &str = "state";

/// One own field of a form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StateField {
    pub name: String,
    /// Rust type, e.g. `i32`, `Vec<String>`.
    pub rust_type: String,
    /// Start value (a Rust expression); `None` = `Default::default()`.
    pub init: Option<String>,
}

impl StateField {
    /// The entry as written in `state`.
    pub fn entry(&self) -> String {
        match &self.init {
            Some(init) => format!("{}: {} = {init}", self.name, self.rust_type),
            None => format!("{}: {}", self.name, self.rust_type),
        }
    }
}

/// Parses `name: Type` / `name: Type = expression` entries (empty ones are
/// skipped).
pub fn parse_state(items: &[String]) -> Result<Vec<StateField>, String> {
    let mut fields: Vec<StateField> = Vec::new();
    for item in items.iter().filter(|i| !i.trim().is_empty()) {
        let (name, rest) = item
            .split_once(':')
            .ok_or_else(|| format!("field {item:?} needs a type: `name: Type`"))?;
        let name = name.trim();
        if !crate::name::is_rust_identifier(name) {
            return Err(format!("field {item:?}: `{name}` is not a Rust name"));
        }
        let (ty, init) = match split_init(rest) {
            Some((ty, init)) => (ty.trim(), Some(init.trim().to_owned())),
            None => (rest.trim(), None),
        };
        if ty.is_empty() || syn::parse_str::<syn::Type>(ty).is_err() {
            return Err(format!("field {name}: {ty:?} is not a Rust type"));
        }
        if let Some(init) = &init
            && syn::parse_str::<syn::Expr>(init).is_err()
        {
            return Err(format!("field {name}: {init:?} is not a Rust expression"));
        }
        if fields.iter().any(|f| f.name == name) {
            return Err(format!("field {name} is given twice"));
        }
        fields.push(StateField {
            name: name.to_owned(),
            rust_type: ty.to_owned(),
            init,
        });
    }
    Ok(fields)
}

/// Splits `Type = expression` at the first `=` outside of `<…>`, `(…)`,
/// `[…]` (types like `HashMap<K, V>` have no `=`, but be safe).
fn split_init(text: &str) -> Option<(&str, &str)> {
    let mut depth = 0i32;
    for (i, c) in text.char_indices() {
        match c {
            '<' | '(' | '[' => depth += 1,
            '>' | ')' | ']' => depth -= 1,
            '=' if depth == 0 && !text[i + 1..].starts_with('=') => {
                return Some((&text[..i], &text[i + 1..]));
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries() {
        let fields = parse_state(&[
            "kunden: std::collections::HashMap<i64, String>".into(),
            "  ".into(),
            "faktor: f64 = 1.5".into(),
        ])
        .unwrap();
        assert_eq!(fields.len(), 2);
        assert_eq!(
            fields[0].rust_type,
            "std::collections::HashMap<i64, String>"
        );
        assert_eq!(fields[1].entry(), "faktor: f64 = 1.5");
        for bad in ["1x: i32", "x: ", "x: i32 = +", "x: i32\nx: u8"] {
            let items: Vec<String> = bad.split('\n').map(str::to_owned).collect();
            assert!(parse_state(&items).is_err(), "{bad}");
        }
    }
}
