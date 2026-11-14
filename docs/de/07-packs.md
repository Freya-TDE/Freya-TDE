# 7 Komponenten-Packs

[Inhalt](README.md) · zurück: [6 UI-Backends](06-backends.md) · weiter: [8 Datenbanken und Datenbindung](08-daten.md)

Komponenten-Packs liefern nicht-visuelle Bausteine – unabhängig vom UI-Backend, auf der Designfläche nur als
Symbol (Vorbild: TTimer, TQuery, DataModule). Jedes Pack ist ein Ordner `components/<name>/` mit einer
`pack.toml`. Details: [`components/README.md`](../../components/README.md).

| Pack | Typen | Wofür |
|---|---|---|
| `tokio` | `Runtime` | die async-Laufzeit; kommt bei Bedarf von selbst dazu |
| `sqlx` | `Pool`, `Query` | PostgreSQL, MySQL/MariaDB, SQLite; `Query` ist Datenquelle und kann zurückschreiben |
| `redis` | `Client`, `Read` | Redis/Valkey; `Read` ist Datenquelle (List, Set, SortedSet, Hash, Keys) |
| `axum` | `Router` | Webserver; lauscht beim Start (`on_start`, `on_error`), Abfragen lassen sich als Endpunkte anhängen |
| `ws` | `Client`, `Endpoint` | WebSocket-Client (ws/wss) und Endpunkt am Axum-Router |
| `http` | `Client`, `Request` | reqwest-Client; `Request` mit `on_response`, wahlweise Datenquelle (JSON) |

TLS läuft überall über rustls mit dem Provider `ring`.

## Wie Packs zusammenspielen

Die Packs sagen über **Provides/Requires**, was sie anbieten und brauchen ([Kapitel 5](05-komponenten.md)).
Ein `Query` braucht `sqlx.pool`; legt man ihn ohne Pool ab, meldet die Prüfung das und schlägt vor, einen
anzulegen. Der Abhängigkeitsgraph sorgt dafür, dass der Pool vor der Abfrage erzeugt wird, und die
`Cargo.toml` bekommt `sqlx` mit den passenden Features.

## Ereignisse aus dem Hintergrund

WebSocket, HTTP und Axum arbeiten auf Tokio. Ihre Handler laufen trotzdem im UI-Thread und mit `&mut self`:
Hintergrund-Tasks stellen Aufrufe in eine Warteschlange des Formulars, das Formular arbeitet sie ab, wenn es
geweckt wird. Ein Datenmodul mit solchen Komponenten läuft als eigener Tokio-Task.

## Eigene Packs

Ein Pack beschreibt seine Typen deklarativ: Eigenschaften, Ereignisse, Provides/Requires, Cargo-Abhängigkeiten
(auch bedingt: `when = { driver = "Postgres" }`, `uses = { columns = "chrono::" }`) und Code-Vorlagen
(MiniJinja) für Typ, Erzeugung, Start und Methoden. `generated-code-license` ist Pflicht – erzeugter Code
gehört dem Nutzer. Ein Pack kann eigene Symbole mitbringen (`icons/<name>.png`).
