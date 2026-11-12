//! Data binding: list widgets filled from a data source (like Delphi's
//! `TDBLookupComboBox` with `TQuery`).
//!
//! A **data source** is a component type with a [`DataSpec`](crate::component_type::DataSpec) (e.g. the SQLx
//! `Query`): it provides the capability [`ROWS`] and has the property
//! `columns` – one entry per column, `name: Type` (e.g. `id: i64`,
//! `name: String`). A **list widget** is a widget type with a [`ListSpec`](crate::component_type::ListSpec)
//! (drop-down, list box …) or a [`TableSpec`](crate::TableSpec) (data grid);
//! it gets the properties
//!
//! | Property | Meaning |
//! |---|---|
//! | `data_source` | reference to a data source (`data.rows`) |
//! | `display_field` | lists: column shown as text |
//! | `fields`, `headers` | tables: columns shown (empty = all) and their titles |
//! | `value_field` | column with the key (empty = the text) |
//! | `load` | `Start` (load when the form opens) or `Manual` |
//! | `editable` | tables: cells can be edited, rows added and deleted |
//!
//! **Editing** (like Delphi's `TDataSet`): an editable table keeps the row
//! being edited in a buffer; it is saved (posted) when another row is chosen
//! or Enter is pressed, Esc discards it. The generator writes
//! `<widget>_edit_cell`, `<widget>_post`, `<widget>_cancel`,
//! `<widget>_insert` and `<widget>_delete`, and calls the events `on_post`
//! (`row`, `inserted`; `false` keeps the row in editing), `on_delete`
//! (`false` keeps the row) and `on_post_error`. If the data source can write
//! (its pack gives `update`/`insert`/`delete`) and has `table` and `key`
//! (key columns, comma-separated), the rows are also written automatically;
//! with `generated_key` the key columns are left out of `INSERT` (the
//! database assigns them) and the table is loaded again.
//!
//! The generator writes, per bound widget, `<widget>_reload()` (loads in
//! the background on Tokio) and `<widget>_selected_value()`; for a table also
//! a row struct `<Widget>Row` with all columns and `<widget>_selected_row()`; the backend
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
/// Properties of a table widget (besides `data_source`, `value_field`, `load`).
pub const FIELDS: &str = "fields";
pub const HEADERS: &str = "headers";
/// Table widgets: cells can be edited, rows added and deleted.
pub const EDITABLE: &str = "editable";
/// Events of an editable table widget.
pub const ON_POST: &str = "on_post";
pub const ON_DELETE: &str = "on_delete";
pub const ON_POST_ERROR: &str = "on_post_error";
/// Properties of a data source that can write rows back.
pub const TABLE: &str = "table";
pub const KEY: &str = "key";
pub const GENERATED_KEY: &str = "generated_key";

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
        if ty
            .data
            .as_ref()
            .is_some_and(crate::component_type::DataSpec::writable)
        {
            add(
                ty,
                PropertyDef::new(TABLE, PropertyType::Text, "")
                    .with_category("Data")
                    .with_description(
                        "Table edited rows are written to (empty = only the events of the grid)",
                    ),
            );
            add(
                ty,
                PropertyDef::new(KEY, PropertyType::Text, "")
                    .with_category("Data")
                    .with_description("Key columns of `table`, comma-separated (e.g. `id`)"),
            );
            add(
                ty,
                PropertyDef::new(GENERATED_KEY, PropertyType::Bool, true)
                    .with_category("Data")
                    .with_description(
                        "The database assigns the key: INSERT leaves the key columns out",
                    ),
            );
        }
        if !ty.provides.iter().any(|p| p.capability.as_str() == ROWS) {
            ty.provides.push(crate::capability::Provide {
                capability: rows_capability(),
                description: "Rows for list widgets".into(),
            });
        }
    }
    let list = ty.list.is_some();
    let table = ty.table.is_some();
    if list || table {
        add(
            ty,
            PropertyDef::new(
                DATA_SOURCE,
                PropertyType::Reference(Some(rows_capability())),
                Value::Reference(None),
            )
            .with_category("Data")
            .with_description("Fill it from this data source (e.g. a query)"),
        );
    }
    if list {
        add(
            ty,
            PropertyDef::new(DISPLAY_FIELD, PropertyType::Text, "")
                .with_category("Data")
                .with_description("Column shown as text"),
        );
    }
    if table {
        add(
            ty,
            PropertyDef::new(
                FIELDS,
                PropertyType::List(Box::new(PropertyType::Text)),
                Value::List(Vec::new()),
            )
            .with_category("Data")
            .with_description("Columns shown, in this order (empty = all columns of the source)"),
        );
        add(
            ty,
            PropertyDef::new(
                HEADERS,
                PropertyType::List(Box::new(PropertyType::Text)),
                Value::List(Vec::new()),
            )
            .with_category("Data")
            .with_description(
                "Column titles, one per shown column (missing ones: the column name)",
            ),
        );
        add(
            ty,
            PropertyDef::new(EDITABLE, PropertyType::Bool, false)
                .with_category("Data")
                .with_description(
                    "Cells can be edited (Enter or another row saves, Esc discards); \
                     `<name>_insert()`/`<name>_delete()` add and delete rows",
                ),
        );
        let row = "&{{ name | pascal }}Row";
        let events = [
            crate::event::EventDef::new(ON_POST)
                .with_param("row", row)
                .with_param("inserted", "bool")
                .returning("bool")
                .with_description(
                    "Before an edited or new row is saved; `false` keeps it in editing",
                ),
            crate::event::EventDef::new(ON_DELETE)
                .with_param("row", row)
                .returning("bool")
                .with_description("Before a row is deleted; `false` keeps it"),
            crate::event::EventDef::new(ON_POST_ERROR)
                .with_param("error", "&str")
                .with_description("Writing to the database failed (the rows are loaded again)"),
        ];
        for event in events {
            if !ty.events.iter().any(|e| e.name == event.name) {
                ty.events.push(event);
            }
        }
    }
    if list || table {
        add(
            ty,
            PropertyDef::new(VALUE_FIELD, PropertyType::Text, "")
                .with_category("Data")
                .with_description(
                    "Column with the key, see `<name>_selected_value()` (lists: empty = the text)",
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
