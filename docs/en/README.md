# Freya TDE – the handbook

🇩🇪 [Deutsche Version](../de/README.md)

Freya TDE is a visual development environment for Rust in the style of Delphi and Lazarus: put forms
together in the designer, wire up components, write the events in Rust – and generate the result for the UI
toolkit you want. This handbook is laid out like a book: first the overview, then Freya TDE itself, then its
two building blocks RUF and CT-Editor, and finally everything about known problems and limits.

## Contents

**Part I – Overview**

1. [What is what?](01-overview.md) – Freya TDE, RUF and RNCL, CT-Editor, the database helper
2. [Installing and building](02-installation.md) – requirements per system, building from source

**Part II – Freya TDE**

3. [First steps](03-first-steps.md) – from the new-project wizard to a running program
4. [The windows](04-windows.md) – main window, designer, object inspector, code, messages …
5. [Forms and components](05-components.md) – the model behind the designer
6. [UI backends](06-backends.md) – GTK 4, egui, iced, Slint, Flutter, WebView
7. [Component packs](07-packs.md) – Tokio, SQLx, Redis, Axum, WebSocket, HTTP
8. [Databases and data binding](08-data.md) – lists, grids, editing, schema browser
9. [Generated and own code](09-code.md) – which file belongs to whom, code completion, snippets
10. [Building, running, debugging](10-building.md) – tool bar, debugger, terminal
11. [Git](11-git.md) – source control like in VS Code
12. [Settings](12-settings.md) – working directory, language, appearance, AI profiles
13. [Automation and AI](13-automation.md) – everything also without the UI

**Part III – RUF**

14. [RUF and RNCL](14-ruf.md) – the UI framework Freya TDE and the CT-Editor run on

**Part IV – CT-Editor**

15. [CT-Editor](15-ct-editor.md) – the visual editor for `Cargo.toml`

**Part V – Known problems**

16. [Known problems and limits](16-problems.md) – what we found, cause and solution

**Appendix**

- [A – Glossary](a-glossary.md)
- [B – Licenses](b-licenses.md)
- [C – How it was made](c-history.md)

## Further references

The handbook explains and points the way; the detailed references live next to the sources:

| Topic | File |
|---|---|
| Windows and operation in detail | [`ide/README.md`](../../ide/README.md) |
| UI backends, pack format, limits per backend | [`widgets/README.md`](../../widgets/README.md) |
| Component packs | [`components/README.md`](../../components/README.md) |
| Component model, code generation | [`core/README.md`](../../core/README.md) |
| Designer logic | [`designer/README.md`](../../designer/README.md) |
| Database helper | [`db/README.md`](../../db/README.md) |
| RUF developer docs | [RUF/docs/en](https://github.com/Freya-TDE/RUF/blob/main/docs/en/README.md) |
| CT-Editor manual | [CT-E/docs/en](https://github.com/Freya-TDE/CT-E/blob/main/docs/en/README.md) |

The detailed reference files above are largely written in German with English summaries.
