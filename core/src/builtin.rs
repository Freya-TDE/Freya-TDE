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
//! | `core/ImageBox` | widget | `on_click` |
//! | `core/ImageScrollBox` | widget (list) | `on_select` |
//!
//! Icons: the lower-case type name (`form`, `container`, `label`, `button`,
//! `edit`, `checkbox`, `imagebox`, `imagescrollbox`).
//!
//! Pictures (`ImageBox`, `ImageScrollBox`) are prepared once by the generated
//! `crate::app::freya_image` (gamma, opacity), so every backend shows them alike.
//!
//! Every backend brings its own palette on top of these; backends may also
//! add backend-specific properties to them ([`crate::TypeRegistry::extend`]).

use crate::component_type::{CargoDependency, ComponentType, TypeKind};
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

pub fn image_box() -> TypeRef {
    TypeRef::core("ImageBox")
}

pub fn image_scroll_box() -> TypeRef {
    TypeRef::core("ImageScrollBox")
}

/// The ways a picture fills its area (also `background_fit`): Cover fills
/// and keeps proportions, Stretch fills and distorts, Contain shows the
/// whole picture, Center keeps the original size.
pub const FITS: [&str; 4] = ["Cover", "Stretch", "Contain", "Center"];

fn fit(default: &str) -> PropertyDef {
    PropertyDef::new(
        "fit",
        PropertyType::Enum(FITS.iter().map(|f| (*f).to_owned()).collect()),
        Value::Enum(default.into()),
    )
    .with_category("Appearance")
    .with_description(
        "Contain: whole picture; Cover: fill, keep proportions; Stretch: fill, distort; Center: original size",
    )
}

/// The crate that decodes pictures for `crate::app::freya_image`.
fn image_crate() -> CargoDependency {
    let mut dep = CargoDependency::new("image", "0.25");
    dep.default_features = false;
    dep.features = ["png", "jpeg", "gif", "webp", "bmp"]
        .map(String::from)
        .to_vec();
    dep
}

/// Opacity and gamma of pictures.
fn picture_tone() -> [PropertyDef; 2] {
    [
        PropertyDef::new(
            "opacity",
            PropertyType::Float {
                min: Some(0.0),
                max: Some(1.0),
            },
            Value::Float(1.0),
        )
        .with_category("Appearance")
        .with_description("0 = invisible, 1 = opaque"),
        PropertyDef::new(
            "gamma",
            PropertyType::Float {
                min: Some(0.1),
                max: Some(5.0),
            },
            Value::Float(1.0),
        )
        .with_category("Appearance")
        .with_description("Brightness curve: 1 = unchanged, above 1 lighter, below 1 darker"),
    ]
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
            PropertyType::Enum(FITS.iter().map(|f| (*f).to_owned()).collect()),
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
            .with_property(
                PropertyDef::new(
                    crate::state::STATE,
                    PropertyType::List(Box::new(PropertyType::Text)),
                    Value::List(Vec::new()),
                )
                .with_category("Code")
                .with_description(
                    "Own fields of the form: `name: Type` or `name: Type = start value` \
                     (otherwise Default::default())",
                ),
            )
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
        widget(
            image_box(),
            false,
            "A picture from a file (PNG, JPEG, GIF, WebP, BMP)",
        )
        .with_look("imagebox")
        .with_property(
            PropertyDef::new("file", PropertyType::Text, "")
                .with_category("Appearance")
                .with_description("Path of the picture, relative to the project"),
        )
        .with_property(fit("Contain"))
        .with_properties(picture_tone())
        .with_cargo(image_crate())
        .with_event(EventDef::new("on_click")),
        image_scroll_box_type(),
    ]
}

/// A scrollable row, column or grid of pictures; filled from `images` or a
/// data source (`display_field` = column with the path).
fn image_scroll_box_type() -> ComponentType {
    let mut ty = widget(
        image_scroll_box(),
        false,
        "Scrollable pictures in a row, a column or a grid",
    )
    .with_look("gallery")
    .with_property(
        PropertyDef::new(
            "images",
            PropertyType::List(Box::new(PropertyType::Text)),
            Value::List(Vec::new()),
        )
        .with_category("Appearance")
        .with_description("Paths of the pictures, relative to the project (without a data source)"),
    )
    .with_property(
        PropertyDef::new(
            "orientation",
            PropertyType::Enum(vec!["Horizontal".into(), "Vertical".into(), "Grid".into()]),
            Value::Enum("Grid".into()),
        )
        .with_category("Layout")
        .with_description("Horizontal: one row; Vertical: one column; Grid: rows that wrap"),
    )
    .with_property(
        PropertyDef::new(
            "thumb_size",
            PropertyType::Size,
            Size {
                width: 96,
                height: 96,
            },
        )
        .with_category("Layout")
        .with_description("Size of each picture"),
    )
    .with_property(
        PropertyDef::new(
            "spacing",
            PropertyType::Int {
                min: Some(0),
                max: Some(200),
            },
            8i64,
        )
        .with_category("Layout"),
    )
    .with_property(fit("Cover"))
    .with_properties(picture_tone())
    .with_cargo(image_crate())
    .with_event(
        EventDef::new("on_select")
            .with_param("index", "usize")
            .with_param("path", "&str")
            .with_description("A picture was chosen"),
    );
    // Listenbindung: jedes Backend liefert sein set-items/selected per
    // [[extend]]; die Eigenschaften (data_source, display_field …) kommen hier.
    ty.list = Some(crate::component_type::ListSpec {
        set_items: String::new(),
        selected: String::new(),
    });
    crate::data::complete(&mut ty);
    ty
}
