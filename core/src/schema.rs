//! Database schemas at design time: the protocol of the helper program
//! `freya-tde-db` and what it reports.
//!
//! The IDE does not link database drivers. It starts `freya-tde-db` as a
//! separate process (like rust-analyzer or a debug adapter) and talks JSON
//! lines over stdin/stdout: one [`Request`] per line in, one [`Reply`] per
//! line out, matched by `id`. A hanging connection or a crash in a driver
//! cannot take the IDE down.
//!
//! *Datenbankschemas zur Entwurfszeit: Protokoll des Helper-Programms
//! `freya-tde-db` (JSON-Zeilen über stdio) und seine Ergebnisse.*
//!
//! ```text
//! → {"id":1,"request":{"kind":"test","driver":"Sqlite","url":"sqlite://app.db"}}
//! ← {"id":1,"ok":{"kind":"connected","server":"SQLite 3.50.4"}}
//! → {"id":2,"request":{"kind":"describe","driver":"Sqlite","url":"sqlite://app.db","sql":"SELECT id, name FROM customers"}}
//! ← {"id":2,"ok":{"kind":"described","columns":[{"name":"id","sql_type":"INTEGER","nullable":false,"rust_type":"i64"}, …],"parameters":0}}
//! ← {"id":3,"error":{"code":"connect_failed","detail":"…"}}
//! ```
//!
//! SQL types become Rust types with [`rust_type`]; types that need an extra
//! SQLx feature (`chrono`, `json`, …) name it in [`TypeMapping::feature`].

use serde::{Deserialize, Serialize};

/// The database driver, as the SQLx pool's `driver` property.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Driver {
    Postgres,
    MySql,
    Sqlite,
    /// Redis (and compatible servers such as Valkey): keys instead of tables.
    Redis,
}

impl Driver {
    pub const ALL: [Driver; 4] = [
        Driver::Postgres,
        Driver::MySql,
        Driver::Sqlite,
        Driver::Redis,
    ];

    /// From the property value (`"Postgres"`, `"MySql"`, `"Sqlite"`).
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|d| d.name() == name)
    }

    pub fn name(self) -> &'static str {
        match self {
            Driver::Postgres => "Postgres",
            Driver::MySql => "MySql",
            Driver::Sqlite => "Sqlite",
            Driver::Redis => "Redis",
        }
    }

    /// SQL databases have tables and columns; Redis has keys.
    pub fn is_sql(self) -> bool {
        self != Driver::Redis
    }
}

/// Where to connect.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Connection {
    pub driver: Driver,
    /// Connection URL as in the pool (`postgres://…`, `mysql://…`,
    /// `sqlite://app.db`; relative SQLite paths from the project folder).
    pub url: String,
}

/// A request to the helper.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Request {
    /// Connect and report the server version.
    Test {
        #[serde(flatten)]
        connection: Connection,
    },
    /// Tables, views and their columns, functions and procedures.
    Schema {
        #[serde(flatten)]
        connection: Connection,
    },
    /// The result columns and parameter count of an SQL statement, without
    /// running it (prepared statement).
    Describe {
        #[serde(flatten)]
        connection: Connection,
        sql: String,
    },
    /// Redis: keys matching `pattern` (`SCAN MATCH`), at most `limit`, with
    /// type, time to live, length and a preview of the value.
    Keys {
        #[serde(flatten)]
        connection: Connection,
        pattern: String,
        limit: usize,
    },
    /// Ends the helper.
    Quit,
}

/// One line sent to the helper.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub id: u64,
    pub request: Request,
}

/// One line the helper answers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reply {
    pub id: u64,
    #[serde(flatten)]
    pub outcome: Outcome,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Ok(Answer),
    Error(DbError),
}

/// A successful answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Answer {
    Connected { server: String },
    Schema(Schema),
    Described(Described),
    Keys(Keys),
    Bye,
}

/// A failure with a stable code (`snake_case`) and the driver's message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{code}: {detail}")]
pub struct DbError {
    pub code: String,
    pub detail: String,
}

/// Stable error codes.
pub mod codes {
    /// The line was not a valid request.
    pub const BAD_REQUEST: &str = "bad_request";
    /// The URL could not be parsed or does not fit the driver.
    pub const BAD_URL: &str = "bad_url";
    /// No connection (host, port, file missing, timeout).
    pub const CONNECT_FAILED: &str = "connect_failed";
    /// The server refused user or password.
    pub const AUTH_FAILED: &str = "auth_failed";
    /// The statement or a catalog query failed (syntax, unknown table …).
    pub const QUERY_FAILED: &str = "query_failed";
    /// The request does not fit the driver (tables of Redis, keys of SQL).
    pub const UNSUPPORTED: &str = "unsupported";
    /// The helper program was not found (IDE side).
    pub const HELPER_MISSING: &str = "helper_missing";
    /// The helper did not answer in time and was stopped (IDE side).
    pub const TIMEOUT: &str = "timeout";
    /// The helper ended or wrote something unreadable (IDE side).
    pub const HELPER_FAILED: &str = "helper_failed";
}

impl DbError {
    pub fn new(code: &str, detail: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            detail: detail.into(),
        }
    }
}

/// The objects of a database, sorted by schema and name.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Schema {
    pub server: String,
    pub tables: Vec<Table>,
    pub routines: Vec<Routine>,
}

impl Schema {
    /// A table or view by name, with or without schema (`public.customers`).
    pub fn table(&self, name: &str) -> Option<&Table> {
        let (schema, bare) = match name.rsplit_once('.') {
            Some((s, n)) => (Some(s), n),
            None => (None, name),
        };
        self.tables
            .iter()
            .find(|t| t.name == bare && schema.is_none_or(|s| t.schema == s))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TableKind {
    Table,
    View,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Table {
    /// Schema (PostgreSQL), database (MySQL) or `main` (SQLite).
    pub schema: String,
    pub name: String,
    pub kind: TableKind,
    pub columns: Vec<Column>,
}

/// A column of a table, view or query result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    /// The type as the database names it (`int4`, `VARCHAR`, `INTEGER`).
    pub sql_type: String,
    /// `None` if unknown (expressions); such columns get `Option<…>`.
    pub nullable: Option<bool>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub primary_key: bool,
    /// The Rust type for `columns` (`i64`, `Option<String>`); `None` if
    /// SQLx cannot decode it without help.
    pub rust_type: Option<String>,
    /// SQLx feature the Rust type needs (`chrono`, `json`, `uuid` …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
}

impl Column {
    /// A column with type mapping for `driver` (nullable or unknown →
    /// `Option<…>`).
    pub fn new(
        driver: Driver,
        name: impl Into<String>,
        sql_type: impl Into<String>,
        nullable: Option<bool>,
    ) -> Self {
        let sql_type = sql_type.into();
        let mapping = rust_type(driver, &sql_type);
        let rust_type = mapping.as_ref().map(|m| {
            if nullable == Some(false) {
                m.rust_type.clone()
            } else {
                format!("Option<{}>", m.rust_type)
            }
        });
        Self {
            name: name.into(),
            sql_type,
            nullable,
            primary_key: false,
            rust_type,
            feature: mapping.and_then(|m| m.feature),
        }
    }

    /// The entry for the `columns` property (`name: Type`), if the type is
    /// known.
    pub fn columns_entry(&self) -> Option<String> {
        self.rust_type
            .as_ref()
            .map(|t| format!("{}: {t}", self.name))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoutineKind {
    Function,
    Procedure,
}

/// A stored function or procedure (PostgreSQL, MySQL).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Routine {
    pub schema: String,
    pub name: String,
    pub kind: RoutineKind,
    /// Parameters as the database writes them (`id integer, name text`).
    pub arguments: String,
    /// Return type of functions.
    pub returns: Option<String>,
}

/// The result of [`Request::Describe`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Described {
    pub columns: Vec<Column>,
    /// Number of bind parameters, if the driver knows it.
    pub parameters: Option<usize>,
}

impl Described {
    /// The `columns` property of a data source; columns without a known
    /// Rust type are left out (and listed in [`Described::unmapped`]).
    pub fn columns_property(&self) -> Vec<String> {
        self.columns
            .iter()
            .filter_map(Column::columns_entry)
            .collect()
    }

    pub fn unmapped(&self) -> Vec<&Column> {
        self.columns
            .iter()
            .filter(|c| c.rust_type.is_none())
            .collect()
    }
}

/// The result of [`Request::Keys`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Keys {
    pub server: String,
    /// Sorted by key.
    pub keys: Vec<RedisKey>,
    /// More keys match than `limit`.
    pub truncated: bool,
}

/// A Redis key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RedisKey {
    pub key: String,
    /// `string`, `list`, `set`, `zset`, `hash`, `stream` …
    pub kind: String,
    /// Seconds to live; `None` = no expiry.
    pub ttl: Option<i64>,
    /// Length (characters, entries, members, fields).
    pub length: Option<u64>,
    /// The first few values (`field = value` for hashes, `member (score)`
    /// for sorted sets), shortened.
    pub preview: Vec<String>,
}

/// How SQLx decodes a SQL type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeMapping {
    pub rust_type: String,
    /// SQLx feature needed for it, if any.
    pub feature: Option<String>,
}

fn plain(t: &str) -> Option<TypeMapping> {
    Some(TypeMapping {
        rust_type: t.to_owned(),
        feature: None,
    })
}

fn with(t: &str, feature: &str) -> Option<TypeMapping> {
    Some(TypeMapping {
        rust_type: t.to_owned(),
        feature: Some(feature.to_owned()),
    })
}

/// The Rust type SQLx decodes `sql_type` into (case-insensitive, length
/// and precision like `varchar(40)` ignored). `None`: no direct mapping.
pub fn rust_type(driver: Driver, sql_type: &str) -> Option<TypeMapping> {
    let lower = sql_type.trim().to_ascii_lowercase();
    // `tinyint(1)` ist in MySQL ein Boolean, sonst zählen Klammern nicht.
    if driver == Driver::MySql && lower.starts_with("tinyint(1)") {
        return plain("bool");
    }
    let base = lower.split('(').next().unwrap_or_default().trim();
    let unsigned = lower.contains("unsigned");
    let base = base.trim_end_matches(" unsigned").trim();
    match driver {
        Driver::Postgres => match base {
            "bool" | "boolean" => plain("bool"),
            "char" | "\"char\"" => plain("i8"),
            "int2" | "smallint" | "smallserial" => plain("i16"),
            "int4" | "int" | "integer" | "serial" => plain("i32"),
            "int8" | "bigint" | "bigserial" => plain("i64"),
            "oid" => plain("sqlx::postgres::types::Oid"),
            "float4" | "real" => plain("f32"),
            "float8" | "double precision" => plain("f64"),
            "text" | "varchar" | "character varying" | "bpchar" | "character" | "name"
            | "citext" => plain("String"),
            "bytea" => plain("Vec<u8>"),
            "numeric" | "decimal" => with("rust_decimal::Decimal", "rust_decimal"),
            "uuid" => with("uuid::Uuid", "uuid"),
            "json" | "jsonb" => with("serde_json::Value", "json"),
            "timestamptz" | "timestamp with time zone" => {
                with("chrono::DateTime<chrono::Utc>", "chrono")
            }
            "timestamp" | "timestamp without time zone" => with("chrono::NaiveDateTime", "chrono"),
            "date" => with("chrono::NaiveDate", "chrono"),
            "time" | "time without time zone" => with("chrono::NaiveTime", "chrono"),
            "_text" | "text[]" | "_varchar" => plain("Vec<String>"),
            "_int4" | "integer[]" | "int4[]" => plain("Vec<i32>"),
            "_int8" | "bigint[]" | "int8[]" => plain("Vec<i64>"),
            "_bool" | "boolean[]" => plain("Vec<bool>"),
            _ => None,
        },
        Driver::MySql => match (base, unsigned) {
            ("boolean" | "bool", _) => plain("bool"),
            ("tinyint", false) => plain("i8"),
            ("tinyint", true) => plain("u8"),
            ("smallint", false) => plain("i16"),
            ("smallint", true) => plain("u16"),
            ("mediumint" | "int" | "integer", false) => plain("i32"),
            ("mediumint" | "int" | "integer", true) => plain("u32"),
            ("bigint", false) => plain("i64"),
            ("bigint", true) => plain("u64"),
            ("float", _) => plain("f32"),
            ("double" | "real", _) => plain("f64"),
            (
                "char" | "varchar" | "text" | "tinytext" | "mediumtext" | "longtext" | "enum"
                | "set",
                _,
            ) => plain("String"),
            ("binary" | "varbinary" | "blob" | "tinyblob" | "mediumblob" | "longblob", _) => {
                plain("Vec<u8>")
            }
            ("decimal" | "numeric", _) => with("rust_decimal::Decimal", "rust_decimal"),
            ("json", _) => with("serde_json::Value", "json"),
            ("datetime", _) => with("chrono::NaiveDateTime", "chrono"),
            ("timestamp", _) => with("chrono::DateTime<chrono::Utc>", "chrono"),
            ("date", _) => with("chrono::NaiveDate", "chrono"),
            ("time", _) => with("chrono::NaiveTime", "chrono"),
            _ => None,
        },
        // SQLite: Typaffinität nach den Regeln von SQLite (Abschnitt 3.1).
        Driver::Sqlite => match base {
            "bool" | "boolean" => plain("bool"),
            "date" => with("chrono::NaiveDate", "chrono"),
            "datetime" | "timestamp" => with("chrono::NaiveDateTime", "chrono"),
            "time" => with("chrono::NaiveTime", "chrono"),
            "" | "null" => None,
            t if t.contains("int") => plain("i64"),
            t if t.contains("char") || t.contains("clob") || t.contains("text") => plain("String"),
            t if t.contains("blob") => plain("Vec<u8>"),
            t if t.contains("real") || t.contains("floa") || t.contains("doub") => plain("f64"),
            // NUMERIC-Affinität: als f64 lesbar.
            _ => plain("f64"),
        },
        // Redis-Werte sind Text; die Spalten kommen aus dem Pack.
        Driver::Redis => plain("String"),
    }
}

/// The text of a Rust string literal (`"SELECT …"`, `r#"…"#`), as SQL
/// properties hold it; `None` for other expressions.
pub fn string_literal(code: &str) -> Option<String> {
    syn::parse_str::<syn::LitStr>(code.trim())
        .ok()
        .map(|l| l.value())
}
