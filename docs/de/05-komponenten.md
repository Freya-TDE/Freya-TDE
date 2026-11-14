# 5 Formulare und Komponenten

[Inhalt](README.md) · zurück: [4 Die Fenster](04-fenster.md) · weiter: [6 UI-Backends](06-backends.md)

Hinter dem Designer steht ein neutrales Modell, das von keinem UI-Toolkit abhängt (Crate `tdr-core`).

## Projekt und Formulare

- Ein **Projekt** ist eine `.ptdr`-Datei (RON): Name, Rust-Version, UI-Backend, Packs, Zielsysteme,
  Formulare. Ein Projekt hat genau ein UI-Backend.
- Ein **Formular** ist eine `.ftdr`-Datei neben seiner `.rs`: `src/main_form.ftdr` ↔ `src/main_form.rs`.
- Ein Formular ohne Widgets ist ein **Datenmodul** (Service-Container, wie das DataModule bei Delphi), z. B.
  `services` mit Datenbank-Pool und Webserver.

## Komponenten und Widgets

- **Alles ist eine Komponente:** Name, Typ, Eigenschaften, Ereignisse.
- Ein **Widget** ist eine Komponente mit visueller Erweiterung (Platzierung, Kinder).
- **Nicht-visuelle Komponenten** (Timer, Datenbank, HTTP …) erscheinen als Symbol in der Komponentenleiste.
- Typen kommen aus Packs: ein kleiner gemeinsamer Kern (Label, Button, Edit, CheckBox, Container), dazu die
  Palette des jeweiligen UI-Backends und die Komponenten-Packs.

## Eigenschaften

Reiche Grundtypen: Bool, Int, Float, Text, Enum, Color, Point, Size, Liste, Referenz, Code-Ausdruck, dazu
`Custom` für backend-eigene Typen. Werte lassen sich **je Plattform** überschreiben – der Generator macht
daraus `cfg`-Zweige.

Allgemeine Eigenschaften aller Widgets: `enabled`, `visible`, `tooltip`, `background`, `foreground` (Alpha 0 =
Standard des Toolkits), `font_size`, `bold`, `italic`. Das Formular hat zusätzlich Titel, Größe,
Mindest-/Höchstgröße, Position, Startzustand, Hell/Dunkel, Schrift, Hintergrundbild – und **`state`**: eigene
Felder des Formulars (`zaehler: i32` oder `titel: String = "Neu".into()`), die der Generator ins Struct
schreibt.

## Verweise, Provides und Requires

Komponenten verweisen aufeinander: `db_pool` im selben Formular, `services::db_pool` in einem anderen. Jede
Komponente sagt, was sie **bereitstellt** (z. B. `sqlx.pool`) und was sie **braucht** (zwingend, optional
oder bedingt – Axum braucht einen Pool nur, wenn eine Abfrage daran hängt). Daraus folgt:

- eine **Prüfung beim Ablegen und Verbinden** (fehlt etwas, kommt eine Meldung oder ein Vorschlag),
- der **Abhängigkeitsgraph**, der die Initialisierungsreihenfolge im erzeugten Code festlegt und Zyklen
  erkennt,
- die **Cargo-Abhängigkeiten** samt Features (SQLx mit `postgres` und `chrono`, wenn eine Spalte
  `chrono::NaiveDate` hat).

## Layout

Grundform sind **Container-Layouts**: senkrecht, waagerecht, Raster, Stapel. Ohne eigenes Layout gilt
senkrecht mit Abstand 8 und Rand 12. Im Layout bekommt ein Widget per Griff eine **Wunschgröße** (wie
Width/Height bei Delphi) oder eine Größe in **Prozent** vom Elternelement. **Absolute Lage** ist eine Option
des Backends; dort gibt es **Anker** wie bei Delphi: Formular oder Container größer ziehen nimmt verankerte
Kinder mit. Am gewählten Widget zeigen Linien mit Nadel, an welchen Rändern es verankert ist.

## Ereignisse

Ereignisse sind im Modell abstrakt (`on_click`, `on_change` …). Das Backend entscheidet, ob daraus ein
Callback, eine Abfrage im Immediate Mode oder eine Message wird; Ihr Code ist in jedem Fall ein
Handler-Stub `fn name(&mut self, …)` im eigenen Formular-Code.
