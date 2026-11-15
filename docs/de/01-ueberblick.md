# 1 Was ist was?

[Inhalt](README.md) · weiter: [2 Installieren und bauen](02-installation.md)

Drei Programme bzw. Bibliotheken aus drei Repositories arbeiten zusammen. Wer sie auseinanderhalten kann,
findet sich schneller zurecht.

```text
┌──────────────────────────────── Freya TDE (Repository Freya-TDE) ───────────────────────────────┐
│  ide/        die Entwicklungsumgebung (Programm freya-tde)                                       │
│  core/       Komponentenmodell, Abhängigkeitsgraph, Pack-Format, Codegen (tdr-core)              │
│  designer/   Designer-Logik ohne Oberfläche (tdr-designer)                                       │
│  components/ Komponenten-Packs (Tokio, SQLx, Redis, Axum, WebSocket, HTTP)                       │
│  widgets/    UI-Backends für Zielprojekte (GTK 4, egui, iced, Slint, Flutter, WebView)           │
│  db/         Datenbank-Helfer freya-tde-db (eigener Prozess)                                     │
│                                                                                                  │
│  RUF/  ──────► Submodul: Freya-TDE/RUF   (UI-Framework der IDE)                         │
│  CT-Editor/ ─► Submodul: Freya-TDE/CT-E  (Editor für Cargo.toml, nutzt ebenfalls RUF)   │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

## Freya TDE

Die Entwicklungsumgebung. Freya + TDE = **T**op-**D**own **E**ditor: Man beginnt mit dem großen Ganzen –
Formulare, Komponenten, Verbindungen – und schreibt nur noch die Logik selbst. Vorbild sind Delphi 7 und
Lazarus: frei bewegliche Einzelfenster, eine Komponentenpalette, ein Objektinspektor, Doppelklick auf einen
Knopf springt in seinen Handler.

Wichtig ist die Trennung in **zwei Welten**:

1. **Die Oberfläche der IDE** selbst ist mit RUF (GTK 4) gebaut – auf Linux, Windows und macOS gleich.
2. **Die Programme, die man mit Freya TDE baut** (Zielprojekte), dürfen jedes unterstützte UI-Toolkit nutzen:
   GTK 4, egui, iced, Slint, Flutter oder eine WebView. Diese Toolkits werden nicht in die IDE einkompiliert.
   Vereinheitlicht ist nur die *Beschreibung* der Formulare; je Toolkit übersetzt ein Backend-Pack sie in Code.

Der Designer zeigt die Formulare deshalb nicht „live“, sondern zeichnet sie selbst nach den Maßen des
Backends. Die echte Ansicht liefert das erzeugte Programm, das als eigener Prozess startet – ein Absturz dort
reißt die IDE nicht mit.

## RUF und RNCL

**RUF** (Rust native UI Framework) ist ein Desktop-UI-Framework für Rust auf Basis von GTK 4. **RNCL** (Rust
native Component Library) liefert darauf die Komponenten: Fenster, Knöpfe, Textfelder, Listen, Baum,
Registerkarten, einen Code-Editor mit GtkSourceView … RUF ist ein eigenständiges Projekt und lässt sich auch
ohne Freya TDE nutzen. Mehr in [Kapitel 14](14-ruf.md).

## CT-Editor

Der **CT-Editor** (Cargo TOML Editor, Repository CT-E) bearbeitet `Cargo.toml`-Dateien visuell: Pakete,
Abhängigkeiten mit Suche auf crates.io, Features, weitere Abschnitte – formatierungserhaltend, Kommentare
bleiben. Er läuft eigenständig mit Datei-Menü oder eingebettet aus Freya TDE heraus (Projektfenster →
`Cargo.toml` öffnen). Mehr in [Kapitel 15](15-ct-editor.md).

## Der Datenbank-Helfer

**freya-tde-db** (Ordner `db/`) ist ein kleines Hilfsprogramm, das Freya TDE zur Entwurfszeit startet, um
Datenbanken zu befragen: Verbindung testen, Tabellen und Spalten lesen, die Ergebnis-Spalten einer Abfrage
ermitteln, Redis-Schlüssel auflisten. Die Datenbank-Treiber stecken nur in diesem Helfer, nicht in der IDE.

## Wie die Teile zusammenspielen

1. Freya TDE lädt die **Packs** (`components/`, `widgets/`) – deklarative Beschreibungen in TOML mit
   Code-Vorlagen.
2. Im **Designer** entsteht das neutrale Formularmodell (Komponenten, Eigenschaften, Ereignisse), gespeichert
   als `.ftdr` neben der `.rs`-Datei, das Projekt als `.ptdr`.
3. **Code erzeugen** übersetzt das Modell über das gewählte Backend in Rust (und bei Flutter in Dart).
4. **Bauen und Starten** ruft Cargo auf; Compilermeldungen führen zurück zur Komponente im Designer.
