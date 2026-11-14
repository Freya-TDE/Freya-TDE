# 8 Datenbanken und Datenbindung

[Inhalt](README.md) · zurück: [7 Komponenten-Packs](07-packs.md) · weiter: [9 Erzeugter und eigener Code](09-code.md)

Vorbild sind Delphis `TQuery`, `TDBLookupComboBox` und `TDBGrid`: Eine Datenquelle liefert Zeilen, Listen und
Tabellen zeigen sie – ohne dass man den Lade-Code selbst schreibt.

## Datenquellen

Eine Datenquelle ist ein Typ mit einem Abschnitt `[component.data]` – etwa die SQLx-`Query` (SQL plus Pool),
`redis/Read` oder `http/Request`. Ihre Eigenschaft `columns` nennt die Spalten mit Rust-Typ:

```text
id: i64
name: String
geboren: Option<chrono::NaiveDate>
```

Die Spalten lassen sich von Hand eintragen oder im Objektinspektor per „columns aus Datenbank übernehmen“
direkt aus der Datenbank holen.

## Listen und Tabellen binden

Listen (DropDown, ListBox …) und das **DataGrid** bekommen die Eigenschaften `data_source`, `display_field`
bzw. `fields`/`headers`, `value_field` und `load` (Start oder Manual). Der Generator schreibt dazu:

- `<liste>_reload()` – lädt im Hintergrund auf Tokio, das Formular zeigt die Zeilen, wenn sie da sind,
- `<liste>_selected_value()` – der Wert der Auswahl, typisiert nach `columns`,
- beim DataGrid ein Zeilen-Struct `<Widget>Row` und `<grid>_selected_row()`.

## Bearbeiten im DataGrid

Mit `editable` lassen sich Zellen ändern, wie bei einem `TDataSet`:

- Gespeichert wird **je Zeile** – beim Wechsel in eine andere Zeile oder mit Enter; **Esc** verwirft.
- Neue und gelöschte Zeilen über `<grid>_insert()` und `<grid>_delete()` (für eigene Knöpfe).
- Eingaben werden nach dem Spaltentyp gelesen; „zwei“ in einer `i64`-Spalte wird abgelehnt. Bei `Option<…>`
  ist eine leere Zelle `None`.
- Ereignisse: `on_post(row, inserted) -> bool` (mit `false` bleibt die Zeile in Bearbeitung, etwa für eine
  Prüfung), `on_delete(row) -> bool`, `on_post_error(error)`.

**Automatisch zurückschreiben:** Bekommt die SQLx-`Query` zusätzlich `table` und `key` (Schlüsselspalten),
erzeugt Freya TDE `UPDATE`, `INSERT` und `DELETE` mit gebundenen Parametern (`$1` bei PostgreSQL, `?` bei
MySQL und SQLite) und schreibt im Hintergrund. Mit `generated_key` lässt das `INSERT` den Schlüssel weg (die
Datenbank vergibt ihn), danach wird neu geladen. Ohne `table` gibt es nur die Ereignisse, und Sie schreiben
selbst. Getestet gegen SQLite, PostgreSQL und MariaDB.

Wie jedes Backend die Bearbeitung darstellt, steht in [Kapitel 16](16-probleme.md#grenzen-der-ui-backends).

## Der Schema-Browser

**Fenster → Datenbank** (Strg+Umschalt+B) zeigt je SQLx-Pool die Tabellen, Sichten, Spalten (mit SQL- und
Rust-Typ, Primärschlüssel), Funktionen und Prozeduren, je Redis-Client die Schlüssel. Im Objektinspektor gibt
es „Verbindung testen“ (Pool, Query) und „columns aus Datenbank übernehmen“.

Dahinter steht der Helfer `freya-tde-db` ([`db/README.md`](../../db/README.md)): ein eigener Prozess, mit dem
Freya TDE über JSON-Zeilen spricht. SQLite-Dateien öffnet er nur lesend und legt nie eine an; Passwörter in
URLs werden in der Anzeige maskiert.
