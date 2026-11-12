//! Editing a data grid against real databases: a generated egui project
//! (egui widgets are plain state, so `App::init` runs without a window)
//! changes, adds and deletes rows of `freya_edit` through the generated
//! `<grid>_…` methods; the program then reads the table back.
//!
//! SQLite always runs (a file in the temp folder); PostgreSQL and MySQL /
//! MariaDB when `FREYA_TDE_TEST_POSTGRES=postgres://…` or
//! `FREYA_TDE_TEST_MYSQL=mysql://…` is set. The first run builds egui and
//! SQLx.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

use tdr_core::codegen::{generate, write};
use tdr_core::data::{COLUMNS, DATA_SOURCE};
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

/// `services` (runtime, pool, writable query on `freya_edit`) and
/// `main_form` with an editable egui grid.
fn forms(driver: &str, url: &str) -> Vec<Form> {
    let mut services = Form::data_module(name("services"), "egui");
    services
        .add_component(Component::new(
            name("runtime"),
            TypeRef::pack("tokio", "Runtime"),
        ))
        .unwrap();
    services
        .add_component(
            Component::new(name("db"), TypeRef::pack("sqlx", "Pool"))
                .with("driver", Value::Enum(driver.into()))
                .with("url", url),
        )
        .unwrap();
    services
        .add_component(
            Component::new(name("rows"), TypeRef::pack("sqlx", "Query"))
                .with("pool", Value::reference(name("db")))
                .with(
                    "sql",
                    Value::Code("\"SELECT id, name, note FROM freya_edit ORDER BY id\"".into()),
                )
                .with(
                    COLUMNS,
                    texts(&["id: i64", "name: String", "note: Option<String>"]),
                )
                .with(data::TABLE, "freya_edit")
                .with(data::KEY, "id"),
        )
        .unwrap();
    let mut main = Form::new(
        name("main_form"),
        "egui",
        Widget::new(name("window"), builtin::form()),
    );
    main.add_widget(
        &name("window"),
        Widget::from_component(
            Component::new(name("grid"), TypeRef::backend("egui", "DataGrid"))
                .with(
                    DATA_SOURCE,
                    Value::reference_on(name("services"), name("rows")),
                )
                .with(data::FIELDS, texts(&["name", "note"]))
                .with(data::EDITABLE, true)
                .with(data::LOAD, Value::Enum("Manual".into()))
                .on(data::ON_POST_ERROR, HandlerName::new("failed").unwrap()),
        ),
    )
    .unwrap();
    vec![services, main]
}

/// The program: create the table, then edit through the grid and print the
/// table after every step.
fn main_rs(create: &str) -> String {
    format!(
        r#"
#[path = "app.gen.rs"]
mod app;

#[allow(dead_code)]
fn started(_app: &mut app::App) -> Result<(), app::Error> {{
    Ok(())
}}

/// Waits until something arrives in the form (a load or a write).
fn settle(app: &mut app::App) {{
    for _ in 0..200 {{
        if app.main_form.freya_take_data() {{
            return;
        }}
        std::thread::sleep(std::time::Duration::from_millis(25));
    }}
    panic!("nothing arrived");
}}

fn table(runtime: &tokio::runtime::Runtime, app: &app::App) -> String {{
    let rows: Vec<(i64, String, Option<String>)> = runtime
        .block_on(
            sqlx::query_as("SELECT id, name, note FROM freya_edit ORDER BY id")
                .fetch_all(&app.services.db),
        )
        .unwrap();
    rows.iter()
        .map(|(_, name, note)| format!("{{name}}/{{}}", note.as_deref().unwrap_or("-")))
        .collect::<Vec<_>>()
        .join(",")
}}

fn main() {{
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _guard = runtime.enter();
    let mut app = runtime.block_on(app::App::init()).unwrap();
    for statement in [{create}] {{
        runtime.block_on(sqlx::query(statement).execute(&app.services.db)).unwrap();
    }}
    app.main_form.grid_reload();
    settle(&mut app);
    println!("loaded={{}}", app.main_form.grid_rows.len());

    // Ändern: Name und Notiz der ersten Zeile, Enter.
    app.main_form.grid_edit_cell(0, 0, "Apfelbaum").unwrap();
    app.main_form.grid_edit_cell(0, 1, "").unwrap();
    assert!(app.main_form.grid_post());
    settle(&mut app);
    println!("updated={{}}", table(&runtime, &app));

    // Neu: eine Zeile anfügen, füllen, speichern (danach wird neu geladen).
    assert!(app.main_form.grid_insert());
    let last = app.main_form.grid_rows.len() - 1;
    app.main_form.grid_edit_cell(last, 0, "Quitte").unwrap();
    app.main_form.grid_edit_cell(last, 1, "neu").unwrap();
    assert!(app.main_form.grid_post());
    settle(&mut app);
    settle(&mut app);
    println!("inserted={{}} rows={{}}", table(&runtime, &app), app.main_form.grid_rows.len());

    // Löschen: die zweite Zeile.
    app.main_form.grid.value.selected = Some(1);
    assert!(app.main_form.grid_delete());
    settle(&mut app);
    println!("deleted={{}}", table(&runtime, &app));
}}
"#
    )
}

const FORM_RS: &str = r#"
include!("main_form.gen.rs");

impl MainForm {
    pub fn failed(&mut self, error: &str) {
        println!("error={error}");
    }
}
"#;

fn run(driver: &str, url: &str, create: &[&str]) {
    let dir =
        std::env::temp_dir().join(format!("freya-write-back-{}-{driver}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let project = ProjectFile::new("write_back", "1.95", "egui").unwrap();
    let generation = generate(
        &project,
        &forms(driver, url),
        &registry(),
        &BTreeMap::<String, String>::new(),
    );
    assert!(!generation.has_errors(), "{:#?}", generation.diagnostics);
    write(&dir, &generation).unwrap();
    let create: Vec<String> = create.iter().map(|s| format!("{s:?}")).collect();
    std::fs::write(dir.join("src/main.rs"), main_rs(&create.join(", "))).unwrap();
    std::fs::write(dir.join("src/main_form.rs"), FORM_RS).unwrap();
    let mut command = Command::new("cargo");
    command.args(["run", "--quiet"]).current_dir(&dir).env(
        "CARGO_TARGET_DIR",
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("write-back-target"),
    );
    if std::env::var_os("FREYA_TDE_CHECK_OFFLINE").is_some() {
        command.arg("--offline");
    }
    let output = command.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{driver}:\n{stdout}\n{stderr}");
    for line in [
        "loaded=2",
        "updated=Apfelbaum/-,Birne/saftig",
        "inserted=Apfelbaum/-,Birne/saftig,Quitte/neu rows=3",
        "deleted=Apfelbaum/-,Quitte/neu",
    ] {
        assert!(
            stdout.contains(line),
            "{driver}: {line}\n{stdout}\n{stderr}"
        );
    }
    assert!(!stdout.contains("error="), "{driver}: {stdout}");
    let _ = std::fs::remove_dir_all(&dir);
}

const ROWS: &str =
    "INSERT INTO freya_edit (name, note) VALUES ('Apfel', 'rot'), ('Birne', 'saftig')";

#[test]
fn sqlite() {
    let file = std::env::temp_dir().join(format!("freya-write-back-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&file);
    run(
        "Sqlite",
        &format!("sqlite://{}?mode=rwc", file.display()),
        &[
            "CREATE TABLE freya_edit (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, note TEXT)",
            ROWS,
        ],
    );
    let _ = std::fs::remove_file(&file);
}

#[test]
fn postgres() {
    let Ok(url) = std::env::var("FREYA_TDE_TEST_POSTGRES") else {
        eprintln!("FREYA_TDE_TEST_POSTGRES not set, skipped");
        return;
    };
    run(
        "Postgres",
        &url,
        &[
            "DROP TABLE IF EXISTS freya_edit",
            "CREATE TABLE freya_edit (id BIGSERIAL PRIMARY KEY, name TEXT NOT NULL, note TEXT)",
            ROWS,
        ],
    );
}

#[test]
fn mysql() {
    let Ok(url) = std::env::var("FREYA_TDE_TEST_MYSQL") else {
        eprintln!("FREYA_TDE_TEST_MYSQL not set, skipped");
        return;
    };
    run(
        "MySql",
        &url,
        &[
            "DROP TABLE IF EXISTS freya_edit",
            "CREATE TABLE freya_edit (id BIGINT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(100) NOT NULL, note VARCHAR(100))",
            ROWS,
        ],
    );
}
