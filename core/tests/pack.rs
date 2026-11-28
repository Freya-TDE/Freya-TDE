//! Pack descriptions: the real packs in `components/`, and error reporting
//! for broken descriptions.

use std::path::PathBuf;

use tdr_core::graph::{ProjectIssue, Suggestion, analyze};
use tdr_core::pack::{Pack, PackError, PackKind, load_all};
use tdr_core::*;

fn components_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../components")
}

fn name(n: &str) -> ComponentName {
    ComponentName::new(n).unwrap()
}

/// Core types plus all packs from `components/`.
fn registry() -> TypeRegistry {
    let (packs, errors) = load_all(&components_dir());
    assert!(errors.is_empty(), "{errors:#?}");
    let mut registry = TypeRegistry::with_builtins();
    for pack in &packs {
        pack.register(&mut registry).unwrap();
    }
    registry
}

/// Parses a pack and returns its problems as "location: message" lines.
fn problems(text: &str) -> Vec<String> {
    match Pack::parse(text) {
        Ok(_) => Vec::new(),
        Err(e @ PackError::Syntax(_)) => vec![e.to_string()],
        Err(e) => e.problems().iter().map(ToString::to_string).collect(),
    }
}

const HEADER: &str = r#"
[pack]
name = "demo"
version = "0.1.0"
license = "MIT"
generated-code-license = "MIT"
"#;

fn with_header(body: &str) -> String {
    format!("{HEADER}\n{body}")
}

#[test]
fn real_packs_load() {
    let (packs, errors) = load_all(&components_dir());
    assert!(errors.is_empty(), "{errors:#?}");
    let names: Vec<&str> = packs.iter().map(|p| p.info.name.as_str()).collect();
    assert_eq!(names, ["axum", "http", "redis", "sqlx", "tokio", "ws"]);
    assert!(
        packs
            .iter()
            .all(|p| p.dir.is_some() && p.info.kind == PackKind::Components)
    );

    let registry = registry();
    // 8 Kern-Typen, 10 aus den Packs (tokio 1, sqlx 2, axum 1, redis 2, ws 2, http 2).
    assert_eq!(registry.len(), 8 + 10);
    let pool = registry.get(&TypeRef::pack("sqlx", "Pool")).unwrap();
    // Property order follows the file (object inspector).
    let names: Vec<&str> = pool.properties.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        ["driver", "url", "max_connections", "connect_timeout"]
    );
    assert!(pool.property("url").unwrap().is_required());
    assert_eq!(
        pool.property("max_connections").unwrap().default,
        Some(Value::Int(5))
    );
    // Natural default: first enum variant.
    let runtime = registry.get(&TypeRef::pack("tokio", "Runtime")).unwrap();
    assert_eq!(
        runtime.property("flavor").unwrap().default,
        Some(Value::Enum("MultiThread".into()))
    );
    let query = registry.get(&TypeRef::pack("sqlx", "Query")).unwrap();
    // The row type follows the pool's driver (template over `refs`).
    let row = &query.event("on_row").unwrap().params[0].rust_type;
    assert!(row.contains("refs.pool.props.driver") && row.contains("MySqlRow"));
}

#[test]
fn project_from_real_packs() {
    let registry = registry();
    let mut services = Form::data_module(name("services"), "egui");
    services
        .add_component(
            Component::new(name("db_pool"), TypeRef::pack("sqlx", "Pool"))
                .with("url", "postgres://localhost/app"),
        )
        .unwrap();
    let mut main = Form::new(
        name("main"),
        "egui",
        Widget::new(name("main_window"), builtin::form()),
    );
    main.add_component(Component::new(name("api"), TypeRef::pack("axum", "Router")))
        .unwrap();
    main.add_component(
        Component::new(name("customers"), TypeRef::pack("sqlx", "Query"))
            .with(
                "pool",
                Value::reference_on(name("services"), name("db_pool")),
            )
            .with("router", Value::reference(name("api"))),
    )
    .unwrap();
    let mut forms = vec![services, main];

    for form in &forms {
        assert!(form.validate(&ValidationContext::new(&registry)).is_empty());
    }
    let analysis = analyze(&forms, &registry);
    let issues: Vec<&ProjectIssue> = analysis.diagnostics.iter().map(|d| &d.issue).collect();
    // No runtime yet, and the router needs the pool because a query is attached.
    assert!(issues.contains(&&ProjectIssue::Unsatisfied {
        capability: Capability::new("tokio.runtime").unwrap(),
        via: None,
        suggestion: Suggestion::Create(vec![TypeRef::pack("tokio", "Runtime")]),
    }));
    assert!(issues.contains(&&ProjectIssue::Unsatisfied {
        capability: Capability::new("sqlx.pool").unwrap(),
        via: Some("pool".into()),
        suggestion: Suggestion::Connect(vec!["services::db_pool".parse().unwrap()]),
    }));

    // Follow the suggestions.
    forms[0]
        .add_component(Component::new(
            name("runtime"),
            TypeRef::pack("tokio", "Runtime"),
        ))
        .unwrap();
    forms[1].component_mut(&name("api")).unwrap().set(
        "pool",
        Value::reference_on(name("services"), name("db_pool")),
    );
    let analysis = analyze(&forms, &registry);
    assert!(
        analysis.diagnostics.is_empty(),
        "{:#?}",
        analysis.diagnostics
    );
    let order: Vec<String> = analysis.order.iter().map(ToString::to_string).collect();
    assert_eq!(
        order,
        [
            "services::runtime",
            "services::db_pool",
            "main::main_window",
            "main::api",
            "main::customers"
        ]
    );
    let crates: Vec<&str> = analysis
        .cargo
        .iter()
        .map(|d| d.crate_name.as_str())
        .collect();
    assert_eq!(crates, ["axum", "sqlx", "tokio"]);
}

#[test]
fn syntax_and_unknown_keys_have_positions() {
    let found = problems(&with_header(
        "[[component]]\nname = \"Pool\"\n[component.properties.size]\ntype = \"int\"\ndefualt = 5\n",
    ));
    assert_eq!(found.len(), 1);
    assert!(found[0].contains("defualt"), "{found:?}");
    assert!(found[0].contains("line"), "{found:?}");

    let found = problems("[pack\nname = 1");
    assert!(found[0].contains("line 1"), "{found:?}");
}

#[test]
fn header_checks() {
    let found = problems(
        "[pack]\nname = \"My Pack\"\nversion = \"1.0\"\nlicense = \"\"\ngenerated-code-license = \"\"\n",
    );
    assert_eq!(found.len(), 4, "{found:#?}");
    assert!(found.iter().any(|p| p.contains("generated-code-license")));

    // generated-code-license is mandatory.
    let found = problems("[pack]\nname = \"x\"\nversion = \"1.0.0\"\nlicense = \"MIT\"\n");
    assert!(found[0].contains("generated-code-license"), "{found:?}");
}

#[test]
fn property_checks() {
    let found = problems(&with_header(
        r#"
[[component]]
name = "Thing"

[component.properties.Bad_Name]
type = "text"

[component.properties.count]
type = "int"
min = 10
max = 1
variants = ["A"]

[component.properties.level]
type = "int"
min = 1
default = 0

[component.properties.mode]
type = "enum"
variants = ["Fast", "Slow"]
default = "Medium"

[component.properties.token]
type = "text"
required = true
default = "x"

[component.properties.items]
type = "list"

[component.properties.target]
type = "reference"
default = "x"

[component.properties.shape]
type = "triangle"

[component.properties.hint]
type = "text"
platforms = ["amiga"]
"#,
    ));
    let has = |location: &str, part: &str| {
        assert!(
            found
                .iter()
                .any(|p| p.starts_with(location) && p.contains(part)),
            "missing {location:?} / {part:?} in {found:#?}"
        );
    };
    has("component Thing, property Bad_Name", "snake_case");
    has(
        "component Thing, property count",
        "min 10 is greater than max 1",
    );
    has(
        "component Thing, property count",
        "`variants` is not allowed for type int",
    );
    has("component Thing, property level", "out of range");
    has("component Thing, property mode", "unknown variant");
    has(
        "component Thing, property token",
        "required property cannot have a default",
    );
    has("component Thing, property items", "needs `item`");
    has(
        "component Thing, property target",
        "references cannot have a default",
    );
    has(
        "component Thing, property shape",
        "unknown type \"triangle\"",
    );
    has("component Thing, property hint", "unknown platform");
    assert_eq!(found.len(), 10, "{found:#?}");
}

#[test]
fn requirement_and_capability_checks() {
    let found = problems(&with_header(
        r#"
[[component]]
name = "thing"
kind = "widget"
provides = ["db", "x.y", "x.y"]
requires = [
    { capability = "sqlx.pool", via = "missing" },
    { capability = "sqlx.pool", via = "plain" },
    { capability = "sqlx.pool", via = "other" },
    { capability = "sqlx.pool", via = "open" },
    { capability = "sqlx.pool", when = { property-set = "nope" } },
    { capability = "sqlx.pool", when = { referenced-by = "a.b", project-provides = "c.d" } },
]
cargo = [{ crate = "bad crate", version = "" }]

[component.properties.plain]
type = "text"

[component.properties.other]
type = "reference"
capability = "http.router"

[component.properties.open]
type = "reference"

[component.events.OnClick]
params = [{ name = "1x", type = "" }]

[[component]]
name = "Twin"
[[component]]
name = "Twin"
"#,
    ));
    let has = |part: &str| {
        assert!(
            found.iter().any(|p| p.contains(part)),
            "missing {part:?} in {found:#?}"
        )
    };
    has("component names must be PascalCase");
    has("component packs are non-visual");
    has("invalid capability \"db\"");
    has("x.y is listed twice");
    has("`via` names \"missing\", which is not a property");
    has("property \"plain\" is not a reference");
    has("property \"other\" references http.router, not sqlx.pool");
    has("property \"open\" must declare `capability = \"sqlx.pool\"`");
    has("`when.property-set` names \"nope\"");
    has("`when` needs exactly one of");
    has("invalid crate name \"bad crate\"");
    has("needs a version");
    has("event names must be snake_case");
    has("parameter name \"1x\"");
    has("component Twin: defined twice");
}

#[test]
fn backend_packs_extend_types() {
    let text = r##"
[pack]
name = "egui"
version = "0.1.0"
kind = "backend"
license = "MIT OR Apache-2.0"
generated-code-license = "MIT OR Apache-2.0"

[[component]]
name = "Slider"
kind = "widget"
[component.properties.value]
type = "float"
min = 0.0
max = 1.0
[component.properties.stroke]
type = "custom"
custom = "egui.Stroke"
default = "Stroke::new(1.0, Color32::BLACK)"

[[extend]]
type = "core/Button"
[extend.properties.small]
type = "bool"
[extend.properties.fill]
type = "color"
default = "#3366ff"
platforms = ["windows", "cfg(unix)"]
"##;
    let pack = Pack::parse(text).unwrap();
    assert_eq!(pack.types[0].type_ref, TypeRef::backend("egui", "Slider"));
    let mut registry = TypeRegistry::with_builtins();
    pack.register(&mut registry).unwrap();
    let fill = registry
        .property_def(&builtin::button(), "egui", "fill")
        .unwrap();
    assert_eq!(
        fill.default,
        Some(Value::Color(Color::rgb(0x33, 0x66, 0xff)))
    );
    assert_eq!(fill.platforms.len(), 2);
    assert!(
        registry
            .property_def(&builtin::button(), "slint", "small")
            .is_none()
    );

    // Registering twice fails and leaves the registry untouched.
    let before = registry.len();
    assert!(pack.register(&mut registry).is_err());
    assert_eq!(registry.len(), before);

    // Extending with an existing property name is rejected.
    let clash = text
        .replace("[extend.properties.small]", "[extend.properties.caption]")
        .replace("name = \"Slider\"", "name = \"Slider2\"");
    let mut registry = TypeRegistry::with_builtins();
    assert_eq!(
        Pack::parse(&clash).unwrap().register(&mut registry),
        Err(ModelError::DuplicateProperty {
            type_ref: builtin::button(),
            property: "caption".into()
        })
    );

    // Component packs may not extend.
    let found = problems(
        &text
            .replace("kind = \"backend\"", "kind = \"components\"")
            .replace("kind = \"widget\"", ""),
    );
    assert!(
        found
            .iter()
            .any(|p| p.contains("only backend packs may extend")),
        "{found:#?}"
    );
}

#[test]
fn load_errors_name_the_file() {
    let dir = std::env::temp_dir().join(format!("tdr-pack-test-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("broken")).unwrap();
    std::fs::write(dir.join("broken/pack.toml"), "[pack]\nname = ").unwrap();
    std::fs::create_dir_all(dir.join("not-a-pack")).unwrap();
    std::fs::create_dir_all(dir.join("wrong")).unwrap();
    std::fs::write(
        dir.join("wrong/pack.toml"),
        "[pack]\nname = \"wrong\"\nversion = \"1\"\nlicense = \"MIT\"\ngenerated-code-license = \"MIT\"\n",
    )
    .unwrap();
    let (packs, errors) = load_all(&dir);
    assert!(packs.is_empty());
    assert_eq!(errors.len(), 2);
    assert!(
        errors[0].to_string().contains("broken/pack.toml"),
        "{}",
        errors[0]
    );
    // Problems in the meaning name the file as well.
    assert!(
        errors[1].to_string().contains("wrong/pack.toml"),
        "{}",
        errors[1]
    );
    assert_eq!(errors[1].problems().len(), 1);
    assert!(matches!(
        Pack::load(&dir.join("nope")),
        Err(PackError::Io { .. })
    ));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn codegen_sections_are_checked() {
    let found = problems(&with_header(
        r#"
[[component]]
name = "Timer"
[component.codegen]
type = "std::time::Duration"
init = "{% if %}"

[[component]]
name = "Clock"
[component.codegen]
type = "u64"
init = "0"
init-file = "clock.rs.j2"

[[component]]
name = "Disk"
[component.codegen]
type = "u64"
init-file = "../secret.j2"
"#,
    ));
    assert_eq!(found.len(), 3, "{found:#?}");
    assert!(found[0].starts_with("component Timer, codegen: template `init`: syntax error"));
    assert_eq!(
        found[1],
        "component Clock, codegen: needs exactly one of `init` and `init-file`"
    );
    assert!(found[2].contains("must be a relative path inside the pack"));

    // The real packs carry code generation.
    let registry = registry();
    let pool = registry.get(&TypeRef::pack("sqlx", "Pool")).unwrap();
    let codegen = pool.codegen.as_ref().unwrap();
    assert!(codegen.rust_type.contains("PgPool") && codegen.rust_type.contains("SqlitePool"));
    assert!(codegen.is_async);
    let runtime = registry.get(&TypeRef::pack("tokio", "Runtime")).unwrap();
    assert!(runtime.codegen.as_ref().unwrap().entry.is_some());
}

#[test]
fn backend_sections_are_checked() {
    let backend = r#"
[pack]
name = "ui"
version = "0.1.0"
kind = "backend"
license = "MIT"
generated-code-license = "MIT"

[backend]
event-loop = "async"
absolute-positioning = true
platforms = ["linux", "amiga"]
form = "{% for %}"
run = "Ok(())"
run-file = "run.j2"

[[extend]]
type = "core/Label"
[extend.codegen]
type = "String"
init = "{{ rust.text }}.to_string()"
runtime = "x()"
"#;
    let found = problems(backend);
    assert_eq!(found.len(), 4, "{found:#?}");
    assert!(
        found
            .iter()
            .any(|p| p == "backend: unknown platform \"amiga\"")
    );
    assert!(
        found
            .iter()
            .any(|p| p.starts_with("backend: template `form`: syntax error"))
    );
    assert!(
        found
            .iter()
            .any(|p| p == "backend: give either `run` or `run-file`, not both")
    );
    assert!(
        found
            .iter()
            .any(|p| p.ends_with("only non-visual components can provide `entry` or `runtime`"))
    );

    // Valid: descriptor and core-widget code generation are registered.
    let valid = backend
        .replace(", \"amiga\"", "")
        .replace("{% for %}", "impl {{ form.struct }} {}")
        .replace("run-file = \"run.j2\"\n", "")
        .replace("runtime = \"x()\"\n", "");
    let pack = Pack::parse(&valid).unwrap();
    let mut registry = TypeRegistry::with_builtins();
    pack.register(&mut registry).unwrap();
    let descriptor = registry.backend("ui").unwrap();
    assert_eq!(descriptor.event_loop, EventLoop::Async);
    assert!(descriptor.absolute_positioning);
    let label = builtin::label();
    assert_eq!(registry.codegen(&label, "ui").unwrap().rust_type, "String");
    assert!(registry.codegen(&label, "other").is_none());

    // Component packs cannot describe a backend.
    let found = problems(&with_header("[backend]\nevent-loop = \"async\"\n"));
    assert_eq!(
        found,
        ["backend: only packs with kind = \"backend\" may have [backend]"]
    );
}

#[test]
fn license_notice_reaches_the_backend() {
    let text = r#"
[pack]
name = "fancy"
version = "0.1.0"
kind = "backend"
license = "MIT"
generated-code-license = "MIT"
license-notice = """
  The generated project depends on a library with special terms.
"""

[backend]
"#;
    let pack = Pack::parse(text).unwrap();
    assert_eq!(
        pack.info.license_notice.as_deref(),
        Some("The generated project depends on a library with special terms.")
    );
    let mut registry = TypeRegistry::with_builtins();
    pack.register(&mut registry).unwrap();
    assert_eq!(
        registry.backend("fancy").unwrap().license_notice,
        pack.info.license_notice
    );
    // Ohne Hinweis bleibt es leer.
    let plain = Pack::parse(&text.replace("license-notice", "description")).unwrap();
    assert_eq!(plain.info.license_notice, None);
}

#[test]
fn title_is_the_shown_name() {
    let text = r#"
[pack]
name = "freya-gui"
title = "freya"
version = "0.1.0"
kind = "backend"
license = "MIT"
generated-code-license = "MIT"

[backend]
"#;
    let pack = Pack::parse(text).unwrap();
    assert_eq!(pack.info.title, "freya");
    let mut registry = TypeRegistry::with_builtins();
    pack.register(&mut registry).unwrap();
    let backend = registry.backend("freya-gui").unwrap();
    assert_eq!(backend.display_name(), "freya");
    // Ohne `title` gilt der Name.
    let plain = Pack::parse(&text.replace("title = \"freya\"\n", "")).unwrap();
    assert_eq!(plain.info.title, "freya-gui");
    assert_eq!(
        tdr_core::BackendDescriptor::new("egui").display_name(),
        "egui"
    );
}

#[test]
fn backend_gives_every_widget_properties_and_events() {
    let text = r#"
[pack]
name = "shiny"
version = "0.1.0"
kind = "backend"
license = "MIT"
generated-code-license = "MIT"

[backend]
[backend.widget-properties.shadow]
type = "int"
min = 0
category = "Style"
[backend.widget-events.on_hover]
params = [{ name = "inside", type = "bool" }]

[[component]]
name = "Gauge"
kind = "widget"
"#;
    let pack = Pack::parse(text).unwrap();
    let mut registry = TypeRegistry::with_builtins();
    pack.register(&mut registry).unwrap();
    // Kern- und Backend-Widgets bekommen beides, nur unter diesem Backend.
    for ty in [builtin::button(), TypeRef::backend("shiny", "Gauge")] {
        assert!(
            registry.property_def(&ty, "shiny", "shadow").is_some(),
            "{ty}"
        );
        let hover = registry.event_def(&ty, "shiny", "on_hover").unwrap();
        assert_eq!(hover.params[0].name, "inside");
        assert!(registry.property_def(&ty, "egui", "shadow").is_none());
        assert!(registry.event_def(&ty, "egui", "on_hover").is_none());
    }
    // Nicht-visuelle Komponenten nicht.
    let runtime = registry
        .types()
        .find(|t| !t.kind.is_widget())
        .map(|t| t.type_ref.clone());
    if let Some(runtime) = runtime {
        assert!(registry.property_def(&runtime, "shiny", "shadow").is_none());
    }
}

#[test]
fn backend_files_copy_a_folder() {
    let dir = std::env::temp_dir().join(format!("tdr-pack-dir-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("tool/cmake")).unwrap();
    std::fs::write(dir.join("tool/run.sh"), "echo {{ not a template }}\n").unwrap();
    std::fs::write(dir.join("tool/cmake/tool.cmake"), "# cmake\n").unwrap();
    let header = r#"[pack]
name = "copier"
version = "0.1.0"
kind = "backend"
license = "MIT"
generated-code-license = "MIT"

[backend]
"#;
    std::fs::write(
        dir.join("pack.toml"),
        format!("{header}\n[[backend.files]]\npath = \"flutter/tool\"\ndir = \"tool\"\nowner = \"user\"\n"),
    )
    .unwrap();
    let pack = Pack::load(&dir).unwrap();
    let mut registry = TypeRegistry::with_builtins();
    pack.register(&mut registry).unwrap();
    let files = &registry.backend("copier").unwrap().files;
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(
        paths,
        ["flutter/tool/cmake/tool.cmake", "flutter/tool/run.sh"]
    );
    assert!(files.iter().all(|f| f.raw && f.owner == FileOwner::User));
    assert_eq!(files[1].template, "echo {{ not a template }}\n");
    // Ordner außerhalb des Packs und dir zusammen mit template sind Fehler.
    for bad in ["dir = \"../x\"", "dir = \"tool\"\ntemplate = \"x\""] {
        std::fs::write(
            dir.join("pack.toml"),
            format!("{header}\n[[backend.files]]\npath = \"p\"\n{bad}\n"),
        )
        .unwrap();
        assert!(Pack::load(&dir).is_err(), "{bad}");
    }
    std::fs::remove_dir_all(dir).ok();
}
