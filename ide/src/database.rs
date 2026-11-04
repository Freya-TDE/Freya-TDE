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

/// The helper program: `FREYA_TDE_DB`, next to the running program, or from
/// the `PATH`.
pub fn helper_program() -> PathBuf {
    if let Some(path) = std::env::var_os("FREYA_TDE_DB") {
        return PathBuf::from(path);
    }
    let file = format!("{HELPER}{}", std::env::consts::EXE_SUFFIX);
    if let Ok(exe) = std::env::current_exe() {
        // Neben freya-tde; bei Tests liegt das Programm eine Ebene höher
        // (target/debug/deps → target/debug).
        for dir in exe.ancestors().skip(1).take(2) {
            let candidate = dir.join(&file);
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    PathBuf::from(file)
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

// --- The SQLx components of a project ------------------------------------------------------

/// Type of the SQLx pool and query (pack `sqlx`).
pub const POOL_TYPE: (&str, &str) = ("sqlx", "Pool");
pub const QUERY_TYPE: (&str, &str) = ("sqlx", "Query");

/// Why a component has no usable connection (stable codes).
pub mod lookup {
    /// The path names no component.
    pub const NOT_FOUND: &str = "component_not_found";
    /// Neither a pool nor a query with a pool.
    pub const NOT_A_DATABASE: &str = "not_a_database";
    /// The query has no pool.
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

/// The pools of the project (paths `form::name`), in form order.
pub fn pools(forms: &[Form]) -> Vec<ComponentPath> {
    forms
        .iter()
        .flat_map(|f| {
            f.elements().into_iter().filter_map(move |e| {
                let path = ComponentPath::on(f.name.clone(), e.name().clone());
                is_type(f, &path, POOL_TYPE).then_some(path)
            })
        })
        .collect()
}

/// The connection of a pool, or of the pool a query uses.
pub fn connection(forms: &[Form], path: &ComponentPath) -> Result<Connection, DbError> {
    let form =
        find(forms, path).ok_or_else(|| DbError::new(lookup::NOT_FOUND, path.to_string()))?;
    let pool_path = if is_type(form, path, POOL_TYPE) {
        path.clone()
    } else if is_type(form, path, QUERY_TYPE) {
        let component = form.element(&path.name).expect("found").component();
        match component.get("pool").map(|v| &v.value) {
            Some(Value::Reference(Some(target))) => target.qualified(&form.name),
            _ => return Err(DbError::new(lookup::NO_POOL, path.to_string())),
        }
    } else {
        return Err(DbError::new(lookup::NOT_A_DATABASE, path.to_string()));
    };
    let pool_form = find(forms, &pool_path)
        .ok_or_else(|| DbError::new(lookup::NOT_FOUND, pool_path.to_string()))?;
    let pool = pool_form
        .element(&pool_path.name)
        .expect("found")
        .component();
    let driver = match pool.get("driver").map(|v| &v.value) {
        Some(Value::Enum(name)) => Driver::from_name(name).unwrap_or(Driver::Postgres),
        _ => Driver::Postgres,
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
