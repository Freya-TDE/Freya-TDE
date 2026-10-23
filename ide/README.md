# freya-td – die Entwicklungsumgebung

Freya-TD auf RUF: frei bewegliche Fenster wie bei Delphi 7 / Lazarus, jedes auf jedem
Bildschirm platzierbar. Die Anordnung wird pro Nutzer gespeichert und beim Start
wiederhergestellt.
*Freya-TD on RUF: free-floating windows (Delphi 7 / Lazarus style); the arrangement
is saved per user and restored on start.*

| Fenster / window | Inhalt / content |
|---|---|
| Hauptfenster | Menü, Komponentenpalette (ein Reiter je Kategorie), Statuszeile |
| Designer | das Formular auf einer Zeichenfläche: klicken wählt (Strg/Umschalt ergänzt), mit gewählter Palette klicken platziert, ziehen ordnet um bzw. verschiebt in einen anderen Container, Entf löscht, Esc bricht ab, Rechtsklick-Menü |
| Objektinspektor | Eigenschaften (Editor passend zum Typ, ↺ = Standardwert) und Ereignisse (… schlägt einen Handler vor) |
| Struktur | Formulare und Elemente als Baum |
| Meldungen | Prüfergebnisse; Auswahl springt zum Element |

```bash
cargo run -p freya-td                          # neues, ungespeichertes Projekt
cargo run -p freya-td -- pfad/app.ptdr         # Projekt öffnen
cargo run -p freya-td -- --commands cmds.json  # Designer-Befehle (JSON) anwenden
cargo run -p freya-td -- --screenshots shots   # alle Fenster als PNG, dann beenden
xvfb-run -a cargo test -p freya-td             # Tests (Linux ohne Bildschirm)
```

Die Logik hinter den Fenstern liegt ohne Oberfläche in `session` (Auswahl, Platzieren,
Ziehen) und in `tdr-designer`; die Fenster leiten nur Eingaben weiter und zeichnen.
Für Automatisierung (Tests, KI): `Ide::run_commands` (JSON-Befehle),
`Ide::design_surface` (Maus/Tastatur simulieren), `Ide::screenshots`.
*Automation (tests, AI): `Ide::run_commands`, `Ide::design_surface`, `Ide::screenshots`.*

Unter Linux startet Freya-TD über X11/XWayland (`GDK_BACKEND=x11`), weil Wayland
Programmen das Positionieren ihrer Fenster verbietet. Packs werden in `components/`
und `widgets/` neben dem Programm bzw. im Repository gesucht, zusätzlich in
`FREYA_TD_PACKS` (Pfadliste).

## Lizenz / License

MIT OR Apache-2.0 – siehe / see `LICENSE-MIT`, `LICENSE-APACHE`.
