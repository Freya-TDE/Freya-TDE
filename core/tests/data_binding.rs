//! Data binding without building: list widgets filled from the SQLx query –
//! the generated loader, typed values, and the errors for wrong bindings.
//!
//! With `FREYA_TDE_CHECK_SQLX=1` the generated project is also checked with
//! `cargo check` (downloads SQLx; no database needed).

use std::collections::BTreeMap;
use std::path::PathBuf;

use tdr_core::codegen::{CodegenIssue, Generation, generate, write};
use tdr_core::data::{COLUMNS, DATA_SOURCE, DISPLAY_FIELD, LOAD, VALUE_FIELD};
use tdr_core::pack::load_all;
use tdr_core::*;

fn name(n: &str) -> ComponentName {
    ComponentName::new(n).unwrap()
}

fn registry() -> TypeRegistry {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut registry = TypeRegistry::with_builtins();
    for dir in ["components", "widgets"] {
        let (packs, errors) = load_all(&root.join(dir));
        assert!(errors.is_empty(), "{errors:#?}");
        for pack in packs {
            pack.register(&mut registry).unwrap();
        }
    }
    registry
}

fn texts(items: &[&str]) -> Value {
    Value::List(items.iter().map(|t| Value::Text((*t).into())).collect())
}

/// `services`: runtime, pool, query `customers` (id, name); `main_form`: a
/// GTK drop-down bound to it.
fn forms(bind: impl Fn(Component) -> Component, with_runtime: bool) -> Vec<Form> {
    let mut services = Form::data_module(name("services"), "gtk4");
    if with_runtime {
        services
            .add_component(Component::new(
                name("runtime"),
                TypeRef::pack("tokio", "Runtime"),
            ))
            .unwrap();
    }
    services
        .add_component(
            Component::new(name("db"), TypeRef::pack("sqlx", "Pool"))
                .with("url", "postgres://localhost/shop"),
        )
        .unwrap();
    services
        .add_component(
            Component::new(name("customers"), TypeRef::pack("sqlx", "Query"))
                .with("pool", Value::reference(name("db")))
                .with(
                    "sql",
                    Value::Code("\"SELECT id, name FROM customers WHERE active = $1\"".into()),
                )
                .with("params", Value::List(vec![Value::Code("true".into())]))
                .with(
                    COLUMNS,
                    texts(&["id: i64", "name: String", "note: Option<String>"]),
                ),
        )
        .unwrap();
    let mut main = Form::new(
        name("main_form"),
        "gtk4",
        Widget::new(name("window"), builtin::form()),
    );
    main.add_widget(
        &name("window"),
        Widget::from_component(bind(
            Component::new(name("customer"), TypeRef::backend("gtk4", "DropDown")).with(
                DATA_SOURCE,
                Value::reference_on(name("services"), name("customers")),
            ),
        )),
    )
    .unwrap();
    vec![services, main]
}

fn generation(forms: &[Form]) -> Generation {
    let project = ProjectFile::new("shop", "1.99", "gtk4").unwrap();
    generate(
        &project,
        forms,
        &registry(),
        &BTreeMap::<String, String>::new(),
    )
}

fn binding_errors(generation: &Generation) -> Vec<String> {
    generation
        .diagnostics
        .iter()
        .filter_map(|d| match &d.issue {
            CodegenIssue::DataBinding { message } => Some(message.clone()),
            _ => None,
        })
        .collect()
}

fn file<'g>(generation: &'g Generation, path: &str) -> &'g str {
    &generation
        .files
        .iter()
        .find(|f| f.path == path)
        .unwrap()
        .content
}

#[test]
fn list_widgets_get_binding_properties() {
    let registry = registry();
    let ty = registry.get(&TypeRef::backend("gtk4", "DropDown")).unwrap();
    for p in [DATA_SOURCE, DISPLAY_FIELD, VALUE_FIELD, LOAD] {
        assert!(ty.property(p).is_some(), "{p}");
    }
    let query = registry.get(&TypeRef::pack("sqlx", "Query")).unwrap();
    assert!(query.property(COLUMNS).is_some());
    assert!(
        query
            .provides
            .iter()
            .any(|p| p.capability.as_str() == data::ROWS)
    );
    // Widgets ohne Listen-Haken bekommen nichts.
    assert!(
        registry
            .get(&builtin::button())
            .unwrap()
            .property(DATA_SOURCE)
            .is_none()
    );
}

#[test]
fn generates_loader_with_typed_values() {
    let forms = forms(
        |c| c.with(DISPLAY_FIELD, "name").with(VALUE_FIELD, "id"),
        true,
    );
    let generation = generation(&forms);
    assert!(!generation.has_errors(), "{:#?}", generation.diagnostics);
    let form = file(&generation, "src/main_form.gen.rs");
    assert!(form.contains("pub fn customer_reload(&self)"), "{form}");
    assert!(
        form.contains("pub fn customer_selected_value(&self) -> Option<i64>"),
        "{form}"
    );
    assert!(form.contains(".bind(true)"), "{form}");
    assert!(
        form.contains("sqlx::Row::try_get::<i64, _>(row, \"id\")?"),
        "{form}"
    );
    assert!(form.contains("pub customer_values: Vec<i64>"), "{form}");
    assert!(
        form.contains("self.customer_reload();"),
        "load = Start: {form}"
    );
    let app = file(&generation, "src/app.gen.rs");
    assert!(app.contains("pub fn freya_spawn_load"), "{app}");
    assert!(
        app.contains("let main_form__customer__source = services__customers.clone();"),
        "{app}"
    );
    assert!(
        app.contains("let _runtime_guard = runtime.enter();"),
        "{app}"
    );

    // Optionale Spalte als Text; ohne Wertspalte ist der Wert der Text.
    let forms = self::forms(
        |c| {
            c.with(DISPLAY_FIELD, "note")
                .with(LOAD, Value::Enum("Manual".into()))
        },
        true,
    );
    let generation = self::generation(&forms);
    assert!(!generation.has_errors(), "{:#?}", generation.diagnostics);
    let form = file(&generation, "src/main_form.gen.rs");
    assert!(form.contains("unwrap_or_default()"), "{form}");
    assert!(form.contains("-> Option<String>"), "{form}");
    assert!(
        !form.contains("self.customer_reload();"),
        "load = Manual: {form}"
    );

    if std::env::var_os("FREYA_TDE_CHECK_SQLX").is_some() {
        let dir = std::env::temp_dir().join(format!("freya-sqlx-check-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let generation = self::generation(&self::forms(
            |c| c.with(DISPLAY_FIELD, "name").with(VALUE_FIELD, "id"),
            true,
        ));
        write(&dir, &generation).unwrap();
        let status = std::process::Command::new("cargo")
            .arg("check")
            .current_dir(&dir)
            .env(
                "CARGO_TARGET_DIR",
                PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("sqlx-check-target"),
            )
            .status()
            .unwrap();
        assert!(status.success(), "cargo check of the SQLx project failed");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn wrong_bindings_are_reported() {
    let check = |bind: &dyn Fn(Component) -> Component, runtime: bool, expected: &str| {
        let generation = generation(&forms(bind, runtime));
        let errors = binding_errors(&generation);
        assert!(
            errors.iter().any(|e| e.contains(expected)),
            "expected {expected:?} in {errors:#?}"
        );
    };
    check(&|c| c, true, "`display_field` is empty");
    check(
        &|c| c.with(DISPLAY_FIELD, "titel"),
        true,
        "has no column `titel` (columns: id, name, note)",
    );
    check(
        &|c| c.with(DISPLAY_FIELD, "name").with(VALUE_FIELD, "key"),
        true,
        "no column `key`",
    );
    check(
        &|c| {
            c.with(DISPLAY_FIELD, "name").with(
                DATA_SOURCE,
                Value::reference_on(name("services"), name("db")),
            )
        },
        true,
        "not a data source",
    );
}
