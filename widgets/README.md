# widgets/

Visuelle Widgets und Backend-Paletten, aufbauend auf `tdr-core` (der gemeinsame Widget-Kern liegt in `core/src/builtin.rs`).
*Visual widgets and backend palettes, built on `tdr-core` (the common widget core lives in `core/src/builtin.rs`).*

| Backend | Zustand / state | Inhalt / content |
|---|---|---|
| `gtk4` | fertig / done | GTK 4 über gtk4-rs (Linux, Windows, macOS); Kern-Widgets plus Palette „GTK“: RadioButton, Switch, SpinButton, Scale, ProgressBar, DropDown, ListBox, TextView, Notebook, Frame, Image, Separator, LinkButton, MainMenu |
| `egui` | fertig / done | egui 0.37 mit eframe (Immediate Mode); Kern-Widgets plus Palette „egui“: RadioButton, Switch, DragValue, Slider, ProgressBar, ComboBox, ListBox, TextEdit, Hyperlink, Separator, CollapsingHeader, Tabs, MainMenu |
| `iced` | fertig / done | iced 0.14 (Elm-Architektur); Kern-Widgets plus Palette „iced“: RadioButton, Toggler, Slider, ProgressBar, PickList, ListBox, TextEditor, Rule, GroupBox, Tabs |
| `slint` | fertig / done | Slint 1.18 (deklarativ, mit Lizenzhinweis); Kern-Widgets plus Palette „Slint“: RadioButton, Switch, SpinBox, Slider, ProgressIndicator, ComboBox, ListView, TextEdit, Separator, Image, AboutSlint, GroupBox, TabWidget, MainMenu |

Der Neu-Assistent (Datei → Neues Projekt …) bietet alle installierten Backends an, `gtk4` vorgewählt.
*The new-project wizard offers all installed backends, `gtk4` preselected.*

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

## iced

Elm-Architektur: das Formular ist der Zustand, jede Eingabe wird eine Nachricht (`main_form::Message`,
z. B. `ok_button_click`, `name_edit_input(String)`), `update` übernimmt sie in den Zustand und ruft den
Handler, `view` baut daraus die Oberfläche. Widgets sind wie bei egui Felder `crate::app::Control<T>`;
ein Handler ändert den Zustand (`self.greeting.value = …`, `self.window.caption = …`).
*Elm architecture: every input is a `Message`; `update` stores it and calls the handler, `view` builds the UI.*

- `TextEditor` hält `iced::widget::text_editor::Content` (`self.notes.text()` liefert den Text).
- `Tabs` zeigt Reiterknöpfe und die Seite `page`, `GroupBox` einen Rahmen mit Titel.
- Layouts: senkrecht, waagerecht, Raster (Zeilen aus je `columns` Kindern), Stapel (erstes Kind), absolut
  (`stack` mit versetzten Kindern); `stretch` dehnt. `enabled` wirkt auf Eingabe-Widgets, nicht auf Container.
- Kein Hauptmenü und keine Zahlen-Drehfelder: iced hat keine; ebenso kein Bild-Widget (bräuchte Bildlader).
- Gezeigt wird das erste Formular; `on_close` entscheidet über das Schließen (`iced::exit()`).
- Test: `xvfb-run -a cargo test -p tdr-core --test backend_iced` baut ein Projekt mit allen Widgets, startet es und
  klickt mit `xdotool` zweimal auf den Knopf.

## Slint

Slint steht unter GPLv3, der Slint Royalty-free License 2.0 (frei für Desktop/Mobil/Web, auch proprietär,
mit Hinweis auf Slint – z. B. das Widget `AboutSlint` –, nicht für Embedded) oder einer kommerziellen Lizenz.
Freya TDE reicht Slint nur durch: Vor dem Anlegen eines Slint-Projekts zeigt die IDE den `license-notice`
des Packs, der Entwickler entscheidet. Freya TDE selbst hängt nicht von Slint ab; der erzeugte Code ist
MIT OR Apache-2.0.
*Slint is GPLv3 / Royalty-free (attribution, not embedded) / commercial; the IDE shows the notice first.*

Deklarativ: das Formular wird eine Slint-Komponente `FormUi` (per `slint::slint!` in `<form>.gen.rs`), das
einzige Feld ist das Fenster. Jedes Widget hat Properties `<name>_<eigenschaft>` (`_text`, `_caption`,
`_checked`, `_value`, `_items`, `_selected`, `_current`, `_title`, `_enabled`, `_visible`, `_source`) und
Callbacks `<name>_<ereignis>`:
*The form is a Slint component; widgets are its properties:*

```rust
impl MainForm {
    pub fn ok_clicked(&mut self) {
        let name = self.window.get_name_edit_text();
        self.window.set_greeting_text(format!("Hallo {name}").into());
        self.window.set_ok_button_enabled(false);
        self.window.set_window_title("Fertig".into());   // <wurzel>_title
    }
}
```

- `start` verbindet die Callbacks mit den Handlern (`Rc<RefCell<…>>` wie bei GTK), lädt Bilder (`Image`, Datei
  zur Laufzeit) und fragt beim Schließen `on_close`; `run` zeigt das erste Formular.
- Layouts: `VerticalLayout`/`HorizontalLayout` (`stretch`, `align`), `GridLayout` (Zellen oder Reihenfolge),
  Stapel (erstes Kind), absolut (`x`/`y`). Optionsfelder eines Containers bilden eine `RadioGroup`.
- `MainMenu` wird eine `MenuBar` (Pfade, `-`, Untermenüs, Kürzel `| Ctrl+O` → `@keys(Control + "o")`).
- Alle Slint-Bezeichner mit `_` statt `-` (Slint behandelt beide gleich): der Makro-Inhalt läuft durch
  `prettyplease`, das `a-b` in `a - b` trennen würde.
- Kein Tooltip (Slint 1.18 hat keinen).
- Test: `xvfb-run -a cargo test -p tdr-core --test backend_slint` baut ein Projekt mit allen Widgets, startet es
  und klickt Knopf und Menü.

## Aussehen und Fenster / appearance and window

Jedes Widget hat `background`, `foreground` (Farbe; Alpha 0 = Standard des Backends), `font_size`
(0 = Standard), `bold` und `italic`. Das Formular hat dazu `font_family`, `background_image` (Pfad relativ zum
Arbeitsordner des Programms) mit `background_fit` (Cover, Stretch, Contain, Center), `position` (Center,
System, Manual mit `location`), `min_size`/`max_size` (0 = ohne Grenze), `start_state` (Normal, Maximized,
Fullscreen) und `theme` (System, Light, Dark). Container haben ebenfalls ein Hintergrundbild. Schrift und
Vordergrund des Formulars erben die Widgets, im Designer wie im erzeugten Programm.
*Every widget has colours and font settings; the form adds font family, background picture and window
settings. Alpha 0 means "backend default".*

| | GTK 4 | egui | iced | Slint |
|---|---|---|---|---|
| Farben, Schriftgröße | CSS je Widget | ja | ja | Text: ja; Standard-Widgets nur Hintergrundfläche |
| fett / kursiv | ja | nur kursiv | ja | ja (Text, Fenster-Standard) |
| Schriftfamilie | ja | nein | ja | ja |
| Hintergrundbild | ja (CSS) | ja | ja | ja (Formular) |
| Position, Höchstgröße | nein (GTK 4 kann es nicht) | ja | ja | ja |
| Mindestgröße, Zustand, Hell/Dunkel | ja | ja | ja | ja |

Grenzen: iced wendet `enabled` nicht auf Container an; Slints Standard-Widgets haben keine Farb-Properties.
*Limits: see table; iced ignores `enabled` on containers, Slint std widgets have no colour properties.*

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

Lizenzhinweis: `license-notice = """…"""` im Abschnitt `[pack]` (z. B. Lizenzbedingungen einer Bibliothek, von
der das erzeugte Projekt abhängt). Die IDE zeigt ihn vor dem Anlegen eines Projekts mit dem Backend und lässt
den Entwickler entscheiden; per API: `Ide::backend_notice`, `BackendDescriptor::license_notice`.
*`license-notice` is shown before a project uses the backend; the developer decides.*

In Templates schreibt der Filter `rust_str` einen Text als Rust-Stringliteral (`{{ item | rust_str }}`).

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

Tabellen (DataGrid, alle vier Backends): `[component.table]` mit `set-rows` (Anweisungen mit `headers: Vec<String>`
und `rows: Vec<Vec<String>>`) und `selected`. Das Widget bekommt `data_source`, `fields` (gezeigte Spalten, leer =
alle), `headers` (Titel), `value_field` und `load`; der Kern erzeugt ein Zeilen-Struct `<Widget>Row` mit allen Spalten
der Quelle (typisiert nach `columns`), `<name>_selected_row()` und `<name>_selected_value()`. Ereignis `on_select(index)`.
GTK: `ColumnView`, egui: gestreiftes Grid, iced: `table`, Slint: `StandardTableView`.
*Data grids declare `set-rows`/`selected`; the core generates a typed row struct and `<name>_selected_row()`.*

Listen-Widgets mit Daten: `[component.list]` mit `set-items` (Anweisungen, die `items: Vec<String>` ins Widget
setzen) und `selected` (Ausdruck `Option<usize>`); Kontext `name`, `root`, `form`. Solche Widgets bekommen
`data_source`, `display_field`, `value_field` und `load`; der Kern erzeugt `<name>_reload()`,
`<name>_selected_value()`, `freya_start_data()` und `freya_take_data()`. Das Backend ruft beim Öffnen
`freya_start_data()` und, wenn Daten da sind, `freya_take_data()` (GTK/Slint: Zeitgeber 100 ms, egui: Repaint über
`crate::app::freya_set_waker`, iced: Subscription). Im Kontext des `form`-Templates zeigt `data_bound`, ob das
Formular gebundene Listen hat. Datenmodell: Modul-Doku von `tdr_core::data`.
*List widgets declare `set-items`/`selected`; the core generates loading, the backend polls or is woken.*

Kontext der Templates und Einzelheiten: Modul-Doku von `tdr_core::codegen` und `tdr_core::pack`.
*Template context and details: module docs of `tdr_core::codegen` and `tdr_core::pack`.*
