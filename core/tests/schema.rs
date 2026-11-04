//! The helper protocol as JSON and the type mapping.

use tdr_core::schema::*;

#[test]
fn protocol_lines() {
    let line = r#"{"id":2,"request":{"kind":"describe","driver":"Sqlite","url":"sqlite://app.db","sql":"SELECT 1"}}"#;
    let envelope: Envelope = serde_json::from_str(line).unwrap();
    assert_eq!(
        envelope.request,
        Request::Describe {
            connection: Connection {
                driver: Driver::Sqlite,
                url: "sqlite://app.db".into()
            },
            sql: "SELECT 1".into()
        }
    );
    assert_eq!(serde_json::to_string(&envelope).unwrap(), line);

    let reply = Reply {
        id: 3,
        outcome: Outcome::Error(DbError::new(codes::CONNECT_FAILED, "no")),
    };
    let text = serde_json::to_string(&reply).unwrap();
    assert_eq!(
        text,
        r#"{"id":3,"error":{"code":"connect_failed","detail":"no"}}"#
    );
    assert_eq!(serde_json::from_str::<Reply>(&text).unwrap(), reply);

    let ok = Reply {
        id: 1,
        outcome: Outcome::Ok(Answer::Connected {
            server: "SQLite 3".into(),
        }),
    };
    let text = serde_json::to_string(&ok).unwrap();
    assert_eq!(
        text,
        r#"{"id":1,"ok":{"kind":"connected","server":"SQLite 3"}}"#
    );
    assert_eq!(serde_json::from_str::<Reply>(&text).unwrap(), ok);
}

#[test]
fn type_mapping() {
    let t = |d, s| rust_type(d, s).map(|m| m.rust_type);
    assert_eq!(t(Driver::Postgres, "int8").as_deref(), Some("i64"));
    assert_eq!(
        t(Driver::Postgres, "VARCHAR(40)").as_deref(),
        Some("String")
    );
    assert_eq!(t(Driver::MySql, "tinyint(1)").as_deref(), Some("bool"));
    assert_eq!(t(Driver::MySql, "BIGINT UNSIGNED").as_deref(), Some("u64"));
    assert_eq!(t(Driver::Sqlite, "INTEGER").as_deref(), Some("i64"));
    assert_eq!(t(Driver::Sqlite, "NVARCHAR(10)").as_deref(), Some("String"));
    assert_eq!(t(Driver::Sqlite, "NULL"), None);
    let ts = rust_type(Driver::Postgres, "timestamptz").unwrap();
    assert_eq!(ts.feature.as_deref(), Some("chrono"));

    let id = Column::new(Driver::Sqlite, "id", "INTEGER", Some(false));
    assert_eq!(id.columns_entry().as_deref(), Some("id: i64"));
    let note = Column::new(Driver::Sqlite, "note", "TEXT", None);
    assert_eq!(
        note.columns_entry().as_deref(),
        Some("note: Option<String>")
    );
}
