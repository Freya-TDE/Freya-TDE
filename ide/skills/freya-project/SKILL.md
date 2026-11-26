---
name: freya-project
description: Structure of a Freya TDE project and where which code belongs (user code, generated code, handler stubs, Cargo.toml, packs, data binding). Use before writing code in a Freya project, filling an event handler, adding a dependency or fixing a build error.
---

# A Freya TDE project

## Files

| File | Owner | What the AI may do |
|---|---|---|
| `<name>.ptdr` | project (backend, packs, forms, target platforms) | only via `designer`/tools |
| `src/<form>.ftdr` | form | only via `designer` |
| `src/<form>.gen.rs`, `src/app.gen.rs` | generator, rewritten on every run, read-only | **never** change; change the form and run `generate_code` |
| `src/<form>.rs` | user: handlers and own logic | `edit_file`/`write_file` |
| `src/main.rs` or `src/lib.rs` | user, created once | `edit_file` |
| `Cargo.toml` | user; the generator only adds | `edit_file` (do not remove what packs need) |

`.gen.rs` files carry a checksum; foreign changes are reported and overwritten.

## Handlers

An event is connected to a handler in the designer (`set_event`). For missing handlers,
`generate_code` appends an empty stub as a new `impl` block to `src/<form>.rs`; existing code
stays. Then fill the body with `edit_file`. Do not change the signature: it comes from the type
(`component_types` shows the parameters, e.g. `on_change(text: &str)`).

In a handler, `self` is the form: every widget is a field with its name. How to address it
depends on the UI backend (`project_info` → `backend`):

- `gtk4`: real gtk4-rs types, e.g. `self.name_edit.text()`, `self.ok_button.set_sensitive(false)`.
- `egui`, `iced`, `flutter`, `webview`: `Control<T>` with `value`, `caption`, `items`, `enabled`,
  `visible`, `tooltip`, e.g. `self.name_edit.value = "x".into()`, `self.ok_button.enabled = false`.
- `slint`: properties `self.window.set_<widget>_<property>(…)` / `get_…()`.

The generated code (`generated_code` with `path`) shows the exact types.

Do not put own fields of the form into the generated struct; put them into the form property
`state` (`name: Type` or `name: Type = initial value`, one line per field), via `designer`
`set_property` on `window`.

## Non-visual components and packs

Packs (e.g. `sqlx`, `redis`, `axum`, `ws`, `http`, `tokio`) provide components for data modules.
The generator builds the service container, derives the initialisation order from the
references and adds the Cargo dependencies with their features. Components with `async` need a
runtime (`tokio/Runtime`) in the project; otherwise `problems` reports the code for it.

## Data binding

A data source (e.g. `sqlx/Query` with `columns` as `name: Type`) fills lists and DataGrids via
`data_source`, `display_field`, `value_field`, `load`. The generator creates `<list>_reload()`,
`<list>_selected_value()`, for tables `<grid>_selected_row()`, and for editing
`on_post`/`on_delete`/`on_post_error`.

## Way of working

1. Read first: `project_info`, `read_form`, `read_file`, `problems`.
2. Change the form (`designer`), `generate_code`, fill the handlers (`edit_file`).
3. `cargo` `check` (or `clippy`), fix the messages. Messages in `.gen.rs` belong to an element of
   the form: look for the cause there, not in the generated code.
4. Small, traceable steps; every change is shown to the developer as a preview first, unless the
   project is set to “autonomous”.
