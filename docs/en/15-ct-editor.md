# 15 CT-Editor

[Contents](README.md) · back: [14 RUF and RNCL](14-ruf.md) · next: [16 Known problems and limits](16-problems.md)

The **CT-Editor** (Cargo TOML Editor) edits `Cargo.toml` files visually. It is a project of its own
(repository [Dragon-InterActive/CT-E](https://github.com/Dragon-InterActive/CT-E), a submodule `CT-Editor/` in
Freya-TDE), built with RUF, and runs on Linux, Windows and macOS. Ready-made packages for all three systems
are on the repository's release page (latest 0.3.1).

## What it does

| Tab | Content |
|---|---|
| Source | the `Cargo.toml` with highlighting and live validation |
| Dependencies | crates.io search (3+ characters), versions, features as check boxes (preset with the crate's default features), target and build dependencies |
| Package | name, version, edition, Rust version, description, license … |
| Features | your own features and what they enable |
| Advanced | every further section: `[lib]`, `[[bin]]`, `[profile.*]`, `[workspace]` … |

It works **format-preserving** (comments and order stay), validates with `cargo metadata` and speaks German
and English (further languages can be added as TOML files).

## On its own or inside Freya TDE

- **On its own** it has a **File** menu with Open (Ctrl+O), Save (Ctrl+S), Save as (Ctrl+Shift+S; a new,
  never saved file asks for the location on saving), Language, Appearance (System, Light, Dark) and Quit. It
  remembers language and appearance; the file dialogs start in the project folder.
- **In Freya TDE:** project window → right-click `Cargo.toml` → open with the CT-Editor. Freya TDE starts it
  with `--embedded` (no File menu) and passes language and appearance from its own settings.

```bash
ct-editor --path /path/to/project                           # on its own
ct-editor --embedded --lang en --theme dark --path …        # how Freya TDE starts it
ct-editor --help                                            # all options
```

## First start of the ready-made packages

The packages are not notarized by Apple or signed for Windows (code signing is planned for later):

- **macOS:** since macOS 15, right-click → Open is no longer enough. After the message, choose "Done", then
  *System Settings → Privacy & Security → "Open Anyway"* (once) – or in the terminal
  `xattr -dr com.apple.quarantine CT-Editor.app`.
- **Windows:** SmartScreen warns: *More info → Run anyway* (or tick "Unblock" in the ZIP's properties before
  extracting).

## Further reading

The CT-Editor manual (German and English) with all options, workflows, shortcuts, languages and the release
workflow: [CT-E/docs/en](https://github.com/Dragon-InterActive/CT-E/blob/main/docs/en/README.md).

**License:** MIT OR Apache-2.0 at your option.
