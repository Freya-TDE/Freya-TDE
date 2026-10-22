//! Provides/Requires: what a component offers and what it needs.
//!
//! Wiring components together is the tool's job, the logic stays with the
//! developer. A component type declares *capabilities* it provides (e.g. a
//! SQLx pool provides `sqlx.pool`) and requirements (a query requires
//! `sqlx.pool`). Requirements are fulfilled by reference properties
//! ([`Require::via`]); the dependency graph (next crate) resolves them,
//! detects cycles and derives the initialisation order.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::ModelError;

/// A capability name: lowercase segments separated by dots, e.g.
/// `sqlx.pool`, `http.router`, `tokio.runtime`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Capability(String);

impl Capability {
    pub fn new(text: impl Into<String>) -> Result<Self, ModelError> {
        let text = text.into();
        let valid = text.split('.').count() >= 2
            && text.split('.').all(|segment| {
                !segment.is_empty()
                    && segment
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            });
        if valid {
            Ok(Self(text))
        } else {
            Err(ModelError::InvalidCapability(text))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Capability {
    type Err = ModelError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::new(text)
    }
}

impl Serialize for Capability {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Capability {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// Something a component type offers to others.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provide {
    pub capability: Capability,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// Mandatory or optional.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RequireKind {
    /// Without it the component cannot work – a design-time error.
    #[default]
    Mandatory,
    /// Used when present – no error if missing.
    Optional,
}

/// When a requirement applies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Condition {
    /// Always.
    #[default]
    Always,
    /// Only if the named property of this component is set (non-empty).
    PropertySet(String),
    /// Only if a component providing this capability references this
    /// component – e.g. an HTTP router needs a pool only if a query component
    /// is attached to it.
    ReferencedBy(Capability),
    /// Only if the project contains any provider of this capability.
    ProjectProvides(Capability),
}

/// Something a component type needs from others.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Require {
    pub capability: Capability,
    #[serde(default)]
    pub kind: RequireKind,
    /// The reference property that connects the provider (e.g. `pool`).
    /// `None` = provided implicitly by the project (e.g. a runtime).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
    #[serde(default, skip_serializing_if = "is_always")]
    pub condition: Condition,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

fn is_always(condition: &Condition) -> bool {
    *condition == Condition::Always
}

impl Require {
    /// A mandatory, unconditional requirement fulfilled via `property`.
    pub fn via(capability: Capability, property: impl Into<String>) -> Self {
        Self {
            capability,
            kind: RequireKind::Mandatory,
            via: Some(property.into()),
            condition: Condition::Always,
            description: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_names() {
        assert!(Capability::new("sqlx.pool").is_ok());
        assert!(Capability::new("http.router.v2").is_ok());
        assert!(Capability::new("pool").is_err());
        assert!(Capability::new("Sqlx.Pool").is_err());
        assert!(Capability::new("sqlx..pool").is_err());
        assert!(Capability::new("sqlx.po-ol").is_err());
    }

    #[test]
    fn require_ron_is_compact() {
        let r = Require::via(Capability::new("sqlx.pool").unwrap(), "pool");
        let text = ron::to_string(&r).unwrap();
        assert_eq!(
            text,
            r#"(capability:"sqlx.pool",kind:Mandatory,via:Some("pool"))"#
        );
        assert_eq!(ron::from_str::<Require>(&text).unwrap(), r);
    }
}
