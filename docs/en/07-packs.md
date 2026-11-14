# 7 Component packs

[Contents](README.md) · back: [6 UI backends](06-backends.md) · next: [8 Databases and data binding](08-data.md)

Component packs provide non-visual building blocks – independent of the UI backend, shown on the design
surface only as an icon (models: TTimer, TQuery, DataModule). Each pack is a folder `components/<name>/` with
a `pack.toml`. Details: [`components/README.md`](../../components/README.md).

| Pack | Types | For |
|---|---|---|
| `tokio` | `Runtime` | the async runtime; added automatically when needed |
| `sqlx` | `Pool`, `Query` | PostgreSQL, MySQL/MariaDB, SQLite; `Query` is a data source and can write back |
| `redis` | `Client`, `Read` | Redis/Valkey; `Read` is a data source (List, Set, SortedSet, Hash, Keys) |
| `axum` | `Router` | web server; listens on start (`on_start`, `on_error`), queries can be attached as endpoints |
| `ws` | `Client`, `Endpoint` | WebSocket client (ws/wss) and an endpoint on the Axum router |
| `http` | `Client`, `Request` | reqwest client; `Request` with `on_response`, optionally a data source (JSON) |

TLS everywhere through rustls with the `ring` provider.

## How packs work together

Through **provides/requires** packs state what they offer and need ([chapter 5](05-components.md)). A `Query`
requires `sqlx.pool`; dropping one without a pool makes the check report it and suggest creating one. The
dependency graph makes sure the pool is created before the query, and `Cargo.toml` gets `sqlx` with the
matching features.

## Events from the background

WebSocket, HTTP and Axum work on Tokio. Their handlers still run on the UI thread and with `&mut self`:
background tasks put calls into a queue of the form, and the form works through it when woken. A data module
with such components runs as a Tokio task of its own.

## Packs of your own

A pack describes its types declaratively: properties, events, provides/requires, Cargo dependencies (also
conditional: `when = { driver = "Postgres" }`, `uses = { columns = "chrono::" }`) and code templates
(MiniJinja) for type, creation, start and methods. `generated-code-license` is mandatory – generated code
belongs to the user. A pack can bring icons of its own (`icons/<name>.png`).
