# 1 What is what?

[Contents](README.md) · next: [2 Installing and building](02-installation.md)

Three programs or libraries from three repositories work together. Telling them apart makes it easier to
find your way around.

```text
┌──────────────────────────────── Freya TDE (repository Freya-TDE) ───────────────────────────────┐
│  ide/        the development environment (program freya-tde)                                     │
│  core/       component model, dependency graph, pack format, code generation (tdr-core)          │
│  designer/   designer logic without UI (tdr-designer)                                            │
│  components/ component packs (Tokio, SQLx, Redis, Axum, WebSocket, HTTP)                         │
│  widgets/    UI backends for target projects (GTK 4, egui, iced, Slint, Flutter, WebView)        │
│  db/         database helper freya-tde-db (separate process)                                     │
│                                                                                                  │
│  RUF/  ──────► submodule: Dragon-InterActive/RUF   (UI framework of the IDE)                     │
│  CT-Editor/ ─► submodule: Dragon-InterActive/CT-E  (editor for Cargo.toml, also built on RUF)    │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

## Freya TDE

The development environment. Freya + TDE = **T**op-**D**own **E**ditor: you start with the big picture –
forms, components, connections – and only write the logic yourself. The models are Delphi 7 and Lazarus:
free-floating windows, a component palette, an object inspector, double-clicking a button jumps to its
handler.

The key point is the split into **two worlds**:

1. **The IDE's own user interface** is built with RUF (GTK 4) – the same on Linux, Windows and macOS.
2. **The programs you build with Freya TDE** (target projects) may use any supported UI toolkit: GTK 4, egui,
   iced, Slint, Flutter or a web view. These toolkits are not compiled into the IDE. Only the *description*
   of the forms is unified; per toolkit, a backend pack translates it into code.

That is why the designer does not show forms "live" but draws them itself from the backend's metrics. The
real view comes from the generated program, started as a separate process – a crash there does not take the
IDE down.

## RUF and RNCL

**RUF** (Rust native UI Framework) is a desktop UI framework for Rust based on GTK 4. **RNCL** (Rust native
Component Library) provides the components on top: windows, buttons, text fields, lists, tree, tabs, a code
editor with GtkSourceView … RUF is a project of its own and can be used without Freya TDE. More in
[chapter 14](14-ruf.md).

## CT-Editor

The **CT-Editor** (Cargo TOML Editor, repository CT-E) edits `Cargo.toml` files visually: package,
dependencies with crates.io search, features, further sections – format-preserving, comments stay. It runs
on its own with a File menu, or embedded from Freya TDE (project window → open `Cargo.toml`). More in
[chapter 15](15-ct-editor.md).

## The database helper

**freya-tde-db** (folder `db/`) is a small helper program that Freya TDE starts at design time to ask
databases: test a connection, read tables and columns, find the result columns of a query, list Redis keys.
The database drivers live only in this helper, not in the IDE.

## How the parts work together

1. Freya TDE loads the **packs** (`components/`, `widgets/`) – declarative descriptions in TOML with code
   templates.
2. The **designer** builds the neutral form model (components, properties, events), saved as `.ftdr` next to
   the `.rs` file, the project as `.ptdr`.
3. **Generate code** translates the model through the chosen backend into Rust (and for Flutter into Dart).
4. **Build and run** calls Cargo; compiler messages lead back to the component in the designer.
