//! Redis: connect, server version, keys with type, TTL, length and preview.

use std::time::Duration;

use redis::aio::ConnectionManager;
use tdr_core::schema::{DbError, Keys, RedisKey, codes};

use crate::CONNECT_TIMEOUT;

/// Entries shown per key.
const PREVIEW: isize = 5;
/// Characters per shown value.
const PREVIEW_CHARS: usize = 120;

fn error(code: &str, e: redis::RedisError) -> DbError {
    let code = match e.kind() {
        redis::ErrorKind::AuthenticationFailed => codes::AUTH_FAILED,
        redis::ErrorKind::InvalidClientConfig => codes::BAD_URL,
        _ => code,
    };
    DbError::new(code, e.to_string())
}

pub async fn connect(url: &str) -> Result<ConnectionManager, DbError> {
    let client = redis::Client::open(url).map_err(|e| error(codes::BAD_URL, e))?;
    let config = redis::aio::ConnectionManagerConfig::new()
        .set_connection_timeout(Some(CONNECT_TIMEOUT))
        .set_response_timeout(Some(Duration::from_secs(20)))
        .set_number_of_retries(1);
    let mut manager = tokio::time::timeout(
        CONNECT_TIMEOUT * 2,
        client.get_connection_manager_with_config(config),
    )
    .await
    .map_err(|_| DbError::new(codes::CONNECT_FAILED, "connection timed out"))?
    .map_err(|e| error(codes::CONNECT_FAILED, e))?;
    // Erst ein PING zeigt, ob Anmeldung und Datenbank stimmen.
    redis::cmd("PING")
        .query_async::<String>(&mut manager)
        .await
        .map_err(|e| error(codes::CONNECT_FAILED, e))?;
    Ok(manager)
}

/// `Redis 8.2.1` or `Valkey 8.1.0` from `INFO server`.
pub async fn server(manager: &ConnectionManager) -> Result<String, DbError> {
    let mut con = manager.clone();
    let info: String = redis::cmd("INFO")
        .arg("server")
        .query_async(&mut con)
        .await
        .map_err(|e| error(codes::QUERY_FAILED, e))?;
    let field = |name: &str| {
        info.lines()
            .find_map(|l| l.strip_prefix(name)?.strip_prefix(':'))
            .map(|v| v.trim().to_owned())
    };
    Ok(match (field("valkey_version"), field("redis_version")) {
        (Some(v), _) => format!("Valkey {v}"),
        (None, Some(v)) => format!("Redis {v}"),
        (None, None) => "Redis".to_owned(),
    })
}

fn short(text: String) -> String {
    if text.chars().count() > PREVIEW_CHARS {
        let cut: String = text.chars().take(PREVIEW_CHARS).collect();
        format!("{cut}…")
    } else {
        text
    }
}

pub async fn keys(
    manager: &ConnectionManager,
    pattern: &str,
    limit: usize,
) -> Result<Keys, DbError> {
    let server = server(manager).await?;
    let mut con = manager.clone();
    let failed = |e| error(codes::QUERY_FAILED, e);
    let pattern = if pattern.trim().is_empty() {
        "*"
    } else {
        pattern
    };
    let mut names: Vec<String> = Vec::new();
    let mut cursor: u64 = 0;
    let mut truncated = false;
    loop {
        let (next, batch): (u64, Vec<String>) = redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg(pattern)
            .arg("COUNT")
            .arg(500)
            .query_async(&mut con)
            .await
            .map_err(failed)?;
        names.extend(batch);
        cursor = next;
        if names.len() > limit {
            truncated = true;
            break;
        }
        if cursor == 0 {
            break;
        }
    }
    names.sort();
    names.dedup();
    if names.len() > limit {
        truncated = true;
        names.truncate(limit);
    }

    let mut keys = Vec::with_capacity(names.len());
    for key in names {
        let kind: String = redis::cmd("TYPE")
            .arg(&key)
            .query_async(&mut con)
            .await
            .map_err(failed)?;
        let ttl: i64 = redis::cmd("TTL")
            .arg(&key)
            .query_async(&mut con)
            .await
            .map_err(failed)?;
        let length_cmd = match kind.as_str() {
            "string" => Some("STRLEN"),
            "list" => Some("LLEN"),
            "set" => Some("SCARD"),
            "zset" => Some("ZCARD"),
            "hash" => Some("HLEN"),
            "stream" => Some("XLEN"),
            _ => None,
        };
        let length = match length_cmd {
            Some(cmd) => Some(
                redis::cmd(cmd)
                    .arg(&key)
                    .query_async::<u64>(&mut con)
                    .await
                    .map_err(failed)?,
            ),
            None => None,
        };
        let preview: Vec<String> = match kind.as_str() {
            "string" => {
                let value: Option<String> = redis::cmd("GETRANGE")
                    .arg(&key)
                    .arg(0)
                    .arg(PREVIEW_CHARS as isize * 4)
                    .query_async(&mut con)
                    .await
                    .unwrap_or(None);
                value.into_iter().collect()
            }
            "list" => redis::cmd("LRANGE")
                .arg(&key)
                .arg(0)
                .arg(PREVIEW - 1)
                .query_async(&mut con)
                .await
                .map_err(failed)?,
            "set" => {
                let (_, members): (u64, Vec<String>) = redis::cmd("SSCAN")
                    .arg(&key)
                    .arg(0)
                    .arg("COUNT")
                    .arg(PREVIEW)
                    .query_async(&mut con)
                    .await
                    .map_err(failed)?;
                members.into_iter().take(PREVIEW as usize).collect()
            }
            "zset" => {
                let members: Vec<(String, f64)> = redis::cmd("ZRANGE")
                    .arg(&key)
                    .arg(0)
                    .arg(PREVIEW - 1)
                    .arg("WITHSCORES")
                    .query_async(&mut con)
                    .await
                    .map_err(failed)?;
                members
                    .into_iter()
                    .map(|(m, s)| format!("{m} ({s})"))
                    .collect()
            }
            "hash" => {
                let (_, pairs): (u64, Vec<(String, String)>) = redis::cmd("HSCAN")
                    .arg(&key)
                    .arg(0)
                    .arg("COUNT")
                    .arg(PREVIEW)
                    .query_async(&mut con)
                    .await
                    .map_err(failed)?;
                pairs
                    .into_iter()
                    .take(PREVIEW as usize)
                    .map(|(f, v)| format!("{f} = {v}"))
                    .collect()
            }
            _ => Vec::new(),
        };
        keys.push(RedisKey {
            key,
            kind,
            ttl: (ttl >= 0).then_some(ttl),
            length,
            preview: preview.into_iter().map(short).collect(),
        });
    }
    Ok(Keys {
        server,
        keys,
        truncated,
    })
}
