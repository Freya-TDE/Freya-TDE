---
name: freya-forms
description: Formulare in Freya TDE über Designer-Befehle anlegen und ändern (Widgets, nicht-visuelle Komponenten, Eigenschaften, Ereignisse, Layout, Platzierung). Verwenden, wenn eine Oberfläche entworfen, ein Widget hinzugefügt, eine Eigenschaft gesetzt oder ein Ereignis verbunden werden soll.
---

# Formulare mit Designer-Befehlen

Formulare (`src/<form>.ftdr`) werden **nie als Datei** geschrieben, sondern über das Werkzeug
`designer` mit einer Liste von Befehlen. Mehrere Befehle in einem Aufruf sind ein
Rückgängig-Schritt; schlägt einer fehl, passiert nichts.

## Vorgehen

1. `project_info` zeigt Formulare und Elemente, `read_form` den Baum eines Formulars.
2. `component_types` (mit `filter`) zeigt Typen, Eigenschaften (`name`, `type`) und Ereignisse.
   Nur Typen verwenden, die dort stehen; Eigenschaften nur mit passendem Werttyp.
3. Befehle mit `designer` senden.
4. Danach `generate_code`, dann Handler in `src/<form>.rs` mit `edit_file` füllen.
5. `cargo` mit `check` prüft das Ergebnis.

## Befehle

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

- Ohne `name` wählt der Designer einen freien Namen (`button1`); Pack-Typen tragen den Pack
  (`sqlx_pool1`). Namen: `snake_case`, eindeutig im Formular. `main` und `app` sind reserviert.
- Die Wurzel eines Formulars heißt `window` (Typ `core/Form`). Ein Datenmodul hat keine Widgets,
  nur nicht-visuelle Komponenten (Service-Container).
- `value: null` setzt eine Eigenschaft auf den Standard zurück, `handler: null` trennt ein Ereignis.

## Werte

`{"Bool":true}`, `{"Int":5}`, `{"Float":1.5}`, `{"Text":"Hallo"}`, `{"Enum":"Postgres"}`,
`{"Color":{"r":255,"g":0,"b":0,"a":255}}`, `{"Point":{"x":10,"y":20}}`,
`{"Size":{"width":200,"height":30}}`, `{"List":[{"Text":"a"},{"Text":"b"}]}`,
`{"Reference":"db_pool"}` (gleiches Formular) bzw. `{"Reference":"services::db_pool"}`
(anderes Formular), `{"Code":"Duration::from_secs(5)"}`.
Pro Plattform: `{"value":{"Text":"OK"},"platforms":{"MacOs":{"Text":"Okay"}}}`.

Farbe mit `a: 0` = Standard des Backends.

## Kern-Widgets

`core/Label` (`text`), `core/Button` (`caption`, `on_click`), `core/Edit` (`text`, `placeholder`,
`read_only`, `password`, `on_change(text)`, `on_submit`), `core/CheckBox` (`caption`, `checked`,
`on_toggle(checked)`), `core/Container` (Kinder, Layout). Alle Widgets: `enabled`, `visible`,
`tooltip`, `background`, `foreground`, `font_size`, `bold`, `italic`. Das Formular: `title`,
`size`, `resizable`, `on_create`, `on_close` u. a. Weitere Widgets bringt das UI-Backend
(`component_types`).

## Layout und Platzierung

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

- Layout-Arten: `Vertical` (Standard), `Horizontal`, `{"Grid":{"columns":n}}`, `Stack`, `Absolute`
  (nur wenn das Backend absolute Positionen erlaubt). Ohne eigenes Layout gilt senkrecht,
  Abstand 8, Rand 12.
- `size` je Achse 0 = natürliche Größe; `percent` (1–100) geht `size` vor.
- `align`: `Fill`, `Start`, `Center`, `End`. `stretch` verteilt freien Platz.

## Verdrahtung (Provides/Requires)

Nicht-visuelle Komponenten verweisen über Referenz-Eigenschaften aufeinander, z. B. eine
SQLx-`Query` auf ihren `Pool`, ein Listen-Widget über `data_source` auf eine Datenquelle. Nach dem
Verdrahten zeigt `problems` fehlende Anforderungen und Zyklen mit Codes.
