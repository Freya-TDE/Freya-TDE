//! `freya-tde-db`: the database helper of Freya TDE.
//!
//! Reads one JSON request per line from stdin and answers one JSON line per
//! request on stdout (protocol: `tdr_core::schema`). Connections are kept
//! per driver and URL until the helper ends. SQLite files are opened read
//! only and never created; relative paths count from the working directory
//! (the IDE starts the helper in the project folder).
//!
//! *Datenbank-Helfer von Freya TDE: JSON-Zeilen über stdio, Protokoll in
//! `tdr_core::schema`.*

mod catalog;
mod keyspace;

use std::collections::HashMap;
use std::time::Duration;

use tdr_core::schema::{
    Answer, Connection, DbError, Driver, Envelope, Outcome, Reply, Request, codes,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// How long connecting may take.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// An open connection pool of one of the drivers.
pub enum Pool {
    Postgres(sqlx::PgPool),
    MySql(sqlx::MySqlPool),
    Sqlite(sqlx::SqlitePool),
    Redis(redis::aio::ConnectionManager),
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if std::env::args().any(|a| a == "--version") {
        println!("freya-tde-db {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let mut pools: HashMap<Connection, Pool> = HashMap::new();
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();
    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        let (reply, quit) = match serde_json::from_str::<Envelope>(&line) {
            Ok(envelope) => {
                let quit = envelope.request == Request::Quit;
                let outcome = match handle(&mut pools, envelope.request).await {
                    Ok(answer) => Outcome::Ok(answer),
                    Err(error) => Outcome::Error(error),
                };
                (
                    Reply {
                        id: envelope.id,
                        outcome,
                    },
                    quit,
                )
            }
            Err(e) => (
                Reply {
                    id: 0,
                    outcome: Outcome::Error(DbError::new(codes::BAD_REQUEST, e.to_string())),
                },
                false,
            ),
        };
        let mut text = serde_json::to_string(&reply).expect("replies serialize");
        text.push('\n');
        if stdout.write_all(text.as_bytes()).await.is_err() || stdout.flush().await.is_err() {
            break;
        }
        if quit {
            break;
        }
    }
    for pool in pools.into_values() {
        match pool {
            Pool::Postgres(p) => p.close().await,
            Pool::MySql(p) => p.close().await,
            Pool::Sqlite(p) => p.close().await,
            Pool::Redis(_) => {}
        }
    }
}

async fn handle(
    pools: &mut HashMap<Connection, Pool>,
    request: Request,
) -> Result<Answer, DbError> {
    match request {
        Request::Quit => Ok(Answer::Bye),
        Request::Test { connection } => {
            let server = match pool(pools, &connection).await? {
                Pool::Redis(manager) => keyspace::server(manager).await?,
                pool => catalog::server(pool).await?,
            };
            Ok(Answer::Connected { server })
        }
        Request::Schema { connection } => {
            not_redis(&connection)?;
            let pool = pool(pools, &connection).await?;
            Ok(Answer::Schema(catalog::schema(pool).await?))
        }
        Request::Describe { connection, sql } => {
            not_redis(&connection)?;
            let pool = pool(pools, &connection).await?;
            Ok(Answer::Described(catalog::describe(pool, &sql).await?))
        }
        Request::Keys {
            connection,
            pattern,
            limit,
        } => match pool(pools, &connection).await? {
            Pool::Redis(manager) => Ok(Answer::Keys(
                keyspace::keys(manager, &pattern, limit).await?,
            )),
            _ => Err(DbError::new(
                codes::UNSUPPORTED,
                "keys are for Redis; SQL databases have tables (schema)",
            )),
        },
    }
}

fn not_redis(connection: &Connection) -> Result<(), DbError> {
    if connection.driver.is_sql() {
        Ok(())
    } else {
        Err(DbError::new(
            codes::UNSUPPORTED,
            "Redis has keys, not tables: use the keys request",
        ))
    }
}

/// The pool for `connection`, connected on first use.
async fn pool<'p>(
    pools: &'p mut HashMap<Connection, Pool>,
    connection: &Connection,
) -> Result<&'p Pool, DbError> {
    if !pools.contains_key(connection) {
        let pool = connect(connection).await?;
        pools.insert(connection.clone(), pool);
    }
    Ok(&pools[connection])
}

async fn connect(connection: &Connection) -> Result<Pool, DbError> {
    use std::str::FromStr;
    let url = connection.url.trim();
    let bad_url = |e: sqlx::Error| DbError::new(codes::BAD_URL, e.to_string());
    let pool = match connection.driver {
        Driver::Postgres => {
            let options = sqlx::postgres::PgConnectOptions::from_str(url).map_err(bad_url)?;
            Pool::Postgres(
                sqlx::postgres::PgPoolOptions::new()
                    .max_connections(2)
                    .acquire_timeout(CONNECT_TIMEOUT)
                    .connect_with(options)
                    .await
                    .map_err(connect_error)?,
            )
        }
        Driver::MySql => {
            let options = sqlx::mysql::MySqlConnectOptions::from_str(url).map_err(bad_url)?;
            Pool::MySql(
                sqlx::mysql::MySqlPoolOptions::new()
                    .max_connections(2)
                    .acquire_timeout(CONNECT_TIMEOUT)
                    .connect_with(options)
                    .await
                    .map_err(connect_error)?,
            )
        }
        Driver::Redis => Pool::Redis(keyspace::connect(url).await?),
        Driver::Sqlite => {
            // Nur lesen und nie anlegen: der Designer soll keine Datei erzeugen.
            let options = sqlx::sqlite::SqliteConnectOptions::from_str(url)
                .map_err(bad_url)?
                .create_if_missing(false)
                .read_only(true);
            Pool::Sqlite(
                sqlx::sqlite::SqlitePoolOptions::new()
                    .max_connections(1)
                    .acquire_timeout(CONNECT_TIMEOUT)
                    .connect_with(options)
                    .await
                    .map_err(connect_error)?,
            )
        }
    };
    Ok(pool)
}

/// Errors while connecting: refused logins become `auth_failed`.
fn connect_error(error: sqlx::Error) -> DbError {
    if let sqlx::Error::Database(db) = &error {
        let code = db.code().unwrap_or_default();
        // PostgreSQL 28000/28P01, MySQL 1045 (Zugriff verweigert).
        if matches!(code.as_ref(), "28000" | "28P01" | "1045") {
            return DbError::new(codes::AUTH_FAILED, error.to_string());
        }
    }
    if matches!(error, sqlx::Error::Configuration(_)) {
        return DbError::new(codes::BAD_URL, error.to_string());
    }
    DbError::new(codes::CONNECT_FAILED, error.to_string())
}

/// Errors of statements and catalog queries.
pub fn query_error(error: sqlx::Error) -> DbError {
    DbError::new(codes::QUERY_FAILED, error.to_string())
}
