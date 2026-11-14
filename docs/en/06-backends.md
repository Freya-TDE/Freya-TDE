# 6 UI backends

[Contents](README.md) · back: [5 Forms and components](05-components.md) · next: [7 Component packs](07-packs.md)

A UI backend translates the neutral form model into code for a particular toolkit. It is a pack in
`widgets/<name>/` (a `pack.toml` with `kind = "backend"` plus templates) and brings its own palette. The
toolkit ends up only in the generated project, never in the IDE. The full description including each
backend's limits is in [`widgets/README.md`](../../widgets/README.md).

| Backend | Toolkit | Toolkit license | How the generated code works |
|---|---|---|---|
| `gtk4` (default) | GTK 4 (gtk4-rs) | MIT / LGPL | every widget is a real gtk4 object; handlers get `&mut self` |
| `egui` | egui/eframe 0.37 | MIT OR Apache-2.0 | immediate mode; widgets are state fields `Control<T>` |
| `iced` | iced 0.14 | MIT | Elm architecture: one `Message` enum per form, `update`, `view` |
| `slint` | Slint 1.18 | GPLv3 / Royalty-free 2.0 / commercial | the form becomes a Slint component (`slint::slint!`) |
| `flutter` | Flutter (Material 3) through flutter_rust_bridge 2.13 | BSD-3 / MIT | Rust as a library, a Flutter app in `flutter/` draws |
| `webview` | the system web view through wry 0.57 + tao 0.37 | MIT OR Apache-2.0 | the form becomes an HTML page, the logic stays in Rust |

## Choosing

The backend is chosen in the new-project wizard; switching later is not intended. GTK 4 is preselected.

**Slint** is available under several licenses (GPLv3, Royalty-free 2.0 with attribution to Slint and not for
embedded, or commercial). Freya TDE only passes Slint through: before creating the project, the wizard shows
the license notice and you decide. For the Royalty-free condition the palette has `AboutSlint`.

## What all backends have in common

- the core widgets and a palette of their own (Switch, Slider, ComboBox, ListBox, Tabs, main menu,
  DataGrid …),
- layouts (vertical, horizontal, grid, stack, absolute), requested size, percent and anchors,
- colours, fonts, window settings,
- data binding of lists and grids, editing in the DataGrid ([chapter 8](08-data.md)),
- events of non-visual components (WebSocket, HTTP, Axum) through a queue of the form.

Each backend does what its toolkit can; where it cannot, it is listed in the "limits" table of
[`widgets/README.md`](../../widgets/README.md) and collected in
[chapter 16](16-problems.md#limits-of-the-ui-backends).

## Flutter in particular

- The Rust project becomes a library (`cdylib`, plus `staticlib` for macOS); the Flutter app lives in
  `flutter/` and is named `<library>_app`.
- **Run** (F9) runs `flutter create` once, then `flutter build`, and starts the app.
- For finished programs, `flutter/rust_builder/` (cargokit) builds the Rust library with every
  `flutter build` and bundles it.
- Window title, size and start state come from the designer through the Dart package `window_manager`.

## WebView in particular

- A native window with the system's web engine (WebKitGTK 4.1, WebView2, WKWebView).
- Form property `style`: **Native** (own CSS with system colours) or **Pico** (Pico CSS, MIT).
- On Linux the program sets `WEBKIT_DISABLE_DMABUF_RENDERER=1`, otherwise the window stays empty with some
  drivers.

## Backends of your own

A backend is a `pack.toml` with `[backend]` (event loop, Cargo dependencies, templates `form`, `run`, `app`,
further files) and `[[extend]]` entries that give the core widgets their code. The format is described in
[`widgets/README.md`](../../widgets/README.md); `cargo run -p tdr-core --example check_packs -- widgets`
checks all packs.
