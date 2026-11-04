//! Catalog queries per driver: server version, tables with columns,
//! functions and procedures, and the result columns of a statement.

use sqlx::{Column as _, Either, Executor, Row, SqlSafeStr, Statement, TypeInfo};
use tdr_core::schema::{
    Column, DbError, Described, Driver, Routine, RoutineKind, Schema, Table, TableKind,
};

use crate::{Pool, query_error};

fn unsupported() -> DbError {
    DbError::new(
        tdr_core::schema::codes::UNSUPPORTED,
        "Redis has keys, not tables: use the keys request",
    )
}

pub async fn server(pool: &Pool) -> Result<String, DbError> {
    let version: String = match pool {
        Pool::Postgres(p) => sqlx::query_scalar("SELECT version()").fetch_one(p).await,
        Pool::MySql(p) => sqlx::query_scalar("SELECT CAST(VERSION() AS CHAR)")
            .fetch_one(p)
            .await
            .map(|v: String| {
                if v.contains("MariaDB") {
                    format!("MariaDB {v}")
                } else {
                    format!("MySQL {v}")
                }
            }),
        Pool::Sqlite(p) => {
            sqlx::query_scalar("SELECT 'SQLite ' || sqlite_version()")
                .fetch_one(p)
                .await
        }
        Pool::Redis(manager) => return crate::keyspace::server(manager).await,
    }
    .map_err(query_error)?;
    Ok(version)
}

const PG_COLUMNS: &str = "
SELECT c.table_schema::text, c.table_name::text, t.table_type::text, c.column_name::text,
       c.udt_name::text, c.is_nullable::text,
       EXISTS (
           SELECT 1 FROM information_schema.table_constraints tc
           JOIN information_schema.key_column_usage k
             ON k.constraint_name = tc.constraint_name AND k.table_schema = tc.table_schema
            AND k.table_name = tc.table_name
           WHERE tc.constraint_type = 'PRIMARY KEY' AND k.table_schema = c.table_schema
             AND k.table_name = c.table_name AND k.column_name = c.column_name
       ) AS pk
FROM information_schema.columns c
JOIN information_schema.tables t ON t.table_schema = c.table_schema AND t.table_name = c.table_name
WHERE c.table_schema NOT IN ('pg_catalog', 'information_schema')
ORDER BY c.table_schema, c.table_name, c.ordinal_position";

const PG_ROUTINES: &str = "
SELECT n.nspname::text, p.proname::text,
       CASE p.prokind WHEN 'p' THEN 'PROCEDURE' ELSE 'FUNCTION' END,
       pg_get_function_arguments(p.oid),
       CASE WHEN p.prokind = 'p' THEN NULL ELSE pg_get_function_result(p.oid) END
FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace
WHERE n.nspname NOT IN ('pg_catalog', 'information_schema') AND p.prokind IN ('f', 'p')
ORDER BY 1, 2";

// MySQL/MariaDB: CAST … AS CHAR, weil information_schema je nach Version
// binäre Zeichenketten liefert.
const MY_COLUMNS: &str = "
SELECT CAST(c.TABLE_SCHEMA AS CHAR), CAST(c.TABLE_NAME AS CHAR), CAST(t.TABLE_TYPE AS CHAR),
       CAST(c.COLUMN_NAME AS CHAR), CAST(c.COLUMN_TYPE AS CHAR), CAST(c.IS_NULLABLE AS CHAR),
       CAST(c.COLUMN_KEY = 'PRI' AS SIGNED)
FROM information_schema.COLUMNS c
JOIN information_schema.TABLES t ON t.TABLE_SCHEMA = c.TABLE_SCHEMA AND t.TABLE_NAME = c.TABLE_NAME
WHERE c.TABLE_SCHEMA = DATABASE()
ORDER BY c.TABLE_SCHEMA, c.TABLE_NAME, c.ORDINAL_POSITION";

const MY_ROUTINES: &str = "
SELECT CAST(r.ROUTINE_SCHEMA AS CHAR), CAST(r.ROUTINE_NAME AS CHAR), CAST(r.ROUTINE_TYPE AS CHAR),
       CAST(COALESCE((
           SELECT GROUP_CONCAT(CONCAT_WS(' ', p.PARAMETER_MODE, p.PARAMETER_NAME, p.DTD_IDENTIFIER)
                               ORDER BY p.ORDINAL_POSITION SEPARATOR ', ')
           FROM information_schema.PARAMETERS p
           WHERE p.SPECIFIC_SCHEMA = r.ROUTINE_SCHEMA AND p.SPECIFIC_NAME = r.SPECIFIC_NAME
             AND p.ORDINAL_POSITION > 0
       ), '') AS CHAR),
       CAST(CASE WHEN r.ROUTINE_TYPE = 'FUNCTION' THEN r.DTD_IDENTIFIER END AS CHAR)
FROM information_schema.ROUTINES r
WHERE r.ROUTINE_SCHEMA = DATABASE()
ORDER BY 1, 2";

const LITE_TABLES: &str = "
SELECT name, type FROM sqlite_master
WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite_%'
ORDER BY name";

const LITE_COLUMNS: &str = "SELECT name, type, \"notnull\", pk FROM pragma_table_info(?1)";

/// One column row of the catalog: schema, table, table type, column, type,
/// nullable, primary key.
type CatalogRow = (String, String, String, String, String, bool, bool);

pub async fn schema(pool: &Pool) -> Result<Schema, DbError> {
    let server = server(pool).await?;
    let (driver, rows, routines) = match pool {
        Pool::Postgres(p) => {
            let rows = sqlx::query(PG_COLUMNS)
                .fetch_all(p)
                .await
                .map_err(query_error)?
                .iter()
                .map(|r| -> Result<CatalogRow, sqlx::Error> {
                    Ok((
                        r.try_get(0)?,
                        r.try_get(1)?,
                        r.try_get(2)?,
                        r.try_get(3)?,
                        r.try_get(4)?,
                        r.try_get::<String, _>(5)? == "YES",
                        r.try_get(6)?,
                    ))
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(query_error)?;
            let routines = sqlx::query(PG_ROUTINES)
                .fetch_all(p)
                .await
                .map_err(query_error)?
                .iter()
                .map(|r| {
                    routine(
                        r.try_get(0),
                        r.try_get(1),
                        r.try_get(2),
                        r.try_get(3),
                        r.try_get(4),
                    )
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(query_error)?;
            (Driver::Postgres, rows, routines)
        }
        Pool::MySql(p) => {
            let rows = sqlx::query(MY_COLUMNS)
                .fetch_all(p)
                .await
                .map_err(query_error)?
                .iter()
                .map(|r| -> Result<CatalogRow, sqlx::Error> {
                    Ok((
                        r.try_get(0)?,
                        r.try_get(1)?,
                        r.try_get(2)?,
                        r.try_get(3)?,
                        r.try_get(4)?,
                        r.try_get::<String, _>(5)? == "YES",
                        r.try_get::<i64, _>(6)? != 0,
                    ))
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(query_error)?;
            let routines = sqlx::query(MY_ROUTINES)
                .fetch_all(p)
                .await
                .map_err(query_error)?
                .iter()
                .map(|r| {
                    routine(
                        r.try_get(0),
                        r.try_get(1),
                        r.try_get(2),
                        r.try_get(3),
                        r.try_get(4),
                    )
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(query_error)?;
            (Driver::MySql, rows, routines)
        }
        Pool::Sqlite(p) => {
            let tables: Vec<(String, String)> = sqlx::query_as(LITE_TABLES)
                .fetch_all(p)
                .await
                .map_err(query_error)?;
            let mut rows = Vec::new();
            for (table, kind) in tables {
                let columns: Vec<(String, String, i64, i64)> = sqlx::query_as(LITE_COLUMNS)
                    .bind(&table)
                    .fetch_all(p)
                    .await
                    .map_err(query_error)?;
                for (name, ty, not_null, pk) in columns {
                    // INTEGER PRIMARY KEY ist die rowid und nie NULL.
                    let rowid = pk > 0 && ty.eq_ignore_ascii_case("integer");
                    rows.push((
                        "main".to_owned(),
                        table.clone(),
                        if kind == "view" { "VIEW" } else { "BASE TABLE" }.to_owned(),
                        name,
                        ty,
                        not_null == 0 && !rowid,
                        pk > 0,
                    ));
                }
            }
            (Driver::Sqlite, rows, Vec::new())
        }
        Pool::Redis(_) => return Err(unsupported()),
    };
    let mut tables: Vec<Table> = Vec::new();
    for (schema, table, kind, name, ty, nullable, pk) in rows {
        let same = tables
            .last()
            .is_some_and(|t| t.schema == schema && t.name == table);
        if !same {
            tables.push(Table {
                schema,
                name: table,
                kind: if kind.contains("VIEW") {
                    TableKind::View
                } else {
                    TableKind::Table
                },
                columns: Vec::new(),
            });
        }
        let mut column = Column::new(driver, name, ty, Some(nullable));
        column.primary_key = pk;
        tables
            .last_mut()
            .expect("pushed above")
            .columns
            .push(column);
    }
    Ok(Schema {
        server,
        tables,
        routines,
    })
}

fn routine(
    schema: Result<String, sqlx::Error>,
    name: Result<String, sqlx::Error>,
    kind: Result<String, sqlx::Error>,
    arguments: Result<String, sqlx::Error>,
    returns: Result<Option<String>, sqlx::Error>,
) -> Result<Routine, sqlx::Error> {
    Ok(Routine {
        schema: schema?,
        name: name?,
        kind: if kind? == "PROCEDURE" {
            RoutineKind::Procedure
        } else {
            RoutineKind::Function
        },
        arguments: arguments?,
        returns: returns?,
    })
}

/// Name, type and origin (`table`, `column`) of the result columns, and the
/// number of parameters.
type Prepared = (
    Vec<(String, String, Option<(String, String)>)>,
    Option<usize>,
);

async fn prepare<'p, DB>(executor: &'p sqlx::Pool<DB>, sql: &str) -> Result<Prepared, DbError>
where
    DB: sqlx::Database,
    &'p sqlx::Pool<DB>: Executor<'p, Database = DB>,
{
    let statement = executor
        .prepare(sqlx::AssertSqlSafe(sql.to_owned()).into_sql_str())
        .await
        .map_err(query_error)?;
    let columns = statement
        .columns()
        .iter()
        .map(|c| {
            let origin = c
                .origin()
                .table_column()
                .map(|t| (t.table.to_string(), t.name.to_string()));
            (c.name().to_owned(), c.type_info().name().to_owned(), origin)
        })
        .collect();
    let parameters = statement.parameters().map(|p| match p {
        Either::Left(types) => types.len(),
        Either::Right(count) => count,
    });
    Ok((columns, parameters))
}

pub async fn describe(pool: &Pool, sql: &str) -> Result<Described, DbError> {
    let (driver, (columns, parameters)) = match pool {
        Pool::Postgres(p) => (Driver::Postgres, prepare(p, sql).await?),
        Pool::MySql(p) => (Driver::MySql, prepare(p, sql).await?),
        Pool::Sqlite(p) => (Driver::Sqlite, prepare(p, sql).await?),
        Pool::Redis(_) => return Err(unsupported()),
    };
    // Nullbarkeit aus dem Katalog, wenn die Spalte aus einer Tabelle stammt;
    // sonst unbekannt (→ Option<…>).
    let catalog = if columns.iter().any(|(_, _, origin)| origin.is_some()) {
        schema(pool).await.ok()
    } else {
        None
    };
    let columns = columns
        .into_iter()
        .map(|(name, ty, origin)| {
            let source = origin.as_ref().and_then(|(table, column)| {
                catalog
                    .as_ref()?
                    .table(table)?
                    .columns
                    .iter()
                    .find(|c| &c.name == column)
            });
            // SQLite nennt bei Ausdrücken „NULL“; dann den Typ der Quelle nehmen.
            let ty = match source {
                Some(s) if ty.eq_ignore_ascii_case("null") => s.sql_type.clone(),
                _ => ty,
            };
            let mut column = Column::new(driver, name, ty, source.and_then(|s| s.nullable));
            column.primary_key = source.is_some_and(|s| s.primary_key);
            column
        })
        .collect();
    Ok(Described {
        columns,
        parameters,
    })
}
