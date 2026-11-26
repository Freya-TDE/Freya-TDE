---
name: freya-forms
description: Create and change forms in Freya TDE with designer commands (widgets, non-visual components, properties, events, layout, placement). Use when a user interface is to be designed, a widget added, a property set or an event connected.
---

# Forms with designer commands

Forms (`src/<form>.ftdr`) are **never written as files**, but changed with the tool `designer`
and a list of commands. Several commands in one call are one undo step; if one fails, nothing
happens.

## Steps

1. `project_info` shows forms and elements, `read_form` the tree of one form.
2. `component_types` (with `filter`) shows types, properties (`name`, `type`) and events.
   Use only types listed there, and properties only with a matching value type.
3. Send the commands with `designer`.
4. Then `generate_code`, then fill the handlers in `src/<form>.rs` with `edit_file`.
5. `cargo` with `check` verifies the result.

## Commands

```json
{"command":"add_form","name":"settings_form"}
{"command":"add_form","name":"services","data_module":true}
{"command":"add_widget","form":"main_form","parent":"window","type":"core/Button","name":"ok_button"}
{"command":"add_component","form":"services","type":"sqlx/Pool","name":"db_pool"}
{"command":"set_property","form":"main_form","element":"ok_button","property":"caption","value":{"Text":"OK"}}
{"command":"set_property","form":"main_form","element":"ok_button","property":"caption","value":null}
{"command":"set_event","form":"main_form","element":"ok_button","event":"on_click","handler":"ok_button_click"}
{"command":"rename","form":"main_form","element":"button1","new_name":"save_button"}
{"command":"move","form":"main_form","element":"save_button","parent":"panel1","index":0}
{"command":"remove","form":"main_form","element":"save_button"}
{"command":"rename_form","form":"settings_form","new_name":"options_form"}
{"command":"remove_form","form":"options_form"}
```

- Without `name` the designer picks a free name (`button1`); pack types carry the pack
  (`sqlx_pool1`). Names: `snake_case`, unique in the form. `main` and `app` are reserved.
- The root of a form is called `window` (type `core/Form`). A data module has no widgets, only
  non-visual components (service container).
- `value: null` resets a property to its default, `handler: null` disconnects an event.

## Values

`{"Bool":true}`, `{"Int":5}`, `{"Float":1.5}`, `{"Text":"Hello"}`, `{"Enum":"Postgres"}`,
`{"Color":{"r":255,"g":0,"b":0,"a":255}}`, `{"Point":{"x":10,"y":20}}`,
`{"Size":{"width":200,"height":30}}`, `{"List":[{"Text":"a"},{"Text":"b"}]}`,
`{"Reference":"db_pool"}` (same form) or `{"Reference":"services::db_pool"}`
(other form), `{"Code":"Duration::from_secs(5)"}`.
Per platform: `{"value":{"Text":"OK"},"platforms":{"MacOs":{"Text":"Okay"}}}`.

A color with `a: 0` = default of the backend.

## Core widgets

`core/Label` (`text`), `core/Button` (`caption`, `on_click`), `core/Edit` (`text`, `placeholder`,
`read_only`, `password`, `on_change(text)`, `on_submit`), `core/CheckBox` (`caption`, `checked`,
`on_toggle(checked)`), `core/Container` (children, layout), `core/ImageBox`, `core/ImageScrollBox`.
All widgets: `enabled`, `visible`, `tooltip`, `background`, `foreground`, `font_size`, `bold`,
`italic`. The form: `title`, `size`, `resizable`, `on_create`, `on_close` and more. The UI backend
brings further widgets (`component_types`).

## Layout and placement

```json
{"command":"set_layout","form":"main_form","element":"window",
 "layout":{"kind":"Horizontal","spacing":8,"padding":{"left":12,"top":12,"right":12,"bottom":12}}}
{"command":"set_layout","form":"main_form","element":"panel1","layout":{"kind":{"Grid":{"columns":2}},"spacing":6}}
{"command":"set_placement","form":"main_form","element":"name_edit",
 "placement":{"Layout":{"stretch":1,"align":"Fill","cell":{"row":0,"column":1}}}}
{"command":"set_placement","form":"main_form","element":"ok_button",
 "placement":{"Layout":{"size":{"width":120,"height":0},"percent":{"width":0,"height":0}}}}
{"command":"set_placement","form":"main_form","element":"logo",
 "placement":{"Absolute":{"position":{"x":10,"y":10},"size":{"width":64,"height":64},
  "anchors":{"left":true,"top":true,"right":false,"bottom":false}}}}
```

- Layout kinds: `Vertical` (default), `Horizontal`, `{"Grid":{"columns":n}}`, `Stack`, `Absolute`
  (only if the backend allows absolute positions). Without an own layout: vertical, spacing 8,
  padding 12.
- `size` per axis 0 = natural size; `percent` (1–100) takes precedence over `size`.
- `align`: `Fill`, `Start`, `Center`, `End`. `stretch` distributes free space.

## Wiring (provides/requires)

Non-visual components refer to each other through reference properties, e.g. an SQLx `Query` to
its `Pool`, a list widget via `data_source` to a data source. After wiring, `problems` shows
missing requirements and cycles with codes.
