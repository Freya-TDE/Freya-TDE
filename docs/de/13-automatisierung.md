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

## Die KI-Anbindung

Der Kern ist eine Bibliothek ohne Oberfläche (`freya_tde::ai`); das Fenster **KI** (Fenster → KI,
Strg+Umschalt+K) baut darauf auf. Oben wählt man die KI des Gesprächs aus allen Profilen (Drop-down); der
Wechsel gilt ab der nächsten Anfrage, das Gespräch läuft mit der neuen KI weiter, und die Wahl wird für das
Projekt gemerkt (wie „Für dieses Projekt“ in den Einstellungen). Daneben steht, was die KI darf. Anfragen
laufen im Hintergrund, `cargo` ebenso; Änderungen der KI gehen über dieselben Befehle wie der Designer
(rückgängig machbar) und erscheinen sofort in offenen Code-Reitern.

- **Anbieter:** drei Protokolle decken die üblichen Cloud-Dienste und lokale Modelle ab, auch im lokalen
  Netzwerk – Anthropic Messages (Claude), OpenAI-kompatibel (OpenAI, xAI/Grok, Mistral, DeepSeek, Groq,
  OpenRouter, Ollama, LM Studio, llama.cpp, vLLM …) und Google Gemini. `Client::models` ruft die Modelle ab,
  `Client::chat` schickt eine Anfrage (synchron, für Hintergrund-Threads), `Client::chat_stream` ebenso, liefert
  den Text aber Stück für Stück, während er entsteht (Server-Sent Events; ein Server ohne Streaming wird normal
  gelesen). Das KI-Fenster zeigt Antworten so schon beim Schreiben. Fehler haben stabile Codes
  (`auth_failed`, `rate_limited`, `unreachable`, `timeout` …). Geht ein API-Key über `http://` an einen
  anderen Rechner, warnt Freya TDE (`insecure_key_transport`).
- **Werkzeuge** sind die Bibliotheks-API: lesen (`project_info`, `list_files`, `read_file`, `search_files`,
  `read_form`, `component_types`, `problems`, `generated_code`) ist immer erlaubt; ändern (`write_file`,
  `edit_file`, `designer`, `generate_code`) und ausführen (`cargo` check/clippy/test) richten sich nach den
  Rechten des Projekts. Formulare ändert die KI nur über Designer-Befehle, generierte Dateien nie.
- **Rechte je Projekt:** „nur lesen“, „mit Vorschau“ (Vorgabe: jede Änderung erst als Vorschlag mit Diff, erst
  „Übernehmen“ führt sie aus) und „selbstständig“. Sie liegen in den persönlichen Einstellungen, nicht in der
  `.ptdr` – ein geklontes Repository kann der KI keine Rechte geben.
- **Gespräch:** `Conversation` ist ein Zustandsautomat ohne Netz und Oberfläche (`say`, `request`, `receive`,
  `decide`); `ai::run` erledigt eine Nachricht in einer Schleife (Tests, Skripte).
- **Skills** im offenen Format Agent Skills: eingebaut sind die Skills von Freya TDE (`ide/skills`), RUF und
  CT-Editor. Eigene Skills eines Projekts (`.agents/skills/`) gelten erst nach Freigabe; die Freigabe merkt
  sich die SHA-256 des Inhalts, ein geänderter Skill braucht eine neue. Die KI sieht zuerst nur Namen und
  Beschreibungen und lädt den Text bei Bedarf (`load_skill`). Signierte Skill-Pakete kommen mit dem Launcher.

Ein Test gegen einen echten Anbieter läuft mit
`FREYA_TDE_TEST_AI="ollama|http://localhost:11434|qwen3:8b" cargo test -p freya-tde --test ai_live`
(Key aus `FREYA_TDE_AI_KEY`).
