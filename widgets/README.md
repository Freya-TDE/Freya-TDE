# widgets/

Visuelle Widgets und Backend-Paletten, aufbauend auf `tdr-core` (der gemeinsame Widget-Kern liegt in `core/src/builtin.rs`).
*Visual widgets and backend palettes, built on `tdr-core` (the common widget core lives in `core/src/builtin.rs`).*

| Backend | Zustand / state | Inhalt / content |
|---|---|---|
| `gtk4` | fertig / done | GTK 4 über gtk4-rs (Linux, Windows, macOS); Kern-Widgets plus Palette „GTK“: RadioButton, Switch, SpinButton, Scale, ProgressBar, DropDown, TextView, Frame, Image, Separator, LinkButton |
| `egui` | geplant / planned | |
| `iced` | geplant / planned | |
| `slint` | geplant / planned (Lizenz prüfen / check license) | |

Neue Projekte nutzen `gtk4`, wenn das Pack gefunden wird.
*New projects use `gtk4` when the pack is found.*

## GTK 4

Jedes Widget wird ein Feld des Formular-Structs mit dem echten GTK-Typ (`ok_button: gtk4::Button`);
die Handler in `<form>.rs` bekommen `&mut self` und sprechen die Widgets direkt an:
*Every widget becomes a field of the form struct with its real GTK type; handlers get `&mut self`:*

```rust
impl MainForm {
    pub fn ok_clicked(&mut self) {
        let name = self.name_edit.text();
        self.greeting.set_text(&format!("Hallo {name}"));
    }
}
```

- Ablauf: `setup` ruft `gtk4::init()`, `App::init` erzeugt die Widgets, `run` legt jedes Formular in ein
  `Rc<RefCell<…>>`, baut die Layouts (`build_ui`), verbindet die Ereignisse (`start`) und zeigt das erste
  Formular. Schließt es, endet das Programm; `on_close` kann das verhindern (Rückgabe `false`).
  *Flow: `setup` initialises GTK, `run` builds layouts, connects events and shows the first form.*
- Layouts: senkrecht/waagerecht → `gtk4::Box`, Raster → `gtk4::Grid`, Stapel → `gtk4::Stack`,
  absolut → `gtk4::Fixed`. `stretch` > 0 dehnt, `align` richtet aus. Container ohne Layout: senkrecht,
  Rand 12, Abstand 8 – wie die Theme-Tabelle des Designers.
- Optionsfelder (`RadioButton`) im selben Container bilden eine Gruppe; `TextView` steht in einem
  Rollbereich.
- Löst ein Handler über eine Widget-Änderung ein weiteres Ereignis desselben Formulars aus, wird dieses
  übersprungen (kein doppeltes `borrow_mut`).
  *An event triggered from inside a handler of the same form is skipped.*
- Test: `xvfb-run -a cargo test -p tdr-core --test backend_gtk4` erzeugt ein Projekt mit allen Widgets,
  baut es und klickt im laufenden Programm (`FREYA_TDE_KEEP_DEMO=1` lässt es liegen).

## Backend-Pack / backend pack

Ein UI-Backend ist ein Ordner `widgets/<backend>/` mit einer `pack.toml` (`kind = "backend"`):
eigene Widgets (`[[component]]` mit `kind = "widget"`/`"container"`; sie bekommen wie die Kern-Widgets
`enabled`, `visible` und `tooltip`), Code für die Core-Widgets (`[[extend]]` mit `[extend.codegen]`) und
der Descriptor `[backend]`:
*A UI backend is a directory `widgets/<backend>/` with a `pack.toml` (`kind = "backend"`):*

```toml
[backend]
event-loop = "main-thread"      # oder "async"
absolute-positioning = false
cargo = [{ crate = "eframe", version = "…" }]
setup = "…"                     # optional: Anweisungen vor App::init (z. B. gtk4::init()?;)
form = """impl {{ form.struct }} { pub fn ui(&mut self, …) { … self.{{ w.events.on_click }}(); … } }"""
run = """eframe::run_native(…)?; Ok(())"""
# oder form-file / run-file / setup-file = "templates/…"
```

Symbole: `icon = "switch"` am Typ; gesucht wird zuerst `icons/<name>.png` im Pack-Ordner (48 × 48 px),
dann der eingebaute Satz der IDE (`ide/assets/palette/`, Quellen und Generator `make.py`).
*Icons: `icon = "switch"`; first `icons/<name>.png` in the pack folder (48 × 48 px), then the IDE's
built-in set.*

Maße für den Designer: `[widgets."backend:<backend>/<Typ>"]` in `designer/themes/<plattform>.toml`.
*Natural sizes for the designer go into the theme tables.*

Kontext der Templates und Einzelheiten: Modul-Doku von `tdr_core::codegen` und `tdr_core::pack`.
*Template context and details: module docs of `tdr_core::codegen` and `tdr_core::pack`.*
