//! Preview: generate a project, build it in debug mode with Cargo and trace
//! compiler errors back to the element that produced the code.
//!
//! The project has no dependencies, so the build works offline.

use std::path::PathBuf;

use tdr_core::codegen::{generate, write};
use tdr_core::pack::Pack;
use tdr_core::*;

fn name(n: &str) -> ComponentName {
    ComponentName::new(n).unwrap()
}

/// A backend that generates plain Rust: `core/Button` becomes a `String`
/// field holding its caption, the form template "clicks" every button.
const PLAIN_BACKEND: &str = r#"
[pack]
name = "plain"
version = "0.1.0"
kind = "backend"
license = "MIT"
generated-code-license = "MIT"

[backend]
form = """
impl {{ form.struct }} {
    pub fn click_all(&mut self) {
    {%- for w in widgets %}{% if w.events.on_click is defined and w.events.on_click %}
        println!("{}", self.{{ w.name }});
        self.{{ w.events.on_click }}();
    {%- endif %}{% endfor %}
    }
}
"""
run = """
{%- for f in forms %}{% if not f.data_module %}{{ app }}.{{ f.name }}.click_all();
{% endif %}{% endfor -%}
Ok(())
"""

[[extend]]
type = "core/Button"
[extend.codegen]
type = "String"
init = "{{ rust.caption }}.to_string()"

[[component]]
name = "Answer"
[component.properties.value]
type = "code"
default = "42"
[component.codegen]
type = "i32"
init = "{{ rust.value }}"
"#;

fn registry() -> TypeRegistry {
    let mut registry = TypeRegistry::with_builtins();
    Pack::parse(PLAIN_BACKEND)
        .unwrap()
        .register(&mut registry)
        .unwrap();
    registry
}

fn forms(answer: &str) -> Vec<Form> {
    let mut services = Form::data_module(name("services"), "plain");
    services
        .add_component(
            Component::new(name("answer"), TypeRef::backend("plain", "Answer"))
                .with("value", Value::Code(answer.into())),
        )
        .unwrap();
    let mut main = Form::new(
        name("main_form"),
        "plain",
        Widget::new(name("window"), builtin::form()),
    );
    main.add_widget(
        &name("window"),
        Widget::from_component(
            Component::new(name("ok_button"), builtin::button())
                .with("caption", "OK")
                .on("on_click", HandlerName::new("ok_clicked").unwrap()),
        ),
    )
    .unwrap();
    vec![services, main]
}

#[test]
fn debug_build_traces_errors_to_elements() {
    let dir: PathBuf = std::env::temp_dir().join(format!("tdr-preview-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let project = ProjectFile::new("preview_demo", "1.85", "plain").unwrap();
    let registry = registry();

    // A broken expression in a property ends up in app.gen.rs.
    let generation = generate(&project, &forms("no_such_fn()"), &registry, &dir);
    assert!(!generation.has_errors(), "{:#?}", generation.diagnostics);
    write(&dir, &generation).unwrap();
    let report = preview::build(&dir, Some(&generation)).unwrap();
    assert!(!report.success);
    let error = report.errors().next().expect("a compiler error");
    assert_eq!(error.code.as_deref(), Some("E0425"), "{error:#?}");
    assert_eq!(error.file.as_deref(), Some("src/app.gen.rs"));
    assert_eq!(error.element, Some("services::answer".parse().unwrap()));

    // Fixed in the designer: builds, and the binary runs the UI loop.
    let generation = generate(&project, &forms("40 + 2"), &registry, &dir);
    write(&dir, &generation).unwrap();
    let report = preview::build(&dir, Some(&generation)).unwrap();
    assert!(report.success, "{:#?}", report.messages);
    let exe = report.executable.expect("debug binary");
    let output = std::process::Command::new(exe).output().unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "OK\n");

    std::fs::remove_dir_all(&dir).unwrap();
}
