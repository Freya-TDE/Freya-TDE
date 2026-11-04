# db/ – freya-tde-db

Datenbank-Helfer von Freya TDE: liest zur Entwurfszeit Schemas und die Ergebniss-Spalten von Abfragen.
Die IDE startet ihn als eigenen Prozess (wie rust-analyzer oder den Debug-Adapter) und spricht JSON-Zeilen über
stdin/stdout. Datenbank-Treiber stecken nur hier, nicht in der IDE; eine hängende Verbindung oder ein Absturz
reißt den Editor nicht mit.
*Database helper of Freya TDE: schemas and query result columns at design time, as a separate process
speaking JSON lines over stdio. Database drivers live only here, not in the IDE.*

| Datenbank / database | Treiber / driver | Schema | Spalten einer Abfrage / query columns |
|---|---|---|---|
| PostgreSQL | `Postgres` | Tabellen, Sichten, Spalten, Primärschlüssel, Funktionen, Prozeduren | ja |
| MySQL / MariaDB | `MySql` | Tabellen, Sichten, Spalten, Primärschlüssel, Funktionen, Prozeduren | ja |
| SQLite | `Sqlite` | Tabellen, Sichten, Spalten, Primärschlüssel (keine Routinen in SQLite) | ja; Ausdrücke ohne Typ bleiben offen |
| Redis / Valkey | `Redis` | Schlüssel (`keys`): Typ, TTL, Länge, Vorschau der ersten Werte | – (Spalten folgen aus der Art im Pack) |

Grundlage sind SQLx 0.9 (MIT OR Apache-2.0) und redis-rs 1.8 (BSD-3-Clause) mit Tokio und rustls (Zertifikate des
Systems; `rediss://` für TLS).

## Protokoll / protocol

Schema und Typen: `tdr_core::schema` (`Envelope`, `Request`, `Reply`, `Schema`, `Described`). Eine Anfrage je
Zeile, eine Antwort je Anfrage, zugeordnet über `id`.

```text
→ {"id":1,"request":{"kind":"test","driver":"Postgres","url":"postgres://user@localhost/shop"}}
← {"id":1,"ok":{"kind":"connected","server":"PostgreSQL 18.1 …"}}
→ {"id":2,"request":{"kind":"schema","driver":"Sqlite","url":"sqlite://shop.db"}}
← {"id":2,"ok":{"kind":"schema","server":"SQLite 3.50.4","tables":[…],"routines":[]}}
→ {"id":3,"request":{"kind":"describe","driver":"MySql","url":"mysql://…","sql":"SELECT id, name FROM customers WHERE active = ?"}}
← {"id":3,"ok":{"kind":"described","columns":[{"name":"id","sql_type":"BIGINT","nullable":false,"primary_key":true,"rust_type":"i64"}, …],"parameters":1}}
← {"id":4,"error":{"code":"connect_failed","detail":"…"}}
→ {"id":5,"request":{"kind":"keys","driver":"Redis","url":"redis://127.0.0.1/","pattern":"user:*","limit":500}}
← {"id":5,"ok":{"kind":"keys","server":"Valkey 9.1.2","keys":[{"key":"user:1","kind":"hash","ttl":null,"length":2,"preview":["name = Freya", …]}],"truncated":false}}
→ {"id":6,"request":{"kind":"quit"}}
```

Fehlercodes / error codes: `bad_request`, `bad_url`, `connect_failed`, `auth_failed`, `query_failed`, `unsupported`
(`schema`/`describe` bei Redis, `keys` bei SQL); auf der Seite
der IDE zusätzlich `helper_missing`, `timeout`, `helper_failed`.

- `describe` führt die Abfrage nicht aus, sondern bereitet sie nur vor (prepared statement). Die Nullbarkeit kommt
  aus dem Katalog, wenn die Spalte aus einer Tabelle stammt; sonst gilt sie als unbekannt und der Rust-Typ wird
  `Option<…>`.
- Typen, die SQLx nur mit einem Zusatz-Feature liest (`chrono`, `json`, `uuid`, `rust_decimal`), nennen es in
  `feature`; Typen ohne Zuordnung haben `rust_type: null`.
- SQLite-Dateien werden nur gelesen und nie angelegt; relative Pfade gelten ab dem Arbeitsordner (die IDE startet
  den Helfer im Projektordner).
- Verbindungen bleiben je Treiber und URL offen, bis der Helfer endet.
- `keys` durchsucht mit `SCAN` (nie `KEYS`, blockiert den Server nicht) bis `limit`; `truncated` sagt, ob es mehr gibt.

## Programm finden / locating the program

`FREYA_TDE_DB`, sonst neben `freya-tde`, sonst im `PATH`. Releases liefern `freya-tde-db` neben `freya-tde` aus.

## Tests

```bash
cargo test -p freya-tde-db        # SQLite immer
FREYA_TDE_TEST_POSTGRES=postgres://user@127.0.0.1/shop \
FREYA_TDE_TEST_MYSQL=mysql://root@127.0.0.1/shop \
FREYA_TDE_TEST_REDIS=redis://127.0.0.1:6379/ cargo test -p freya-tde-db   # mit echten Servern
```

Die Tests legen eigene Tabellen `freya_customers`, `freya_orders`, die Sicht `freya_names` und je eine Routine an
und löschen sie wieder; bei Redis nur Schlüssel unter `freya:test:`.
