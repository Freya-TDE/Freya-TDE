//! Identifiers: component names and event handler names.
//!
//! Both end up in generated Rust code (struct fields, functions), so they must
//! be valid Rust identifiers. They are validated on construction and on
//! deserialization – an invalid name can never enter the model.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};

use crate::error::ModelError;

/// Rust keywords that cannot be used as identifiers (2024 edition, including
/// reserved words).
const KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "gen", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut",
    "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while", "abstract", "become", "box", "do", "final", "macro",
    "override", "priv", "try", "typeof", "unsized", "virtual", "yield",
];

/// Is `text` a valid (non-raw) Rust identifier that is not a keyword?
pub fn is_rust_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
        && text != "_"
        && !KEYWORDS.contains(&text)
}

macro_rules! identifier_type {
    ($(#[$doc:meta])* $name:ident, $what:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Validates and creates the identifier.
            pub fn new(text: impl Into<String>) -> Result<Self, ModelError> {
                let text = text.into();
                if is_rust_identifier(&text) {
                    Ok(Self(text))
                } else {
                    Err(ModelError::InvalidIdentifier { kind: $what, text })
                }
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<&str> for $name {
            type Error = ModelError;

            fn try_from(text: &str) -> Result<Self, Self::Error> {
                Self::new(text)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let text = String::deserialize(deserializer)?;
                Self::new(text).map_err(serde::de::Error::custom)
            }
        }
    };
}

identifier_type!(
    /// The name of a component or widget, unique within a form (e.g. `button1`,
    /// `db_pool`). It becomes a field of the generated form struct.
    ComponentName,
    "component name"
);

identifier_type!(
    /// The name of the user function that handles an event (e.g.
    /// `button1_on_click`). The code generator creates a stub with this name.
    HandlerName,
    "handler name"
);

/// A reference to a component, possibly on another form:
///
/// | Text | Meaning |
/// |---|---|
/// | `db_pool` | `db_pool` on the same form |
/// | `services::db_pool` | `db_pool` on the form (or data module) `services` |
///
/// ```
/// use tdr_core::ComponentPath;
///
/// let p: ComponentPath = "services::db_pool".parse().unwrap();
/// assert_eq!(p.form.as_ref().unwrap().as_str(), "services");
/// assert_eq!(p.to_string(), "services::db_pool");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentPath {
    /// `None` = the form the reference is written on.
    pub form: Option<ComponentName>,
    pub name: ComponentName,
}

impl ComponentPath {
    /// A reference within the same form.
    pub fn local(name: ComponentName) -> Self {
        Self { form: None, name }
    }

    /// A reference to a component on `form`.
    pub fn on(form: ComponentName, name: ComponentName) -> Self {
        Self {
            form: Some(form),
            name,
        }
    }

    /// The form this path points to, seen from `current` (the form the
    /// reference is written on).
    pub fn target_form<'a>(&'a self, current: &'a ComponentName) -> &'a ComponentName {
        self.form.as_ref().unwrap_or(current)
    }

    /// Does this path, written on form `current`, point to `name` on `form`?
    pub fn points_to(
        &self,
        current: &ComponentName,
        form: &ComponentName,
        name: &ComponentName,
    ) -> bool {
        &self.name == name && self.target_form(current) == form
    }

    /// The fully qualified path, seen from `current`.
    pub fn qualified(&self, current: &ComponentName) -> ComponentPath {
        ComponentPath::on(self.target_form(current).clone(), self.name.clone())
    }
}

impl From<ComponentName> for ComponentPath {
    fn from(name: ComponentName) -> Self {
        Self::local(name)
    }
}

impl fmt::Display for ComponentPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.form {
            Some(form) => write!(f, "{form}::{}", self.name),
            None => write!(f, "{}", self.name),
        }
    }
}

impl std::str::FromStr for ComponentPath {
    type Err = ModelError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text.split_once("::") {
            Some((form, name)) => Ok(Self::on(
                ComponentName::new(form)?,
                ComponentName::new(name)?,
            )),
            None => Ok(Self::local(ComponentName::new(text)?)),
        }
    }
}

impl Serialize for ComponentPath {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ComponentPath {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers() {
        assert!(is_rust_identifier("button1"));
        assert!(is_rust_identifier("_hidden"));
        assert!(is_rust_identifier("DbPool"));
        assert!(!is_rust_identifier(""));
        assert!(!is_rust_identifier("_"));
        assert!(!is_rust_identifier("1button"));
        assert!(!is_rust_identifier("my-button"));
        assert!(!is_rust_identifier("Größe"));
        assert!(!is_rust_identifier("fn"));
        assert!(!is_rust_identifier("gen"));
    }

    #[test]
    fn paths() {
        let local: ComponentPath = "db_pool".parse().unwrap();
        assert_eq!(local.form, None);
        let main = ComponentName::new("main").unwrap();
        let services = ComponentName::new("services").unwrap();
        let pool = ComponentName::new("db_pool").unwrap();
        assert!(local.points_to(&main, &main, &pool));
        assert!(!local.points_to(&main, &services, &pool));
        let remote: ComponentPath = "services::db_pool".parse().unwrap();
        assert!(remote.points_to(&main, &services, &pool));
        assert_eq!(remote.qualified(&main), remote);
        assert_eq!(local.qualified(&main).to_string(), "main::db_pool");
        assert!("a::b::c".parse::<ComponentPath>().is_err());
        assert!("::x".parse::<ComponentPath>().is_err());
    }

    #[test]
    fn validated_on_deserialize() {
        let ok: ComponentName = ron::from_str("\"button1\"").unwrap();
        assert_eq!(ok.as_str(), "button1");
        assert!(ron::from_str::<ComponentName>("\"1x\"").is_err());
        assert!(ComponentName::new("match").is_err());
        assert!(HandlerName::new("on_click").is_ok());
    }
}
