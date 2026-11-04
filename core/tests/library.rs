//! Library projects: `src/lib.rs` exports the service container `App` of
//! the data modules; no `main`, no UI, async components on the caller's
//! runtime.
//!
//! With `FREYA_TDE_CHECK_SQLX=1` the library is checked with `cargo check`
//! together with a small program using it.

use std::collections::BTreeMap;
use std::path::PathBuf;

use tdr_core::codegen::{CodegenIssue, Generation, generate, write};
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

fn project() -> ProjectFile {
    let mut project = ProjectFile::new("shopdata", "1.99", "none").unwrap();
    project.kind = ProjectKind::Library;
    project
}

/// `services`: runtime, SQLite pool, query, Redis client.
fn services() -> Form {
    let mut services = Form::data_module(name("services"), "none");
    for component in [
        Component::new(name("runtime"), TypeRef::pack("tokio", "Runtime")),
        Component::new(name("db"), TypeRef::pack("sqlx", "Pool"))
            .with("driver", Value::Enum("Sqlite".into()))
            .with("url", "sqlite://shop.db"),
        Component::new(name("customers"), TypeRef::pack("sqlx", "Query"))
            .with("pool", Value::reference(name("db")))
            .with(
                "sql",
                Value::Code("\"SELECT id, name FROM customers\"".into()),
            ),
        Component::new(name("cache"), TypeRef::pack("redis", "Client"))
            .with("url", "redis://127.0.0.1/"),
    ] {
        services.add_component(component).unwrap();
    }
    services
}

fn file<'g>(generation: &'g Generation, path: &str) -> Option<&'g str> {
    generation
        .files
        .iter()
        .find(|f| f.path == path)
        .map(|f| f.content.as_str())
}

#[test]
fn library_exports_the_service_container() {
    let generation = generate(
        &project(),
        &[services()],
        &registry(),
        &BTreeMap::<String, String>::new(),
    );
    assert!(!generation.has_errors(), "{:#?}", generation.diagnostics);
    assert!(file(&generation, "src/main.rs").is_none());
    let lib = file(&generation, "src/lib.rs").unwrap();
    assert!(lib.contains("pub use app::*;"));
    let app = file(&generation, "src/app.gen.rs").unwrap();
    assert!(
        app.contains("pub async fn init() -> Result<Self, Error>"),
        "{app}"
    );
    assert!(!app.contains("fn run("), "{app}");
    assert!(!app.contains("started"), "{app}");
    assert!(app.contains("tokio::runtime::Handle::current()"));

    if std::env::var_os("FREYA_TDE_CHECK_SQLX").is_some() {
        let dir = std::env::temp_dir().join(format!("freya-lib-check-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        write(&dir.join("shopdata"), &generation).unwrap();
        // Ein Programm, das die Bibliothek benutzt.
        let user = dir.join("user");
        std::fs::create_dir_all(user.join("src")).unwrap();
        std::fs::write(
            user.join("Cargo.toml"),
            "[package]\nname = \"user\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n\
             [dependencies]\nshopdata = { path = \"../shopdata\" }\n\
             tokio = { version = \"1.54\", features = [\"rt-multi-thread\", \"macros\"] }\n",
        )
        .unwrap();
        std::fs::write(
            user.join("src/main.rs"),
            "#[tokio::main]\nasync fn main() -> Result<(), shopdata::Error> {\n\
             let app = shopdata::App::init().await?;\n\
             let _ = (&app.services.db, app.services.customers.1, &app.services.cache);\n\
             Ok(())\n}\n",
        )
        .unwrap();
        let status = std::process::Command::new("cargo")
            .arg("check")
            .current_dir(&user)
            .env(
                "CARGO_TARGET_DIR",
                PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("lib-check-target"),
            )
            .status()
            .unwrap();
        assert!(status.success(), "cargo check of the library user failed");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn library_without_async_components_has_sync_init() {
    let plain = Form::data_module(name("settings"), "none");
    let generation = generate(
        &project(),
        &[plain],
        &registry(),
        &BTreeMap::<String, String>::new(),
    );
    assert!(!generation.has_errors(), "{:#?}", generation.diagnostics);
    let app = file(&generation, "src/app.gen.rs").unwrap();
    assert!(
        app.contains("pub fn init() -> Result<Self, Error>"),
        "{app}"
    );
}

#[test]
fn library_rejects_forms_with_widgets() {
    let form = Form::new(
        name("main_form"),
        "gtk4",
        Widget::new(name("window"), builtin::form()),
    );
    let generation = generate(
        &project(),
        &[services(), form],
        &registry(),
        &BTreeMap::<String, String>::new(),
    );
    assert!(generation.diagnostics.iter().any(|d| matches!(
        &d.issue,
        CodegenIssue::LibraryWithUi(f) if f.as_str() == "main_form"
    )));
    let json = serde_json::to_string(&generation.diagnostics).unwrap();
    assert!(json.contains("library_with_ui"), "{json}");
}
