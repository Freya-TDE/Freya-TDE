# widgets/

Visuelle Widgets und Backend-Paletten, aufbauend auf `tdr-core` (der gemeinsame Widget-Kern liegt in `core/src/builtin.rs`).
*Visual widgets and backend palettes, built on `tdr-core` (the common widget core lives in `core/src/builtin.rs`).*

| Backend | Zustand / state | Inhalt / content |
|---|---|---|
| `gtk4` | fertig / done | GTK 4 über gtk4-rs (Linux, Windows, macOS); Kern-Widgets plus Palette „GTK“: RadioButton, Switch, SpinButton, Scale, ProgressBar, DropDown, ListBox, TextView, Notebook, Frame, Image, Separator, LinkButton, MainMenu |
| `egui` | fertig / done | egui 0.37 mit eframe (Immediate Mode); Kern-Widgets plus Palette „egui“: RadioButton, Switch, DragValue, Slider, ProgressBar, ComboBox, ListBox, TextEdit, Hyperlink, Separator, CollapsingHeader, Tabs, MainMenu |
| `iced` | geplant / planned | |
| `slint` | geplant / planned (Lizenz prüfen / check license) | |

Neue Projekte nutzen `gtk4`, wenn das Pack gefunden wird; „Datei → Neues Projekt mit Oberfläche“ wählt ein
anderes (GTK 4, egui, ohne Oberfläche).
*New projects use `gtk4`; File → New project with UI picks another backend.*

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
- `Notebook`: jedes Kind ist eine Seite (meist ein Container), Titel aus `pages`, sonst der Name;
  `page` ist die Startseite und die Seite, die der Designer zeigt (Klick auf einen Reiter wechselt).
- `MainMenu` (nicht-visuell, in der Komponentenleiste) setzt eine Menüleiste über das Formular. Einträge
  als Pfade, einer je Zeile: `Datei/Öffnen | Ctrl+O`, `Datei/-` (Trennlinie), `Datei/Export/PDF`
  (Untermenü). Gewählt wird `on_select(item: &str)` mit dem Pfad, z. B. `"Datei/Öffnen"`; Kürzel gelten
  im ganzen Fenster (macOS: Ctrl = Befehlstaste).
  *Entries as paths, `| shortcut`, `-` = separator; `on_select` gets the path.*
- Löst ein Handler über eine Widget-Änderung ein weiteres Ereignis desselben Formulars aus, wird dieses
  übersprungen (kein doppeltes `borrow_mut`).
  *An event triggered from inside a handler of the same form is skipped.*
- Test: `xvfb-run -a cargo test -p tdr-core --test backend_gtk4` erzeugt ein Projekt mit allen Widgets,
  baut es und klickt im laufenden Programm (`FREYA_TDE_KEEP_DEMO=1` lässt es liegen).

## egui

Immediate Mode: egui zeichnet jedes Bild neu aus dem Zustand des Formulars. Jedes Widget ist ein Feld
`crate::app::Control<T>` mit `value` (Text, Häkchen, Zahl, Auswahl …), `caption`, `items`, `enabled`,
`visible` und `tooltip`; ein Handler ändert ein Widget, indem er diesen Zustand ändert:
*Every widget is a `Control<T>` state field; handlers change widgets by changing that state:*

```rust
impl MainForm {
    pub fn ok_clicked(&mut self) {
        self.greeting.value = format!("Hallo {}", *self.name_edit); // value per Deref
        self.ok_button.enabled = false;
        self.window.caption = "Fertig".into();                     // Fenstertitel
    }
}
```

- Ablauf: `run` startet eframe mit dem ersten Formular; dessen `ui()` zeichnet Menüleiste und Inhalt,
  jeder Container hat eine Methode `freya_ui_<name>`. Ereignisse werden direkt nach dem Zeichnen des
  Widgets geprüft (`response.clicked()` …) und rufen die Handler auf – ohne `Rc`/`RefCell`.
- Layouts: senkrecht (Fill = volle Breite für Knöpfe, Eingaben, Listen), waagerecht, Raster (`egui::Grid`,
  Reihenfolge der Kinder), Stapel (nur das erste Kind), absolut (`ui.put`). Rand und Abstand wie im Designer.
- `Tabs`: Reiterleiste und die Seite `page`; `CollapsingHeader`: aufklappbarer Container; `MainMenu`: wie bei
  GTK (`Datei/Öffnen | Ctrl+O`, `-`, Untermenüs), Kürzel mit Strg = Befehlstaste unter macOS.
- Weitere Formulare behalten ihren Zustand in `app`; gezeigt wird das erste.
- Hilfen in `app.gen.rs` (Template `app`): `Control<T>`, `switch`, `shortcut`, `menu_bar`.
- Test: `xvfb-run -a cargo test -p tdr-core --test backend_egui` baut ein Projekt mit allen Widgets, startet es
  und klickt mit `xdotool` auf Knopf und Menü (`FREYA_TDE_SHOT=<ordner>` speichert ein Bild).

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
app = "…"                       # optional: Code für app.gen.rs (gemeinsame Hilfen, z. B. Control<T>)
form = """impl {{ form.struct }} { pub fn ui(&mut self, …) { … self.{{ w.events.on_click }}(); … } }"""
run = """eframe::run_native(…)?; Ok(())"""
# oder form-file / run-file / setup-file / app-file = "templates/…"
```

Symbole: `icon = "switch"` am Typ; gesucht wird zuerst `icons/<name>.png` im Pack-Ordner (48 × 48 px),
dann der eingebaute Satz der IDE (`ide/assets/palette/`, Quellen und Generator `make.py`).
*Icons: `icon = "switch"`; first `icons/<name>.png` in the pack folder (48 × 48 px), then the IDE's
built-in set.*

Darstellung im Designer: `look = "…"` am Typ (`switch`, `slider`, `spin`, `progress`, `dropdown`, `memo`,
`list`, `radio`, `separator`, `image`, `frame`, `link`, `tabs`; `menubar` für ein nicht-visuelles Hauptmenü),
gezeichnet in den Farben der Theme-Tabelle; ohne `look` ein Kasten mit Symbol und Namen. Maße:
`[widgets."backend:<backend>/<Typ>"]` in `designer/themes/<plattform>.toml`, bei Containern `header` =
Platz oben (Rahmentitel, Reiterleiste).
*Designer drawing: `look` on the type (same looks for every backend); sizes and `header` in the theme tables.*

Kontext der Templates und Einzelheiten: Modul-Doku von `tdr_core::codegen` und `tdr_core::pack`.
*Template context and details: module docs of `tdr_core::codegen` and `tdr_core::pack`.*
