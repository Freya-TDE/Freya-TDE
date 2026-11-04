//! Appearance in the backend tests: colours, fonts, a background picture
//! and window settings on the form `window` and two of its widgets.

use tdr_core::*;

fn name(n: &str) -> ComponentName {
    ComponentName::new(n).unwrap()
}

/// Sets the appearance properties (the generated code must build and run
/// with them; the look is checked on screenshots).
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
    label.component.set("font_size", 20i64);
    label.component.set("bold", true);
    label.component.set("italic", true);
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
