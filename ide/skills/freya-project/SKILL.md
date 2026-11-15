---
name: freya-project
description: Aufbau eines Freya-TDE-Projekts und wo welcher Code hingehört (Nutzercode, generierter Code, Handler-Stubs, Cargo.toml, Packs, Datenbindung). Verwenden, bevor Code in einem Freya-Projekt geschrieben, ein Ereignis-Handler gefüllt, eine Abhängigkeit ergänzt oder ein Build-Fehler behoben wird.
---

# Ein Freya-TDE-Projekt

## Dateien

| Datei | Gehört | Darf die KI |
|---|---|---|
| `<name>.ptdr` | Projekt (Backend, Packs, Formulare, Zielsysteme) | nur über `designer`/Werkzeuge |
| `src/<form>.ftdr` | Formular | nur über `designer` |
| `src/<form>.gen.rs`, `src/app.gen.rs` | Generator, bei jedem Lauf neu, schreibgeschützt | **nie** ändern; Formular ändern und `generate_code` |
| `src/<form>.rs` | Nutzer: Handler und eigene Logik | `edit_file`/`write_file` |
| `src/main.rs` bzw. `src/lib.rs` | Nutzer, einmal angelegt | `edit_file` |
| `Cargo.toml` | Nutzer; der Generator ergänzt nur | `edit_file` (nichts entfernen, was Packs brauchen) |

`.gen.rs` tragen eine Prüfsumme; fremde Änderungen werden gemeldet und überschrieben.

## Handler

Ein Ereignis wird im Designer mit einem Handler verbunden (`set_event`). `generate_code` hängt für
fehlende Handler einen leeren Stub als neuen `impl`-Block an `src/<form>.rs` an; vorhandener Code
bleibt. Dann den Rumpf mit `edit_file` füllen. Die Signatur nicht ändern: sie kommt aus dem Typ
(`component_types` zeigt die Parameter, z. B. `on_change(text: &str)`).

Im Handler ist `self` das Formular: jedes Widget ist ein Feld mit seinem Namen. Wie man es
anspricht, hängt vom UI-Backend ab (`project_info` → `backend`):

- `gtk4`: echte gtk4-rs-Typen, z. B. `self.name_edit.text()`, `self.ok_button.set_sensitive(false)`.
- `egui`, `iced`, `flutter`, `webview`: `Control<T>` mit `value`, `caption`, `items`, `enabled`,
  `visible`, `tooltip`, z. B. `self.name_edit.value = "x".into()`, `self.ok_button.enabled = false`.
- `slint`: Properties `self.window.set_<widget>_<eigenschaft>(…)` / `get_…()`.

Der generierte Code (`generated_code` mit `path`) zeigt die genauen Typen.

Eigene Felder des Formulars nicht ins generierte Struct schreiben, sondern in die
Formular-Eigenschaft `state` (`name: Typ` oder `name: Typ = Startwert`, eine Zeile je Feld), per
`designer` `set_property` auf `window`.

## Nicht-visuelle Komponenten und Packs

Packs (z. B. `sqlx`, `redis`, `axum`, `ws`, `http`, `tokio`) liefern Komponenten für Datenmodule.
Der Generator baut den Service-Container, bestimmt die Initialisierungsreihenfolge aus den
Verweisen und trägt die Cargo-Abhängigkeiten samt Features ein. Komponenten mit `async` brauchen
eine Runtime (`tokio/Runtime`) im Projekt; `problems` meldet sonst den Code dafür.

## Datenbindung

Eine Datenquelle (z. B. `sqlx/Query` mit `columns` als `name: Typ`) füllt Listen und DataGrids über
`data_source`, `display_field`, `value_field`, `load`. Der Generator erzeugt `<liste>_reload()`,
`<liste>_selected_value()`, bei Tabellen `<grid>_selected_row()` und beim Bearbeiten
`on_post`/`on_delete`/`on_post_error`.

## Arbeitsweise

1. Erst lesen: `project_info`, `read_form`, `read_file`, `problems`.
2. Formular ändern (`designer`), `generate_code`, Handler füllen (`edit_file`).
3. `cargo` `check` (oder `clippy`), Meldungen beheben. Meldungen in `.gen.rs` gehören zu einem
   Element des Formulars: dort die Ursache suchen, nicht im generierten Code.
4. Kleine, nachvollziehbare Schritte; jede Änderung erscheint dem Entwickler vorher als Vorschau,
   sofern das Projekt nicht auf „selbstständig“ steht.
