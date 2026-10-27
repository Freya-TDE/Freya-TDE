# CLAUDE.md – Freya-TD

Visuelle Entwicklungsumgebung für Rust im Delphi/Lazarus-Stil (RAD, Top-Down).

## Rahmenbedingungen

- Sprache: Rust >= 1.99.1, immer aktuelle Best Practices und aktuelle Edition.
- Kommunikation: Deutsch, Dokumentation: Deutsch/Englisch Code, Bezeichner und Commit-Messages: Englisch.
- Leitprinzip: "Alles kann, nichts muss". Dem Entwickler wird nichts vorgeschrieben.
- Lizenzen: keine kostenpflichtigen Abhängigkeiten, keine Lizenzprobleme. Slint, egui und iced sind als Runtime des Editors ausgeschieden.

## Ziel dieser Phase

Das Komponentenmodell (Kern) wird vor der Oberfläche entwickelt. Die Oberfläche greift später darauf zu. Entwicklung von unten nach oben: Kern, dann Code-Editor, dann Designer, dann UI-Werkzeuge. KI-Modul und weitere Module erst, wenn der Kern fertig ist.

## Zwei getrennte Welten

1. **Editor-UI:** eigenes UI-Framework auf Basis von GTK 4 für Linux, Windows und macOS. Nur für den Editor selbst. (RUF, RNCL) Releases für Windows und macOS liefern die GTK-Laufzeit mit.
2. **Zielprojekte:** der Nutzer entwirft Formulare für beliebige UI-Crates (egui, Slint, winit, GTK, nativ u. a.). Diese Crates werden nicht in den Editor einkompiliert und ihre Technik wird nicht vereinheitlicht. Vereinheitlicht wird nur die Beschreibung, pro Backend übersetzt ein Adapter.

## Architektur

### Neutrales Formularmodell (Kern)

- Alles ist eine Komponente. Ein Widget ist eine Komponente mit visueller Erweiterung.
- Komponentenbaum mit Properties und Events. Ein Projekt hat genau ein aktives UI-Backend, ein späterer Wechsel ist kein Ziel.
- Kleiner gemeinsamer Widget-Kern (Label, Button, Edit, Checkbox, Container). Jedes Backend bringt zusätzlich eine eigene Palette mit.
- Backend-spezifische Zusatz-Properties und plattformspezifische Properties ("gilt nur auf Plattform X") sind von Anfang an im Modell vorgesehen.
- Layout: Container-Layouts sind die Grundform. Absolute Positionierung ist eine Option, die ein Backend per Capability erlaubt oder nicht.
- Events sind im Modell abstrakt (z. B. OnClick). Das Backend entscheidet, ob daraus Callback, Rückgabewert-Abfrage oder Message-Handler wird. Der Nutzercode ist ein Handler-Stub.

### Zwei Plugin-Achsen

1. **UI-Backends** (windows-rs, GTK, egui, Slint, winit usw.): liefern Widget-Palette, UI-Codegen und Vorschau. Pro Projekt meist nur eines aktiv.
2. **Komponenten-Packs** (SQLx, Axum, WS, Tokio usw.): nicht-visuell, unabhängig vom UI-Backend. Auf der Designfläche nur als Icon, ohne Rendering (Vorbild: TTimer, TQuery, DataModule).

### Plugin-Technik

- Kein stabiles Rust-ABI: keine nachgeladenen Rust-Libraries ohne C-ABI-Schnittstelle.
- Backends und Packs sind deklarative Beschreibungen (TOML oder RON) plus Codegen-Templates.
- Keine Live-Vorschau (kein Hot-Reload wie bei Flutter). Der Designer zeichnet die Widgets selbst nach Backend-Descriptor und Theme-/Metrik-Tabellen. Die echte Ansicht liefert das generierte Projekt im Debug-Modus als eigener Prozess: kein ABI-Problem, ein Absturz reißt den Editor nicht mit, fremde Event-Loops stören die Editor-UI nicht.
- WASM (Component Model, WIT-Namespace `tdr:`) nur bei konkretem Bedarf, etwa für Design-Time-Logik, die sich nicht deklarativ beschreiben lässt. Netzwerkzugriffe nur über Host-Funktionen oder Helper-Prozess. Erst nachrüsten, wenn nötig.

### Provides/Requires (Pflichtbestandteil)

Verdrahtung zwischen Komponenten ist Aufgabe des Tools, die Logik bleibt beim Entwickler.

- Provides: was eine Komponente anbietet (z. B. DB-Verbindung, HTTP-Routing).
- Requires: was eine Komponente fordert, zwingend oder optional. Auch bedingte Anforderungen (Axum braucht einen Pool nur, wenn eine Query-Komponente darauf verweist).
- Design-Time-Prüfung beim Ablegen und Verbinden: ist die geforderte Fähigkeit im Projekt vorhanden? Wenn nicht, Fehlermeldung oder Vorschlag, den passenden Baustein anzulegen.
- Abhängigkeitsgraph bestimmt die Initialisierungsreihenfolge im generierten Code und erkennt Zyklen.
- Dasselbe Prinzip steuert Cargo-Dependencies und Features (SQLx im Projekt, passender Eintrag in der `Cargo.toml`).
- Ownership: ein zentraler Service-Container pro Projekt (DataModule-Äquivalent). Der Generator entscheidet über `Arc`, `Clone` oder Referenz.
- Async: Tokio für nicht-visuelle Komponenten; das Zusammenspiel mit der Event-Loop legt der Backend-Descriptor fest (siehe Entscheidungen).

### Plattformunterschiede

- Plattformlayer klein und klar abgegrenzt, alles rein Optische oder Verhaltensbezogene pro OS getrennt vom Formularmodell.
- Theme- und Metrik-Abstraktion: Abstände, Schriftgrößen, Control-Größen und Farben kommen aus Tabellen pro Plattform. Optische Anpassungen sind Datenänderungen, keine Codeänderungen.
- Codegen bildet Plattformunterschiede als `cfg`-Zweige ab.

## Ablauf (PAP)

1. Projekt öffnen.
2. UI-Backend wählen und Komponenten-Packs laden.
3. Beides fließt ins neutrale Formularmodell (Komponentenbaum, Properties, Events).
4. Vom Modell gehen zwei Wege ab:
   - Vorschau: generiertes Projekt im Debug-Modus bauen und starten; Compilermeldungen führen zurück zur Komponente im Designer.
   - Codegen: Backend-Adapter und Pack-Templates erzeugen das Rust-Projekt mit `cfg`-Zweigen pro Plattform.
5. `cargo build`.

PAP als Bild findest du in /pap/

## Dateiformate

- `.ptdr` Projekt, `.ftdr` Designer-Formular, weitere Typen nach dem Schema Typbuchstabe plus `tdr`.
- Ein Board pro Rust-Datei: `name.ftdr` neben `name.rs`. Eine `.rs` ohne `.ftdr` bleibt unverändert, die `.ftdr` wird per Menü "ftdr aus rs erzeugen" angelegt.
- Die Projektdatei hält die verwendete Rust-Version fest.

## Plugin-Lizenzen

Generierter Code gehört dem Nutzer. Jedes Plugin deklariert die Lizenz seines erzeugten Codes im Manifest.

## Reihenfolge der Umsetzung

1. Kern-Crate: Komponentenmodell (Komponente, Property, Event, Referenz-Typ, Provides/Requires).
2. Abhängigkeitsgraph: Auflösung, Zyklenerkennung, Initialisierungsreihenfolge.
3. Pack-Beschreibungsformat (deklarativ) mit Parser und Validierung.
4. Codegen-Grundgerüst mit Templates, Cargo-Dependencies und Features.
5. Backend-Descriptor und Vorschau (Debug-Build, Cargo-Meldungen als JSON Lines über stdio).
6. Erst danach Designer-Oberfläche auf Basis des Kerns.

## Entscheidungen

- Produktname: **Freya-TD**.
- Verzeichnisse: `core/` (Paket `tdr-core`, die Basis-Structs, auf denen alles aufbaut), `components/` (nicht-visuelle Komponenten), `widgets/` (visuelle Widgets). Root-`Cargo.toml` ist der Workspace; RUF und CT-Editor bleiben eigenständig.
- Komponente und Widget sind zwei getrennte Typen: `Widget` enthält eine `Component` plus visuelle Erweiterung und Kinder. Ein gemeinsamer Trait erlaubt Graph, Codegen und Formular den einheitlichen Zugriff auf beide.
- Property-Typsystem: reiche Grundtypen (Bool, Int, Float, Text, Enum, Color, Point, Size, Liste, Referenz, Code-Ausdruck) plus `Custom` für backend-eigene Typen. Werte lassen sich pro Plattform überschreiben.
- Dateiformat für `.ftdr`/`.ptdr`: RON.
- Verweise sind qualifiziert: `db_pool` (gleiches Formular) oder `services::db_pool` (anderes Formular). Ein Formular hat einen eigenen Namen; ein Formular ohne Widgets ist ein Datenmodul (Service-Container).
- Der Abhängigkeitsgraph ist das Modul `tdr_core::graph` (keine eigene Crate). Formularprüfung (`validate`) und Projektanalyse (`graph::analyze`) ergänzen sich.
- Pack-Beschreibungen: TOML mit eigenem lesbarem Schema, eine `pack.toml` je Pack-Ordner (`components/<name>/pack.toml`), Lader im Modul `tdr_core::pack`. `generated-code-license` ist Pflicht. Backend-Paletten nutzen dasselbe Format mit `kind = "backend"` und dürfen vorhandene Typen per `[[extend]]` erweitern.
- Codegen (Modul `tdr_core::codegen`): Templates mit MiniJinja 3 im Abschnitt `[component.codegen]` der `pack.toml` (`type`, `init` bzw. `init-file`, `async`, `share`, `entry`, `runtime`). Erzeugter Code wird mit `syn` geprüft und mit `prettyplease` formatiert. `generate` ist ein reiner Probelauf (Dateien mit Create/Update/Unchanged plus Diagnosen), erst `write` schreibt und verweigert bei Fehlern oder zwischenzeitlich geänderten Dateien.
- Projektaufbau: `src/main.rs` und `src/<form>.rs` gehören dem Nutzer und werden nur einmal angelegt; danach hängt der Generator nur fehlende Handler-Stubs als neuen `impl`-Block an. `src/app.gen.rs` und `src/<form>.gen.rs` werden bei jedem Lauf überschrieben. `Cargo.toml` wird per `toml_edit` zusammengeführt, ohne etwas zu entfernen; die Version des Nutzers bleibt erhalten.
- Async: Die Initialisierung ist async auf Tokio. Die Runtime-Komponente liefert `entry` (`#[tokio::main]`) und `runtime` (von Hand gebaute Runtime); welches genutzt wird, entscheidet die Event-Loop des Backends. Komponenten mit `async = true` brauchen eine Runtime im Projekt.
- Diagnosen (`Issue`, `ProjectIssue`, `CodegenIssue`, `PackError` …) sind per serde serialisierbar mit stabilem `code` (snake_case) und `detail`.
- Projektdatei `.ptdr` (RON): `ProjectFile` mit Name, Rust-Version, Backend, Packs und Formularpfaden.
- Backend-Descriptor: Abschnitt `[backend]` in der `pack.toml` eines Backend-Packs (`event-loop`, `absolute-positioning`, `platforms`, `cargo`, Templates `form` und `run`). Core-Widgets bekommen ihren Code pro Backend über `[[extend]]` mit `codegen`. Das `form`-Template erzeugt z. B. `ui()`/`build()` des Formulars und ruft die Handler-Stubs auf.
- Event-Loop: Der Descriptor entscheidet. `main-thread` (Standard): UI-Loop auf dem Hauptthread, Tokio wird per `runtime`-Template von Hand gebaut und läuft im Hintergrund, `App::init` per `block_on`, `started` synchron. `async`: `run()` trägt das `entry`-Attribut (`#[tokio::main]`), `started` ist async.
- Vorschau (Modul `tdr_core::preview`): `cargo build --message-format=json` im Projektordner; Compilermeldungen in `.gen.rs` werden über die Source-Map der Generation der Komponente zugeordnet. Der Report nennt das Debug-Binary zum Starten.
- `.gen.rs` schreibt nur Freya-TD: schreibgeschützt nach dem Schreiben, Prüfsumme im Kopf. Fremde Änderungen werden gemeldet und überschrieben; eine gleichnamige Datei, die nie generiert wurde, wird nicht überschrieben. Im Projektbaum des Designers können die Dateien ausgeblendet werden (Schritt 6).
- Leere Handler-Stubs behalten lesbare Parameternamen; die Warnung „unused variable“ wird in Kauf genommen.
- Schritt 6 in drei Teilen: 6a Designer-Logik ohne Oberfläche (`designer/`, Paket `tdr-designer`), 6b RUF erweitern (Zeichenfläche, Menü-/Werkzeugleiste, Baumansicht, Dateidialoge, Fensterposition setzen und lesen), 6c Anwendung in `ide/` (Paket `freya-td`). Ein erstes UI-Backend-Pack für Zielprojekte kommt später.
- Designer-Logik (`tdr-designer`): `Document` mit serialisierbaren Befehlen (`Command`, JSON) und Undo/Redo über Schnappschüsse; Layout aus Container-Layouts und Theme-Tabellen (`designer/themes/<plattform>.toml`, Textbreite geschätzt); Zeichenliste (`Primitive`), die die UI nur malt; Hit-Test und Ablageziel; Palette und Inspektor als Daten; SVG-Ausgabe zum Ansehen ohne Fenster. Nicht-visuelle Komponenten erscheinen als Icons in einer Leiste unter dem Formular. Formulardateien liegen als `src/<form>.ftdr`, die Wurzel neuer Formulare heißt `window`.
- Oberfläche aus frei beweglichen Einzelfenstern (wie Delphi 7/Lazarus, 23.10.2026): Designer, Quellcode-Editor, Objektinspektor, Komponentenbaum, Palette usw. sind eigene Fenster und lassen sich auf beliebigen Bildschirmen anordnen. Kein angedocktes Hauptfenster mit Splittern.
- Fensteranordnung wird gespeichert und beim Start wiederhergestellt. GTK 4 kann Fenster nicht positionieren, deshalb setzt RUF die Position nativ: Windows über das HWND (`SetWindowPos`), macOS über das NSWindow (`setFrameOrigin`), Linux über X11. Unter Linux startet Freya-TD immer über X11/XWayland (`GDK_BACKEND=x11`), weil Wayland Programmen das Positionieren verbietet.
- Schritt 6c in Etappen: 6c.1 Anwendung `ide/` (Paket `freya-td`) mit Hauptfenster (Menü, Palette), Designer, Objektinspektor, Struktur, Meldungen; 6c.2 Code-Editor, Handler-Sprung, Code erzeugen, bauen, starten; 6c.3 Projektfenster, danach Git.
- IDE-Logik ohne Oberfläche in `ide/src/session.rs` (Auswahl, Platzieren, Ziehen); die Fenster leiten nur weiter. Automatisierung: `--commands` (JSON-Befehle), `--screenshots`, `Ide::design_surface`. Fensteranordnung pro Nutzer in `<config>/freya-td/desktop.toml`. Neue Projekte nutzen vorerst das Backend `none`, bis es Backend-Packs gibt.
- UDF-Prinzip (Unidirectional Data Flow, 26.10.2026) für Designer und Code-Editor, umgesetzt auf RUF (nicht relm4): ein Zustand (`Session`/`Document`), Änderungen nur über Aktionen/Befehle, die Fenster zeichnen den Zustand und leiten Eingaben weiter.
- Code-Editor mit GtkSourceView 5 (Rust-Bindung `sourceview5`, MIT; Bibliothek LGPL, wird wie GTK mitgeliefert): Hervorhebung, Zeilennummern, Fehlermarken, Vervollständigung. Faltung baut RUF selbst dazu (Klammerblöcke aus `ruf::fold_regions`, Pfeile in der Markenleiste, eigene Zeilennummern). IntelliSense, Live-Fehler und „Gehe zu Definition“ über rust-analyzer als eigener Prozess (LSP über stdio, Modul `freya_td::lsp`; startet, sobald der Projektordner eine `Cargo.toml` hat).
- Git (26.10.2026), Vorbild VS Code (Bilder `images/vsc-git*.png`, lokal):
  - Über die `git`-Kommandozeile wie VS Code (nutzt Zugangsdaten, SSH-Schlüssel, Hooks und Einstellungen des Nutzers); kein `gix`/`git2`. Programm `git` im `PATH` oder `FREYA_TD_GIT`. Später: Releases bzw. Launcher bringen `git` mit, wenn es fehlt.
  - „Repository anlegen“ schreibt wie `cargo init` eine `.gitignore` mit `/target`, falls keine da ist.
  - Erst Bibliotheks-API ohne Oberfläche (`freya_td::git`): Status, Diffs, Log als Daten; Stagen, Commit, push/pull usw. als serialisierbare Befehle mit Vorschau und stabilen Fehlercodes (z. B. `auth_required`, `rejected`, `merge_conflict`).
  - Passphrase/Passwort: `git` läuft mit `GIT_ASKPASS`/`SSH_ASKPASS` (`SSH_ASKPASS_REQUIRE=force`) auf `freya-td` im Hilfsmodus; die Frage geht an das laufende Freya-TD (verdeckte Eingabe), nichts wird gespeichert. `GIT_TERMINAL_PROMPT=0`. Ohne Oberfläche (KI, Tests) gibt es statt einer Frage den Fehler `auth_required`.
  - Fenster „Quellcodeverwaltung“: oben Repository mit Zweig (`main+` bei gestagten Änderungen, ↓/↑ gegenüber Upstream, pull/push, Zweig wechseln/anlegen); Commit-Feld (Strg+Enter) mit Knopf und Auswahl Commit, Commit (Amend), Commit & Push, Commit & Sync (Pull Request später, nur mit `gh`); ohne Änderungen, aber mit ausstehenden Commits wird der Knopf zu „Sync“. Darunter „Gestaged“ und „Änderungen“ mit Zähler und Statusbuchstaben (A, M, D, R, U, C = Konflikt): stagen, unstagen, verwerfen, Doppelklick zeigt den Diff. Unten der Graph (Commits, Autor, `main`/`origin/main`, „ausgehend“).
  - Repository-Zeile wie VS Code: Zweig-Knopf (Zweig/Tag auschecken), Sync, „…“ = weitere Aktionen (Pull, Push, Klonen, Auschecken, Fetch; Untermenüs Commit inkl. „Letzten Commit rückgängig“, Änderungen, Pull/Push inkl. Rebase und Fetch aller Remotes, Zweig: zusammenführen, rebase, anlegen, umbenennen, löschen, veröffentlichen; Remote, Stash, Tags; Git-Ausgabe). Worktrees vorerst nicht.
  - Nachrichtenfeld wächst mit dem Text (2–12 Zeilen). Änderungen als Liste oder Baum. Graph-Leiste: angezeigte Zweige (Auto = HEAD und Upstream, Alle, Auswahl), zum aktuellen Commit, Fetch (alle), Pull, Push, Aktualisieren.
  - GPG-signierte Commits: die Passphrase fragt `gpg-agent` (pinentry) selbst ab; Fehler → `signing_failed`.
  - Außerdem: Markierungen im Projektbaum, Zweig in der Statuszeile, Änderungsmarken am Rand des Code-Editors.
- Meldungsfenster als Panel mit Reitern wie VS Code (27.10.2026): Probleme (Modell, Codegen, Build, rust-analyzer; `freya_td::problems`), Ausgabe (Kanäle Build, Programm, Git, rust-analyzer; `freya_td::output`), Terminal, Debug-Konsole. Terminal als eigenes Terminal: Pseudo-Terminal mit `portable-pty` (MIT) und Parser `vt100` (MIT), dargestellt in RUF, auf allen Systemen. Debug-Konsole erst mit dem Debugger (DAP, z. B. `lldb-dap`).
- Tastenkürzel (27.10.2026): Die Kürzel des Hauptmenüs gelten in allen IDE-Fenstern (RUF `inherit_menu_shortcuts`); das eigene Menü eines Fensters geht vor. Menükürzel gehen dem fokussierten Widget vor. Unter macOS ist `Ctrl` die Befehlstaste.
- Projektfenster (26.10.2026): Datei-Manager mit dem Projektordner als Wurzel (ohne `target/`, `.git/`; `.gen.rs` und versteckte Dateien zuschaltbar). Logik ohne Oberfläche in `freya_td::files` (Dateioperationen als JSON-Befehle mit Probelauf, stabile Codes). Löschen geht in den Papierkorb (Crate `trash`, MIT; unter macOS über `NSFileManager`, damit keine Finder-Berechtigung nötig ist), Projektdatei und Formulare sind geschützt. „Einbinden“ kopiert eine Datei von außen ins Projekt; `Cargo.toml` öffnet der CT-Editor (`--path`). Der Tab „Projekt-Manager“ folgt mit Git.
- Code-Fenster (26.10.2026): ein Reiter je Datei, `.gen.rs` schreibgeschützt, Build-Fehler als Marken, F12 wechselt Designer/Code, der Designer folgt dem Reiter. „Code erzeugen“ speichert vorher alle Editoren. Neue Projekte beginnen mit dem Formular `main_form` (`main`/`app` sind reserviert). Projektdatei hat `platforms` (Zielsysteme).
- Designer ohne Formular- und Plattform-Auswahl (26.10.2026): Ein Formular wird immer zusammen mit seiner `.rs` geöffnet. Das Zielsystem ist das Entwicklersystem bzw. das bei Projekterstellung eingestellte; andere Systeme rechnet der Designer über die Metrik-Tabellen (Schriftgrößen, Positionen) um.
- Editor-UI nur noch mit GTK 4 (23.10.2026): RUF hat ein GTK-4-Backend für alle Systeme, Win32 und Cocoa sind entfernt, der Trait `Backend` bleibt. Windows wird in MSYS2 (UCRT64), macOS mit Homebrew gebaut; `RUF/tools/bundle-windows.sh` und `bundle-macos.sh` legen die GTK-Laufzeit dazu. Zielprojekte bleiben frei in der Wahl des UI-Backends.

## Offene Punkte

- Panel: Probleme, Ausgabe und Terminal erledigt (`freya_td::terminal`; Grenze: keine Eingabemethoden/tote Tasten, ein Terminal). Später: mehrere Terminals, Debugger mit Debug-Konsole.
- Schritt 6c.3: Projektfenster und Git-Fenster erledigt; offen sind Änderungsmarken am Rand des Code-Editors, „Commit & Create Pull Request“ (mit `gh`) und der Tab „Projekt-Manager“.
- Erstes echtes UI-Backend-Pack (z. B. egui oder nativ) in `widgets/`.
- Ideen aus `notes/gedanken.txt` (26.10.2026), noch einzuplanen:
  - Struktur und Objektinspektor per Drag-and-drop als Tabs zusammenlegen (und wieder trennen).
  - Datei-Manager (Projektordner als Wurzel; Kontextmenü: kopieren, einbinden, löschen, `.toml` mit dem CT-Editor öffnen) mit Tab „Projekt-Manager“: alle Projekte, Unterprojekte (wie RUF und CT-Editor in Freya-TD), Git-Angaben in der Projektdatei; das Hauptprojekt darf Unterprojekte bearbeiten und pushen, wenn es dasselbe Repository ist, Unterprojekte nur sich selbst.
  - Git-Fenster: hinzugefügte Dateien, Commit, Push/Pull, Tab mit Git-Graph.
  - Code-Editor: Faltung, Fehlermarken, IntelliSense, „Gehe zu Definition“, Event-Code mit Autovervollständigung; Komponenten- und Klassenvervollständigung nach dem Vorbild der Lazarus CodeTools (fehlende Felder, Methodenrümpfe, Ereignis-Handler anlegen, nichts löschen; Einfügereihenfolge einstellbar).
  - SQL-Info-Fenster: nach Verbindungsaufbau Struktur, Funktionen und Prozeduren anzeigen.
  - Hauptfenster: Rust-Werkzeuge (cargo, clippy, SQLx …) als Symbole mit Text, Paletten je UI-Crate, Code-Snippets (mitgeliefert und eigene).
  - Neu-Assistent: Library oder Binary; bei Binary Zielsystem(e), dann UI-Toolkit (GTK4, iced, egui, Slint, WebView, Flutter, eigenes). Bei Flutter zusätzliches Flutter-Verzeichnis mit flutter_rust_bridge vorbereiten.
  - Später (nach der KI-Einbindung): Blender und Bevy für Spieleentwicklung.
- Design-Time-Funktionen für Packs (z. B. SQLx-Verbindungstest, Schema-Browser): Helper-Prozess oder WASM.
- Eigener Installer/Launcher (festgehalten 23.10.2026):
  - Eigenständiges Programm mit winit + egui, damit er ohne GTK läuft. Der Ausschluss von egui gilt nur für die Editor-UI, nicht für den Launcher.
  - Installiert Freya-TD und nach und nach weitere eigene Programme (Programmkatalog).
  - Läuft auf Windows, macOS und Linux; auch unter Linux wird über den Launcher installiert, da nicht jedes System GTK hat.
  - Prüft bei der Installation, ob eine passende GTK-4-Laufzeit (≥ 4.12) vorhanden ist. Fehlt sie oder ist sie zu alt, installiert er die mitgelieferten GTK-Dateien.
  - Update-Prüfung per API gegen die eigenen Server, und zwar beim Start des Launchers (Systemstart oder manuell, danach im Tray) und beim Öffnen seiner Oberfläche – nicht beim Start von Freya-TD oder anderer Programme.
  - API-Format und Server werden später entschieden.

## Arbeitsweise für Claude

- Erst Kern, dann Oberfläche. Keine UI-Arbeit, solange das Komponentenmodell nicht steht.
- KI immer mitdenken: Später bekommt eine KI-Anbindung Direktzugriff auf das Tool. Jede Funktion muss deshalb als Bibliotheks-API ohne Oberfläche nutzbar sein, Ergebnisse und Diagnosen maschinenlesbar (serialisierbar, stabile Codes, genaue Fundorte), Schreibvorgänge vorher als Vorschau abrufbar, Ausgaben deterministisch, Formate als Schema dokumentiert und generierter Code sauber von Nutzercode getrennt.
- Entscheidungen aus diesem Dokument nicht stillschweigend ändern. Bei Konflikten nachfragen.
- Keine Abhängigkeiten mit kostenpflichtiger oder unklarer Lizenz einführen.
- Neue Crates nur in aktuellen, stabilen Versionen. Versionen und Lizenzen vor Aufnahme prüfen.
