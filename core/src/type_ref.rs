//! [`TypeRef`]: which component type an instance belongs to, and where that
//! type comes from (the core, a UI backend or a component pack).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::ModelError;

/// Where a component type is defined.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Origin {
    /// The small common widget core shipped with Freya-TD (Label, Button …).
    Core,
    /// A UI backend's own palette (e.g. `egui`, `slint`, `win32`).
    Backend(String),
    /// A non-visual component pack (e.g. `sqlx`, `axum`, `tokio`).
    Pack(String),
}

/// A reference to a component type, written as text:
///
/// | Text | Meaning |
/// |---|---|
/// | `core/Button` | Button from the common widget core |
/// | `backend:egui/Slider` | Slider from the egui backend palette |
/// | `pack:sqlx/Pool` | Pool from the sqlx component pack |
///
/// ```
/// use tdr_core::{Origin, TypeRef};
///
/// let t: TypeRef = "pack:sqlx/Pool".parse().unwrap();
/// assert_eq!(t.origin, Origin::Pack("sqlx".into()));
/// assert_eq!(t.to_string(), "pack:sqlx/Pool");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeRef {
    pub origin: Origin,
    /// Type name within its origin, e.g. `Button`.
    pub name: String,
}

impl TypeRef {
    pub fn core(name: impl Into<String>) -> Self {
        Self {
            origin: Origin::Core,
            name: name.into(),
        }
    }

    pub fn backend(backend: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            origin: Origin::Backend(backend.into()),
            name: name.into(),
        }
    }

    pub fn pack(pack: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            origin: Origin::Pack(pack.into()),
            name: name.into(),
        }
    }
}

fn valid_segment(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

impl FromStr for TypeRef {
    type Err = ModelError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let invalid = || ModelError::InvalidTypeRef(text.to_owned());
        let (origin, name) = text.split_once('/').ok_or_else(invalid)?;
        let origin = match origin.split_once(':') {
            None if origin == "core" => Origin::Core,
            Some(("backend", id)) if valid_segment(id) => Origin::Backend(id.to_owned()),
            Some(("pack", id)) if valid_segment(id) => Origin::Pack(id.to_owned()),
            _ => return Err(invalid()),
        };
        if !valid_segment(name) {
            return Err(invalid());
        }
        Ok(Self {
            origin,
            name: name.to_owned(),
        })
    }
}

impl fmt::Display for TypeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.origin {
            Origin::Core => write!(f, "core/{}", self.name),
            Origin::Backend(b) => write!(f, "backend:{b}/{}", self.name),
            Origin::Pack(p) => write!(f, "pack:{p}/{}", self.name),
        }
    }
}

// Stored as compact text in `.ftdr` files.
impl Serialize for TypeRef {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for TypeRef {
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
    fn round_trip() {
        for text in [
            "core/Button",
            "backend:egui/Slider",
            "pack:sqlx/Pool",
            "pack:my-pack/Query_2",
        ] {
            let t: TypeRef = text.parse().unwrap();
            assert_eq!(t.to_string(), text);
        }
    }

    #[test]
    fn invalid() {
        for text in [
            "Button",
            "core/",
            "core:x/Button",
            "lib:x/Y",
            "pack:/Pool",
            "pack:sqlx/Po ol",
            "backend:egui",
        ] {
            assert!(text.parse::<TypeRef>().is_err(), "{text}");
        }
    }
}
