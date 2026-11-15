# 12 Settings

[Contents](README.md) · back: [11 Git](11-git.md) · next: [13 Automation and AI](13-automation.md)

**Datei → Einstellungen …** (File → settings, Ctrl+,) applies to Freya TDE as a whole. Only "OK" takes the
changes over.

## General

| Setting | Effect |
|---|---|
| Working directory | start folder of the file dialogs and the new-project wizard, e.g. `~/DevArea/RUST` (empty = home folder) |
| Language | System, Deutsch, English – for now only for the CT-Editor; Freya TDE's own UI is still German |
| Appearance | System (default), Light, Dark – for Freya TDE and the CT-Editor |
| New methods | where code completion puts new methods: after the calling one, alphabetically, at the end |

## AI

Several **AI profiles** with provider (Anthropic, OpenAI, Google Gemini, xAI, Mistral, DeepSeek, Groq,
OpenRouter, Ollama, LM Studio or an own OpenAI-compatible server – local or in the network), model, address and API key; one default profile and one per project ("because project X is like this, I use
AI Y"). **Abrufen** next to the model asks the provider for its models – with address and key as they are in
the window, so already before "OK" – and fills the drop-down with them; the model can still be typed freely.
If the API key would go over `http://` to another machine, a warning shows under the key.
**Rechte der KI** (AI permission) applies to the current project: "read only", "with preview" (default) or
"autonomous" (the same as in the AI window, see [chapter 13](13-automation.md)).
Plus "show the AI window for a new project" and "always show".

**API keys** are kept in the system keyring (Secret Service, macOS Keychain, Windows Credential Manager);
only if none is available, in the settings file. `FREYA_TDE_AI_KEY` applies to profiles without a key.
Changes reach the keyring only with "OK".

## Where things are stored

Everything under `<config>/freya-tde/` (Linux `~/.config/freya-tde`, macOS
`~/Library/Application Support/freya-tde`, Windows `%APPDATA%\freya-tde`):

| File | Content |
|---|---|
| `settings.toml` | these settings (readable only by the user) |
| `desktop.toml` | window arrangement, combined windows |
| `projects.toml` | project list (main and subprojects) |
| `debug.toml` | breakpoints and watches |
| `snippets.toml` | code snippets |
