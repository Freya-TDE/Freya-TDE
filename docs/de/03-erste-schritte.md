# 3 Erste Schritte

[Inhalt](README.md) · zurück: [2 Installieren und bauen](02-installation.md) · weiter: [4 Die Fenster](04-fenster.md)

Dieses Kapitel führt einmal durch: neues Projekt, Formular gestalten, Ereignis schreiben, starten.

## Ein neues Projekt

**Datei → Neues Projekt …** (Strg+N) öffnet den Neu-Assistenten. Seite für Seite:

1. **Art:** Programm (mit Oberfläche, `main.rs`) oder Bibliothek (nur Datenmodule, `lib.rs`).
2. **Name und Ort:** Paketname, übergeordneter Ordner (vorbelegt mit dem Arbeitsverzeichnis aus den
   Einstellungen), Rust-Version (vorbelegt mit der installierten).
3. **Zielsysteme:** Linux, Windows, macOS, Android, iOS, Web – das eigene ist vorgewählt.
4. **Oberfläche:** das UI-Toolkit (GTK 4 ist vorgewählt). Bringt ein Toolkit Lizenzbedingungen mit (Slint),
   steht der Hinweis hier und muss bestätigt werden.
5. **Komponenten** für das Datenmodul `services`: Datenbank (SQLx mit Treiber und URL), Redis, Webserver
   (Axum, wahlweise mit WebSocket-Endpunkt), WebSocket-Client, HTTP-Client, Tokio-Laufzeit. Tokio kommt bei
   Bedarf von selbst dazu.
6. **Git und Zusammenfassung:** Repository anlegen, ersten Commit machen; darunter alle Dateien, die
   entstehen, und was noch fehlt.

„Anlegen“ speichert das Projekt, erzeugt den Code (er baut sofort) und öffnet es.

## Das Formular gestalten

Das neue Projekt hat das Formular `main_form`.

- In der **Palette** (Hauptfenster) einen Typ anklicken und dann in den **Designer** klicken – oder den
  Paletten-Knopf in den Designer ziehen. Die Reiter der Palette: „Standard“ (Label, Button, Edit, CheckBox,
  Container), die Palette des UI-Toolkits (z. B. „GTK“), die Komponenten-Packs („Database“, „Web“ …) und
  „Snippets“.
- Im **Objektinspektor** die Eigenschaften setzen: `caption` des Knopfs, Farben, Schrift, im Abschnitt
  „Platzierung“ Größe, Prozent oder Anker.
- Nicht-visuelle Komponenten (Datenbank, Timer, HTTP …) landen als Symbol in der Komponentenleiste unten im
  Designer.

## Ein Ereignis schreiben

Doppelklick auf den Knopf: Freya TDE legt den Handler an, erzeugt den Code und springt im Code-Fenster an die
Stelle – wie bei Delphi:

```rust
impl MainForm {
    pub fn button1_on_click(&mut self) {
        // Ihr Code. Beim GTK-Backend ist jedes Widget ein echtes gtk4-Objekt:
        self.label1.set_text("Hallo Freya!");
    }
}
```

Der Handler steht in `src/main_form.rs` – diese Datei gehört Ihnen. Was Freya TDE erzeugt, steht in
`src/main_form.gen.rs` und wird bei jedem Lauf neu geschrieben (siehe [Kapitel 9](09-code.md)).

## Bauen und starten

- **F9** baut und startet das Programm im Debugger (ohne Debugger: Umschalt+Strg+F9).
- Fehler erscheinen im Fenster **Meldungen** (Reiter „Probleme“) und als Marken im Code; ein Klick springt hin.
- Die Werkzeugleiste über der Palette hat Check, Clippy, Format, Test, Update, Doku und SQLx.

## Ein bestehendes Cargo-Projekt übernehmen

**Datei → Projekt aus Cargo.toml erstellen …** legt für ein vorhandenes Cargo-Projekt (oder einen
Workspace samt Mitgliedern) eine `.ptdr` an. Solche Projekte werden verwaltet und gebaut, ohne dass Freya TDE
Code erzeugt.
