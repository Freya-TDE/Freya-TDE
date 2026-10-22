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
//! Every backend brings its own palette on top of these; backends may also
//! add backend-specific properties to them ([`crate::TypeRegistry::extend`]).

use crate::component_type::{ComponentType, TypeKind};
use crate::event::EventDef;
use crate::property::PropertyDef;
use crate::type_ref::TypeRef;
use crate::value::{PropertyType, Size};

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

/// Properties every core widget has.
fn common() -> Vec<PropertyDef> {
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
    ]
}

fn widget(type_ref: TypeRef, container: bool, description: &str) -> ComponentType {
    ComponentType::new(type_ref, TypeKind::Widget { container })
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
            .with_event(EventDef::new("on_create").with_description("After the form was built"))
            .with_event(
                EventDef::new("on_close")
                    .returning("bool")
                    .with_description("Before closing; return false to keep the form open"),
            ),
        widget(container(), true, "Groups child widgets with a layout"),
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
