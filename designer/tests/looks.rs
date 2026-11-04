//! Backend widgets drawn by their type's `look`: tabs show one page and
//! switch on click, frames and tab bars reserve a header, a main menu puts
//! a menu bar into the form.

use tdr_core::pack::Pack;
use tdr_core::*;
use tdr_designer::{Point, Primitive, Theme, layout, render, tab_at};

const BACKEND: &str = r#"
[pack]
name = "demo"
version = "0.1.0"
kind = "backend"
license = "MIT"
generated-code-license = "MIT"

[[component]]
name = "Switch"
kind = "widget"
look = "switch"
[component.properties.active]
type = "bool"

[[component]]
name = "Tabs"
kind = "container"
look = "tabs"
[component.properties.pages]
type = "list"
item = "text"
[component.properties.page]
type = "int"

[[component]]
name = "Menu"
look = "menubar"
[component.properties.items]
type = "list"
item = "text"
"#;

fn name(n: &str) -> ComponentName {
    ComponentName::new(n).unwrap()
}

fn texts(items: &[&str]) -> Value {
    Value::List(items.iter().map(|t| Value::Text((*t).into())).collect())
}

fn setup(page: i64, with_menu: bool) -> (Form, TypeRegistry) {
    let mut registry = TypeRegistry::with_builtins();
    Pack::parse(BACKEND)
        .unwrap()
        .register(&mut registry)
        .unwrap();
    let demo = |t: &str| TypeRef::backend("demo", t);
    let mut form = Form::new(
        name("main_form"),
        "demo",
        Widget::new(name("window"), builtin::form()),
    );
    form.add_widget(
        &name("window"),
        Widget::from_component(Component::new(name("dark"), demo("Switch")).with("active", true)),
    )
    .unwrap();
    form.add_widget(
        &name("window"),
        Widget::from_component(
            Component::new(name("pages"), demo("Tabs"))
                .with("pages", texts(&["Eins", "Zwei"]))
                .with("page", page),
        ),
    )
    .unwrap();
    for child in ["first", "second"] {
        form.add_widget(&name("pages"), Widget::new(name(child), builtin::label()))
            .unwrap();
    }
    if with_menu {
        form.add_component(Component::new(name("menu"), demo("Menu")).with(
            "items",
            texts(&["Datei/Neu | Ctrl+N", "Datei/-", "Hilfe/Über"]),
        ))
        .unwrap();
    }
    (form, registry)
}

fn theme() -> Theme {
    let mut theme = Theme::for_platform(&Platform::Linux);
    theme.widgets.insert(
        "backend:demo/Tabs".into(),
        tdr_designer::WidgetMetrics {
            height: 100,
            min_width: 150,
            padding_x: 0,
            header: 30,
        },
    );
    theme
}

fn has_text(primitives: &[Primitive], text: &str) -> bool {
    primitives
        .iter()
        .any(|p| matches!(p, Primitive::Text { text: t, .. } if t == text))
}

#[test]
fn tabs_show_one_page_below_the_header() {
    let theme = theme();
    let (form, registry) = setup(1, false);
    let laid = layout(&form, &registry, &theme);
    assert!(
        laid.rect(&name("first")).is_none(),
        "nur die gewählte Seite"
    );
    let tabs = laid.rect(&name("pages")).unwrap();
    let second = laid.rect(&name("second")).unwrap();
    assert_eq!(second.y, tabs.y + 30, "unter der Reiterleiste");
    let primitives = render(&form, &laid, &registry, &theme);
    assert!(has_text(&primitives, "Eins") && has_text(&primitives, "Zwei"));

    // Klick auf den ersten Reiter.
    let first_tab = Point::new(tabs.x + 5, tabs.y + 5);
    assert_eq!(
        tab_at(&form, &laid, &registry, &theme, first_tab),
        Some((name("pages"), 0))
    );
    let below = Point::new(tabs.x + 5, tabs.y + 50);
    assert_eq!(tab_at(&form, &laid, &registry, &theme, below), None);
}

#[test]
fn switch_is_drawn_as_track_and_knob() {
    let theme = theme();
    let (form, registry) = setup(0, false);
    let laid = layout(&form, &registry, &theme);
    let area = laid.rect(&name("dark")).unwrap();
    let primitives = render(&form, &laid, &registry, &theme);
    let rounded: Vec<_> = primitives
        .iter()
        .filter_map(|p| match p {
            Primitive::Rect {
                rect, fill, radius, ..
            } if *radius > 4 && area.contains(rect.center()) => Some(*fill),
            _ => None,
        })
        .collect();
    assert_eq!(rounded.len(), 2, "{primitives:#?}");
    assert_eq!(rounded[0], Some(theme.colors.accent), "eingeschaltet");
}

#[test]
fn main_menu_adds_a_menu_bar() {
    let theme = theme();
    let (plain, registry) = setup(0, false);
    let (with_menu, _) = setup(0, true);
    let a = layout(&plain, &registry, &theme);
    let b = layout(&with_menu, &registry, &theme);
    let shift = b.rect(&name("dark")).unwrap().y - a.rect(&name("dark")).unwrap().y;
    assert_eq!(shift, theme.menu_bar_height);
    let primitives = render(&with_menu, &b, &registry, &theme);
    assert!(has_text(&primitives, "Datei") && has_text(&primitives, "Hilfe"));
    assert!(!has_text(&render(&plain, &a, &registry, &theme), "Datei"));
}

#[test]
fn bound_lists_show_their_source() {
    let mut registry = TypeRegistry::with_builtins();
    let pack = r#"
[pack]
name = "demo2"
version = "0.1.0"
kind = "backend"
license = "MIT"
generated-code-license = "MIT"

[[component]]
name = "Choice"
kind = "widget"
look = "dropdown"
[component.properties.items]
type = "list"
item = "text"
[component.list]
set-items = "let _ = items;"
selected = "None"
"#;
    Pack::parse(pack).unwrap().register(&mut registry).unwrap();
    let mut form = Form::new(
        name("main_form"),
        "demo2",
        Widget::new(name("window"), builtin::form()),
    );
    form.add_widget(
        &name("window"),
        Widget::from_component(
            Component::new(name("choice"), TypeRef::backend("demo2", "Choice"))
                .with(
                    tdr_core::data::DATA_SOURCE,
                    Value::reference_on(name("services"), name("customers")),
                )
                .with(tdr_core::data::DISPLAY_FIELD, "name"),
        ),
    )
    .unwrap();
    let theme = theme();
    let laid = layout(&form, &registry, &theme);
    let primitives = render(&form, &laid, &registry, &theme);
    assert!(
        has_text(&primitives, "⟨services::customers.name⟩"),
        "{primitives:#?}"
    );
}
