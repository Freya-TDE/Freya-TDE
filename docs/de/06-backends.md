# 6 UI-Backends

[Inhalt](README.md) · zurück: [5 Formulare und Komponenten](05-komponenten.md) · weiter: [7 Komponenten-Packs](07-packs.md)

Ein UI-Backend übersetzt das neutrale Formularmodell in Code für ein bestimmtes Toolkit. Es ist ein Pack in
`widgets/<name>/` (eine `pack.toml` mit `kind = "backend"` plus Vorlagen) und bringt eine eigene Palette
mit. Das Toolkit landet nur im erzeugten Projekt, nie in der IDE. Die vollständige Beschreibung samt Grenzen
jedes Backends steht in [`widgets/README.md`](../../widgets/README.md).

| Backend | Toolkit | Lizenz des Toolkits | Arbeitsweise im erzeugten Code |
|---|---|---|---|
| `gtk4` (Standard) | GTK 4 (gtk4-rs) | MIT / LGPL | jedes Widget ein echtes gtk4-Objekt; Handler bekommen `&mut self` |
| `egui` | egui/eframe 0.37 | MIT OR Apache-2.0 | Immediate Mode; Widgets sind Zustandsfelder `Control<T>` |
| `iced` | iced 0.14 | MIT | Elm-Architektur: je Formular ein `Message`-Enum, `update`, `view` |
| `slint` | Slint 1.18 | GPLv3 / Royalty-free 2.0 / kommerziell | das Formular wird eine Slint-Komponente (`slint::slint!`) |
| `flutter` | Flutter (Material 3) über flutter_rust_bridge 2.13 | BSD-3 / MIT | Rust als Bibliothek, Flutter-App in `flutter/` zeichnet |
| `webview` | System-WebView über wry 0.57 + tao 0.37 | MIT OR Apache-2.0 | das Formular wird eine HTML-Seite, Logik bleibt in Rust |

## Auswahl

Das Backend wählt man im Neu-Assistenten; ein späterer Wechsel ist nicht vorgesehen. GTK 4 ist vorgewählt.

**Slint** steht unter mehreren Lizenzen (GPLv3, Royalty-free 2.0 mit Hinweis auf Slint und nicht für
Embedded, oder kommerziell). Freya TDE reicht Slint nur durch: Vor dem Anlegen zeigt der Assistent den
Lizenzhinweis, und Sie entscheiden. Für die Royalty-free-Bedingung gibt es in der Palette `AboutSlint`.

## Was alle Backends gemeinsam können

- die Kern-Widgets und eine eigene Palette (Switch, Slider, ComboBox, ListBox, Tabs, Hauptmenü, DataGrid …),
- Layouts (senkrecht, waagerecht, Raster, Stapel, absolut), Wunschgröße, Prozent und Anker,
- Farben, Schrift, Fenster-Einstellungen,
- Datenbindung an Listen und Tabellen, Bearbeiten im DataGrid ([Kapitel 8](08-daten.md)),
- Ereignisse nicht-visueller Komponenten (WebSocket, HTTP, Axum) über eine Warteschlange des Formulars.

Jedes Backend setzt um, was sein Toolkit kann; wo es nicht geht, steht es in der Tabelle „Grenzen“ in
[`widgets/README.md`](../../widgets/README.md) und gesammelt in [Kapitel 16](16-probleme.md#grenzen-der-ui-backends).

## Flutter im Besonderen

- Das Rust-Projekt wird eine Bibliothek (`cdylib`, zusätzlich `staticlib` für macOS); die Flutter-App liegt
  in `flutter/` und heißt `<bibliothek>_app`.
- **Starten** (F9) führt einmal `flutter create`, dann `flutter build` aus und startet die App.
- Für fertige Programme baut `flutter/rust_builder/` (cargokit) die Rust-Bibliothek bei jedem
  `flutter build` mit und packt sie ein.
- Fenstertitel, -größe und Startzustand kommen über das Dart-Paket `window_manager` aus dem Designer.

## WebView im Besonderen

- Ein natives Fenster mit der Web-Engine des Systems (WebKitGTK 4.1, WebView2, WKWebView).
- Formular-Eigenschaft `style`: **Native** (eigenes CSS mit Systemfarben) oder **Pico** (Pico CSS, MIT).
- Unter Linux setzt das Programm `WEBKIT_DISABLE_DMABUF_RENDERER=1`, sonst bleibt das Fenster mit manchen
  Treibern leer.

## Eigene Backends

Ein Backend ist eine `pack.toml` mit `[backend]` (Event-Loop, Cargo-Abhängigkeiten, Vorlagen `form`, `run`,
`app`, weitere Dateien) und `[[extend]]`-Einträgen, die den Kern-Widgets ihren Code geben. Das Format ist in
[`widgets/README.md`](../../widgets/README.md) beschrieben; `cargo run -p tdr-core --example check_packs --
widgets` prüft alle Packs.
