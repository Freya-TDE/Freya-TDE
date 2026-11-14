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

## The planned AI integration

The AI profiles are already prepared in the settings. The integration itself – an AI window that uses the
library API directly – is the next big step.
