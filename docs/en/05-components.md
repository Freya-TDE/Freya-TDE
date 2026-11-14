# 5 Forms and components

[Contents](README.md) · back: [4 The windows](04-windows.md) · next: [6 UI backends](06-backends.md)

Behind the designer is a neutral model that does not depend on any UI toolkit (crate `tdr-core`).

## Project and forms

- A **project** is a `.ptdr` file (RON): name, Rust version, UI backend, packs, target systems, forms. A
  project has exactly one UI backend.
- A **form** is a `.ftdr` file next to its `.rs`: `src/main_form.ftdr` ↔ `src/main_form.rs`.
- A form without widgets is a **data module** (service container, like Delphi's DataModule), e.g. `services`
  with a database pool and a web server.

## Components and widgets

- **Everything is a component:** name, type, properties, events.
- A **widget** is a component with a visual extension (placement, children).
- **Non-visual components** (timer, database, HTTP …) appear as an icon in the component bar.
- Types come from packs: a small common core (Label, Button, Edit, CheckBox, Container), plus the palette of
  the UI backend and the component packs.

## Properties

Rich basic types: Bool, Int, Float, Text, Enum, Color, Point, Size, List, Reference, code expression, plus
`Custom` for backend-specific types. Values can be overridden **per platform** – the generator turns them
into `cfg` branches.

Common properties of all widgets: `enabled`, `visible`, `tooltip`, `background`, `foreground` (alpha 0 = the
toolkit's default), `font_size`, `bold`, `italic`. The form also has title, size, minimum/maximum size,
position, start state, light/dark, font, background picture – and **`state`**: the form's own fields
(`counter: i32` or `title: String = "New".into()`), which the generator writes into the struct.

## References, provides and requires

Components refer to each other: `db_pool` in the same form, `services::db_pool` in another one. Each
component states what it **provides** (e.g. `sqlx.pool`) and what it **requires** (mandatory, optional or
conditional – Axum needs a pool only if a query is attached). This gives:

- a **check on dropping and connecting** (if something is missing, a message or a suggestion appears),
- the **dependency graph**, which sets the initialisation order in the generated code and detects cycles,
- the **Cargo dependencies** with features (SQLx with `postgres` and `chrono` if a column is
  `chrono::NaiveDate`).

## Layout

The basic form is **container layouts**: vertical, horizontal, grid, stack. Without a layout of its own,
vertical with spacing 8 and padding 12 applies. Inside a layout a widget gets a **requested size** by handle
(like Width/Height in Delphi) or a size in **percent** of its parent. **Absolute placement** is an option of
the backend; there, **anchors** work like in Delphi: making the form or container bigger takes anchored
children along. Lines with a pin on the selected widget show which edges it is anchored to.

## Events

Events are abstract in the model (`on_click`, `on_change` …). The backend decides whether they become a
callback, a check in immediate mode or a message; your code is always a handler stub
`fn name(&mut self, …)` in your own form code.
