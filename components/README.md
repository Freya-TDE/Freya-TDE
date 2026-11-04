# components/

Nicht-visuelle Komponenten-Packs (SQLx, Axum, Tokio …), aufbauend auf `tdr-core`.
Auf der Designfläche nur als Icon, ohne Rendering.
*Non-visual component packs (SQLx, Axum, Tokio …), built on `tdr-core`. Shown as icons only.*

Jedes Pack ist ein Ordner mit einer `pack.toml`; Codegen-Templates stehen im Abschnitt
`[component.codegen]` oder als Datei daneben (`init-file`).
*Each pack is a directory with a `pack.toml`; code generation templates live in
`[component.codegen]` or in files next to it (`init-file`).*

| Pack | Typen / types | Fähigkeiten / capabilities |
|---|---|---|
| `tokio` | `Runtime` | stellt `tokio.runtime` bereit |
| `sqlx` | `Pool`, `Query` | `sqlx.pool`, `sqlx.query`; Pool braucht `tokio.runtime`, Query braucht `sqlx.pool`; `driver` = Postgres, MySql, Sqlite |
| `axum` | `Router` | `http.router`; braucht `sqlx.pool` nur, wenn eine Query angehängt ist |

## SQLx-Treiber / SQLx drivers

Der Pool hat die Eigenschaft `driver` (Postgres, MySql, Sqlite). Sie bestimmt den Pool-Typ (`sqlx::PgPool`,
`MySqlPool`, `SqlitePool`), das Cargo-Feature (`postgres`, `mysql`, `sqlite`) und über den Verweis `pool` auch den
Feldtyp der Query und den Zeilentyp von `on_row` (`PgRow`, `MySqlRow`, `SqliteRow`). Platzhalter in SQL
schreibt der Entwickler im Stil der Datenbank (`$1` bei Postgres und SQLite, `?` bei MySQL).
*The pool's `driver` selects pool type, Cargo feature, the query's field type and the row type.*

Dafür gibt es zwei allgemeine Bausteine im Pack-Format / two general pack features:

```toml
# Cargo-Abhängigkeit nur bei passendem Eigenschaftswert / dependency only for a matching property value
cargo = [{ crate = "sqlx", version = "0.9", features = ["mysql"], when = { driver = "MySql" } }]
# Templates sehen verwiesene Komponenten / templates see referenced components
type = "(sqlx::{{ {'Postgres': 'PgPool', 'MySql': 'MySqlPool', 'Sqlite': 'SqlitePool'}[refs.pool.props.driver] }}, &'static str)"
```

Spaltentypen aus anderen Crates (`chrono::…`, `serde_json::…`, `uuid::…`, `rust_decimal::…`) bringen das
SQLx-Feature und das Crate selbst in die `Cargo.toml` (Bedingung `uses = { columns = "chrono::" }`: die
Eigenschaft enthält den Text, bei Listen ein Eintrag). `rust_decimal` gibt es in SQLx nicht für SQLite. Listen und
DataGrid zeigen Werte mit `to_string()`, Vektoren (`Vec<u8>`, Arrays) mit `{:?}`.
*Column types from other crates add the SQLx feature and the crate (`uses` condition).*

Auch Parametertypen von Ereignissen dürfen solche Templates sein (`props`, `refs`); Objektinspektor und
Handler-Stubs zeigen den aufgelösten Typ.

## Datenquelle für Listen / data source for lists

Die SQLx-`Query` ist eine Datenquelle (`[component.data]`, Fähigkeit `data.rows`): Unter `columns` stehen ihre
Spalten mit Typ (`id: i64`, `name: String`, `note: Option<String>`). Eine DropDown/ListBox/ComboBox … wählt im
Objektinspektor `data_source` (die Query), `display_field` (angezeigte Spalte), `value_field` (Schlüssel) und
`load` (`Start` = beim Öffnen laden, `Manual` = `self.<liste>_reload()`). Die `columns` liest der Objektinspektor auf
Wunsch aus der Datenbank (Abschnitt „Datenbank“ → „Aus Datenbank übernehmen“, über den Helfer `freya-tde-db`). Geladen wird im Hintergrund auf Tokio;
`self.<liste>_selected_value()` liefert den Schlüssel der Auswahl, typisiert nach `columns`. Ein **DataGrid** zeigt
alle (oder unter `fields` gewählte) Spalten mit Titeln aus `headers`; `self.<grid>_selected_row()` liefert die ganze
Zeile als Struct (`CustomerGridRow { id, name, … }`).
*The SQLx `Query` is a data source: list widgets pick it as `data_source` plus display and value columns.*

```toml
[component.data]
fetch = "sqlx::query(source.1).fetch_all(&source.0).await?"     # Zeilen; `source` = Wert der Komponente
get = "sqlx::Row::try_get::<{{ type }}, _>({{ row }}, {{ column | rust_str }})?"
```

## Neues Pack / new pack

1. Ordner `components/<name>/` mit `pack.toml` anlegen – Vorlage: `sqlx/pack.toml`. Symbol: `icon = "…"`,
   eigenes als `icons/<name>.png` (48 × 48 px) im Pack-Ordner, sonst aus dem Satz der IDE.
2. Schema-Referenz: Modul-Doku von `tdr_core::pack` (`cargo doc -p tdr-core --open`).
3. Prüfen / check: `cargo run -p tdr-core --example check_packs -- components`

## Codegen

```toml
[component.codegen]
type = "sqlx::PgPool"     # Rust-Typ des Feldes (Template) / field type
async = true              # init nutzt .await → braucht eine Runtime / needs a runtime
init = """
sqlx::postgres::PgPoolOptions::new()
    .max_connections({{ rust.max_connections }})
    .connect({{ rust.url }})
    .await?
"""
share = "clone"           # clone | arc | ref – wie abhängige Komponenten es bekommen
# entry = "#[tokio::main]"   # nur die Runtime: Attribut für run() bei async-Backends
# runtime = "tokio::runtime::Builder::new_multi_thread().enable_all().build()?"
#                             # nur die Runtime: von Hand gebaut, wenn die UI den Hauptthread belegt
```

Im Template / in templates: `props.<name>` (Wert als Daten), `rust.<name>` (fertiger
Rust-Ausdruck, Verweise als Abhängigkeit), `events.<name>` (Handler), `name`, `form`.
Details: Modul-Doku von `tdr_core::codegen`.

Pflichtangaben / mandatory: `name`, `version` (major.minor.patch), `license` und
`generated-code-license` (Lizenz des erzeugten Codes / license of the generated code).
