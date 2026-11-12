# freya-tde – die Entwicklungsumgebung

Freya TDE auf RUF: frei bewegliche Fenster wie bei Delphi 7 / Lazarus, jedes auf jedem
Bildschirm platzierbar. Die Anordnung wird pro Nutzer gespeichert und beim Start
wiederhergestellt.
*Freya TDE on RUF: free-floating windows (Delphi 7 / Lazarus style); the arrangement
is saved per user and restored on start.*

| Fenster / window | Inhalt / content |
|---|---|
| Hauptfenster | Menü; Werkzeugleiste über den Reitern (immer sichtbar): Check, Clippy (Meldungen unter Probleme und als Marken im Code), Format (`rustfmt` je eigener Datei, generierte `.gen.rs` bleiben), Test, Update (`cargo update` mit Vorschau), Doku (`cargo doc`, öffnet im Browser), SQLx (`cargo sqlx prepare` mit der URL des ersten SQLx-Pools, nur bei SQLx-Projekten); Ausgabe im Kanal Build. Reiter **Snippets** hinter den Paletten-Reitern: Drop-Down mit allen Snippets (Hover zeigt die Kurzbeschreibung), Einfügen am Cursor des Code-Fensters (eine use-Zeile kommt an den use-Block, wenn sie fehlt), Ablegen/Lösen als Knopf auf dem Reiter, Neu/Bearbeiten (Name, Kurzbeschreibung, use-Zeile, Code)/Löschen; gespeichert in `<config>/freya-tde/snippets.toml`. Komponentenpalette wie Lazarus: ein Reiter je Kategorie (Standard, dann die Palette des UI-Backends, dann die Packs), ein Symbol je Typ, Name und Beschreibung im Tooltip, der gewählte Typ ist hinterlegt; Statuszeile |
| Designer | das Formular auf einer Zeichenfläche: klicken wählt (Strg/Umschalt ergänzt), mit gewählter Palette klicken platziert, ebenso einen Palette-Knopf in den Designer ziehen (Einfügemarke beim Ziehen), ziehen ordnet um bzw. verschiebt in einen anderen Container, Doppelklick springt zum Handler (legt ihn an), Griffe ändern die Größe (gestrichelte Vorschau, Mauszeiger zeigt die Richtung, rückgängig machbar): Formular und absolut platzierte Widgets direkt, Widgets im Layout bekommen eine Wunschgröße wie Width/Height bei Delphi (aus „Fill“ wird dabei „Start“, Dehnen der Achse entfällt; eine Achse in Prozent bleibt in Prozent; Rechtsklick → „Natürliche Größe“ setzt beides zurück); im absoluten Layout verschiebt Ziehen das Widget, und beim Vergrößern von Formular oder Container wandern verankerte Kinder mit, Entf löscht, Esc bricht ab, Rechtsklick-Menü, F12 = Code; nicht-visuelle Komponenten (Datenbank, WebSocket, HTTP …) stehen als Symbole in der Komponentenleiste unten im Fenster statt auf dem Formular: Klick wählt sie für den Objektinspektor, Doppelklick öffnet den Handler, Palette-Knöpfe lassen sich dort ablegen (ein Datenmodul zeigt sie auf seiner Fläche) |
| Code | ein Reiter je Datei (GtkSourceView): Hervorhebung, Zeilennummern, Fehlermarken vom Build und von rust-analyzer; Strg+Leertaste (und `.`/`::`) vervollständigt, Strg+Klick oder Strg+F12 springt zur Definition; Strg+Umschalt+C vervollständigt wie die Lazarus CodeTools, was an der Cursorstelle fehlt: einen Methodenrumpf (`self.speichern(&kunde)` → `fn speichern(&mut self, kunde: &Kunde) {}`, Typen von rust-analyzer, eingefügt hinter der aufrufenden Methode, alphabetisch oder am Ende – Einstellungen), ein eigenes Feld des Formulars (`self.zaehler += 1` → Eintrag `zaehler: i32` in der Formular-Eigenschaft `state`, Typ vorgeschlagen und bestätigt) oder fehlende Trait-Methoden; gelöscht wird nichts; Klammerblöcke falten per Pfeil am Rand (Code → Alles falten/aufklappen); Git-Änderungsmarken am Rand (grün neu, blau geändert, rotes Dreieck gelöscht – verglichen mit dem gestagten Stand, aktualisiert beim Tippen); `.gen.rs` schreibgeschützt; Strg+S speichert, Strg+W schließt, F12 = Designer; der Designer folgt dem Reiter |
| Objektinspektor | Eigenschaften (Editor passend zum Typ, Farben als Auswahl mit Vorgaben oder `#rrggbb`, leer = Standard; ↺ = Standardwert) und Ereignisse (… schlägt einen Handler vor); Gruppe „Platzierung“ (Breite/Höhe in Pixeln oder Prozent, Ausrichtung, Dehnen, Rasterzelle; absolut: x, y, Größe und die Anker links/oben/rechts/unten wie bei Delphi) und für Container „Layout“ (Art, Spalten, Abstand, Innenabstand; Wechsel auf „Absolute“ behält die Lage der Kinder). Logik ohne Oberfläche: `tdr_designer::layout_rows`/`layout_command`, `Session::set_layout_row` |
| Struktur | Formulare und Elemente als Baum |
| Meldungen | Reiter wie in VS Code. **Probleme** (Strg+Umschalt+M): Prüfergebnisse, Code-, Compiler- und rust-analyzer-Meldungen nach Datei gruppiert, mit Textfilter und Fehler/Warnungen/Hinweise; Auswahl springt zum Element bzw. zur Stelle. **Ausgabe** (Strg+Umschalt+U): Kanäle Build, Programm (Ausgabe des mit F9 gestarteten Programms), Git, rust-analyzer. **Terminal** (Strg+Umschalt+T): die Shell des Nutzers im Projektordner (Login-Shell unter Linux/macOS, `%ComSpec%` unter Windows) mit Farben, Vollbildprogrammen (`less`, `htop`, `vim`), toten Tasten und Eingabemethoden (`^`, `` ` ``, `ê`, IME), Mausauswahl, Strg+Umschalt+C/V (macOS ⌘C/⌘V), Mausrad bzw. Umschalt+Bild↑/↓ für die Rückschau, Rechtsklick: Kopieren, Einfügen, Leeren, Neu starten, Neues Terminal, Terminal beenden. Mehrere Terminals: „+ Neu“ öffnet eine weitere Shell in einem eigenen Reiter („1: bash“, „2: bash“), „✕ Beenden“ beendet das sichtbare; jedes hat eigene Rückschau. **Debug-Konsole** folgt mit dem Debugger |
| Projekt | Reiter **Projekt-Manager**: alle Projekte der Projektliste des Nutzers (`<config>/freya-tde/projects.toml`) als Baum aus Haupt- und Unterprojekten, geöffnete mit `*`, das aktive blau. Geöffnet ist ein Hauptprojekt und beliebig viele seiner Unterprojekte (Doppelklick öffnet bzw. aktiviert). Kontextmenü: öffnen, aktivieren, schließen, Unterprojekt hinzufügen, zuordnen, Unterprojekte suchen, ausblenden, entfernen, Git-Einstellungen (Autor, E-Mail, Signierschlüssel in `.git/config`; Remote und Hauptzweig in der `.ptdr`), Projekt aus Cargo.toml erstellen (auch Datei-Menü; solche Projekte werden nur verwaltet: bauen ohne Codeerzeugung). Reiter **Dateien**: der Projektordner als Baum (ohne `target/` und `.git/`; erzeugte und versteckte Dateien zuschaltbar). Doppelklick öffnet: `.ftdr` als Formular samt `.rs`, sonst im Code-Editor. Kontextmenü: neue Datei/neuer Ordner, Datei einbinden (von außen kopieren), Kopieren/Einfügen, Umbenennen, Löschen (in den Papierkorb), `Cargo.toml` mit dem CT-Editor öffnen. Projektdatei und Formulare sind geschützt. Änderungen von außen erscheinen nach spätestens 2 s; offene Dateien ohne ungespeicherte Änderungen werden neu geladen. Git-Status als farbiger Buchstabe (M, A, D, R, U, C = Konflikt), Ordner mit Änderungen bekommen einen Punkt |
| Quellcodeverwaltung | Git wie in VS Code (Strg+Umschalt+G): Repository-Zeile mit Zweig (`main+` = etwas gestaged; Klick = Zweig/Tag auschecken), Sync (↓/↑ zum Upstream) und ⋯ (Pull, Push, Klonen, Auschecken, Fetch; Commit, Änderungen, Pull/Push, Zweig, Remote, Stash, Tags; Git-Ausgabe). Commit-Nachricht (Strg+Enter, wächst mit dem Text) mit Commit, Commit (Amend), Commit & Push, Commit & Sync, Commit & Create Pull Request (GitHub-CLI `gh` im `PATH` oder `FREYA_TDE_GH`, einmal `gh auth login`; nicht auf dem Hauptzweig; die Adresse des Pull Requests steht danach in der Statuszeile) – ohne Änderungen, aber mit ausstehenden Commits wird der Knopf zu „Sync“. „Gestaged“ und „Änderungen“ mit Statusbuchstaben, als Liste oder Baum (Doppelklick = Diff, Kontextmenü: stagen, unstagen, verwerfen, öffnen). Graph mit Leiste: angezeigte Zweige (Auto, Alle, Auswahl), zum aktuellen Commit, Fetch (alle Remotes), Pull, Push, Aktualisieren. Der Zweig steht auch in der Statuszeile des Hauptfensters. Signierte Commits (GPG) fragt `gpg-agent` mit seinem eigenen Dialog ab |
| Debugger | wie „Ausführen und Debuggen“ in VS Code (Strg+Umschalt+D, erscheint beim Start mit F9): Werkzeugleiste (Weiter, Anhalten, Über, Hinein, Heraus, Beenden), „Bei Panic anhalten“, Zustand; **Variablen** als Baum (Kinder laden beim Aufklappen, Aufgeklapptes bleibt über Haltepunkte hinweg offen; Kontextmenü: Überwachen, Wert kopieren); Reiter **Überwacht** (Eingabefeld, Werte im gewählten Rahmen), **Aufrufstapel** (Klick wählt den Rahmen und zeigt die Stelle), **Threads**, **Haltepunkte** (Bedingung, Trefferzahl, Logmeldung, aktivieren, entfernen) |
| Datenbank | Schema-Browser (Fenster → Datenbank, Strg+Umschalt+B): alle SQLx-Pools und Redis-Clients des Projekts (Redis: Schlüssel mit Typ, Länge, Ablauf und Vorschau, bis 500; Passwörter in URLs als `***`) mit Server-Version, Tabellen und Sichten (Spalten mit SQL- und Rust-Typ, 🔑 = Primärschlüssel, orange = ohne Rust-Typ), Funktionen und Prozeduren (PostgreSQL, MySQL/MariaDB); Aktualisieren, Verbindung testen; Doppelklick auf einen Pool wählt ihn aus. Im Objektinspektor hat der Pool „Verbindung: Testen“, die Query „columns: Aus Datenbank übernehmen“ (rückgängig machbar; Spalten ohne Rust-Typ behalten einen von Hand eingetragenen Eintrag). Ergebnisse in der Statuszeile und im Ausgabekanal „Datenbank“ |
| Einstellungen | Datei → Einstellungen … (Strg+,), gilt für Freya TDE als Ganzes. Reiter **Allgemein**: Arbeitsverzeichnis (z. B. `~/DevArea/RUST`; dort beginnen Dateidialoge und der Neu-Assistent, leer = Home-Ordner), Sprache (System, Deutsch, English; vorerst nur für den CT-Editor, die Übersetzung von Freya TDE folgt), Darstellung (System, Hell, Dunkel; auch für den CT-Editor), wohin „Code vervollständigen“ neue Methoden setzt. Reiter **KI**: mehrere Profile (Name, Anbieter Anthropic/OpenAI/Ollama/OpenAI-kompatibel, Modell, Adresse, API-Key), Standardprofil, Profil für das aktuelle Projekt, „KI-Fenster bei neuem Projekt anzeigen“ und „KI-Fenster immer anzeigen“ (wirksam, sobald eine KI angebunden ist). Erst „OK“ übernimmt. Gespeichert in `<config>/freya-tde/settings.toml` (nur für den Nutzer lesbar); API-Keys im Schlüsselbund des Systems (Secret Service, macOS-Schlüsselbund, Windows-Anmeldeinformationen), nur wenn der fehlt, in der Datei; `FREYA_TDE_AI_KEY` gilt für Profile ohne Key |
| Tastenkürzel | Hilfe → Tastenkürzel: alle Kürzel je Fenster und Menü mit Erklärung, erzeugt aus den Menüs |
| Über Freya TDE | Hilfe → Über: Logo, Version, Oberfläche (GTK-Version), Rechteinhaber, Lizenz |

Beim Start zeigt Freya TDE einen Startbildschirm mit dem Logo, bis die Fenster bereit sind
(mindestens gut eine Sekunde; nicht bei `--commands`/`--screenshots`, abschaltbar mit
`--no-splash`). Das Programmsymbol (das F aus `images/Freya-F.svg`) erscheint in Titelleiste,
Taskleiste und Alt+Tab; Bilder und Symbole liegen in `ide/assets/`. Der Konfigordner heißt
`<config>/freya-tde/`; ein alter Ordner `freya-td` wird beim ersten Start übernommen.
*On start a splash screen with the logo appears; the program icon shows in title bar, task
bar and Alt+Tab (assets in `ide/assets/`). An old `freya-td` config folder is taken over.*

Datei → Neues Projekt … (Strg+N) öffnet den **Neu-Assistenten** mit Weiter/Zurück:
1. Art: Programm (Formulare mit UI-Toolkit, `main.rs`) oder Bibliothek (Datenmodule, `lib.rs`, exportiert `App`).
2. Name, übergeordneter Ordner, Rust-Version (vorbelegt mit der installierten).
3. Zielsysteme (vorbelegt mit dem eigenen).
4. Oberfläche (nur Programme): installierte Backends in der Reihenfolge GTK 4, egui, iced, Slint, „ohne
   Oberfläche“; ein Lizenzhinweis (Slint) muss bestätigt werden.
5. Komponenten im Datenmodul `services`: Datenbank (SQLx mit Treiber und URL), Redis, Webserver (Axum),
   Tokio-Laufzeit (kommt bei Bedarf von selbst).
6. Git (Repository, erster Commit) und Zusammenfassung mit allen Dateien und offenen Punkten.

„Anlegen“ speichert das Projekt sofort, erzeugt den Code (es baut gleich), trägt es in die Projektliste ein und
öffnet es. Logik ohne Oberfläche: `wizard` (`NewProject` als JSON, `plan` = Vorschau mit Codes wie
`invalid_name`, `folder_not_empty`, `license_not_accepted`, `create`); `Ide::create_project`,
`Ide::show_wizard`, `Ide::set_wizard_request`, `Ide::wizard_request`, `Ide::wizard_step`. Ein ungespeichertes
Projekt ohne Assistent: `Ide::new_project_with` / `Ide::new_project_with_notice_accepted` (Text per
`Ide::backend_notice`).
*File → New project … opens the wizard (program or library, name and folder, targets, UI toolkit with license
notice, components, Git); library API `wizard` (`NewProject`, `plan`, `create`).*
Symbole der Palette und der Komponentenleiste: `ide/assets/palette/` (Quellen `*.svg`, erzeugt mit
`make.py`), ein Pack kann eigene in `icons/<name>.png` mitbringen.
*New projects use the GTK 4 backend; palette icons live in `ide/assets/palette/` (packs may bring
their own in `icons/`).*

Projekt-Menü: Code erzeugen (Strg+G), Bauen (Strg+F9, im Hintergrund).
Start-Menü wie Delphi/Lazarus: Starten/Weiter (F9, im Debugger), Ohne Debugger starten
(Strg+Umschalt+F9), Anhalten, Programm beenden (Strg+F2), Einzelne Anweisung (F7), Gesamte
Routine (F8), Bis Rückgabe (Umschalt+F8), Bis Cursor (F4, startet bei Bedarf), Haltepunkt
umschalten (F5), Ausdruck überwachen (Strg+F5). Im Code-Fenster setzt ein Klick in die
linke Randspalte einen Haltepunkt, ein Rechtsklick bietet Bedingung, Trefferzahl,
Logpoint und „Bis hier ausführen“; Haltepunkte wandern beim Tippen mit ihren Zeilen. Beim
Anhalten zeigt der Code-Editor die Zeile mit einem Pfeil, die Debug-Konsole im Panel
(Strg+Umschalt+Y) Programmausgabe und Auswertungen.
*Start menu like Delphi: F9 run/continue in the debugger, F7/F8/Shift+F8 steps, F4 run to
cursor, F5 breakpoint, Ctrl+F5 watch, Ctrl+F2 stop; click the gutter for breakpoints.*
Flutter-Projekte (Backend `flutter`): Starten baut die Rust-Bibliothek, legt einmal mit
`flutter create` den Plattform-Ordner an, baut die App (`flutter build <system> --debug`) und
startet sie, ohne Debugger; Ausgabe im Kanal „Programm“ (`freya_tde::flutter`; Programm `flutter`
im `PATH` oder `FREYA_TDE_FLUTTER`).
*Flutter projects: Run builds the Rust library and the Flutter app, then starts it (no debugger).*
Die Kürzel des Hauptmenüs gelten in jedem Fenster; hat ein Fenster dasselbe Kürzel im
eigenen Menü (z. B. Strg+S im Code-Fenster), gilt dort seines. Unter macOS ist Strg die
Befehlstaste (⌘).
*Main menu shortcuts work in every window (a window's own menu wins); on macOS Ctrl is ⌘.*

rust-analyzer läuft als eigener Prozess (LSP über stdio, Modul `lsp` ohne Oberfläche),
sobald der Projektordner eine `Cargo.toml` hat. Er bekommt Änderungen nach einer kurzen
Tipppause und prüft beim Speichern mit `cargo check`. Gesucht wird `rust-analyzer` im
`PATH` oder in `RUST_ANALYZER` (z. B. `rustup component add rust-analyzer`); fehlt er,
arbeitet der Editor ohne diese Funktionen.
*rust-analyzer runs as a separate process (LSP over stdio, module `lsp` without UI) once the
project folder has a `Cargo.toml`: live errors, completion, go to definition.*

```bash
cargo run -p freya-tde                          # neues, ungespeichertes Projekt
cargo run -p freya-tde -- pfad/app.ptdr         # Projekt öffnen
cargo run -p freya-tde -- --commands cmds.json  # Designer-Befehle (JSON) anwenden
cargo run -p freya-tde -- --generate --show-code # Code erzeugen und im Editor öffnen
cargo run -p freya-tde -- --screenshots shots   # alle Fenster als PNG, dann beenden
cargo run -p freya-tde -- --no-splash           # ohne Startbildschirm
env -u WAYLAND_DISPLAY xvfb-run -a -s "-screen 0 1920x1080x24" \
    cargo test -p freya-tde                     # Tests (Linux ohne Bildschirm; Standard 640×480 ist zu klein)
```

Die Logik hinter den Fenstern liegt ohne Oberfläche in `session` (Auswahl, Platzieren,
Ziehen) und in `tdr-designer`; die Fenster leiten nur Eingaben weiter und zeichnen.
Für Automatisierung (Tests, KI): `Ide::run_commands` (JSON-Befehle),
`Ide::design_surface` (Maus/Tastatur simulieren), `Ide::screenshots`, `Ide::code_text`,
`Ide::set_code_text`, `Ide::code_marks`, `lsp::LspClient` (Ereignisse serialisierbar),
`files` (Dateibaum und Dateioperationen als JSON-Befehle mit Probelauf: `files::plan`,
`files::apply`), `Ide::project_files`, `Ide::plan_file_op`, `Ide::file_op`,
`Ide::open_project_file`, `git` (Status, Log, Diff, Graph als Daten; Git-Befehle als JSON
mit Vorschau: `Git::preview`, `Git::run`), `Ide::repo`, `Ide::git_status`, `Ide::git_changes`,
`Ide::git_preview`, `Ide::git_run`, `problems`/`Ide::problems` (alle Probleme als Daten),
`output`/`Ide::output` (Ausgabekanäle), `projects` (globale Projektliste mit Haupt- und
Unterprojekten, Suche nach Unterprojekten, „Projekt aus Cargo.toml erstellen“ mit Vorschau),
`git::Identity`/`GitOp::SetIdentity` (Autor, E-Mail, Signierschlüssel in `.git/config`), `terminal` (Shell im Pseudo-Terminal ohne
Oberfläche), `Ide::terminal_run`, `Ide::terminal_text` (wirken auf das sichtbare Terminal), `Ide::new_terminal`/`close_terminal`/`select_terminal`/`terminal_count`, `Ide::debug_menu`/`debug_command`/`debug_snapshot`/`debug_settings`/`toggle_breakpoint`/`set_breakpoint`/`add_watch`/`debug_evaluate`/`debug_console`/`click_gutter`/`code_breakpoint_marks`/`code_execution_line`, `ui::shortcuts::shortcuts` (alle Tastenkürzel als Daten), `debug` (Debugger über DAP ohne Oberfläche: `DebugSession` mit serialisierbarem `Snapshot` – Zustand, Threads, Aufrufstapel, Variablen, Überwachte Ausdrücke, Haltepunkt-Status, Debug-Konsole –, Befehle als JSON `DebugCommand`, `wait_for`; Haltepunkte und Ausdrücke pro Nutzer in `debug.toml`).
*Automation (tests, AI): `Ide::run_commands`, `Ide::design_surface`, `Ide::screenshots`,
`Ide::code_text`/`set_code_text`/`code_marks`, `lsp::LspClient`, `files` (file tree and file
operations with dry run), `Ide::project_files`/`plan_file_op`/`file_op`/`open_project_file`,
`git` (status, log, diff, graph as data; Git operations as JSON with preview),
`Ide::repo`/`git_status`/`git_changes`/`git_preview`/`git_run`, `problems`/`Ide::problems`,
`output`/`Ide::output`, `terminal`, `Ide::terminal_run`/`terminal_text`, `Ide::new_terminal`/`close_terminal`/`select_terminal`/`terminal_count`, `debug` (DAP debugger without UI: `DebugSession`, serializable `Snapshot`, JSON `DebugCommand`s, `wait_for`).*

Unter Linux startet Freya TDE über X11/XWayland (`GDK_BACKEND=x11`), weil Wayland
Programmen das Positionieren ihrer Fenster verbietet. Git läuft über die `git`-Kommandozeile (`PATH` oder `FREYA_TDE_GIT`) mit den Zugangsdaten,
SSH-Schlüsseln und Einstellungen des Nutzers. Fragt `git` oder `ssh` nach Passwort oder
Passphrase, startet es `freya-tde` als `GIT_ASKPASS`/`SSH_ASKPASS`; die Frage erscheint als
Dialog im laufenden Freya TDE, gespeichert wird nichts.
*Git uses the `git` command line; password/passphrase questions appear as a dialog in the
running Freya TDE (askpass helper), nothing is stored.*

Der Debugger spricht das Debug Adapter Protocol mit `lldb-dap` (LLVM) oder, falls das fehlt,
mit `gdb -i dap` (ab gdb 14); `FREYA_TDE_DAP` nennt einen anderen Adapter. Unter macOS wird
auch das `lldb-dap` aus Xcode bzw. Homebrew gefunden. Die Rust-Darstellung (String, Vec …)
kommt aus den Skripten der Toolchain (`rustc --print sysroot`). In der Debug-Konsole wird
Eingabe als Ausdruck ausgewertet; mit `` ` `` am Anfang geht sie als Befehl an den Debugger.
*The debugger speaks DAP with `lldb-dap`, else `gdb -i dap` (gdb ≥ 14), or `FREYA_TDE_DAP`.
Debug console input is an expression; a leading `` ` `` sends a debugger command.*
Datenbanken liest Freya TDE zur Entwurfszeit über den Helfer `freya-tde-db` (eigener Prozess,
JSON über stdio, siehe `db/README.md`); `FREYA_TDE_DB` nennt ihn, sonst wird er neben dem
Programm bzw. im `PATH` gesucht. Modul `database`: `Database` (`test`, `schema`, `describe`
mit Zeitlimit), `pools`, `connection` und `query_sql` (Verbindung und SQL der SQLx-Komponenten),
`columns_command` (Spalten als Designer-Befehl). In der IDE: `Ide::database_test`, `Ide::database_schema`,
`Ide::query_columns_preview` (Vorschau), `Ide::take_query_columns`, `Ide::database_job` (im Hintergrund),
`Ide::database_tree`, `Ide::database_keys` (Redis).
*Databases are read through the helper `freya-tde-db` (module `database`).*
Werkzeuge und Snippets ohne Oberfläche: Module `tools` (`Tool`, `run` liefert `ToolReport` mit Cargo-Meldungen,
`update_preview`, `format_files`) und `snippets` (`SnippetStore` mit `put`/`remove`/`toggle_pinned`, `insert` setzt
Code und use-Zeile ein; Codes `name_empty`, `name_taken`, `code_empty`, `unknown_snippet`, `write_failed`). In der IDE:
`Ide::run_tool`, `Ide::tool_running`, `Ide::snippets`, `Ide::put_snippet`, `Ide::insert_snippet`, `Ide::toggle_snippet_pinned`.
*Tool bar (check, clippy, format, test, update, doc, sqlx prepare) and snippets (palette tab), modules `tools`, `snippets`.*
Code-Vervollständigung ohne Oberfläche: Modul `codetools` (`plan` wählt aus den Code-Aktionen von rust-analyzer –
`lsp::LspClient::code_actions` – Methode, Feld oder Trait-Methoden; Codes `nothing_to_complete`, `generated_file`,
`analyzer_missing`, `edits_generated_file`, `no_form`; `apply` wendet Änderungen auf einen Text an, `suggest_type`
schlägt Feldtypen vor). In der IDE: `Ide::complete_code`, `Ide::apply_completion`, `Ide::add_state_field`.
*Code completion like the Lazarus CodeTools (Ctrl+Shift+C): method bodies, own form fields (`state`), trait members;
module `codetools`.*
Einstellungen ohne Oberfläche: Modul `settings` (`Settings` mit `load`/`save`, `validate` mit Codes wie
`base_dir_missing`, `profile_duplicate`, `endpoint_missing`, `key_missing`; `SettingsEdit` mit `rename`,
`remove` und `commit`, damit Keys erst beim Übernehmen in den Schlüsselbund gehen; `SecretStore` mit
`SystemKeyring` und `MemoryStore`). In der IDE: `Ide::settings`, `Ide::update_settings`, `Ide::show_settings`,
`Ide::settings_edit`/`set_settings_edit`, `Ide::set_secret_store`, `Ide::ai_key` (Key des Profils für das
aktuelle Projekt).
*Settings (File → Settings, Ctrl+,): working directory, language, appearance, AI profiles; module `settings`,
API keys in the system keyring, file fallback.*
Den CT-Editor sucht Freya TDE in
`FREYA_TDE_CT_EDITOR`, neben dem Programm, im Repository (`CT-Editor/target/…`) und im `PATH`. Packs werden in `components/`
und `widgets/` neben dem Programm bzw. im Repository gesucht, zusätzlich in
`FREYA_TDE_PACKS` (Pfadliste).

## Lizenz / License

MIT OR Apache-2.0 – siehe / see `LICENSE-MIT`, `LICENSE-APACHE`.
