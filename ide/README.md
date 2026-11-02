# freya-td – die Entwicklungsumgebung

Freya-TD auf RUF: frei bewegliche Fenster wie bei Delphi 7 / Lazarus, jedes auf jedem
Bildschirm platzierbar. Die Anordnung wird pro Nutzer gespeichert und beim Start
wiederhergestellt.
*Freya-TD on RUF: free-floating windows (Delphi 7 / Lazarus style); the arrangement
is saved per user and restored on start.*

| Fenster / window | Inhalt / content |
|---|---|
| Hauptfenster | Menü, Komponentenpalette (ein Reiter je Kategorie), Statuszeile |
| Designer | das Formular auf einer Zeichenfläche: klicken wählt (Strg/Umschalt ergänzt), mit gewählter Palette klicken platziert, ziehen ordnet um bzw. verschiebt in einen anderen Container, Doppelklick springt zum Handler (legt ihn an), Entf löscht, Esc bricht ab, Rechtsklick-Menü, F12 = Code |
| Code | ein Reiter je Datei (GtkSourceView): Hervorhebung, Zeilennummern, Fehlermarken vom Build und von rust-analyzer; Strg+Leertaste (und `.`/`::`) vervollständigt, Strg+Klick oder Strg+F12 springt zur Definition; Klammerblöcke falten per Pfeil am Rand (Code → Alles falten/aufklappen); `.gen.rs` schreibgeschützt; Strg+S speichert, Strg+W schließt, F12 = Designer; der Designer folgt dem Reiter |
| Objektinspektor | Eigenschaften (Editor passend zum Typ, ↺ = Standardwert) und Ereignisse (… schlägt einen Handler vor) |
| Struktur | Formulare und Elemente als Baum |
| Meldungen | Reiter wie in VS Code. **Probleme** (Strg+Umschalt+M): Prüfergebnisse, Code-, Compiler- und rust-analyzer-Meldungen nach Datei gruppiert, mit Textfilter und Fehler/Warnungen/Hinweise; Auswahl springt zum Element bzw. zur Stelle. **Ausgabe** (Strg+Umschalt+U): Kanäle Build, Programm (Ausgabe des mit F9 gestarteten Programms), Git, rust-analyzer. **Terminal** (Strg+Umschalt+T): die Shell des Nutzers im Projektordner (Login-Shell unter Linux/macOS, `%ComSpec%` unter Windows) mit Farben, Vollbildprogrammen (`less`, `htop`, `vim`), toten Tasten und Eingabemethoden (`^`, `` ` ``, `ê`, IME), Mausauswahl, Strg+Umschalt+C/V (macOS ⌘C/⌘V), Mausrad bzw. Umschalt+Bild↑/↓ für die Rückschau, Rechtsklick: Kopieren, Einfügen, Leeren, Neu starten. **Debug-Konsole** folgt mit dem Debugger |
| Projekt | der Projektordner als Baum (ohne `target/` und `.git/`; erzeugte und versteckte Dateien zuschaltbar). Doppelklick öffnet: `.ftdr` als Formular samt `.rs`, sonst im Code-Editor. Kontextmenü: neue Datei/neuer Ordner, Datei einbinden (von außen kopieren), Kopieren/Einfügen, Umbenennen, Löschen (in den Papierkorb), `Cargo.toml` mit dem CT-Editor öffnen. Projektdatei und Formulare sind geschützt. Änderungen von außen erscheinen nach spätestens 2 s; offene Dateien ohne ungespeicherte Änderungen werden neu geladen. Git-Status als farbiger Buchstabe (M, A, D, R, U, C = Konflikt), Ordner mit Änderungen bekommen einen Punkt |
| Quellcodeverwaltung | Git wie in VS Code (Strg+Umschalt+G): Repository-Zeile mit Zweig (`main+` = etwas gestaged; Klick = Zweig/Tag auschecken), Sync (↓/↑ zum Upstream) und ⋯ (Pull, Push, Klonen, Auschecken, Fetch; Commit, Änderungen, Pull/Push, Zweig, Remote, Stash, Tags; Git-Ausgabe). Commit-Nachricht (Strg+Enter, wächst mit dem Text) mit Commit, Commit (Amend), Commit & Push, Commit & Sync – ohne Änderungen, aber mit ausstehenden Commits wird der Knopf zu „Sync“. „Gestaged“ und „Änderungen“ mit Statusbuchstaben, als Liste oder Baum (Doppelklick = Diff, Kontextmenü: stagen, unstagen, verwerfen, öffnen). Graph mit Leiste: angezeigte Zweige (Auto, Alle, Auswahl), zum aktuellen Commit, Fetch (alle Remotes), Pull, Push, Aktualisieren. Der Zweig steht auch in der Statuszeile des Hauptfensters. Signierte Commits (GPG) fragt `gpg-agent` mit seinem eigenen Dialog ab |

Projekt-Menü: Code erzeugen (Strg+G), Bauen (Strg+F9, im Hintergrund), Starten (F9).
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
cargo run -p freya-td                          # neues, ungespeichertes Projekt
cargo run -p freya-td -- pfad/app.ptdr         # Projekt öffnen
cargo run -p freya-td -- --commands cmds.json  # Designer-Befehle (JSON) anwenden
cargo run -p freya-td -- --generate --show-code # Code erzeugen und im Editor öffnen
cargo run -p freya-td -- --screenshots shots   # alle Fenster als PNG, dann beenden
xvfb-run -a cargo test -p freya-td             # Tests (Linux ohne Bildschirm)
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
`output`/`Ide::output` (Ausgabekanäle), `terminal` (Shell im Pseudo-Terminal ohne
Oberfläche), `Ide::terminal_run`, `Ide::terminal_text`.
*Automation (tests, AI): `Ide::run_commands`, `Ide::design_surface`, `Ide::screenshots`,
`Ide::code_text`/`set_code_text`/`code_marks`, `lsp::LspClient`, `files` (file tree and file
operations with dry run), `Ide::project_files`/`plan_file_op`/`file_op`/`open_project_file`,
`git` (status, log, diff, graph as data; Git operations as JSON with preview),
`Ide::repo`/`git_status`/`git_changes`/`git_preview`/`git_run`, `problems`/`Ide::problems`,
`output`/`Ide::output`, `terminal`, `Ide::terminal_run`/`terminal_text`.*

Unter Linux startet Freya-TD über X11/XWayland (`GDK_BACKEND=x11`), weil Wayland
Programmen das Positionieren ihrer Fenster verbietet. Git läuft über die `git`-Kommandozeile (`PATH` oder `FREYA_TD_GIT`) mit den Zugangsdaten,
SSH-Schlüsseln und Einstellungen des Nutzers. Fragt `git` oder `ssh` nach Passwort oder
Passphrase, startet es `freya-td` als `GIT_ASKPASS`/`SSH_ASKPASS`; die Frage erscheint als
Dialog im laufenden Freya-TD, gespeichert wird nichts.
*Git uses the `git` command line; password/passphrase questions appear as a dialog in the
running Freya-TD (askpass helper), nothing is stored.*
Den CT-Editor sucht Freya-TD in
`FREYA_TD_CT_EDITOR`, neben dem Programm, im Repository (`CT-Editor/target/…`) und im `PATH`. Packs werden in `components/`
und `widgets/` neben dem Programm bzw. im Repository gesucht, zusätzlich in
`FREYA_TD_PACKS` (Pfadliste).

## Lizenz / License

MIT OR Apache-2.0 – siehe / see `LICENSE-MIT`, `LICENSE-APACHE`.
