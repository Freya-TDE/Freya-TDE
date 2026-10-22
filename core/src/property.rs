//! Properties: definitions on component types ([`PropertyDef`]) and values on
//! instances ([`PropertyValue`], [`Properties`]).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::platform::Platform;
use crate::value::{PropertyType, Value};

/// A property declared by a component type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropertyDef {
    /// Name in snake_case, e.g. `caption`, `max_connections`.
    pub name: String,
    pub ty: PropertyType,
    /// Value when the instance does not set one. Without a default the
    /// property is *required* (see [`PropertyDef::is_required`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    /// Only when the project uses this UI backend (backend-specific extra
    /// property, e.g. egui's `sense` on a core button). `None` = every backend.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    /// Only on these platforms (e.g. a Windows-only `taskbar_progress`).
    /// Empty = every platform. Independent of `backend`: a backend may add a
    /// property that exists on one platform only.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub platforms: BTreeSet<Platform>,
    /// Grouping in the object inspector (e.g. `"Appearance"`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub category: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

impl PropertyDef {
    /// A common property with a default value.
    pub fn new(name: impl Into<String>, ty: PropertyType, default: impl Into<Value>) -> Self {
        Self {
            name: name.into(),
            ty,
            default: Some(default.into()),
            backend: None,
            platforms: BTreeSet::new(),
            category: String::new(),
            description: String::new(),
        }
    }

    /// A common property the user must set (no default).
    pub fn required(name: impl Into<String>, ty: PropertyType) -> Self {
        Self {
            name: name.into(),
            ty,
            default: None,
            backend: None,
            platforms: BTreeSet::new(),
            category: String::new(),
            description: String::new(),
        }
    }

    /// Restricts the property to one UI backend.
    pub fn for_backend(mut self, backend: impl Into<String>) -> Self {
        self.backend = Some(backend.into());
        self
    }

    /// Restricts the property to some platforms.
    pub fn for_platforms(mut self, platforms: impl IntoIterator<Item = Platform>) -> Self {
        self.platforms.extend(platforms);
        self
    }

    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = category.into();
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Must the instance set this property?
    pub fn is_required(&self) -> bool {
        self.default.is_none()
    }

    /// May this property carry a value for `platform`?
    pub fn allows_platform(&self, platform: &Platform) -> bool {
        self.platforms.is_empty() || self.platforms.contains(platform)
    }
}

/// The value of a property on an instance: a base value plus optional
/// per-platform overrides (turned into `cfg` branches by the code generator).
///
/// In `.ftdr` files a value without overrides is written plainly
/// (`Text("OK")`), one with overrides as
/// `(value: Text("OK"), platforms: {MacOs: Text("Okay")})`.
#[derive(Debug, Clone, PartialEq)]
pub struct PropertyValue {
    pub value: Value,
    pub platforms: BTreeMap<Platform, Value>,
}

impl PropertyValue {
    pub fn new(value: impl Into<Value>) -> Self {
        Self {
            value: value.into(),
            platforms: BTreeMap::new(),
        }
    }

    /// Adds an override for `platform`.
    pub fn on(mut self, platform: Platform, value: impl Into<Value>) -> Self {
        self.platforms.insert(platform, value.into());
        self
    }

    /// The value that applies on `platform`.
    pub fn for_platform(&self, platform: &Platform) -> &Value {
        self.platforms.get(platform).unwrap_or(&self.value)
    }

    /// Base value and all overrides.
    pub fn all_values(&self) -> impl Iterator<Item = &Value> {
        std::iter::once(&self.value).chain(self.platforms.values())
    }

    pub(crate) fn all_values_mut(&mut self) -> impl Iterator<Item = &mut Value> {
        std::iter::once(&mut self.value).chain(self.platforms.values_mut())
    }
}

impl<T: Into<Value>> From<T> for PropertyValue {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum PropertyValueRepr {
    Plain(Value),
    WithPlatforms {
        value: Value,
        platforms: BTreeMap<Platform, Value>,
    },
}

impl Serialize for PropertyValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.platforms.is_empty() {
            PropertyValueRepr::Plain(self.value.clone()).serialize(serializer)
        } else {
            PropertyValueRepr::WithPlatforms {
                value: self.value.clone(),
                platforms: self.platforms.clone(),
            }
            .serialize(serializer)
        }
    }
}

impl<'de> Deserialize<'de> for PropertyValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match PropertyValueRepr::deserialize(deserializer)? {
            PropertyValueRepr::Plain(value) => Self::new(value),
            PropertyValueRepr::WithPlatforms { value, platforms } => Self { value, platforms },
        })
    }
}

/// The properties set on an instance, by name. Unset properties use the
/// default of their definition, so only deviations are stored.
pub type Properties = BTreeMap<String, PropertyValue>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_overrides() {
        let v = PropertyValue::new("OK").on(Platform::MacOs, "Okay");
        assert_eq!(
            v.for_platform(&Platform::MacOs),
            &Value::Text("Okay".into())
        );
        assert_eq!(v.for_platform(&Platform::Linux), &Value::Text("OK".into()));
        assert_eq!(v.all_values().count(), 2);
    }

    #[test]
    fn compact_ron() {
        let plain = PropertyValue::new("OK");
        assert_eq!(ron::to_string(&plain).unwrap(), r#"Text("OK")"#);
        let with = PropertyValue::new("OK").on(Platform::MacOs, "Okay");
        let text = ron::to_string(&with).unwrap();
        assert_eq!(text, r#"(value:Text("OK"),platforms:{MacOs:Text("Okay")})"#);
        assert_eq!(ron::from_str::<PropertyValue>(&text).unwrap(), with);
        assert_eq!(
            ron::from_str::<PropertyValue>(r#"Text("OK")"#).unwrap(),
            plain
        );
    }

    #[test]
    fn scopes() {
        let def = PropertyDef::new("taskbar", PropertyType::Bool, false)
            .for_platforms([Platform::Windows]);
        assert!(def.allows_platform(&Platform::Windows));
        assert!(!def.allows_platform(&Platform::Linux));
        assert!(PropertyDef::required("url", PropertyType::Text).is_required());
    }
}
