//! The inspector's layout rows as data: where a widget sits in its parent
//! ([`Placement`]) and, for containers, how they arrange their children
//! ([`ContainerLayout`]). The rows are pseudo-properties named
//! `placement.<field>` and `layout.<field>`; [`layout_command`] turns an
//! edit of one into a command, so a UI or an AI can change them like any
//! other property.
//!
//! | Row | Type | Meaning |
//! |---|---|---|
//! | `placement.width`, `placement.height` | int ≥ 0 | requested size in pixels (layout, 0 = natural) or size (absolute) |
//! | `placement.width_percent`, `placement.height_percent` | int 0–100 | share of the parent's content area (layout, 0 = not set; goes before pixels) |
//! | `placement.align` | enum `Fill`/`Start`/`Center`/`End` | cross-axis alignment (layout) |
//! | `placement.stretch` | int ≥ 0 | share of extra space (layout) |
//! | `placement.row`, `placement.column` | int ≥ 0 | grid cell (parent with grid layout) |
//! | `placement.x`, `placement.y` | int | position (absolute) |
//! | `placement.anchor_left` … `placement.anchor_bottom` | bool | [`Anchors`] (absolute) |
//! | `layout.kind` | enum `Vertical`/`Horizontal`/`Grid`/`Stack`/`Absolute` | arrangement of the children |
//! | `layout.columns` | int ≥ 1 | columns (grid) |
//! | `layout.spacing`, `layout.padding` | int ≥ 0 | gap between children, inner margin (all sides) |
//!
//! Switching a container to `Absolute` keeps its children where the
//! designer shows them; switching away puts them back into the flow.

use tdr_core::{
    Align, Anchors, ComponentName, ContainerLayout, Form, GridCell, Insets, LayoutKind, Placement,
    Point, PropertyType, PropertyValue, Size, Value, builtin,
};

use crate::command::Command;
use crate::inspector::PropertyRow;
use crate::layout::FormLayout;

/// Category of the placement rows.
pub const PLACEMENT: &str = "Placement";
/// Category of a container's layout rows.
pub const LAYOUT: &str = "Layout";

const ALIGNS: [&str; 4] = ["Fill", "Start", "Center", "End"];
const KINDS: [&str; 5] = ["Vertical", "Horizontal", "Grid", "Stack", "Absolute"];

/// Is `name` one of the pseudo-properties of this module?
pub fn is_layout_row(name: &str) -> bool {
    name.starts_with("placement.") || name.starts_with("layout.")
}

fn row(
    category: &str,
    name: &str,
    description: &str,
    ty: PropertyType,
    default: Value,
    value: Value,
) -> PropertyRow {
    PropertyRow {
        name: name.to_owned(),
        category: category.to_owned(),
        description: description.to_owned(),
        ty,
        value: (value != default).then(|| PropertyValue::new(value)),
        default: Some(default),
        backend_specific: false,
        platforms: Default::default(),
    }
}

fn int(min: i64, max: Option<i64>) -> PropertyType {
    PropertyType::Int {
        min: Some(min),
        max,
    }
}

/// The layout rows of `element`: its placement (not for the form itself)
/// and, for a container, the layout of its children. `absolute` = the
/// backend allows absolute positioning.
pub fn layout_rows(form: &Form, element: &ComponentName, absolute: bool) -> Vec<PropertyRow> {
    let Some(widget) = form.widget(element) else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    let parent_kind = form
        .parent_of(element)
        .map(|p| p.visual.layout.clone().unwrap_or_default().kind);
    if let Some(parent_kind) = parent_kind {
        let p = PLACEMENT;
        match &widget.visual.placement {
            Placement::Layout {
                stretch,
                align,
                cell,
                size,
                percent,
            } => {
                let px = |v: i32| Value::Int(i64::from(v));
                rows.push(row(
                    p,
                    "placement.width",
                    "Desired width in pixels (0 = natural)",
                    int(0, None),
                    px(0),
                    px(size.width),
                ));
                rows.push(row(
                    p,
                    "placement.height",
                    "Desired height in pixels (0 = natural)",
                    int(0, None),
                    px(0),
                    px(size.height),
                ));
                rows.push(row(
                    p,
                    "placement.width_percent",
                    "Width in percent of the parent (0 = off; takes precedence over pixels)",
                    int(0, Some(100)),
                    px(0),
                    px(percent.width),
                ));
                rows.push(row(
                    p,
                    "placement.height_percent",
                    "Height in percent of the parent (0 = off; takes precedence over pixels)",
                    int(0, Some(100)),
                    px(0),
                    px(percent.height),
                ));
                rows.push(row(
                    p,
                    "placement.align",
                    "Alignment across the layout (Fill = full width or height)",
                    PropertyType::Enum(ALIGNS.map(String::from).to_vec()),
                    Value::Enum("Fill".into()),
                    Value::Enum(format!("{align:?}")),
                ));
                rows.push(row(
                    p,
                    "placement.stretch",
                    "Share of the free space along the layout (0 = natural size)",
                    int(0, None),
                    Value::Int(0),
                    Value::Int(i64::from(*stretch)),
                ));
                if matches!(parent_kind, LayoutKind::Grid { .. }) {
                    let cell = cell.unwrap_or(GridCell {
                        row: 0,
                        column: 0,
                        row_span: 1,
                        column_span: 1,
                    });
                    rows.push(row(
                        p,
                        "placement.row",
                        "Row in the grid",
                        int(0, None),
                        Value::Int(0),
                        Value::Int(i64::from(cell.row)),
                    ));
                    rows.push(row(
                        p,
                        "placement.column",
                        "Column in the grid",
                        int(0, None),
                        Value::Int(0),
                        Value::Int(i64::from(cell.column)),
                    ));
                }
            }
            Placement::Absolute {
                position,
                size,
                anchors,
            } => {
                let px = |v: i32| Value::Int(i64::from(v));
                let any = PropertyType::Int {
                    min: None,
                    max: None,
                };
                rows.push(row(
                    p,
                    "placement.x",
                    "Distance from the left edge",
                    any.clone(),
                    px(0),
                    px(position.x),
                ));
                rows.push(row(
                    p,
                    "placement.y",
                    "Distance from the top edge",
                    any,
                    px(0),
                    px(position.y),
                ));
                rows.push(row(
                    p,
                    "placement.width",
                    "Width in pixels",
                    int(0, None),
                    px(0),
                    px(size.width),
                ));
                rows.push(row(
                    p,
                    "placement.height",
                    "Height in pixels",
                    int(0, None),
                    px(0),
                    px(size.height),
                ));
                let edges = [
                    (
                        "placement.anchor_left",
                        "Anchored to the left edge",
                        anchors.left,
                        true,
                    ),
                    (
                        "placement.anchor_top",
                        "Anchored to the top edge",
                        anchors.top,
                        true,
                    ),
                    (
                        "placement.anchor_right",
                        "Anchored to the right edge (with left: grows in width)",
                        anchors.right,
                        false,
                    ),
                    (
                        "placement.anchor_bottom",
                        "Anchored to the bottom edge (with top: grows in height)",
                        anchors.bottom,
                        false,
                    ),
                ];
                for (name, description, on, default) in edges {
                    rows.push(row(
                        p,
                        name,
                        description,
                        PropertyType::Bool,
                        Value::Bool(default),
                        Value::Bool(on),
                    ));
                }
            }
        }
    }
    if widget.visual.layout.is_some()
        || widget.component.type_ref == builtin::form()
        || !widget.children.is_empty()
    {
        let l = LAYOUT;
        let layout = widget
            .visual
            .layout
            .clone()
            .unwrap_or(ContainerLayout::STANDARD);
        let mut kinds: Vec<String> = KINDS
            .iter()
            .filter(|k| absolute || **k != "Absolute" || layout.kind == LayoutKind::Absolute)
            .map(|k| k.to_string())
            .collect();
        let (kind, columns) = match &layout.kind {
            LayoutKind::Grid { columns } => ("Grid".to_owned(), Some(*columns)),
            LayoutKind::Custom(name) => {
                kinds.push(name.clone());
                (name.clone(), None)
            }
            other => (format!("{other:?}"), None),
        };
        rows.push(row(
            l,
            "layout.kind",
            "Arrangement of the children",
            PropertyType::Enum(kinds),
            Value::Enum("Vertical".into()),
            Value::Enum(kind),
        ));
        if let Some(columns) = columns {
            rows.push(row(
                l,
                "layout.columns",
                "Columns of the grid",
                int(1, None),
                Value::Int(2),
                Value::Int(i64::from(columns)),
            ));
        }
        rows.push(row(
            l,
            "layout.spacing",
            "Spacing between the children",
            int(0, None),
            Value::Int(8),
            Value::Int(i64::from(layout.spacing)),
        ));
        rows.push(row(
            l,
            "layout.padding",
            "Padding on all sides",
            int(0, None),
            Value::Int(12),
            Value::Int(i64::from(layout.padding.left)),
        ));
    }
    rows
}

/// Why a layout row could not be set.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, thiserror::Error)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum LayoutRowError {
    #[error("unbekanntes Element {0}")]
    UnknownElement(ComponentName),
    #[error("unknown layout row {0}")]
    UnknownRow(String),
    #[error("invalid value for {0}")]
    InvalidValue(String),
}

fn as_int(name: &str, value: &Option<Value>) -> Result<i32, LayoutRowError> {
    match value {
        None => Ok(0),
        Some(Value::Int(v)) => {
            i32::try_from(*v).map_err(|_| LayoutRowError::InvalidValue(name.into()))
        }
        _ => Err(LayoutRowError::InvalidValue(name.into())),
    }
}

/// The command that sets the layout row `name` of `element` to `value`
/// (`None` = default). `shown` (the current design surface) places the
/// children when a container becomes absolute.
pub fn layout_command(
    form: &Form,
    element: &ComponentName,
    name: &str,
    value: Option<Value>,
    shown: Option<&FormLayout>,
) -> Result<Command, LayoutRowError> {
    let widget = form
        .widget(element)
        .ok_or_else(|| LayoutRowError::UnknownElement(element.clone()))?;
    let invalid = || LayoutRowError::InvalidValue(name.into());
    let form_name = form.name.clone();
    if let Some(field) = name.strip_prefix("placement.") {
        let mut placement = widget.visual.placement.clone();
        match &mut placement {
            Placement::Layout {
                stretch,
                align,
                cell,
                size,
                percent,
            } => match field {
                "width" => size.width = as_int(name, &value)?.max(0),
                "height" => size.height = as_int(name, &value)?.max(0),
                "width_percent" => percent.width = as_int(name, &value)?.clamp(0, 100),
                "height_percent" => percent.height = as_int(name, &value)?.clamp(0, 100),
                "stretch" => *stretch = as_int(name, &value)?.max(0) as u32,
                "align" => {
                    *align = match value {
                        None => Align::Fill,
                        Some(Value::Enum(v)) => match v.as_str() {
                            "Fill" => Align::Fill,
                            "Start" => Align::Start,
                            "Center" => Align::Center,
                            "End" => Align::End,
                            _ => return Err(invalid()),
                        },
                        _ => return Err(invalid()),
                    }
                }
                "row" | "column" => {
                    let v = as_int(name, &value)?.max(0) as u32;
                    let c = cell.get_or_insert(GridCell {
                        row: 0,
                        column: 0,
                        row_span: 1,
                        column_span: 1,
                    });
                    if field == "row" {
                        c.row = v;
                    } else {
                        c.column = v;
                    }
                }
                _ => return Err(LayoutRowError::UnknownRow(name.into())),
            },
            Placement::Absolute {
                position,
                size,
                anchors,
            } => {
                let flag = |default: bool| match &value {
                    None => Ok(default),
                    Some(Value::Bool(b)) => Ok(*b),
                    _ => Err(invalid()),
                };
                match field {
                    "x" => position.x = as_int(name, &value)?,
                    "y" => position.y = as_int(name, &value)?,
                    "width" => size.width = as_int(name, &value)?.max(0),
                    "height" => size.height = as_int(name, &value)?.max(0),
                    "anchor_left" => anchors.left = flag(true)?,
                    "anchor_top" => anchors.top = flag(true)?,
                    "anchor_right" => anchors.right = flag(false)?,
                    "anchor_bottom" => anchors.bottom = flag(false)?,
                    _ => return Err(LayoutRowError::UnknownRow(name.into())),
                }
            }
        }
        return Ok(Command::SetPlacement {
            form: form_name,
            element: element.clone(),
            placement,
        });
    }
    let field = name
        .strip_prefix("layout.")
        .ok_or_else(|| LayoutRowError::UnknownRow(name.into()))?;
    let mut layout: ContainerLayout = widget
        .visual
        .layout
        .clone()
        .unwrap_or(ContainerLayout::STANDARD);
    let old_kind = layout.kind.clone();
    match field {
        "kind" => {
            layout.kind = match &value {
                None => LayoutKind::Vertical,
                Some(Value::Enum(v)) => match v.as_str() {
                    "Vertical" => LayoutKind::Vertical,
                    "Horizontal" => LayoutKind::Horizontal,
                    "Grid" => LayoutKind::Grid {
                        columns: match old_kind {
                            LayoutKind::Grid { columns } => columns,
                            _ => 2,
                        },
                    },
                    "Stack" => LayoutKind::Stack,
                    "Absolute" => LayoutKind::Absolute,
                    other => LayoutKind::Custom(other.to_owned()),
                },
                _ => return Err(invalid()),
            }
        }
        "columns" => {
            let columns = match &value {
                None => 2,
                _ => as_int(name, &value)?.max(1) as u32,
            };
            layout.kind = LayoutKind::Grid { columns };
        }
        "spacing" => {
            layout.spacing = match value {
                None => ContainerLayout::STANDARD.spacing,
                _ => as_int(name, &value)?.max(0),
            }
        }
        "padding" => {
            layout.padding = match value {
                None => ContainerLayout::STANDARD.padding,
                _ => Insets::all(as_int(name, &value)?.max(0)),
            }
        }
        _ => return Err(LayoutRowError::UnknownRow(name.into())),
    }
    let set = Command::SetLayout {
        form: form_name.clone(),
        element: element.clone(),
        layout: Some(layout.clone()),
    };
    // Kinder mitnehmen: in die absolute Lage dort, wo sie zu sehen sind,
    // bzw. zurück in den Fluss.
    let to_absolute = layout.kind == LayoutKind::Absolute && old_kind != LayoutKind::Absolute;
    let from_absolute = layout.kind != LayoutKind::Absolute && old_kind == LayoutKind::Absolute;
    if !to_absolute && !from_absolute {
        return Ok(set);
    }
    let content = shown.and_then(|l| l.item(element)?.content);
    let mut commands = vec![set];
    for child in &widget.children {
        let placement = if to_absolute {
            let shown_rect = shown.and_then(|l| l.rect(&child.component.name));
            let (x, y, w, h) = match (shown_rect, content) {
                (Some(r), Some(c)) => (r.x - c.x, r.y - c.y, r.width, r.height),
                _ => (0, 0, 120, 30),
            };
            Placement::Absolute {
                position: Point { x, y },
                size: Size {
                    width: w,
                    height: h,
                },
                anchors: Anchors::TOP_LEFT,
            }
        } else {
            Placement::default()
        };
        commands.push(Command::SetPlacement {
            form: form_name.clone(),
            element: child.component.name.clone(),
            placement,
        });
    }
    Ok(Command::Batch { commands })
}
