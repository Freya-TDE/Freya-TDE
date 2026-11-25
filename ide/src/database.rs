//! Databases at design time: talks to the helper `freya-tde-db` (protocol in
//! [`tdr_core::schema`]) and finds the connection and SQL of the SQLx
//! components in the project.
//!
//! The helper runs in the project folder (relative SQLite paths match the
//! generated program started there). It is started on first use; if it does
//! not answer within the time limit it is stopped and started again on the
//! next request.
//!
//! *Datenbanken zur Entwurfszeit über den Helfer `freya-tde-db`.*

use crate::tools::NoConsole;
use crate::tr;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::Duration;

use tdr_core::schema::{
    Answer, Connection, DbError, Described, Driver, Envelope, Outcome, Reply, Request, Schema,
    codes,
};
use tdr_core::{ComponentPath, Form, Value};

/// Program name of the helper.
pub const HELPER: &str = "freya-tde-db";

/// Time limit per request (connecting has its own limit of 10 s in the
/// helper).
pub const TIMEOUT: Duration = Duration::from_secs(30);

/// The helper program: `FREYA_TDE_DB`, next to the running program, in the
/// repository's `target` (development and tests) or from the `PATH`.
pub fn helper_program() -> PathBuf {
    if let Some(path) = std::env::var_os("FREYA_TDE_DB") {
        return PathBuf::from(path);
    }
    let file = format!("{HELPER}{}", std::env::consts::EXE_SUFFIX);
    let beside = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join(&file)))
        .filter(|p| p.is_file());
    if let Some(beside) = beside {
        return beside;
    }
    // Entwicklung und Tests: Test-Programme liegen nicht neben dem Helfer
    // (seit Cargo 1.100 unter target/debug/build/<paket>/<hash>/out).
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target");
    ["debug", "release"]
        .into_iter()
        .map(|profile| target.join(profile).join(&file))
        .find(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(file))
}

/// A running helper.
struct Process {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Access to databases for one project folder.
pub struct Database {
    dir: PathBuf,
    program: Option<PathBuf>,
    process: Option<Process>,
    next: u64,
    timeout: Duration,
}

impl Database {
    /// For the project in `dir`; the helper starts on the first request.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            program: None,
            process: None,
            next: 1,
            timeout: TIMEOUT,
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Uses `program` instead of [`helper_program`].
    pub fn with_program(mut self, program: impl Into<PathBuf>) -> Self {
        self.program = Some(program.into());
        self
    }

    /// Changes the time limit per request.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    fn start(&mut self) -> Result<&mut Process, DbError> {
        if self.process.is_none() {
            let program = self.program.clone().unwrap_or_else(helper_program);
            let mut child = Command::new(&program)
                .no_console()
                .current_dir(&self.dir)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| {
                    DbError::new(codes::HELPER_MISSING, format!("{}: {e}", program.display()))
                })?;
            let stdin = child.stdin.take().expect("piped");
            let stdout = child.stdout.take().expect("piped");
            let (send, lines) = channel();
            std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    if send.send(line).is_err() {
                        break;
                    }
                }
            });
            self.process = Some(Process {
                child,
                stdin,
                lines,
            });
        }
        Ok(self.process.as_mut().expect("started above"))
    }

    /// Sends `request` and waits for the answer (at most the time limit).
    pub fn request(&mut self, request: Request) -> Result<Answer, DbError> {
        let id = self.next;
        self.next += 1;
        let timeout = self.timeout;
        let process = self.start()?;
        let line = serde_json::to_string(&Envelope { id, request }).expect("requests serialize");
        let sent = writeln!(process.stdin, "{line}").and_then(|()| process.stdin.flush());
        if let Err(e) = sent {
            self.process = None;
            return Err(DbError::new(codes::HELPER_FAILED, e.to_string()));
        }
        loop {
            match process.lines.recv_timeout(timeout) {
                Ok(text) => {
                    let Ok(reply) = serde_json::from_str::<Reply>(&text) else {
                        self.process = None;
                        return Err(DbError::new(codes::HELPER_FAILED, text));
                    };
                    // Antworten auf abgebrochene frühere Anfragen überspringen.
                    if reply.id != id {
                        continue;
                    }
                    return match reply.outcome {
                        Outcome::Ok(answer) => Ok(answer),
                        Outcome::Error(error) => Err(error),
                    };
                }
                Err(RecvTimeoutError::Timeout) => {
                    self.process = None;
                    return Err(DbError::new(
                        codes::TIMEOUT,
                        format!("no answer within {} s", timeout.as_secs()),
                    ));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    self.process = None;
                    return Err(DbError::new(codes::HELPER_FAILED, "the helper ended"));
                }
            }
        }
    }

    /// Connects and returns the server version.
    pub fn test(&mut self, connection: &Connection) -> Result<String, DbError> {
        match self.request(Request::Test {
            connection: connection.clone(),
        })? {
            Answer::Connected { server } => Ok(server),
            other => Err(unexpected(&other)),
        }
    }

    /// Tables, views, columns, functions and procedures.
    pub fn schema(&mut self, connection: &Connection) -> Result<Schema, DbError> {
        match self.request(Request::Schema {
            connection: connection.clone(),
        })? {
            Answer::Schema(schema) => Ok(schema),
            other => Err(unexpected(&other)),
        }
    }

    /// Redis keys matching `pattern` (at most `limit`).
    pub fn keys(
        &mut self,
        connection: &Connection,
        pattern: &str,
        limit: usize,
    ) -> Result<tdr_core::schema::Keys, DbError> {
        match self.request(Request::Keys {
            connection: connection.clone(),
            pattern: pattern.to_owned(),
            limit,
        })? {
            Answer::Keys(keys) => Ok(keys),
            other => Err(unexpected(&other)),
        }
    }

    /// Result columns of `sql` (not executed).
    pub fn describe(&mut self, connection: &Connection, sql: &str) -> Result<Described, DbError> {
        match self.request(Request::Describe {
            connection: connection.clone(),
            sql: sql.to_owned(),
        })? {
            Answer::Described(described) => Ok(described),
            other => Err(unexpected(&other)),
        }
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        if let Some(process) = self.process.as_mut() {
            // Höflich beenden; Drop von Process beendet notfalls hart.
            let line = serde_json::to_string(&Envelope {
                id: 0,
                request: Request::Quit,
            })
            .expect("requests serialize");
            let _ = writeln!(process.stdin, "{line}");
        }
    }
}

fn unexpected(answer: &Answer) -> DbError {
    DbError::new(
        codes::HELPER_FAILED,
        format!("unexpected answer {answer:?}"),
    )
}

// --- The database components of a project -------------------------------------------------

/// Type of the SQLx pool and query (pack `sqlx`).
pub const POOL_TYPE: (&str, &str) = ("sqlx", "Pool");
pub const QUERY_TYPE: (&str, &str) = ("sqlx", "Query");
/// Type of the Redis client and data source (pack `redis`).
pub const REDIS_CLIENT_TYPE: (&str, &str) = ("redis", "Client");
pub const REDIS_READ_TYPE: (&str, &str) = ("redis", "Read");

/// Keys the window shows per Redis client.
pub const KEY_LIMIT: usize = 500;

/// Why a component has no usable connection (stable codes).
pub mod lookup {
    /// The path names no component.
    pub const NOT_FOUND: &str = "component_not_found";
    /// Neither a pool/client nor a query/read using one.
    pub const NOT_A_DATABASE: &str = "not_a_database";
    /// The query has no pool (the Redis read no client).
    pub const NO_POOL: &str = "no_pool";
    /// The pool has no URL.
    pub const NO_URL: &str = "no_url";
    /// The SQL is not a plain string literal (e.g. built with `format!`).
    pub const SQL_NOT_LITERAL: &str = "sql_not_literal";
}

fn is_type(form: &Form, path: &ComponentPath, (pack, name): (&str, &str)) -> bool {
    form.element(&path.name)
        .is_some_and(|e| *e.type_ref() == tdr_core::TypeRef::pack(pack, name))
}

fn find<'f>(forms: &'f [Form], path: &ComponentPath) -> Option<&'f Form> {
    forms
        .iter()
        .find(|f| path.form.as_ref() == Some(&f.name) && f.element(&path.name).is_some())
}

/// The connections of the project – SQLx pools and Redis clients (paths
/// `form::name`), in form order.
pub fn pools(forms: &[Form]) -> Vec<ComponentPath> {
    forms
        .iter()
        .flat_map(|f| {
            f.elements().into_iter().filter_map(move |e| {
                let path = ComponentPath::on(f.name.clone(), e.name().clone());
                (is_type(f, &path, POOL_TYPE) || is_type(f, &path, REDIS_CLIENT_TYPE))
                    .then_some(path)
            })
        })
        .collect()
}

/// The URL for display: a password (`user:secret@`) becomes `***`.
pub fn display_url(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_owned();
    };
    let Some((auth, host)) = rest.rsplit_once('@') else {
        return url.to_owned();
    };
    let user = auth.split_once(':').map_or("", |(user, _)| user);
    if auth.contains(':') {
        format!("{scheme}://{user}:***@{host}")
    } else {
        url.to_owned()
    }
}

/// Is `path` a Redis read (a data source whose columns follow its kind)?
pub fn is_redis_read(forms: &[Form], path: &ComponentPath) -> bool {
    find(forms, path).is_some_and(|f| is_type(f, path, REDIS_READ_TYPE))
}

/// The connection of a pool or Redis client, or of the one a query or
/// Redis read uses.
pub fn connection(forms: &[Form], path: &ComponentPath) -> Result<Connection, DbError> {
    let form =
        find(forms, path).ok_or_else(|| DbError::new(lookup::NOT_FOUND, path.to_string()))?;
    let via = |property: &str| {
        let component = form.element(&path.name).expect("found").component();
        match component.get(property).map(|v| &v.value) {
            Some(Value::Reference(Some(target))) => Ok(target.qualified(&form.name)),
            _ => Err(DbError::new(lookup::NO_POOL, path.to_string())),
        }
    };
    let pool_path = if is_type(form, path, POOL_TYPE) || is_type(form, path, REDIS_CLIENT_TYPE) {
        path.clone()
    } else if is_type(form, path, QUERY_TYPE) {
        via("pool")?
    } else if is_type(form, path, REDIS_READ_TYPE) {
        via("client")?
    } else {
        return Err(DbError::new(lookup::NOT_A_DATABASE, path.to_string()));
    };
    let pool_form = find(forms, &pool_path)
        .ok_or_else(|| DbError::new(lookup::NOT_FOUND, pool_path.to_string()))?;
    let pool = pool_form
        .element(&pool_path.name)
        .expect("found")
        .component();
    let driver = if is_type(pool_form, &pool_path, REDIS_CLIENT_TYPE) {
        Driver::Redis
    } else {
        match pool.get("driver").map(|v| &v.value) {
            Some(Value::Enum(name)) => Driver::from_name(name).unwrap_or(Driver::Postgres),
            _ => Driver::Postgres,
        }
    };
    let url = match pool.get("url").map(|v| &v.value) {
        Some(Value::Text(url)) if !url.trim().is_empty() => url.clone(),
        _ => return Err(DbError::new(lookup::NO_URL, pool_path.to_string())),
    };
    Ok(Connection { driver, url })
}

/// The SQL of a query (its `sql` property is a Rust string expression; only
/// plain literals can be read at design time).
pub fn query_sql(forms: &[Form], path: &ComponentPath) -> Result<String, DbError> {
    let form =
        find(forms, path).ok_or_else(|| DbError::new(lookup::NOT_FOUND, path.to_string()))?;
    if !is_type(form, path, QUERY_TYPE) {
        return Err(DbError::new(lookup::NOT_A_DATABASE, path.to_string()));
    }
    let component = form.element(&path.name).expect("found").component();
    let code = match component.get("sql").map(|v| &v.value) {
        Some(Value::Code(code)) => code.clone(),
        Some(Value::Text(text)) => return Ok(text.clone()),
        _ => "\"SELECT 1\"".to_owned(),
    };
    tdr_core::schema::string_literal(&code)
        .ok_or_else(|| DbError::new(lookup::SQL_NOT_LITERAL, code))
}

/// The columns of a Redis read, given by its kind (`field: String; value:
/// String` for a hash …) – no server needed.
pub fn redis_columns(
    forms: &[Form],
    path: &ComponentPath,
    registry: &tdr_core::TypeRegistry,
) -> Result<Described, DbError> {
    let form =
        find(forms, path).ok_or_else(|| DbError::new(lookup::NOT_FOUND, path.to_string()))?;
    if !is_type(form, path, REDIS_READ_TYPE) {
        return Err(DbError::new(lookup::NOT_A_DATABASE, path.to_string()));
    }
    let element = form.element(&path.name).expect("found");
    let texts = tdr_core::codegen::implied_columns(form, element, registry).unwrap_or_default();
    let columns = tdr_core::data::parse_columns(&texts)
        .map_err(|e| DbError::new(tdr_core::schema::codes::QUERY_FAILED, e))?
        .into_iter()
        .map(|c| tdr_core::schema::Column {
            name: c.name,
            sql_type: "redis".to_owned(),
            nullable: Some(false),
            primary_key: false,
            rust_type: Some(c.rust_type),
            feature: None,
        })
        .collect();
    Ok(Described {
        columns,
        parameters: None,
    })
}

/// The command that writes the result columns of a query into its
/// `columns` property (undoable like every designer command). Columns
/// without a known Rust type keep an entry the developer wrote by hand.
pub fn columns_command(
    forms: &[Form],
    path: &ComponentPath,
    described: &Described,
) -> Result<tdr_designer::Command, DbError> {
    let form =
        find(forms, path).ok_or_else(|| DbError::new(lookup::NOT_FOUND, path.to_string()))?;
    if !is_type(form, path, QUERY_TYPE) && !is_type(form, path, REDIS_READ_TYPE) {
        return Err(DbError::new(lookup::NOT_A_DATABASE, path.to_string()));
    }
    let component = form.element(&path.name).expect("found").component();
    let existing: Vec<String> = match component.get(tdr_core::data::COLUMNS).map(|v| &v.value) {
        Some(Value::List(items)) => items
            .iter()
            .filter_map(|v| match v {
                Value::Text(t) => Some(t.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    let entries = described
        .columns
        .iter()
        .filter_map(|c| {
            c.columns_entry().or_else(|| {
                existing
                    .iter()
                    .find(|e| e.split(':').next().map(str::trim) == Some(c.name.as_str()))
                    .cloned()
            })
        })
        .map(Value::Text)
        .collect();
    Ok(tdr_designer::Command::SetProperty {
        form: form.name.clone(),
        element: path.name.clone(),
        property: tdr_core::data::COLUMNS.to_owned(),
        value: Some(tdr_core::PropertyValue::new(Value::List(entries))),
    })
}

/// A readable summary of described columns: what was taken over, what is
/// left open, which SQLx features the types bring (code generation adds
/// them to `Cargo.toml`).
pub fn columns_summary(described: &Described) -> String {
    let taken = described.columns_property();
    let mut text = format!("{} Spalten: {}", taken.len(), taken.join(", "));
    let open: Vec<String> = described
        .unmapped()
        .iter()
        .map(|c| format!("{} ({})", c.name, c.sql_type))
        .collect();
    if !open.is_empty() {
        text.push_str(&tr!("database.no_rust_type", columns = open.join(", ")));
    }
    let mut features: Vec<&str> = described
        .columns
        .iter()
        .filter_map(|c| c.feature.as_deref())
        .collect();
    features.sort_unstable();
    features.dedup();
    if !features.is_empty() {
        text.push_str(&tr!(
            "database.feature_hint",
            features = features.join(", ")
        ));
    }
    text
}
