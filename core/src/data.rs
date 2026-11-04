//! Data binding: list widgets filled from a data source (like Delphi's
//! `TDBLookupComboBox` with `TQuery`).
//!
//! A **data source** is a component type with a [`DataSpec`] (e.g. the SQLx
//! `Query`): it provides the capability [`ROWS`] and has the property
//! `columns` – one entry per column, `name: Type` (e.g. `id: i64`,
//! `name: String`). A **list widget** is a widget type with a [`ListSpec`]
//! (drop-down, list box …); it gets the properties
//!
//! | Property | Meaning |
//! |---|---|
//! | `data_source` | reference to a data source (`data.rows`) |
//! | `display_field` | column shown as text |
//! | `value_field` | column with the key (empty = the text) |
//! | `load` | `Start` (load when the form opens) or `Manual` |
//!
//! The generator writes, per bound widget, `<widget>_reload()` (loads in
//! the background on Tokio) and `<widget>_selected_value()`; the backend
//! only says how entries are set and the selection is read.
//!
//! ```
//! use tdr_core::data::parse_columns;
//!
//! let columns = parse_columns(&["id: i64".into(), "name: String".into()]).unwrap();
//! assert_eq!(columns[1].name, "name");
//! assert_eq!(columns[1].rust_type, "String");
//! assert!(parse_columns(&["id".into()]).is_err());
//! ```

use serde::Serialize;

use crate::capability::Capability;
use crate::component_type::ComponentType;
use crate::property::PropertyDef;
use crate::value::{PropertyType, Value};

/// The capability of a data source.
pub const ROWS: &str = "data.rows";
/// Property of a data source with its columns.
pub const COLUMNS: &str = "columns";
/// Properties of a list widget.
pub const DATA_SOURCE: &str = "data_source";
pub const DISPLAY_FIELD: &str = "display_field";
pub const VALUE_FIELD: &str = "value_field";
pub const LOAD: &str = "load";

/// One column of a data source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Column {
    pub name: String,
    /// Rust type, e.g. `i64`, `String`, `Option<String>`.
    pub rust_type: String,
}

/// Parses `name: Type` entries.
pub fn parse_columns(items: &[String]) -> Result<Vec<Column>, String> {
    items
        .iter()
        .filter(|item| !item.trim().is_empty())
        .map(|item| {
            let (name, ty) = item
                .split_once(':')
                .ok_or_else(|| format!("column {item:?} needs a type: `name: Type`"))?;
            let (name, ty) = (name.trim(), ty.trim());
            if !crate::name::is_rust_identifier(name) || ty.is_empty() {
                return Err(format!("column {item:?}: expected `name: Type`"));
            }
            if syn::parse_str::<syn::Type>(ty).is_err() {
                return Err(format!("column {name}: {ty:?} is not a Rust type"));
            }
            Ok(Column {
                name: name.to_owned(),
                rust_type: ty.to_owned(),
            })
        })
        .collect()
}

/// The texts of a list value (other items are skipped).
pub fn texts(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::List(items)) => items
            .iter()
            .filter_map(|v| match v {
                Value::Text(t) => Some(t.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn rows_capability() -> Capability {
    Capability::new(ROWS).expect("valid capability")
}

/// Adds what a data source or list widget needs (called when a pack is
/// loaded): `columns` and the capability for sources, the binding
/// properties for list widgets. Existing definitions are kept.
pub fn complete(ty: &mut ComponentType) {
    let add = |ty: &mut ComponentType, def: PropertyDef| {
        if ty.property(&def.name).is_none() {
            ty.properties.push(def);
        }
    };
    if ty.data.is_some() {
        add(
            ty,
            PropertyDef::new(
                COLUMNS,
                PropertyType::List(Box::new(PropertyType::Text)),
                Value::List(Vec::new()),
            )
            .with_category("Data")
            .with_description("Columns of a row: `name: Type`, e.g. `id: i64`, `name: String`"),
        );
        if !ty.provides.iter().any(|p| p.capability.as_str() == ROWS) {
            ty.provides.push(crate::capability::Provide {
                capability: rows_capability(),
                description: "Rows for list widgets".into(),
            });
        }
    }
    if ty.list.is_some() {
        add(
            ty,
            PropertyDef::new(
                DATA_SOURCE,
                PropertyType::Reference(Some(rows_capability())),
                Value::Reference(None),
            )
            .with_category("Data")
            .with_description("Fill the list from this data source (e.g. a query)"),
        );
        add(
            ty,
            PropertyDef::new(DISPLAY_FIELD, PropertyType::Text, "")
                .with_category("Data")
                .with_description("Column shown as text"),
        );
        add(
            ty,
            PropertyDef::new(VALUE_FIELD, PropertyType::Text, "")
                .with_category("Data")
                .with_description(
                    "Column with the key, see `<name>_selected_value()` (empty = the text)",
                ),
        );
        add(
            ty,
            PropertyDef::new(
                LOAD,
                PropertyType::Enum(vec!["Start".into(), "Manual".into()]),
                Value::Enum("Start".into()),
            )
            .with_category("Data")
            .with_description("Start: load when the form opens; Manual: call `<name>_reload()`"),
        );
    }
}
