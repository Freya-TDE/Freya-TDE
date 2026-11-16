# 13 Automation and AI

[Contents](README.md) · back: [12 Settings](12-settings.md) · next: [14 RUF and RNCL](14-ruf.md)

From the start, Freya TDE has been built so that an AI (or a script) can use it without the UI. Every
function exists as a library API; results and messages are machine-readable.

## Principles

- **Library first:** the logic lives in modules without UI (`tdr_core`, `tdr_designer`, `freya_tde::wizard`,
  `git`, `debug`, `files`, `settings`, `codetools`, `tools`, `snippets` …); the windows only pass things on.
- **Stable codes:** messages have a fixed `code` in snake_case and a `detail`, and are serializable with
  serde (`Issue`, `CodegenIssue`, `WizardIssue`, `GitError` …).
- **Preview before writing:** code generation, file operations, Git commands, the new-project wizard and
  `cargo update` show first what would happen.
- **Deterministic:** same input, same output; formats are documented as schemas.

## Command line

```bash
freya-tde path/app.ptdr                 # open a project
freya-tde --commands cmds.json          # apply designer commands (JSON)
freya-tde --generate --show-code        # generate code and open it in the editor
freya-tde --screenshots shots           # every window as PNG, then quit
freya-tde --no-splash                   # without splash screen
```

Designer commands are JSON, e.g.:

```json
[{"command": "add_widget", "form": "main_form", "parent": "window", "type": "core/Button", "name": "ok"},
 {"command": "set_property", "form": "main_form", "element": "ok", "property": "caption", "value": {"Text": "OK"}}]
```

## From Rust

The IDE itself (`freya_tde::Ide`) offers automation for tests and a future AI integration: `run_commands`,
`design_surface` (simulate mouse and keys), `screenshots`, `code_text`/`set_code_text`, `create_project`,
`update_settings`, `run_tool`, `insert_snippet`, `dock_view`, `git_run`, `debug_command` and many more – an
overview is in [`ide/README.md`](../../ide/README.md).

## The AI integration

The core is a library without UI (`freya_tde::ai`); the window **KI** (Window → KI, Ctrl+Shift+K) builds on
it. At the top you choose the AI of the conversation from all profiles (drop-down); the switch applies from the
next request, the conversation goes on with the new AI, and the choice is remembered for the project (like
"Für dieses Projekt" in the settings). Next to it is what the AI may do. Requests run in the background, so
does `cargo`; the AI's changes go through the same commands as the designer (undoable) and show up in open
code tabs at once.

- **Providers:** three protocols cover the usual cloud services and local models, also in the local network –
  Anthropic Messages (Claude), OpenAI-compatible (OpenAI, xAI/Grok, Mistral, DeepSeek, Groq, OpenRouter,
  Ollama, LM Studio, llama.cpp, vLLM …) and Google Gemini. `Client::models` lists the models, `Client::chat`
  sends a request (synchronous, for worker threads), `Client::chat_stream` too, but hands over the text piece by
  piece as it is written (server-sent events; a server without streaming is read the usual way). The AI window
  shows answers this way while they are being written. Errors have stable codes (`auth_failed`, `rate_limited`,
  `unreachable`, `timeout` …). If an API key would go over `http://` to another machine, Freya TDE warns
  (`insecure_key_transport`).
- **Tools** are the library API: reading (`project_info`, `list_files`, `read_file`, `search_files`,
  `read_form`, `component_types`, `problems`, `generated_code`) is always allowed; changing (`write_file`,
  `edit_file`, `designer`, `generate_code`) and running (`cargo` check/clippy/test) follow the project's
  permission. The AI changes forms only through designer commands and never touches generated files.
- **Permission per project:** "read only", "with preview" (default: every change is first a proposal with a
  diff and only "Übernehmen" applies it) and "autonomous". It lives in the personal settings, not in the
  `.ptdr` – a cloned repository cannot grant the AI rights.
- **Conversation:** `Conversation` is a state machine without network or UI (`say`, `request`, `receive`,
  `decide`); `ai::run` handles one message in a loop (tests, scripts).
- **Skills** in the open Agent Skills format: built in are the skills of Freya TDE (`ide/skills`), RUF and the
  CT-Editor. A project's own skills (`.agents/skills/`) apply only after approval; the approval remembers the
  SHA-256 of the content, so a changed skill needs a new one. The AI first sees only names and descriptions
  and loads the text when needed (`load_skill`).
- **Skill packs** bring new skills between two releases. The launcher puts them into the write-protected
  installation folder: `<program>/../share/freya-tde/skills` (Linux, Homebrew) or `<program folder>/skills`
  (Windows); `FREYA_TDE_SKILL_PACK` comes first (tests). A pack consists of `pack.json` (`format`, `version`,
  `min_freya`, the SHA-256 of every `<name>/SKILL.md`), `pack.json.sig` (Ed25519 signature over `pack.json`,
  hex) and the skill folders. Freya TDE checks the signature against the built-in key, the minimum version and
  every file; unlisted files are not allowed. Any error refuses the whole pack, the message appears under
  "Probleme" (source KI, codes `skill_pack_no_key`, `skill_pack_signature`, `skill_pack_hash`,
  `skill_pack_version`, `skill_pack_invalid`). A pack skill replaces the built-in one of the same name and is
  trusted like it.

A test against a real provider runs with
`FREYA_TDE_TEST_AI="ollama|http://localhost:11434|qwen3:8b" cargo test -p freya-tde --test ai_live`
(key from `FREYA_TDE_AI_KEY`).
