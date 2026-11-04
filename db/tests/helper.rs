//! The helper as the IDE uses it: a process, JSON lines over stdio.
//!
//! SQLite always runs. PostgreSQL and MySQL/MariaDB run when a server is
//! given: `FREYA_TDE_TEST_POSTGRES=postgres://…`, `FREYA_TDE_TEST_MYSQL=mysql://…`
//! (the test creates and drops its own tables `freya_customers`, `freya_orders`).

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use sqlx::Executor;
use tdr_core::schema::*;

struct Helper {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next: u64,
}

impl Helper {
    fn start(dir: &std::path::Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_freya-tde-db"))
            .current_dir(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            stdin,
            stdout,
            next: 1,
        }
    }

    fn ask(&mut self, request: Request) -> Outcome {
        let id = self.next;
        self.next += 1;
        let line = serde_json::to_string(&Envelope { id, request }).unwrap();
        writeln!(self.stdin, "{line}").unwrap();
        let mut answer = String::new();
        self.stdout.read_line(&mut answer).unwrap();
        let reply: Reply = serde_json::from_str(&answer).unwrap();
        assert_eq!(reply.id, id);
        reply.outcome
    }

    fn ok(&mut self, request: Request) -> Answer {
        match self.ask(request) {
            Outcome::Ok(answer) => answer,
            Outcome::Error(e) => panic!("{e}"),
        }
    }

    fn quit(mut self) {
        assert_eq!(self.ok(Request::Quit), Answer::Bye);
        assert!(self.child.wait().unwrap().success());
    }
}

fn connection(driver: Driver, url: &str) -> Connection {
    Connection {
        driver,
        url: url.to_owned(),
    }
}

/// Tables `freya_customers` (id, name, note) and view `freya_names`; the
/// statements fit all three databases.
const SETUP: [&str; 3] = [
    "CREATE TABLE freya_customers (id BIGINT NOT NULL PRIMARY KEY, name VARCHAR(80) NOT NULL, note TEXT, active BOOLEAN NOT NULL)",
    "CREATE TABLE freya_orders (id BIGINT NOT NULL PRIMARY KEY, customer BIGINT NOT NULL, amount DOUBLE PRECISION)",
    "CREATE VIEW freya_names AS SELECT id, name FROM freya_customers",
];
const TEARDOWN: [&str; 3] = [
    "DROP VIEW IF EXISTS freya_names",
    "DROP TABLE IF EXISTS freya_orders",
    "DROP TABLE IF EXISTS freya_customers",
];

/// Schema and describe against a prepared database.
fn check(helper: &mut Helper, connection: Connection, placeholder: &str) {
    let Answer::Connected { server } = helper.ok(Request::Test {
        connection: connection.clone(),
    }) else {
        panic!()
    };
    assert!(!server.is_empty());

    let Answer::Schema(schema) = helper.ok(Request::Schema {
        connection: connection.clone(),
    }) else {
        panic!()
    };
    let customers = schema.table("freya_customers").expect("table found");
    assert_eq!(customers.kind, TableKind::Table);
    let names: Vec<&str> = customers.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["id", "name", "note", "active"]);
    let id = &customers.columns[0];
    assert!(id.primary_key);
    assert_eq!(id.nullable, Some(false));
    assert_eq!(id.rust_type.as_deref(), Some("i64"), "{id:?}");
    assert_eq!(customers.columns[1].rust_type.as_deref(), Some("String"));
    assert_eq!(
        customers.columns[2].rust_type.as_deref(),
        Some("Option<String>")
    );
    assert_eq!(customers.columns[3].rust_type.as_deref(), Some("bool"));
    let view = schema.table("freya_names").expect("view found");
    assert_eq!(view.kind, TableKind::View);

    let sql = format!(
        "SELECT c.id, c.name, c.note, COUNT(o.id) AS orders FROM freya_customers c \
         LEFT JOIN freya_orders o ON o.customer = c.id WHERE c.active = {placeholder} \
         GROUP BY c.id, c.name, c.note"
    );
    let Answer::Described(described) = helper.ok(Request::Describe {
        connection: connection.clone(),
        sql,
    }) else {
        panic!()
    };
    let entries = described.columns_property();
    assert_eq!(entries[0], "id: i64", "{described:#?}");
    assert_eq!(entries[1], "name: String");
    assert_eq!(entries[2], "note: Option<String>");
    // Ausdrücke: SQLite nennt keinen Typ, dann bleibt die Spalte offen.
    match entries.get(3) {
        Some(orders) => assert!(orders.starts_with("orders: "), "{entries:?}"),
        None => assert_eq!(described.unmapped()[0].name, "orders"),
    }
    assert_eq!(described.parameters, Some(1));

    // Fehler mit stabilem Code.
    let Outcome::Error(error) = helper.ask(Request::Describe {
        connection,
        sql: "SELECT nothing FROM freya_missing".into(),
    }) else {
        panic!()
    };
    assert_eq!(error.code, codes::QUERY_FAILED);
}

#[tokio::test]
async fn sqlite() {
    let dir = std::env::temp_dir().join(format!("freya-db-helper-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    {
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(dir.join("shop.db"))
            .create_if_missing(true);
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        for statement in SETUP {
            pool.execute(statement).await.unwrap();
        }
        pool.close().await;
    }

    let mut helper = Helper::start(&dir);
    // Relativer Pfad: aus dem Arbeitsordner (dem Projektordner).
    check(
        &mut helper,
        connection(Driver::Sqlite, "sqlite://shop.db"),
        "?",
    );

    // Fehlende Datei wird nicht angelegt.
    let Outcome::Error(error) = helper.ask(Request::Test {
        connection: connection(Driver::Sqlite, "sqlite://missing.db"),
    }) else {
        panic!()
    };
    assert_eq!(error.code, codes::CONNECT_FAILED);
    assert!(!dir.join("missing.db").exists());

    // Unlesbare Zeile → bad_request, der Helfer läuft weiter.
    writeln!(helper.stdin, "kein json").unwrap();
    let mut line = String::new();
    helper.stdout.read_line(&mut line).unwrap();
    let reply: Reply = serde_json::from_str(&line).unwrap();
    assert!(matches!(reply.outcome, Outcome::Error(e) if e.code == codes::BAD_REQUEST));

    let Outcome::Error(error) = helper.ask(Request::Test {
        connection: connection(Driver::Postgres, "nicht-url"),
    }) else {
        panic!()
    };
    assert_eq!(error.code, codes::BAD_URL);

    helper.quit();
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn postgres() {
    let Ok(url) = std::env::var("FREYA_TDE_TEST_POSTGRES") else {
        eprintln!("FREYA_TDE_TEST_POSTGRES not set, skipped");
        return;
    };
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    for s in TEARDOWN.into_iter().chain(SETUP) {
        pool.execute(s).await.unwrap();
    }
    pool.execute(
        "CREATE OR REPLACE FUNCTION freya_count(active boolean) RETURNS bigint \
         LANGUAGE sql AS 'SELECT COUNT(*) FROM freya_customers WHERE freya_customers.active = $1'",
    )
    .await
    .unwrap();
    let mut helper = Helper::start(&std::env::temp_dir());
    check(&mut helper, connection(Driver::Postgres, &url), "$1");
    let Answer::Schema(schema) = helper.ok(Request::Schema {
        connection: connection(Driver::Postgres, &url),
    }) else {
        panic!()
    };
    let function = schema
        .routines
        .iter()
        .find(|r| r.name == "freya_count")
        .unwrap();
    assert_eq!(function.kind, RoutineKind::Function);
    assert_eq!(function.arguments, "active boolean");
    assert_eq!(function.returns.as_deref(), Some("bigint"));
    helper.quit();
    pool.execute("DROP FUNCTION freya_count(boolean)")
        .await
        .unwrap();
    for s in TEARDOWN {
        pool.execute(s).await.unwrap();
    }
}

#[tokio::test]
async fn mysql() {
    let Ok(url) = std::env::var("FREYA_TDE_TEST_MYSQL") else {
        eprintln!("FREYA_TDE_TEST_MYSQL not set, skipped");
        return;
    };
    let pool = sqlx::MySqlPool::connect(&url).await.unwrap();
    for s in TEARDOWN.into_iter().chain(SETUP) {
        pool.execute(s).await.unwrap();
    }
    pool.execute("DROP PROCEDURE IF EXISTS freya_touch")
        .await
        .unwrap();
    pool.execute(
        "CREATE PROCEDURE freya_touch(IN customer BIGINT) UPDATE freya_customers SET note = 'x' WHERE id = customer",
    )
    .await
    .unwrap();
    let mut helper = Helper::start(&std::env::temp_dir());
    check(&mut helper, connection(Driver::MySql, &url), "?");
    let Answer::Schema(schema) = helper.ok(Request::Schema {
        connection: connection(Driver::MySql, &url),
    }) else {
        panic!()
    };
    let procedure = schema
        .routines
        .iter()
        .find(|r| r.name == "freya_touch")
        .unwrap();
    assert_eq!(procedure.kind, RoutineKind::Procedure);
    // MariaDB schreibt `bigint(20)`, MySQL 8 `bigint`.
    assert!(
        procedure.arguments.starts_with("IN customer bigint"),
        "{procedure:?}"
    );
    helper.quit();
    pool.execute("DROP PROCEDURE freya_touch").await.unwrap();
    for s in TEARDOWN {
        pool.execute(s).await.unwrap();
    }
}
