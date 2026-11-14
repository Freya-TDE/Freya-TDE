# 13 Automatisierung und KI

[Inhalt](README.md) · zurück: [12 Einstellungen](12-einstellungen.md) · weiter: [14 RUF und RNCL](14-ruf.md)

Freya TDE ist von Anfang an so gebaut, dass eine KI (oder ein Skript) es ohne Oberfläche bedienen kann. Jede
Funktion gibt es als Bibliotheks-API; Ergebnisse und Meldungen sind maschinenlesbar.

## Grundsätze

- **Bibliothek zuerst:** die Logik liegt in Modulen ohne Oberfläche (`tdr_core`, `tdr_designer`,
  `freya_tde::wizard`, `git`, `debug`, `files`, `settings`, `codetools`, `tools`, `snippets` …); die Fenster
  leiten nur weiter.
- **Stabile Codes:** Meldungen haben einen festen `code` in snake_case und ein `detail`, sind per serde
  serialisierbar (`Issue`, `CodegenIssue`, `WizardIssue`, `GitError` …).
- **Vorschau vor dem Schreiben:** Codeerzeugung, Dateioperationen, Git-Befehle, der Neu-Assistent und
  `cargo update` zeigen erst, was passieren würde.
- **Deterministisch:** gleiche Eingabe, gleiche Ausgabe; Formate sind als Schema dokumentiert.

## Kommandozeile

```bash
freya-tde pfad/app.ptdr                 # Projekt öffnen
freya-tde --commands cmds.json          # Designer-Befehle (JSON) anwenden
freya-tde --generate --show-code        # Code erzeugen und im Editor öffnen
freya-tde --screenshots shots           # alle Fenster als PNG, dann beenden
freya-tde --no-splash                   # ohne Startbildschirm
```

Designer-Befehle sind JSON, z. B.:

```json
[{"command": "add_widget", "form": "main_form", "parent": "window", "type": "core/Button", "name": "ok"},
 {"command": "set_property", "form": "main_form", "element": "ok", "property": "caption", "value": {"Text": "OK"}}]
```

## Aus Rust heraus

Die IDE selbst (`freya_tde::Ide`) bietet Automatisierung für Tests und eine spätere KI-Anbindung:
`run_commands`, `design_surface` (Maus und Tastatur simulieren), `screenshots`, `code_text`/`set_code_text`,
`create_project`, `update_settings`, `run_tool`, `insert_snippet`, `dock_view`, `git_run`, `debug_command` und
viele mehr – eine Übersicht steht in [`ide/README.md`](../../ide/README.md).

## Die geplante KI-Anbindung

Die KI-Profile sind in den Einstellungen schon vorbereitet. Die Anbindung selbst – ein KI-Fenster, das die
Bibliotheks-API direkt nutzt – ist der nächste große Schritt.
