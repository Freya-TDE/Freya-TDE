//! The Redis pack: client, the five kinds of `Read` as data sources for
//! lists and a grid, columns implied by the kind.
//!
//! With `FREYA_TDE_CHECK_REDIS=1` the generated project is also checked with
//! `cargo check` (downloads redis-rs; no server needed).

use std::collections::BTreeMap;
use std::path::PathBuf;

use tdr_core::codegen::{Generation, generate, implied_columns, write};
use tdr_core::data::{COLUMNS, DATA_SOURCE, DISPLAY_FIELD, VALUE_FIELD};
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

const KINDS: [(&str, &str, &str); 5] = [
    ("List", "todo", "value"),
    ("Set", "tags", "value"),
    ("SortedSet", "ranking", "member"),
    ("Hash", "user:1", "field"),
    ("Keys", "user:*", "key"),
];

/// `services`: runtime, client, one `Read` per kind; `main_form`: a GTK
/// drop-down per kind and a grid on the sorted set.
fn forms() -> Vec<Form> {
    let mut services = Form::data_module(name("services"), "gtk4");
    services
        .add_component(Component::new(
            name("runtime"),
            TypeRef::pack("tokio", "Runtime"),
        ))
        .unwrap();
    services
        .add_component(
            Component::new(name("cache"), TypeRef::pack("redis", "Client"))
                .with("url", "redis://127.0.0.1/"),
        )
        .unwrap();
    let mut main = Form::new(
        name("main_form"),
        "gtk4",
        Widget::new(name("window"), builtin::form()),
    );
    for (kind, key, display) in KINDS {
        let source = format!("read_{}", kind.to_lowercase());
        services
            .add_component(
                Component::new(name(&source), TypeRef::pack("redis", "Read"))
                    .with("client", Value::reference(name("cache")))
                    .with("kind", Value::Enum(kind.into()))
                    .with("key", key),
            )
            .unwrap();
        main.add_widget(
            &name("window"),
            Widget::from_component(
                Component::new(
                    name(&format!("list_{}", kind.to_lowercase())),
                    TypeRef::backend("gtk4", "DropDown"),
                )
                .with(
                    DATA_SOURCE,
                    Value::reference_on(name("services"), name(&source)),
                )
                .with(DISPLAY_FIELD, display),
            ),
        )
        .unwrap();
    }
    main.add_widget(
        &name("window"),
        Widget::from_component(
            Component::new(name("ranking"), TypeRef::backend("gtk4", "DataGrid"))
                .with(
                    DATA_SOURCE,
                    Value::reference_on(name("services"), name("read_sortedset")),
                )
                .with(VALUE_FIELD, "member"),
        ),
    )
    .unwrap();
    vec![services, main]
}

fn generation(forms: &[Form]) -> Generation {
    let project = ProjectFile::new("cache", "1.99", "gtk4").unwrap();
    generate(
        &project,
        forms,
        &registry(),
        &BTreeMap::<String, String>::new(),
    )
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
fn columns_follow_the_kind() {
    let registry = registry();
    let forms = forms();
    let implied = |source: &str| {
        let element = forms[0].element(&name(source)).unwrap();
        implied_columns(&forms[0], element, &registry).unwrap()
    };
    assert_eq!(implied("read_list"), ["value: String"]);
    assert_eq!(implied("read_sortedset"), ["member: String", "score: f64"]);
    assert_eq!(implied("read_hash"), ["field: String", "value: String"]);
    assert_eq!(implied("read_keys"), ["key: String"]);
}

#[test]
fn redis_sources_fill_lists_and_grids() {
    let generation = generation(&forms());
    assert!(!generation.has_errors(), "{:#?}", generation.diagnostics);
    let app = file(&generation, "src/app.gen.rs");
    assert!(
        app.contains("redis::Client::open(\"redis://127.0.0.1/\")?"),
        "{app}"
    );
    let form = file(&generation, "src/main_form.gen.rs");
    for call in [
        "lrange",
        "smembers",
        "zrange_withscores",
        "hgetall",
        "scan_match",
    ] {
        assert!(form.contains(call), "{call}");
    }
    assert!(form.contains("pub struct RankingRow"), "{form}");
    assert!(form.contains("pub score: f64"), "{form}");
    let cargo = file(&generation, "Cargo.toml");
    assert!(cargo.contains("redis = { version = \"1.8.1\""), "{cargo}");

    // Von Hand gesetzte Spalten gehen vor.
    let mut forms = forms();
    forms[0]
        .component_mut(&name("read_sortedset"))
        .unwrap()
        .set(
            COLUMNS,
            Value::List(vec![
                Value::Text("member: String".into()),
                Value::Text("score: Option<i64>".into()),
            ]),
        );
    let generation = self::generation(&forms);
    assert!(!generation.has_errors(), "{:#?}", generation.diagnostics);
    assert!(file(&generation, "src/main_form.gen.rs").contains("pub score: Option<i64>"));

    if std::env::var_os("FREYA_TDE_CHECK_REDIS").is_some() {
        let dir = std::env::temp_dir().join(format!("freya-redis-check-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        write(&dir, &generation).unwrap();
        let status = std::process::Command::new("cargo")
            .arg("check")
            .current_dir(&dir)
            .env(
                "CARGO_TARGET_DIR",
                PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("redis-check-target"),
            )
            .status()
            .unwrap();
        assert!(status.success(), "cargo check of the Redis project failed");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
