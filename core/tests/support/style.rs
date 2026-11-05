//! Appearance in the backend tests: colours, fonts, a background picture
//! and window settings on the form `window` and two of its widgets.

use tdr_core::*;

fn name(n: &str) -> ComponentName {
    ComponentName::new(n).unwrap()
}

/// Sets the appearance properties, a requested size and a size in percent
/// in the layout and an anchored label in the absolute container `canvas`
/// (the generated code must build and run with them; the look is checked
/// on screenshots).
pub fn apply(form: &mut Form, greeting: &str, button: &str) {
    let window = form.widget_mut(&name("window")).unwrap();
    let c = &mut window.component;
    c.set("background", Value::Color(Color::rgb(0x20, 0x30, 0x40)));
    c.set("foreground", Value::Color(Color::rgb(0xf0, 0xf0, 0xf0)));
    c.set("font_family", "Sans");
    c.set("background_image", "logo.png");
    c.set("background_fit", Value::Enum("Contain".into()));
    c.set(
        "min_size",
        Value::Size(Size {
            width: 300,
            height: 200,
        }),
    );
    c.set("theme", Value::Enum("Dark".into()));
    let label = form.widget_mut(&name(greeting)).unwrap();
    // Wunschgröße im Layout (wie die Anfasser im Designer sie setzen).
    label.visual.placement = Placement::Layout {
        stretch: 0,
        align: Align::Start,
        cell: None,
        size: Size {
            width: 260,
            height: 40,
        },
        percent: Size::default(),
    };
    label.component.set("font_size", 20i64);
    label.component.set("bold", true);
    label.component.set("italic", true);
    // Prozent: das Eingabefeld nimmt 60 % der Breite ein.
    if let Some(edit) = form.widget_mut(&name("name_edit")) {
        edit.visual.placement = Placement::Layout {
            stretch: 0,
            align: Align::Fill,
            cell: None,
            size: Size::default(),
            percent: Size {
                width: 60,
                height: 0,
            },
        };
    }
    // Anker: der absolute Container füllt die Breite, ein Label hängt rechts.
    if form.widget(&name("canvas")).is_some() {
        form.add_widget(
            &name("canvas"),
            Widget::from_component(
                Component::new(name("anchored"), builtin::label()).with("text", "rechts"),
            )
            .with_placement(Placement::Absolute {
                position: Point { x: 120, y: 2 },
                size: Size {
                    width: 60,
                    height: 20,
                },
                anchors: Anchors::new(false, true, true, false),
            }),
        )
        .unwrap();
    }
    let ok = form.widget_mut(&name(button)).unwrap();
    ok.component
        .set("background", Value::Color(Color::rgb(0xc0, 0x39, 0x2b)));
    ok.component
        .set("foreground", Value::Color(Color::rgb(0xff, 0xff, 0xff)));
}

/// The picture for `background_image`, next to the generated project.
pub fn picture(dir: &std::path::Path) {
    std::fs::write(
        dir.join("logo.png"),
        include_bytes!("../../../ide/assets/palette/image.png"),
    )
    .unwrap();
}
