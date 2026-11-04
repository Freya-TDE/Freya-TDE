//! The database client: finding connection and SQL in the project, the
//! helper process (real with SQLite, fake for time limits and failures).

use std::path::PathBuf;
use std::time::Duration;

use freya_tde::database::{Database, connection, lookup, pools, query_sql};
use tdr_core::schema::{Driver, codes};
use tdr_core::*;

fn name(n: &str) -> ComponentName {
    ComponentName::new(n).unwrap()
}

fn forms() -> Vec<Form> {
    let mut services = Form::data_module(name("services"), "gtk4");
    services
        .add_component(
            Component::new(name("db"), TypeRef::pack("sqlx", "Pool"))
                .with("driver", Value::Enum("Sqlite".into()))
                .with("url", "sqlite://shop.db"),
        )
        .unwrap();
    services
        .add_component(
            Component::new(name("customers"), TypeRef::pack("sqlx", "Query"))
                .with("pool", Value::reference(name("db")))
                .with(
                    "sql",
                    Value::Code("r#\"SELECT id, name FROM customers WHERE name = \"x\"\"#".into()),
                ),
        )
        .unwrap();
    services
        .add_component(
            Component::new(name("dynamic"), TypeRef::pack("sqlx", "Query"))
                .with("sql", Value::Code("format!(\"SELECT {}\", 1)".into())),
        )
        .unwrap();
    vec![services]
}

fn path(p: &str) -> ComponentPath {
    p.parse().unwrap()
}

#[test]
fn connection_and_sql_from_the_project() {
    let forms = forms();
    assert_eq!(pools(&forms), [path("services::db")]);
    let c = connection(&forms, &path("services::customers")).unwrap();
    assert_eq!(c.driver, Driver::Sqlite);
    assert_eq!(c.url, "sqlite://shop.db");
    assert_eq!(connection(&forms, &path("services::db")).unwrap(), c);
    assert_eq!(
        query_sql(&forms, &path("services::customers")).unwrap(),
        "SELECT id, name FROM customers WHERE name = \"x\""
    );
    assert_eq!(
        connection(&forms, &path("services::dynamic"))
            .unwrap_err()
            .code,
        lookup::NO_POOL
    );
    assert_eq!(
        query_sql(&forms, &path("services::dynamic"))
            .unwrap_err()
            .code,
        lookup::SQL_NOT_LITERAL
    );
    assert_eq!(
        connection(&forms, &path("services::nope"))
            .unwrap_err()
            .code,
        lookup::NOT_FOUND
    );
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("freya-db-client-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(unix)]
#[test]
fn helper_failures_have_codes() {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch("fake");
    let forms = forms();
    let c = connection(&forms, &path("services::db")).unwrap();

    let mut missing = Database::new(&dir).with_program(dir.join("does-not-exist"));
    assert_eq!(missing.test(&c).unwrap_err().code, codes::HELPER_MISSING);

    // Ein Helfer, der nie antwortet: Zeitlimit, danach Neustart.
    let silent = dir.join("silent.sh");
    std::fs::write(&silent, "#!/bin/sh\nexec sleep 30\n").unwrap();
    std::fs::set_permissions(&silent, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut db = Database::new(&dir)
        .with_program(&silent)
        .with_timeout(Duration::from_millis(300));
    assert_eq!(db.test(&c).unwrap_err().code, codes::TIMEOUT);

    // Ein Helfer, der sofort endet.
    let gone = dir.join("gone.sh");
    std::fs::write(&gone, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&gone, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut db = Database::new(&dir).with_program(&gone);
    assert_eq!(db.test(&c).unwrap_err().code, codes::HELPER_FAILED);
    let _ = std::fs::remove_dir_all(&dir);
}

/// With the real helper (built by `cargo test --workspace`) and the
/// `sqlite3` program to create the file; skipped if either is missing.
#[test]
fn real_helper_with_sqlite() {
    let helper = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../target/debug")
        .join(format!("freya-tde-db{}", std::env::consts::EXE_SUFFIX));
    if !helper.is_file() {
        eprintln!("{} not built, skipped", helper.display());
        return;
    }
    let dir = scratch("real");
    let created = std::process::Command::new("sqlite3")
        .arg(dir.join("shop.db"))
        .arg("CREATE TABLE customers (id INTEGER PRIMARY KEY, name TEXT NOT NULL, note TEXT);")
        .status();
    if !created.is_ok_and(|s| s.success()) {
        eprintln!("sqlite3 not available, skipped");
        return;
    }
    let forms = forms();
    let c = connection(&forms, &path("services::customers")).unwrap();
    let mut db = Database::new(&dir).with_program(&helper);
    assert!(db.test(&c).unwrap().starts_with("SQLite "));
    let schema = db.schema(&c).unwrap();
    assert_eq!(schema.tables[0].name, "customers");
    let sql = query_sql(&forms, &path("services::customers")).unwrap();
    let described = db.describe(&c, &sql).unwrap();
    assert_eq!(described.columns_property(), ["id: i64", "name: String"]);
    drop(db);
    let _ = std::fs::remove_dir_all(&dir);
}

fn full_registry() -> TypeRegistry {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut registry = TypeRegistry::with_builtins();
    for dir in ["components", "widgets"] {
        let (packs, _) = tdr_core::pack::load_all(&root.join(dir));
        for pack in packs {
            pack.register(&mut registry).unwrap();
        }
    }
    registry
}

#[test]
fn redis_client_and_read() {
    use freya_tde::database::{is_redis_read, redis_columns};
    let mut cache = Form::data_module(name("cache"), "gtk4");
    cache
        .add_component(
            Component::new(name("redis"), TypeRef::pack("redis", "Client"))
                .with("url", "redis://127.0.0.1:6379/2"),
        )
        .unwrap();
    cache
        .add_component(
            Component::new(name("user"), TypeRef::pack("redis", "Read"))
                .with("client", Value::reference(name("redis")))
                .with("kind", Value::Enum("Hash".into()))
                .with("key", "user:1"),
        )
        .unwrap();
    let forms = vec![cache];
    assert_eq!(pools(&forms), [path("cache::redis")]);
    let c = connection(&forms, &path("cache::user")).unwrap();
    assert_eq!(c.driver, Driver::Redis);
    assert_eq!(c.url, "redis://127.0.0.1:6379/2");
    assert!(is_redis_read(&forms, &path("cache::user")));
    let described = redis_columns(&forms, &path("cache::user"), &full_registry()).unwrap();
    assert_eq!(
        described.columns_property(),
        ["field: String", "value: String"]
    );
}

#[test]
fn passwords_are_hidden() {
    use freya_tde::database::display_url;
    assert_eq!(
        display_url("redis://:geheim@host:6379/0"),
        "redis://:***@host:6379/0"
    );
    assert_eq!(
        display_url("postgres://freya:geheim@db/shop"),
        "postgres://freya:***@db/shop"
    );
    assert_eq!(display_url("sqlite://shop.db"), "sqlite://shop.db");
    assert_eq!(display_url("mysql://root@db/shop"), "mysql://root@db/shop");
}
