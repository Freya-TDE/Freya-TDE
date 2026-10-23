//! Builds a small form with designer commands and writes it as SVG.
//!
//! ```bash
//! cargo run -p tdr-designer --example svg -- form.svg [linux|windows|macos]
//! ```

use tdr_core::{Platform, ProjectFile, TypeRegistry};
use tdr_designer::*;

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "form.svg".into());
    let platform = match args.next().as_deref() {
        Some("windows") => Platform::Windows,
        Some("macos") => Platform::MacOs,
        _ => Platform::Linux,
    };
    let mut doc = Document::new(
        ProjectFile::new("demo", "1.99", "plain").unwrap(),
        TypeRegistry::with_builtins(),
    );
    let script = r#"[
        {"command": "add_form", "name": "login"},
        {"command": "set_property", "form": "login", "element": "window", "property": "size", "value": {"Size": {"width": 360, "height": 220}}},
        {"command": "add_widget", "form": "login", "parent": "window", "type": "core/Label", "name": "hint"},
        {"command": "set_property", "form": "login", "element": "hint", "property": "text", "value": {"Text": "Please sign in"}},
        {"command": "add_widget", "form": "login", "parent": "window", "type": "core/Edit", "name": "user"},
        {"command": "set_property", "form": "login", "element": "user", "property": "placeholder", "value": {"Text": "User name"}},
        {"command": "add_widget", "form": "login", "parent": "window", "type": "core/Edit", "name": "password"},
        {"command": "set_property", "form": "login", "element": "password", "property": "password", "value": {"Bool": true}},
        {"command": "set_property", "form": "login", "element": "password", "property": "text", "value": {"Text": "secret"}},
        {"command": "add_widget", "form": "login", "parent": "window", "type": "core/CheckBox", "name": "remember"},
        {"command": "set_property", "form": "login", "element": "remember", "property": "caption", "value": {"Text": "Remember me"}},
        {"command": "set_property", "form": "login", "element": "remember", "property": "checked", "value": {"Bool": true}},
        {"command": "add_widget", "form": "login", "parent": "window", "type": "core/Button", "name": "sign_in",
         "placement": {"Layout": {"align": "End"}}},
        {"command": "set_property", "form": "login", "element": "sign_in", "property": "caption", "value": {"Text": "Sign in"}}
    ]"#;
    for command in serde_json::from_str::<Vec<Command>>(script).unwrap() {
        doc.apply(command).unwrap();
    }
    let theme = Theme::for_platform(&platform);
    let form = &doc.forms()[0];
    let layout = layout(form, doc.registry(), &theme);
    let mut primitives = render(form, &layout, doc.registry(), &theme);
    primitives.extend(render_selection(
        &layout,
        &[tdr_core::ComponentName::new("sign_in").unwrap()],
        &theme,
    ));
    std::fs::write(&out, to_svg(&primitives, layout.width, layout.height)).unwrap();
    println!("{out}");
}
