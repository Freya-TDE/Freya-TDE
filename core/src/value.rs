//! The property type system: [`PropertyType`] describes what a property
//! accepts, [`Value`] is what an instance stores.
//!
//! The types are rich enough for real UI backends (colors, geometry, lists,
//! references, code expressions) and open for backend-specific types via
//! [`PropertyType::Custom`] / [`Value::Custom`] – so the model does not shrink
//! to the lowest common denominator.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::capability::Capability;
use crate::name::{ComponentName, ComponentPath};

/// An RGBA color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    #[serde(default = "opaque")]
    pub a: u8,
}

fn opaque() -> u8 {
    255
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

/// A point in logical pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// A size in logical pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Size {
    pub width: i32,
    pub height: i32,
}

/// What a property accepts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PropertyType {
    Bool,
    /// Integer, optionally bounded (inclusive).
    Int {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<i64>,
    },
    /// Floating point number, optionally bounded (inclusive).
    Float {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<f64>,
    },
    Text,
    /// One of a fixed set of variants (e.g. `["Left", "Center", "Right"]`).
    Enum(Vec<String>),
    Color,
    Point,
    Size,
    /// A homogeneous list (e.g. the items of a combo box).
    List(Box<PropertyType>),
    /// A reference to another component of the same project. With a
    /// capability, the target must provide it (e.g. `sqlx.pool`).
    Reference(Option<Capability>),
    /// A Rust expression copied verbatim into the generated code.
    Code,
    /// A backend- or pack-specific type, identified by name (e.g.
    /// `"egui.Stroke"`). Its value is stored as opaque text.
    Custom(String),
}

/// A property value as stored on a component instance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    Enum(String),
    Color(Color),
    Point(Point),
    Size(Size),
    List(Vec<Value>),
    /// `None` = not connected yet. Written as `"db_pool"` (same form) or
    /// `"services::db_pool"` (other form or data module).
    Reference(Option<ComponentPath>),
    Code(String),
    Custom {
        type_name: String,
        data: String,
    },
}

/// Why a value does not fit a property type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum Mismatch {
    /// Wrong kind of value, e.g. text for a bool property.
    Kind { expected: String, found: String },
    OutOfRange {
        value: String,
        min: Option<String>,
        max: Option<String>,
    },
    UnknownVariant {
        value: String,
        variants: Vec<String>,
    },
    /// Custom value for a different custom type.
    CustomType { expected: String, found: String },
    /// An element of a list does not fit; carries the index.
    ListItem(usize, Box<Mismatch>),
}

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Mismatch::Kind { expected, found } => write!(f, "expected {expected}, found {found}"),
            Mismatch::OutOfRange { value, min, max } => write!(
                f,
                "{value} is out of range ({}..={})",
                min.as_deref().unwrap_or(""),
                max.as_deref().unwrap_or("")
            ),
            Mismatch::UnknownVariant { value, variants } => {
                write!(f, "unknown variant {value:?}, expected one of {variants:?}")
            }
            Mismatch::CustomType { expected, found } => {
                write!(f, "custom type {found:?} given, {expected:?} expected")
            }
            Mismatch::ListItem(i, inner) => write!(f, "list item {i}: {inner}"),
        }
    }
}

impl PropertyType {
    /// Short name for messages.
    pub fn kind_name(&self) -> String {
        match self {
            PropertyType::Bool => "bool".into(),
            PropertyType::Int { .. } => "int".into(),
            PropertyType::Float { .. } => "float".into(),
            PropertyType::Text => "text".into(),
            PropertyType::Enum(_) => "enum".into(),
            PropertyType::Color => "color".into(),
            PropertyType::Point => "point".into(),
            PropertyType::Size => "size".into(),
            PropertyType::List(inner) => format!("list of {}", inner.kind_name()),
            PropertyType::Reference(_) => "reference".into(),
            PropertyType::Code => "code".into(),
            PropertyType::Custom(name) => format!("custom {name}"),
        }
    }
}

impl Value {
    /// Short name for messages.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Value::Bool(_) => "bool",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Text(_) => "text",
            Value::Enum(_) => "enum",
            Value::Color(_) => "color",
            Value::Point(_) => "point",
            Value::Size(_) => "size",
            Value::List(_) => "list",
            Value::Reference(_) => "reference",
            Value::Code(_) => "code",
            Value::Custom { .. } => "custom",
        }
    }

    /// The value as text for conditions (`when` of Cargo dependencies):
    /// enum variant, text, `true`/`false`, number. `None` for other kinds.
    pub fn as_condition_text(&self) -> Option<String> {
        match self {
            Value::Enum(v) | Value::Text(v) => Some(v.clone()),
            Value::Bool(b) => Some(b.to_string()),
            Value::Int(i) => Some(i.to_string()),
            Value::Float(f) => Some(f.to_string()),
            _ => None,
        }
    }

    /// Does the value fit `ty`? References are only checked for their kind
    /// here – whether the target exists and provides the capability is
    /// checked by form validation, which knows the other components.
    pub fn check(&self, ty: &PropertyType) -> Result<(), Mismatch> {
        let kind = || Mismatch::Kind {
            expected: ty.kind_name(),
            found: self.kind_name().to_owned(),
        };
        match (ty, self) {
            (PropertyType::Bool, Value::Bool(_))
            | (PropertyType::Text, Value::Text(_))
            | (PropertyType::Color, Value::Color(_))
            | (PropertyType::Point, Value::Point(_))
            | (PropertyType::Size, Value::Size(_))
            | (PropertyType::Reference(_), Value::Reference(_))
            | (PropertyType::Code, Value::Code(_)) => Ok(()),
            (PropertyType::Int { min, max }, Value::Int(v)) => {
                if min.is_some_and(|m| *v < m) || max.is_some_and(|m| *v > m) {
                    Err(Mismatch::OutOfRange {
                        value: v.to_string(),
                        min: min.map(|m| m.to_string()),
                        max: max.map(|m| m.to_string()),
                    })
                } else {
                    Ok(())
                }
            }
            (PropertyType::Float { min, max }, Value::Float(v)) => {
                if !v.is_finite() || min.is_some_and(|m| *v < m) || max.is_some_and(|m| *v > m) {
                    Err(Mismatch::OutOfRange {
                        value: v.to_string(),
                        min: min.map(|m| m.to_string()),
                        max: max.map(|m| m.to_string()),
                    })
                } else {
                    Ok(())
                }
            }
            (PropertyType::Enum(variants), Value::Enum(v)) => {
                if variants.contains(v) {
                    Ok(())
                } else {
                    Err(Mismatch::UnknownVariant {
                        value: v.clone(),
                        variants: variants.clone(),
                    })
                }
            }
            (PropertyType::List(inner), Value::List(items)) => {
                items.iter().enumerate().try_for_each(|(i, item)| {
                    item.check(inner)
                        .map_err(|e| Mismatch::ListItem(i, Box::new(e)))
                })
            }
            (PropertyType::Custom(expected), Value::Custom { type_name, .. }) => {
                if expected == type_name {
                    Ok(())
                } else {
                    Err(Mismatch::CustomType {
                        expected: expected.clone(),
                        found: type_name.clone(),
                    })
                }
            }
            _ => Err(kind()),
        }
    }

    /// A reference to `name` on the same form.
    pub fn reference(name: ComponentName) -> Self {
        Value::Reference(Some(ComponentPath::local(name)))
    }

    /// A reference to `name` on another form or data module.
    pub fn reference_on(form: ComponentName, name: ComponentName) -> Self {
        Value::Reference(Some(ComponentPath::on(form, name)))
    }

    /// All components this value refers to (also inside lists).
    pub fn references(&self) -> Vec<&ComponentPath> {
        match self {
            Value::Reference(Some(path)) => vec![path],
            Value::List(items) => items.iter().flat_map(Value::references).collect(),
            _ => Vec::new(),
        }
    }

    /// Removes references to `form::gone` from a value written on `current`:
    /// a single reference becomes `None`, list entries are dropped.
    pub(crate) fn clear_reference(
        &mut self,
        current: &ComponentName,
        form: &ComponentName,
        gone: &ComponentName,
    ) {
        let hit =
            |v: &Value| matches!(v, Value::Reference(Some(p)) if p.points_to(current, form, gone));
        match self {
            Value::Reference(target) if hit(&Value::Reference(target.clone())) => *target = None,
            Value::List(items) => {
                items.retain(|v| !hit(v));
                items
                    .iter_mut()
                    .for_each(|v| v.clear_reference(current, form, gone));
            }
            _ => {}
        }
    }

    /// Points references to `form::old`, written on `current`, to `new`
    /// (also inside lists).
    pub(crate) fn rename_reference(
        &mut self,
        current: &ComponentName,
        form: &ComponentName,
        old: &ComponentName,
        new: &ComponentName,
    ) {
        match self {
            Value::Reference(Some(path)) if path.points_to(current, form, old) => {
                path.name = new.clone()
            }
            Value::List(items) => items
                .iter_mut()
                .for_each(|v| v.rename_reference(current, form, old, new)),
            _ => {}
        }
    }

    /// Points qualified references to the form `old` to `new`.
    pub(crate) fn rename_form_reference(&mut self, old: &ComponentName, new: &ComponentName) {
        match self {
            Value::Reference(Some(path)) if path.form.as_ref() == Some(old) => {
                path.form = Some(new.clone())
            }
            Value::List(items) => items
                .iter_mut()
                .for_each(|v| v.rename_form_reference(old, new)),
            _ => {}
        }
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::Int(v)
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::Float(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::Text(v.to_owned())
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::Text(v)
    }
}

impl From<Color> for Value {
    fn from(v: Color) -> Self {
        Value::Color(v)
    }
}

impl From<Size> for Value {
    fn from(v: Size) -> Self {
        Value::Size(v)
    }
}

impl From<Point> for Value {
    fn from(v: Point) -> Self {
        Value::Point(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(n: &str) -> ComponentName {
        ComponentName::new(n).unwrap()
    }

    #[test]
    fn basic_checks() {
        assert!(Value::Bool(true).check(&PropertyType::Bool).is_ok());
        assert!(Value::Text("x".into()).check(&PropertyType::Bool).is_err());
        let range = PropertyType::Int {
            min: Some(0),
            max: Some(10),
        };
        assert!(Value::Int(10).check(&range).is_ok());
        assert!(matches!(
            Value::Int(11).check(&range),
            Err(Mismatch::OutOfRange { .. })
        ));
        assert!(
            Value::Float(f64::NAN)
                .check(&PropertyType::Float {
                    min: None,
                    max: None
                })
                .is_err()
        );
        let align = PropertyType::Enum(vec!["Left".into(), "Right".into()]);
        assert!(Value::Enum("Left".into()).check(&align).is_ok());
        assert!(matches!(
            Value::Enum("Up".into()).check(&align),
            Err(Mismatch::UnknownVariant { .. })
        ));
    }

    #[test]
    fn lists_and_custom() {
        let ty = PropertyType::List(Box::new(PropertyType::Text));
        assert!(Value::List(vec!["a".into(), "b".into()]).check(&ty).is_ok());
        let err = Value::List(vec!["a".into(), Value::Int(1)])
            .check(&ty)
            .unwrap_err();
        assert!(matches!(err, Mismatch::ListItem(1, _)));
        assert_eq!(err.to_string(), "list item 1: expected text, found int");

        let stroke = PropertyType::Custom("egui.Stroke".into());
        let value = Value::Custom {
            type_name: "egui.Stroke".into(),
            data: "(1.0, BLACK)".into(),
        };
        assert!(value.check(&stroke).is_ok());
        let other = Value::Custom {
            type_name: "slint.Brush".into(),
            data: String::new(),
        };
        assert!(other.check(&stroke).is_err());
    }

    #[test]
    fn references_and_rename() {
        let (main, other) = (name("main"), name("other"));
        let mut v = Value::List(vec![
            Value::reference(name("a")),
            Value::reference_on(other.clone(), name("a")),
            Value::Reference(None),
        ]);
        assert_eq!(v.references().len(), 2);
        // Only the local "a" is renamed when "main::a" is renamed.
        v.rename_reference(&main, &main, &name("a"), &name("b"));
        let names: Vec<String> = v.references().iter().map(|p| p.to_string()).collect();
        assert_eq!(names, ["b", "other::a"]);
        v.clear_reference(&main, &other, &name("a"));
        assert_eq!(v.references().len(), 1);
    }
}
