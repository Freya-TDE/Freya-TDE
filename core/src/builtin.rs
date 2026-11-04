//! The small common widget core: types every UI backend supports.
//!
//! | Type | Kind | Events |
//! |---|---|---|
//! | `core/Form` | container (root of a form) | `on_create`, `on_close` |
//! | `core/Container` | container | – |
//! | `core/Label` | widget | – |
//! | `core/Button` | widget | `on_click` |
//! | `core/Edit` | widget | `on_change`, `on_submit` |
//! | `core/CheckBox` | widget | `on_toggle` |
//!
//! Icons: the lower-case type name (`form`, `container`, `label`, `button`,
//! `edit`, `checkbox`).
//!
//! Every backend brings its own palette on top of these; backends may also
//! add backend-specific properties to them ([`crate::TypeRegistry::extend`]).

use crate::component_type::{ComponentType, TypeKind};
use crate::event::EventDef;
use crate::property::PropertyDef;
use crate::type_ref::TypeRef;
use crate::value::{Point, PropertyType, Size, Value};

pub fn form() -> TypeRef {
    TypeRef::core("Form")
}

pub fn container() -> TypeRef {
    TypeRef::core("Container")
}

pub fn label() -> TypeRef {
    TypeRef::core("Label")
}

pub fn button() -> TypeRef {
    TypeRef::core("Button")
}

pub fn edit() -> TypeRef {
    TypeRef::core("Edit")
}

pub fn check_box() -> TypeRef {
    TypeRef::core("CheckBox")
}

/// Properties every widget has (core widgets and widgets of backend packs).
pub(crate) fn common() -> Vec<PropertyDef> {
    vec![
        PropertyDef::new("enabled", PropertyType::Bool, true)
            .with_category("Behavior")
            .with_description("Can the user interact with it?"),
        PropertyDef::new("visible", PropertyType::Bool, true)
            .with_category("Behavior")
            .with_description("Is it shown?"),
        PropertyDef::new("tooltip", PropertyType::Text, "")
            .with_category("Behavior")
            .with_description("Hint shown on hover"),
        PropertyDef::new("background", PropertyType::Color, STANDARD_COLOR)
            .with_category("Appearance")
            .with_description("Background colour (empty = standard of the theme)"),
        PropertyDef::new("foreground", PropertyType::Color, STANDARD_COLOR)
            .with_category("Appearance")
            .with_description("Text colour (empty = standard; containers pass it on)"),
        PropertyDef::new(
            "font_size",
            PropertyType::Int {
                min: Some(0),
                max: Some(200),
            },
            0i64,
        )
        .with_category("Appearance")
        .with_description("Font size in pixels (0 = standard; containers pass it on)"),
        PropertyDef::new("bold", PropertyType::Bool, false).with_category("Appearance"),
        PropertyDef::new("italic", PropertyType::Bool, false).with_category("Appearance"),
    ]
}

/// A colour that is not set: the theme's colour applies (alpha 0).
pub const STANDARD_COLOR: crate::value::Color = crate::value::Color::rgba(0, 0, 0, 0);

/// Is `color` set (not [`STANDARD_COLOR`])?
pub fn is_set(color: &crate::value::Color) -> bool {
    color.a != 0
}

/// Background picture of forms and containers.
fn background_image() -> [PropertyDef; 2] {
    [
        PropertyDef::new("background_image", PropertyType::Text, "")
            .with_category("Appearance")
            .with_description("Picture behind the content (path relative to the project; PNG, JPEG)"),
        PropertyDef::new(
            "background_fit",
            PropertyType::Enum(vec![
                "Cover".into(),
                "Stretch".into(),
                "Contain".into(),
                "Center".into(),
            ]),
            Value::Enum("Cover".into()),
        )
        .with_category("Appearance")
        .with_description(
            "Cover: fill, keep proportions; Stretch: fill, distort; Contain: whole picture; Center: original size",
        ),
    ]
}

fn widget(type_ref: TypeRef, container: bool, description: &str) -> ComponentType {
    let icon = type_ref.name.to_lowercase();
    ComponentType::new(type_ref, TypeKind::Widget { container })
        .with_icon(icon)
        .with_category("Standard")
        .with_description(description)
        .with_properties(common())
}

/// All core types.
pub fn types() -> Vec<ComponentType> {
    vec![
        widget(form(), true, "A window; the root of every form")
            .with_property(
                PropertyDef::new("title", PropertyType::Text, "Form").with_category("Appearance"),
            )
            .with_property(
                PropertyDef::new(
                    "size",
                    PropertyType::Size,
                    Size {
                        width: 640,
                        height: 480,
                    },
                )
                .with_category("Layout"),
            )
            .with_property(
                PropertyDef::new("resizable", PropertyType::Bool, true).with_category("Behavior"),
            )
            .with_property(
                PropertyDef::new(
                    "position",
                    PropertyType::Enum(vec!["Center".into(), "System".into(), "Manual".into()]),
                    Value::Enum("Center".into()),
                )
                .with_category("Window")
                .with_description(
                    "Where the window opens: centred, where the system puts it, or at `location`",
                ),
            )
            .with_property(
                PropertyDef::new("location", PropertyType::Point, Point { x: 100, y: 100 })
                    .with_category("Window")
                    .with_description("Screen position for `position = Manual`"),
            )
            .with_property(
                PropertyDef::new(
                    "min_size",
                    PropertyType::Size,
                    Size {
                        width: 0,
                        height: 0,
                    },
                )
                .with_category("Window")
                .with_description("Smallest size (0 × 0 = no limit)"),
            )
            .with_property(
                PropertyDef::new(
                    "max_size",
                    PropertyType::Size,
                    Size {
                        width: 0,
                        height: 0,
                    },
                )
                .with_category("Window")
                .with_description("Largest size (0 × 0 = no limit)"),
            )
            .with_property(
                PropertyDef::new(
                    "start_state",
                    PropertyType::Enum(vec![
                        "Normal".into(),
                        "Maximized".into(),
                        "Fullscreen".into(),
                    ]),
                    Value::Enum("Normal".into()),
                )
                .with_category("Window"),
            )
            .with_property(
                PropertyDef::new(
                    "theme",
                    PropertyType::Enum(vec!["System".into(), "Light".into(), "Dark".into()]),
                    Value::Enum("System".into()),
                )
                .with_category("Window")
                .with_description("Light or dark look (System = as the operating system)"),
            )
            .with_property(
                PropertyDef::new("font_family", PropertyType::Text, "")
                    .with_category("Appearance")
                    .with_description(
                        "Font of the whole form, e.g. `Noto Sans` (empty = standard)",
                    ),
            )
            .with_properties(background_image())
            .with_event(EventDef::new("on_create").with_description("After the form was built"))
            .with_event(
                EventDef::new("on_close")
                    .returning("bool")
                    .with_description("Before closing; return false to keep the form open"),
            ),
        widget(container(), true, "Groups child widgets with a layout")
            .with_properties(background_image()),
        widget(label(), false, "Static text").with_property(
            PropertyDef::new("text", PropertyType::Text, "").with_category("Appearance"),
        ),
        widget(button(), false, "Push button")
            .with_property(
                PropertyDef::new("caption", PropertyType::Text, "Button")
                    .with_category("Appearance"),
            )
            .with_event(EventDef::new("on_click")),
        widget(edit(), false, "Single-line text input")
            .with_property(
                PropertyDef::new("text", PropertyType::Text, "").with_category("Appearance"),
            )
            .with_property(
                PropertyDef::new("placeholder", PropertyType::Text, "").with_category("Appearance"),
            )
            .with_property(
                PropertyDef::new("read_only", PropertyType::Bool, false).with_category("Behavior"),
            )
            .with_property(
                PropertyDef::new("password", PropertyType::Bool, false).with_category("Behavior"),
            )
            .with_event(EventDef::new("on_change").with_param("text", "&str"))
            .with_event(EventDef::new("on_submit").with_description("Enter pressed")),
        widget(check_box(), false, "Check box with caption")
            .with_property(
                PropertyDef::new("caption", PropertyType::Text, "").with_category("Appearance"),
            )
            .with_property(
                PropertyDef::new("checked", PropertyType::Bool, false).with_category("Behavior"),
            )
            .with_event(EventDef::new("on_toggle").with_param("checked", "bool")),
    ]
}
