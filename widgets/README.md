# widgets/

Visuelle Widgets und Backend-Paletten, aufbauend auf `tdr-core` (der gemeinsame Widget-Kern liegt in `core/src/builtin.rs`).
*Visual widgets and backend palettes, built on `tdr-core` (the common widget core lives in `core/src/builtin.rs`).*

| Backend | Zustand / state | Inhalt / content |
|---|---|---|
| `gtk4` | fertig / done | GTK 4 über gtk4-rs (Linux, Windows, macOS); Kern-Widgets plus Palette „GTK“: RadioButton, Switch, SpinButton, Scale, ProgressBar, DropDown, ListBox, TextView, Notebook, Frame, Image, Separator, LinkButton, MainMenu |
| `egui` | fertig / done | egui 0.37 mit eframe (Immediate Mode); Kern-Widgets plus Palette „egui“: RadioButton, Switch, DragValue, Slider, ProgressBar, ComboBox, ListBox, TextEdit, Hyperlink, Separator, CollapsingHeader, Tabs, MainMenu |
| `iced` | fertig / done | iced 0.14 (Elm-Architektur); Kern-Widgets plus Palette „iced“: RadioButton, Toggler, Slider, ProgressBar, PickList, ListBox, TextEditor, Rule, GroupBox, Tabs |
| `slint` | fertig / done | Slint 1.18 (deklarativ, mit Lizenzhinweis); Kern-Widgets plus Palette „Slint“: RadioButton, Switch, SpinBox, Slider, ProgressIndicator, ComboBox, ListView, TextEdit, Separator, Image, AboutSlint, GroupBox, TabWidget, MainMenu |
| `flutter` | fertig / done | Flutter (Material 3) mit flutter_rust_bridge 2.13, Logik in Rust; Kern-Widgets plus Palette „Flutter“: RadioButton, Switch, Slider, ProgressBar, ComboBox, ListBox, TextEdit, Separator, Card, Tabs, MainMenu, DataGrid |
| `webview` | fertig / done | Natives Fenster mit System-WebView (wry 0.57 + tao 0.37), Formular als HTML/CSS, Logik in Rust; Stil Native oder Pico; Kern-Widgets plus Palette „WebView“: RadioButton, Switch, Slider, ProgressBar, ComboBox, ListBox, TextEdit, Separator, Frame, Tabs, MainMenu, DataGrid |
| `freya-gui` | fertig / done | **freya** ([freyaui.dev](https://freyaui.dev), Marc Espín, MIT; eigenständiges Projekt, von Freya TDE nur unterstützt) 0.5.4, reaktiv, Skia; angezeigt als „freya“; Kern-Widgets plus Palette „freya“: RadioButton, Switch, Slider, ProgressBar, Select, ListBox, TextEdit, Separator, Card, Tabs, Link, MainMenu, DataGrid, Accordion, Calendar, Chip, ColorPicker, Loader, Skeleton, SegmentedButton, SelectableText, Paragraph, Marquee, Svg, Gif, VirtualList, SideBar, Splitter, DragCanvas, Popup, ContextMenu, DragZone, DropZone, Lazy, DockArea, Markdown, CodeEditor, Terminal, Chart, Canvas, Camera, Video, HtmlView, Icon |

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

## Flutter

Flutter zeichnet die Oberfläche (Dart, Material 3), die Logik bleibt in Rust. Das Rust-Projekt wird eine
Bibliothek (`src/lib.rs`, `crate-type = ["cdylib"]`, kein `main.rs`), die die Flutter-App im Ordner
`flutter/` über flutter_rust_bridge lädt. Widgets sind wie bei egui Felder `crate::app::Control<T>`; ein
Handler ändert den Zustand (`self.greeting.value = …`), Flutter zeichnet danach neu.
*Flutter draws the UI, the logic stays in Rust: the Rust project is a library loaded by the Flutter app in
`flutter/`; widgets are `Control<T>` state fields like in egui.*

| Datei / file | Besitzer / owner | Inhalt / content |
|---|---|---|
| `src/lib.rs` | Nutzer | Module und `started` |
| `src/api.gen.rs`, `src/frb_generated.gen.rs` | Generator | die Brücke (für alle Projekte gleich) |
| `flutter/pubspec.yaml`, `flutter/lib/main.dart`, `flutter/README.md` | Nutzer | App, Start, Anleitung |
| `flutter/lib/freya/<form>.g.dart` | Generator | die Widgets des Formulars (`MainFormView`) |
| `flutter/lib/freya/freya.g.dart`, `flutter/lib/src/rust/*.dart` | Generator | Hilfen, Dart-Seite der Brücke |

Die Brücke ist fest (`bridge/api.rs`): Flutter fragt den Zustand eines Formulars ab (`freyaOpen`,
`freyaSnapshot`, `freyaPoll`) und meldet Eingaben (`freyaEvent(form, widget, event, input)`); die Rust-Seite
übernimmt die Eingabe in das Widget und ruft den Handler. Deshalb liegen die Bindungen von flutter_rust_bridge
fertig im Pack (`bridge/`, neu erzeugen mit `bridge/regenerate.sh`), und ein Projekt braucht nur Flutter,
nicht `flutter_rust_bridge_codegen`. Rust-Crate und Dart-Paket haben genau dieselbe Version (2.13.0).
*The bridge API is fixed, so the flutter_rust_bridge bindings ship pre-generated; projects only need Flutter.*

Starten / run: In der IDE „Start“ (F9): `cargo build`, einmal `flutter create --platforms=<system> .` (legt nur
fehlende Plattform-Ordner an), `flutter build <system> --debug`, dann das Programm in `flutter/` (es lädt die
Bibliothek aus `../target/debug/`); Ausgabe im Kanal „Programm“ (`freya_tde::flutter`). Von Hand: siehe
`flutter/README.md` im Projekt.

- Layouts: senkrecht (`Column`), waagerecht (`Row`, `stretch` und Eingabe-Widgets mit `Expanded`), Raster
  (Zeilen aus je `columns` Kindern), Stapel (erstes Kind), absolut (`Stack`/`Positioned`). Das Formular
  scrollt, wenn es nicht passt. `ListBox` ist 120, `DataGrid` 160 Pixel hoch (absolut: die gesetzte Höhe).
- Optionsfelder eines Containers bilden eine Gruppe (die Rust-Seite setzt die anderen zurück).
- `MainMenu` wird eine `MenuBar` (Pfade, `-`, Untermenüs); Kürzel werden angezeigt, aber nicht gebunden.
- `on_close` über `AppLifecycleListener` (wenn die App beendet werden soll).
- Gezeigt wird das erste Formular; weitere gibt es als `<Form>View` für `Navigator.push`.
- Fenster: Titel, Größe, Mindest-/Höchstgröße, Position (Mitte, System, `location`), `resizable` und
  Startzustand setzt `<Form>View.setupWindow()` beim Start über das Dart-Paket window_manager (0.5, MIT). Der
  native Runner zeigt das Fenster kurz in seiner eigenen Größe, bevor die Einstellungen greifen (vermeidbar
  durch „hidden at launch“ im Runner, siehe window_manager). Nicht unterstützt: Hintergrundbild von Containern.
- Fertiges Programm: `flutter/rust_builder/` (cargokit, MIT, Stand flutter_rust_bridge 2.13.0) baut die
  Rust-Bibliothek bei jedem `flutter build` selbst mit `cargo` und packt sie ins Programm (Linux `bundle/lib/`,
  Windows neben die `.exe`, macOS als Framework; dafür ist `crate-type` zusätzlich `staticlib`). Braucht
  `rustup`. Das Plugin heißt wie die Bibliothek (so findet flutter_rust_bridge sie unter macOS), die App
  deshalb `<bibliothek>_app`. Beim Entwickeln lädt die App weiter `../target/debug/`; im Release-Build
  (`kReleaseMode`) die mitgelieferte. Ältere Projekte: in `flutter/pubspec.yaml` den Namen auf
  `<bibliothek>_app` ändern und `<bibliothek>: {path: rust_builder}` sowie `window_manager: ^0.5.2` ergänzen.
  Später (Entscheidung 12.11.2026): Dart Build-Hooks statt cargokit, sobald flutter_rust_bridge sie stabil
  unterstützt.
- Kein Debugger: F9 startet ohne Debugger. Der erste `flutter build` baut die Bibliothek ein zweites Mal
  (cargokit hat ein eigenes Zielverzeichnis), danach geht es schnell.
- `flutter create` legt `test/widget_test.dart` für eine Beispiel-App an; vor `flutter test` anpassen.
- Getestet unter Linux; Windows und macOS sind vorgesehen, aber ungetestet.
- Test: `xvfb-run -a cargo test -p tdr-core --test backend_flutter` (braucht `flutter` und `xdotool`) erzeugt ein
  Projekt mit allen Widgets, baut Bibliothek und App, prüft `flutter analyze`, startet und klickt.

*Window settings via window_manager (`<Form>View.setupWindow()`); the Rust library is built and bundled by
cargokit (`flutter/rust_builder/`) on every `flutter build`. Limits: no container background pictures, menu
shortcuts not bound, no debugger; tested on Linux.*

## WebView

Ein natives Fenster (tao) mit der Web-Engine des Systems (wry): WebKitGTK unter Linux, WebView2 unter Windows,
WKWebView unter macOS. Das Formular wird eine HTML-Seite, die Layouts werden CSS; die Logik bleibt in Rust.
Widgets sind wie bei egui und Flutter Felder `crate::app::Control<T>`; ein Handler ändert den Zustand
(`self.greeting.value = …`), die Seite zeichnet danach neu.
*A native window (tao) with the system's web engine (wry); the form becomes an HTML page, layouts become CSS,
the logic stays in Rust (`Control<T>` state fields).*

Ablauf: Die Seite meldet Eingaben per IPC (`window.ipc.postMessage({widget, event, input})`), Rust übernimmt
sie in den Zustand, ruft den Handler und schickt den neuen Zustand mit `evaluate_script("freya.render(…)")`
zurück. Daten aus dem Hintergrund (Datenbindung, WebSocket, HTTP) wecken die Ereignisschleife
(`EventLoopProxy`). Aussehen über die Formular-Eigenschaft **`style`**: *Native* (eigenes schlichtes CSS mit
Systemfarben, folgt Hell/Dunkel) oder *Pico* ([Pico CSS](https://picocss.com) 2.1.1, MIT, klassenlos, als
`src/freya_pico.gen.rs` eingebettet). Das Programm braucht keinen Server und keine Dateien neben sich.
*Input comes in via IPC, the new state goes back via `evaluate_script`; `style` = Native (own CSS) or Pico.*

- Layouts als CSS: senkrecht/waagerecht als Flexbox, Raster als CSS-Grid (gleich breite Spalten), Stapel als
  übereinanderliegende Grid-Zellen, absolut mit `position: absolute`. Prozent und Anker sind direkt CSS-Werte und
  entsprechen dem Designer genau.
- Optionsfelder eines Containers bilden eine Gruppe; `MainMenu` wird eine Menüleiste aus aufklappenden Menüs
  (Pfade, `-`, Untermenüs als „Export › PDF“); Kürzel werden angezeigt, aber nicht gebunden.
- Gezeigt wird das erste Formular (weitere Fenster noch nicht).
- Fenster: Titel, Größe, Mindest-/Höchstgröße, `position = Manual`, Startzustand und Hell/Dunkel über tao;
  `position = Center` setzt tao nicht (das System platziert).
- Nicht unterstützt: Hintergrundbilder (die Seite lädt keine lokalen Dateien), Bild-Widget.
- Linux: braucht WebKitGTK 4.1 (`webkit2gtk-4.1`, Arch `webkit2gtk-4.1`, Debian/Ubuntu `libwebkit2gtk-4.1-dev`).
  Das Programm setzt `WEBKIT_DISABLE_DMABUF_RENDERER=1`, falls nicht gesetzt – sonst zeigen manche Treiber
  (z. B. NVIDIA) nur ein leeres Fenster. Windows braucht die WebView2-Laufzeit (in Windows 10/11 enthalten).
- Getestet unter Linux; Windows und macOS sind vorgesehen, aber ungetestet.
- Test: `xvfb-run -a cargo test -p tdr-core --test backend_webview` (braucht `xdotool`) erzeugt ein Projekt mit
  allen Widgets, baut, startet, klickt OK und das Menü und baut dasselbe Formular im Stil Pico.

*Limits: first form only, menu shortcuts not bound, no background pictures or image widget, `Center`
position not applied; tested on Linux.*

## freya

> **freya** ([freyaui.dev](https://freyaui.dev), [github.com/marc2332/freya](https://github.com/marc2332/freya),
> Marc Espín, MIT) ist ein **eigenständiges Projekt und hat nichts mit Freya TDE zu tun** – trotz des ähnlichen
> Namens. Freya TDE unterstützt freya nur als UI-Backend; der Neu-Assistent zeigt diesen Hinweis vor dem
> Anlegen, das erzeugte Projekt bekommt ihn als `FREYA.md`.
> *freya is an independent project and has nothing to do with Freya TDE, which only supports it.*

Intern heißt das Backend `freya-gui` (Ordner, Backend-ID in der `.ptdr`, `TypeRef::backend("freya-gui", …)`),
angezeigt wird es als „freya“ (`[pack] title`). Reaktiv wie React: der Zustand des Formulars liegt in
`Rc<RefCell<…>>`, Widgets sind wie bei egui und iced Felder `crate::app::Control<T>`. Jede Eingabe wird eine
Nachricht (`main_form::Message`), `update` (`crate::app::FreyaUpdate`) übernimmt sie und ruft den Handler, danach
zeichnet die Wurzel-Komponente `crate::app::FreyaRoot` neu (Zähler in `use_state`). Die Oberfläche baut das
Modul `freya_ui` je Formular mit freyas Elementen (`rect`, `label`) und Komponenten (`Button`, `Input`, `Switch`,
`Slider`, `Select`, `Table`, `ScrollView`, `Menu` …). freyas Prelude bleibt in diesem Modul – im Modul des Nutzers
gäbe es sonst Namenskonflikte (freyas `Table`).
*Reactive: state in `Rc<RefCell<…>>`, `Control<T>` fields, one `Message` per input; the view lives in the
module `freya_ui` with freya's elements and components.*

- Stil, Animation und Ereignisse an **jedem** Widget (Etappe C; Objektinspektor, Gruppen „Style“ und
  „Animation“): `corner_radius`, `border_width`/`border_color`, Schatten (`shadow_blur`, `shadow_x`/`_y`,
  `shadow_color`), Verlauf (`gradient` Linear/Radial/Conic mit `gradient_from`/`_to`/`_angle`), `blur`,
  `backdrop_blur` (Milchglas), `alpha`, `rotate`, `scale`, `cursor`; Animationen `hover_background` (Farbe
  beim Darüberfahren, eingeblendet), `appear` (Fade, Slide, Grow), `animate_moves` (freyas Portal: gleitet bei
  Layoutänderungen; nur mit Wunschgröße in Pixeln, freyas Portal braucht eine feste Größe), `ripple` (Material Design), Dauer `animation_ms`. Ereignisse `on_hover(inside)`,
  `on_file_drop(paths)`, `on_visible()`, `on_secondary_click()`.
- Fenster-Schalter am Formular: `decorations` (aus = rahmenlos, mit freyas Borderless-Plugin; Ecken über
  `corner_radius` des Formulars), `custom_titlebar` (freyas Titelleiste mit Minimieren, Maximieren,
  Schließen), `transparent`, `icon` (PNG, wird einkompiliert), `scale_factor`, `app_id`; Schalter für freyas
  `zoom_shortcuts`, `devtools`, `metrics`, `hot_reload` (schalten nur das Cargo-Feature ein). Komponente
  **Tray**: Symbol im Infobereich mit Menü, `on_select(item)` (nur am Hauptformular).
- Weitere Formulare öffnen sich als eigene Fenster: `crate::app::show_<formular>()`.
- Bilder aus dem Netz: eine ImageBox mit `file` = Adresse (`https://…`) schaltet freyas `remote-asset` ein.
- Layouts: senkrecht/waagerecht als `rect` mit `Content::flex()` (Dehnen = `Size::flex`), Raster als Zeilen aus
  je `columns` gleich breiten Zellen, Stapel (erstes Kind), absolut mit `Position::new_absolute()`.
- `MainMenu`: Menüleiste aus Knöpfen, darunter freyas `Menu` (Pfade, `-`, tiefere Ebenen als „Export › PDF“);
  Kürzel wie `Ctrl+O` sind gebunden (unter macOS die Befehlstaste).
- `TextEdit` ist freyas `Input` mit `multiline`; `Link` öffnet die Adresse mit `Platform::open_url`.
- Weitere Komponenten von freya (Etappe A): Accordion, Calendar (`self.<name>.value.selected` = Jahr, Monat,
  Tag), Chip, ColorPicker (`freya::prelude::Color`), Loader, Skeleton (Container, zeigt die Kinder, wenn
  `value` false ist), SegmentedButton, SelectableText, Paragraph, Marquee (freyas OverflowedContent), Svg, Gif,
  VirtualList (nur sichtbare Zeilen), SideBar, Splitter (freyas ResizableContainer, ein Panel je Kind, `sizes`
  in Prozent), DragCanvas (Kinder per Maus verschiebbar), Popup (öffnen mit `self.<name>.value = true`,
  `on_close` entscheidet), ContextMenu (Rechtsklick auf das Widget `target`, Einträge wie beim Hauptmenü),
  DragZone/DropZone (`data` als Text), Lazy, DockArea (Reiter per Ziehen zu Panels aufteilen). Der Button hat
  dazu `variant` (Normal, Filled, Outline, Flat) und `compact`, das Eingabefeld `variant`.
- Feature-Widgets (Etappe B; jedes schaltet sein Cargo-Feature von freya ein): Markdown, CodeEditor
  (tree-sitter: Plain, Rust, Python, JavaScript, TypeScript, JSON, TOML, HTML, CSS, Bash, C, YAML – je Sprache
  kommt die Grammatik-Crate dazu), Terminal (Pseudo-Terminal, Programm oder Shell), Chart (plotters: Line,
  Bar, Points, Area aus `values`), Canvas (Handler `on_draw(canvas, width, height)` malt mit Skia), Camera,
  Video (ffmpeg; fehlt es im `PATH`, lädt das Programm es beim Start herunter; `self.<name>.value.playing`),
  HtmlView (Blitz, ohne JavaScript; `url` oder `html`), Icon (Lucide-Symbol per Name). Camera, Video, HtmlView,
  Terminal und CodeEditor halten ihr freya-Handle in einer kleinen Komponente in `app.gen.rs`
  (`CameraView`, `VideoView` …), weil freya es über Hooks anlegt; die Handler steuern über den Zustand.
- Daten aus dem Hintergrund (Datenbindung, WebSocket, HTTP) wecken das Fenster über `futures-channel` und
  freyas `spawn`.
- Das erste Formular ist das Hauptfenster; `on_close` entscheidet über das Schließen (`CloseDecision`).
- Der erste Build lädt die Skia-Binärdateien von freya herunter (einmal Netz nötig). Unter Linux braucht es
  keine weiteren Systempakete.
- Nicht unterstützt: Hintergrundbilder von Containern (nur das Formular), `position = Center` (das System
  platziert), Mitte als Anker (ohne Anker links/rechts bleibt das Widget an seiner linken Kante).
- Getestet unter Linux; Windows und macOS sind vorgesehen, aber ungetestet.
- Test: `xvfb-run -a cargo test -p tdr-core --test backend_freya_gui` (braucht `xdotool`) erzeugt ein Projekt
  mit allen Widgets, baut, startet, klickt OK, drückt Strg+O (Menü) und prüft Datenbindung, DataGrid und Bilder.

*Limits: no container background pictures, `Center` position not applied, no centred anchors, tray only on
the main form; tested on Linux.*

## Aussehen und Fenster / appearance and window

Jedes Widget hat `background`, `foreground` (Farbe; Alpha 0 = Standard des Backends), `font_size`
(0 = Standard), `bold` und `italic`. Das Formular hat dazu `font_family`, `background_image` (Pfad relativ zum
Arbeitsordner des Programms) mit `background_fit` (Cover, Stretch, Contain, Center), `position` (Center,
System, Manual mit `location`), `min_size`/`max_size` (0 = ohne Grenze), `start_state` (Normal, Maximized,
Fullscreen) und `theme` (System, Light, Dark). Container haben ebenfalls ein Hintergrundbild. Schrift und
Vordergrund des Formulars erben die Widgets, im Designer wie im erzeugten Programm.
*Every widget has colours and font settings; the form adds font family, background picture and window
settings. Alpha 0 means "backend default".*

| | GTK 4 | egui | iced | Slint | Flutter | WebView | freya |
|---|---|---|---|---|---|---|---|
| Farben, Schriftgröße | CSS je Widget | ja | ja | Text: ja; Standard-Widgets nur Hintergrundfläche | ja (Knöpfe über `ButtonStyle`, sonst Text und Hintergrund) | ja (CSS je Element) | ja (Knöpfe über das Button-Theme, sonst Hülle mit Hintergrund und Textfarbe) |
| fett / kursiv | ja | nur kursiv | ja | ja (Text, Fenster-Standard) | ja (Texte) | ja | ja (Texte) |
| Schriftfamilie | ja | nein | ja | ja | ja | ja | ja |
| Hintergrundbild | ja (CSS) | ja | ja | ja (Formular) | nur Formular | nein | nur Formular |
| Position, Höchstgröße | nein (GTK 4 kann es nicht) | ja | ja | ja | nein (nativer Runner) | ja (`Manual`; `Center` nicht) | ja (`Manual`; `Center` nicht) |
| Mindestgröße, Zustand | ja | ja | ja | ja | nein (nativer Runner) | ja | ja |
| Hell/Dunkel | ja | ja | ja | ja | ja | ja | ja (freyas Themen; System = Vorgabe beim Start) |

Grenzen: iced wendet `enabled` nicht auf Container an; Slints Standard-Widgets haben keine Farb-Properties;
bei Flutter bestimmt der native Runner das Fenster.
*Limits: see table; iced ignores `enabled` on containers, Slint std widgets have no colour properties,
Flutter's native runner owns the window.*

## Wunschgröße im Layout / requested size in layouts

Zieht man im Designer am Anfasser eines Widgets in einem Layout, bekommt es eine Wunschgröße (wie Width/Height
bei Delphi): `Placement::Layout::size`, je Achse, 0 = natürliche Größe; im Template `placement.width` und
`placement.height`. Fill und Dehnen können das Widget weiter vergrößern.
*Resizing a widget in a layout sets a requested size per axis (0 = natural); fill and stretch may still enlarge it.*

| GTK 4 | egui | iced | Slint | Flutter | WebView | freya |
|---|---|---|---|---|---|---|
| `set_size_request` (Mindestgröße: kleiner als das Minimum des Widgets geht nicht) | `add_sized` (fehlende Achse: Breite verfügbar, Höhe Standard) | fester `container` (fehlende Achse: natürlich) | `width`/`height` | `SizedBox` (im Raster linksbündig) | CSS `width`/`height` (auf einer Fill-Achse `min-width`/`min-height`) | `rect` mit `width`/`height` in Pixeln |

## Größe in Prozent / size in percent

Statt Pixeln lässt sich je Achse ein Anteil am Inhalt des Elternelements angeben (Objektinspektor, Gruppe
„Platzierung“: `width_percent`, `height_percent`; `Placement::Layout::percent`, 0 = aus, 1–100). Prozent geht
der Wunschgröße in Pixeln vor; auf dieser Achse wirkt Fill wie Start und Dehnen entfällt. Zieht man am
Anfasser, bleibt eine Achse in Prozent in Prozent. Im Template `placement.width_percent` und
`placement.height_percent`. Der Designer rechnet genau mit dem Inhalt des Containers (ohne Innenabstand).
*Per axis a share of the parent's content area instead of pixels; it goes before the requested size, fill acts
like start and stretch does not apply on that axis.*

| | GTK 4 | egui | iced | Slint | Flutter | WebView | freya |
|---|---|---|---|---|---|---|---|
| quer zum Layout (Breite in senkrechten, Höhe in waagerechten Layouts) | Hülle `freya_percent::Percent` (natürliche Größe = Anteil am Elternelement, Minimum des Widgets bleibt) | `add_sized` mit Anteil an der verfügbaren Fläche zu Beginn des Containers | `FillPortion` neben einem Leerraum | Dehnung neben einem Leerraum (bevorzugte und Mindestgröße 0; eine feste Breite gäbe eine Bindungsschleife) | `FractionallySizedBox` | CSS `%` | `Size::percent` |
| in Layout-Richtung | wie quer | wie quer | Anteil am **freien** Platz (`FillPortion`, Rest als Leerraum) | Anteil am **freien** Platz (Dehnung, Rest als Leerraum) | Höhe: nicht (die Spalte scrollt, ihre Höhe ist unbegrenzt); Breite in Zeilen: Anteil am freien Platz (`Expanded`) | CSS `%` (Höhe, solange die des Elternelements feststeht – das Formular füllt das Fenster) | `Size::percent` (Anteil am Elternelement, nicht am freien Platz) |
| im Raster | wie quer | wie quer | nicht (Pixel bzw. natürlich) | nicht | nicht | CSS `%` | nicht |

GTK passt die Größe einen Durchlauf nach einer Größenänderung des Elternelements an (gemessen wird mit
dessen letzter Größe).

## Anker bei absoluter Lage / anchors with absolute positions

Absolut platzierte Widgets hängen wie bei Delphi (`Anchors`) an Kanten des Containers
(`Placement::Absolute::anchors`, Objektinspektor `anchor_left` … `anchor_bottom`, Standard links oben):
links und rechts = wächst in der Breite mit, nur rechts = wandert mit dem rechten Rand, weder links noch rechts =
bleibt mittig; oben/unten ebenso. Bezug ist die Entwurfsfläche (`Widget::design_area`: `size` des Formulars
bzw. Größe des Containers ohne Innenabstand, sonst die Ausdehnung der Kinder). Zieht man im Designer das
Formular oder einen Container größer, wandern verankerte Kinder mit (ein Schritt zum Rückgängigmachen).
Ein absoluter Container mit Ankern rechts bzw. unten nimmt den verfügbaren Platz ein (sonst so groß wie
seine Kinder). Im Template `placement.anchors.{left,top,right,bottom,any}`, `placement.margin_right`,
`placement.margin_bottom` und `placement.area`. Wechselt ein Container im Objektinspektor auf `Absolute`,
behalten seine Kinder ihre Lage; zurück in ein anderes Layout kommen sie wieder in den Fluss.
*Absolutely placed widgets keep their distance to the anchored edges of their container (like Delphi's
`Anchors`); resizing the form or container in the designer moves anchored children with it.*

| GTK 4 | egui | iced | Slint | Flutter | WebView | freya |
|---|---|---|---|---|---|---|
| `Overlay` mit `get-child-position` | `crate::app::anchored` und `ui.put` | Stapel aus Ebenen mit Ausrichtung und Innenabstand | Bindungen an `parent.width`/`parent.height` | `LayoutBuilder` und `Positioned` (unten nur bei begrenzter Höhe; im scrollenden Formular nicht) | CSS `position: absolute` mit `left`/`right`/`top`/`bottom` (mittig über `calc(50% …)`) | `Position::new_absolute()` mit `left`/`right`/`top`/`bottom`; links und rechts: Breite per `Size::func_data`; mittig nicht (bleibt links) |

Die Editor-Oberfläche selbst (RUF/RNCL) hat Anker ebenfalls: `Component::set_anchors` (siehe RUF-Doku).
*The editor UI (RUF/RNCL) has anchors as well.*

## Bilder: ImageBox und ImageScrollBox / pictures

Die Kern-Widgets `ImageBox` (ein Bild, `file`, `on_click`) und `ImageScrollBox` (Bilderliste `images` oder
Datenquelle mit dem Pfad in `display_field`, `orientation` Horizontal/Vertical/Grid, `thumb_size`, `spacing`,
`on_select(index, path)`) gibt es in allen sieben Backends. Beide haben `fit` (Contain, Cover, Stretch, Center),
`opacity` (0–1) und `gamma` (0,1–5; > 1 hellt auf). Gamma und Transparenz rechnet der Kern einmal in
`app.gen.rs` ein (`freya_image`, Crate `image`), deshalb sehen sie überall gleich aus. Formate: PNG, JPEG,
GIF (erstes Bild), WebP, BMP – **kein SVG** (dafür das Backend-Widget `Image` von GTK 4 oder Slint). Pfade
gelten relativ zum Arbeitsverzeichnis des Programms; eine Datei, die fehlt, bleibt leer und wird auf stderr
gemeldet. Ohne Wunschgröße ist die ImageBox 96 px hoch, die ImageScrollBox 240 × 128 px (wie im Designer).
Ein anderes Bild zur Laufzeit: `self.<name>.value.set_file(…)` (egui, iced, Flutter, WebView), bei GTK 4
`self.<name>.set_filename(…)` (ohne Gamma/Transparenz), bei Slint `set_<name>_source(…)`; die Galerie füllt
die Datenbindung neu (`<name>_reload()`).
*Core widgets in all seven backends; gamma and opacity are applied once by the core (`freya_image`), no SVG;
paths are relative to the working directory.*

| GTK 4 | egui | iced | Slint | Flutter | WebView | freya |
|---|---|---|---|---|---|---|
| `Picture` mit `MemoryTexture` (`ContentFit`, braucht GTK ≥ 4.12); Galerie `FlowBox` in `ScrolledWindow`, Auswahl über `child-activated` | Textur im Zustand (`crate::app::Picture`/`Gallery`), Galerie in einer `ScrollArea` | `image::Handle::from_rgba`, Galerie `scrollable` mit `row`/`column`/`row().wrap()` | `slint::Image::from_rgba8`, Galerie `ScrollView` mit `for`-Wiederholung | Rust legt ein aufbereitetes PNG im Temp-Ordner ab (`freya-pictures`), Dart zeigt es mit `Image.file`; Pfade relativ zum Projekt (das Programm läuft in `flutter/`) | eigenes Schema `freya://` (Windows: `http://freya.localhost/`), CSS `object-fit`; Galerie per Flexbox | freya lädt ein aufbereitetes PNG aus dem Speicher (`ImageViewer`, `AspectRatio`); Galerie `ScrollView`, Raster mit `Content::wrap()` |

Grenzen: `Center` zeigt das Bild in Originalgröße und schneidet ab (GTK: `ScaleDown`, verkleinert also große
Bilder). WebView behält aufbereitete Bilder bis zum Programmende im Speicher, Flutter im Temp-Ordner.
*Limits: `Center` keeps the natural size (GTK scales big pictures down).*

## Backend-Pack / backend pack

Ein UI-Backend ist ein Ordner `widgets/<backend>/` mit einer `pack.toml` (`kind = "backend"`):
eigene Widgets (`[[component]]` mit `kind = "widget"`/`"container"`; sie bekommen wie die Kern-Widgets
`enabled`, `visible` und `tooltip`), Code für die Core-Widgets (`[[extend]]` mit `[extend.codegen]`) und
der Descriptor `[backend]`:
*A UI backend is a directory `widgets/<backend>/` with a `pack.toml` (`kind = "backend"`):*

```toml
[backend]
event-loop = "main-thread"      # oder "async", "host" (Rust als Bibliothek, z. B. Flutter)
absolute-positioning = false
cargo = [{ crate = "eframe", version = "…" }]
setup = "…"                     # optional: Anweisungen vor App::init (z. B. gtk4::init()?;)
app = "…"                       # optional: Code für app.gen.rs (gemeinsame Hilfen, z. B. Control<T>)
form = """impl {{ form.struct }} { pub fn ui(&mut self, …) { … self.{{ w.events.on_click }}(); … } }"""
run = """eframe::run_native(…)?; Ok(())"""
# oder form-file / run-file / setup-file / app-file = "templates/…"
crate-type = ["cdylib"]         # optional: [lib] crate-type des Projekts (bei "host")

[[backend.files]]               # optional: weitere Dateien (Dart, pubspec.yaml …)
path = "flutter/lib/freya/{{ form.name }}.g.dart"   # Template
template-file = "templates/form.dart.j2"            # oder template
each = "form"                   # "project" (Standard) oder "form" (je Formular mit Widgets)
owner = "generator"             # oder "user" (einmal angelegt, dann Nutzerdatei)
raw = false                     # true = unverändert kopieren (z. B. erzeugte Bindungen)

[[backend.files]]               # einen ganzen Ordner des Packs unverändert kopieren
path = "flutter/rust_builder/cargokit"
dir = "cargokit"                # alle Dateien darunter, sortiert, landen in path/…
owner = "user"
```

Bei `event-loop = "host"` treibt ein anderes Programm die Oberfläche: kein `main.rs`, `run()` erzeugt die
Anwendung und gibt sie samt Laufzeit an das `run`-Template, das sie für die Aufrufe des Hosts aufbewahrt;
`src/lib.rs` bringt das Backend über `files` mit. Generator-Dateien bekommen Kopf und Prüfsumme als
`//`-Kommentar (Rust, Dart …), YAML und Markdown daher nur als Nutzerdateien.
*`host`: another program drives the UI; the Rust side is a library. `files` adds further files; generator
files carry the `//` header with checksum.*

Lizenzhinweis: `license-notice = """…"""` im Abschnitt `[pack]` (z. B. Lizenzbedingungen einer Bibliothek, von
der das erzeugte Projekt abhängt). Die IDE zeigt ihn vor dem Anlegen eines Projekts mit dem Backend und lässt
den Entwickler entscheiden; per API: `Ide::backend_notice`, `BackendDescriptor::license_notice`.
*`license-notice` is shown before a project uses the backend; the developer decides.*

Eigenschaften und Ereignisse für alle Widgets eines Backends: `[backend.widget-properties.<name>]` und
`[backend.widget-events.<name>]` (Aufbau wie bei `[component.properties.*]`/`[component.events.*]`). Jedes
Widget – Kern-Widgets, eigene und die anderer Packs – hat sie, solange sein Formular dieses Backend nutzt;
eigene Eigenschaften gleichen Namens gehen vor. Im Template wie alle anderen über `props`/`events`. Per API:
`TypeRegistry::property_defs`/`event_defs` (das freya-Backend gibt so jedem Widget Schatten, Verlauf,
Animationen, Hover- und Datei-Drop-Ereignisse).
*Backend-wide widget properties and events: every widget of a form using the backend has them.*

Bedingte Abhängigkeiten im `[backend] cargo`: Einträge mit `when`/`uses` gelten, sobald irgendein Element eines
Formulars mit diesem Backend die Bedingung erfüllt (z. B. `{ crate = "freya", features = ["devtools"], when = {
devtools = true } }` am Formular-Schalter, `uses = { file = "://" }` an einem Bild aus dem Netz); Einträge
ohne Bedingung gelten wie bisher immer.
*Conditional backend dependencies apply as soon as one element of the backend's forms meets the condition.*

Den Templates `app` und `run` gibt der Kern zusätzlich `types` mit: alle im Projekt verwendeten Typen als Text
(`backend:freya-gui/Calendar` …), damit Hilfen nur bei Bedarf entstehen.

Anzeigename: `title = "…"` im Abschnitt `[pack]`, wenn der Name für den Entwickler anders lauten soll als die ID
(`freya-gui` erscheint als „freya“); ohne `title` gilt `name`. Per API: `PackInfo::title`,
`BackendDescriptor::display_name`.
*`title` is the name shown to the user (wizard); default: `name`.*

In Templates schreibt der Filter `rust_str` einen Text als Rust-Stringliteral (`{{ item | rust_str }}`),
`dart_str` als Dart-Stringliteral (`'…'`, `$` maskiert).

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

Tabellen (DataGrid, alle Backends): `[component.table]` mit `set-rows` (Anweisungen mit `headers: Vec<String>`
und `rows: Vec<Vec<String>>`) und `selected`. Das Widget bekommt `data_source`, `fields` (gezeigte Spalten, leer =
alle), `headers` (Titel), `value_field` und `load`; der Kern erzeugt ein Zeilen-Struct `<Widget>Row` mit allen Spalten
der Quelle (typisiert nach `columns`), `<name>_selected_row()` und `<name>_selected_value()`. Ereignis `on_select(index)`.
GTK: `ColumnView`, egui: gestreiftes Grid, iced: `table`, Slint: `StandardTableView`, Flutter: `DataTable`.
*Data grids declare `set-rows`/`selected`; the core generates a typed row struct and `<name>_selected_row()`.*

Bearbeiten (12.11.2026, wie Delphis `TDataSet`): Mit `editable` lassen sich Zellen ändern; die Zeile wird
gespeichert, wenn eine andere Zeile gewählt oder Enter gedrückt wird, Esc verwirft sie. Der Kern erzeugt
`<name>_edit_cell(zeile, spalte, text)` (Text wird nach dem Spaltentyp gelesen; `Option<…>`: leer = `None`;
Listen-Spalten sind nicht bearbeitbar), `<name>_post()`, `<name>_cancel()`, `<name>_insert()`, `<name>_delete()`
und `<name>_row_changed(index)` (ruft das Backend beim Zeilenwechsel) und die Ereignisse `on_post(row, inserted) ->
bool` (`false` = weiter bearbeiten), `on_delete(row) -> bool` und `on_post_error(error)`. Kann die Datenquelle
schreiben (`[component.data]` mit `update`, `insert`, `delete`; SQLx-`Query`) und hat sie `table` und `key`
(Schlüsselspalten, durch Komma getrennt), schreibt der Kern die Zeilen zusätzlich im Hintergrund zurück
(`generated_key`: INSERT ohne Schlüsselspalten, danach wird neu geladen; Fehler → `on_post_error` und neu laden).
Ohne `table` bleibt es bei den Ereignissen, der Entwickler schreibt selbst. Backend-Pflicht: bearbeitbare Zellen
zeichnen, beim Ende einer Zellbearbeitung `<name>_edit_cell`, bei Enter `<name>_post`, bei Esc `<name>_cancel`, beim
Zeilenwechsel `<name>_row_changed`; `[component.table] select` (Anweisungen mit `index: Option<usize>`) hält die
Auswahl, wenn die Zeilen neu gezeigt werden. Die Templates sehen die Eigenschaften des Widgets als `props`.

| GTK 4 | egui | iced | Slint | Flutter | WebView | freya |
|---|---|---|---|---|---|---|
| `EditableLabel` je Zelle (Doppelklick/Enter bearbeitet), Aktionen `freya.edit`/`freya.cancel` am Grid | Doppelklick: Textfeld in der Zelle | Klick auf eine Zelle der gewählten Zeile: `text_input`; **Esc nicht gebunden** (`<name>_cancel()` per Knopf) | `StandardTableView` kann nicht bearbeiten: darunter ein Feld je Spalte für die gewählte Zeile (Esc über `FocusScope`) | gewählte Zeile als `TextField`s in der `DataTable` | Doppelklick: `<input>` in der Zelle | Klick auf eine Zelle der gewählten Zeile: `Input`; **Esc nicht gebunden** (`<name>_cancel()` per Knopf) |

Test gegen echte Datenbanken: `cargo test -p tdr-core --test write_back` (egui-Projekt ohne Fenster: ändern, neu,
löschen, dann die Tabelle lesen); SQLite immer, PostgreSQL/MySQL mit `FREYA_TDE_TEST_POSTGRES`/`FREYA_TDE_TEST_MYSQL`.

*Editing: with `editable`, cells can be changed; a row is posted when another row is chosen or Enter is pressed,
Esc discards it. The core generates `<name>_edit_cell/_post/_cancel/_insert/_delete/_row_changed` and the events
`on_post`, `on_delete`, `on_post_error`; with a writable source (`update`/`insert`/`delete`, SQLx `Query`) and
`table`/`key` set, rows are written back in the background.*

Listen-Widgets mit Daten: `[component.list]` mit `set-items` (Anweisungen, die `items: Vec<String>` ins Widget
setzen) und `selected` (Ausdruck `Option<usize>`); Kontext `name`, `root`, `form`. Solche Widgets bekommen
`data_source`, `display_field`, `value_field` und `load`; der Kern erzeugt `<name>_reload()`,
`<name>_selected_value()`, `freya_start_data()` und `freya_take_data()`. Das Backend ruft beim Öffnen
`freya_start_data()` und, wenn Daten da sind, `freya_take_data()` (GTK/Slint: Zeitgeber 100 ms, egui: Repaint über
`crate::app::freya_set_waker`, iced: Subscription, Flutter: Zeitgeber 100 ms über `freyaPoll`). Im Kontext des `form`-Templates zeigt `data_bound`, ob das
Formular gebundene Listen hat. Datenmodell: Modul-Doku von `tdr_core::data`.
*List widgets declare `set-items`/`selected`; the core generates loading, the backend polls or is woken.*

Kontext der Templates und Einzelheiten: Modul-Doku von `tdr_core::codegen` und `tdr_core::pack`.
*Template context and details: module docs of `tdr_core::codegen` and `tdr_core::pack`.*
