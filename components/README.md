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
| `sqlx` | `Pool`, `Query` | `sqlx.pool`, `sqlx.query`; Pool braucht `tokio.runtime`, Query braucht `sqlx.pool` |
| `axum` | `Router` | `http.router`; braucht `sqlx.pool` nur, wenn eine Query angehängt ist |

## Neues Pack / new pack

1. Ordner `components/<name>/` mit `pack.toml` anlegen – Vorlage: `sqlx/pack.toml`.
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
# entry = "#[tokio::main]"  # nur die Runtime: Attribut für run()
```

Im Template / in templates: `props.<name>` (Wert als Daten), `rust.<name>` (fertiger
Rust-Ausdruck, Verweise als Abhängigkeit), `events.<name>` (Handler), `name`, `form`.
Details: Modul-Doku von `tdr_core::codegen`.

Pflichtangaben / mandatory: `name`, `version` (major.minor.patch), `license` und
`generated-code-license` (Lizenz des erzeugten Codes / license of the generated code).
