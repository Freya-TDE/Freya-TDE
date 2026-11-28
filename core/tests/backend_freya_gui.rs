//! The freya backend pack (`widgets/freya-gui`; freya = freyaui.dev, an
//! independent project only supported by Freya TDE) end to end: generate a project
//! with every widget, build it with Cargo, run it and click with the mouse –
//! the button's handler changes the label and, on the second click, ends.
//!
//! Needs a display (`xvfb-run` on Linux) and `xdotool` for the clicks; the
//! first build fetches and compiles freya (and downloads Skia). Without them the test only
//! generates (set `FREYA_TDE_REQUIRE_GUI=1` to make that an error).

use std::io::Read as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use tdr_core::codegen::{generate, write};
use tdr_core::pack::load_all;
use tdr_core::*;

#[path = "support/data.rs"]
mod data;
#[path = "support/dirs.rs"]
mod dirs;
#[path = "support/edit.rs"]
mod edit;
#[path = "support/pictures.rs"]
mod pictures;
#[path = "support/style.rs"]
mod style;

fn name(n: &str) -> ComponentName {
    ComponentName::new(n).unwrap()
}

fn handler(n: &str) -> HandlerName {
    HandlerName::new(n).unwrap()
}

fn registry() -> TypeRegistry {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut registry = TypeRegistry::with_builtins();
    for dir in ["components", "widgets"] {
        let (packs, errors) = load_all(&root.join(dir));
        assert!(errors.is_empty(), "{errors:#?}");
        for pack in packs {
            pack.register(&mut registry).unwrap();
        }
    }
    data::register(&mut registry);
    registry
}

fn widget(n: &str, type_ref: TypeRef) -> Component {
    Component::new(name(n), type_ref)
}

fn texts(items: &[&str]) -> Value {
    Value::List(items.iter().map(|t| Value::Text((*t).into())).collect())
}

/// The OK button comes first (the test clicks it at the top of the window).
fn form() -> Form {
    let fg = |t: &str| TypeRef::backend("freya-gui", t);
    let mut form = Form::new(
        name("main_form"),
        "freya-gui",
        Widget::from_component(
            widget("window", builtin::form())
                .with("title", "freya-Test")
                .with(
                    "size",
                    Value::Size(Size {
                        width: 400,
                        height: 640,
                    }),
                )
                .on("on_create", handler("created"))
                .on("on_close", handler("may_close")),
        ),
    );
    let window = name("window");
    let add = |form: &mut Form, parent: &ComponentName, w: Widget| {
        form.add_widget(parent, w).unwrap();
    };
    add(
        &mut form,
        &window,
        Widget::from_component(
            widget("ok_button", builtin::button())
                .with("caption", "OK")
                .on("on_click", handler("ok_clicked")),
        ),
    );
    add(
        &mut form,
        &window,
        Widget::from_component(widget("greeting", builtin::label()).with("text", "Hallo")),
    );
    add(
        &mut form,
        &window,
        Widget::from_component(
            widget("name_edit", builtin::edit())
                .with("text", "Freya")
                .with("placeholder", "Name")
                .on("on_change", handler("name_changed"))
                .on("on_submit", handler("name_submitted")),
        ),
    );
    add(
        &mut form,
        &window,
        Widget::from_component(
            widget("agree", builtin::check_box())
                .with("caption", "Einverstanden")
                .on("on_toggle", handler("agree_toggled")),
        ),
    );
    add(
        &mut form,
        &window,
        Widget::new(name("options"), builtin::container()).with_layout(ContainerLayout {
            kind: LayoutKind::Grid { columns: 2 },
            spacing: 4,
            padding: Insets::all(0),
        }),
    );
    let options = name("options");
    add(
        &mut form,
        &options,
        Widget::from_component(
            widget("red", fg("RadioButton"))
                .with("caption", "Rot")
                .with("checked", true),
        ),
    );
    add(
        &mut form,
        &options,
        Widget::from_component(
            widget("blue", fg("RadioButton"))
                .with("caption", "Blau")
                .on("on_toggle", handler("blue_toggled")),
        ),
    );
    add(
        &mut form,
        &options,
        Widget::from_component(
            widget("dark", fg("Switch")).on("on_toggle", handler("dark_toggled")),
        ),
    );
    add(
        &mut form,
        &window,
        Widget::from_component(
            widget("level", fg("Slider"))
                .with("value", 40.0)
                .on("on_change", handler("level_changed")),
        ),
    );
    add(
        &mut form,
        &window,
        Widget::from_component(
            widget("progress", fg("ProgressBar"))
                .with("fraction", 0.5)
                .with("text", "50 %"),
        ),
    );
    add(
        &mut form,
        &window,
        Widget::from_component(
            widget("fruit", fg("Select"))
                .with("items", texts(&["Apfel", "Birne"]))
                .on("on_select", handler("fruit_selected")),
        ),
    );
    add(
        &mut form,
        &window,
        Widget::from_component(widget("empty", fg("Select"))),
    );
    add(
        &mut form,
        &window,
        Widget::from_component(widget("details", fg("Card")).with("title", "Notizen")),
    );
    add(
        &mut form,
        &name("details"),
        Widget::from_component(
            widget("notes", fg("TextEdit"))
                .with("rows", 2i64)
                .on("on_change", handler("notes_changed")),
        ),
    );
    add(
        &mut form,
        &window,
        Widget::from_component(
            widget("pages", fg("Tabs"))
                .with("pages", texts(&["Allgemein"]))
                .on("on_page", handler("page_changed")),
        ),
    );
    for (page, text) in [("general", "Seite 1"), ("extra", "Seite 2")] {
        add(
            &mut form,
            &name("pages"),
            Widget::new(name(page), builtin::container()),
        );
        add(
            &mut form,
            &name(page),
            Widget::from_component(
                widget(&format!("{page}_label"), builtin::label()).with("text", text),
            ),
        );
    }
    add(
        &mut form,
        &window,
        Widget::from_component(
            widget("colors", fg("ListBox"))
                .with("items", texts(&["Rot", "Grün", "Blau"]))
                .on("on_select", handler("color_selected")),
        ),
    );
    add(
        &mut form,
        &window,
        Widget::new(name("canvas"), builtin::container()).with_layout(ContainerLayout {
            kind: LayoutKind::Absolute,
            spacing: 0,
            padding: Insets::all(0),
        }),
    );
    add(
        &mut form,
        &name("canvas"),
        Widget::from_component(widget("corner", builtin::label()).with("text", "absolut"))
            .with_placement(Placement::absolute(10, 2, 80, 20)),
    );
    add(
        &mut form,
        &window,
        Widget::from_component(widget("line", fg("Separator"))),
    );
    add(
        &mut form,
        &window,
        Widget::from_component(
            widget("home", fg("Link"))
                .with("uri", "https://freyaui.dev")
                .with("caption", "freya"),
        ),
    );
    form.add_component(
        Component::new(name("menu"), fg("MainMenu"))
            .with(
                "items",
                texts(&["Datei/Öffnen | Ctrl+O", "Datei/-", "Datei/Beenden"]),
            )
            .on("on_select", handler("menu_chosen")),
    )
    .unwrap();
    // Etappe A: weitere Komponenten von freya.
    let extra = |form: &mut Form, parent: &str, w: Widget| {
        form.add_widget(&name(parent), w).unwrap();
    };
    extra(
        &mut form,
        "window",
        Widget::from_component(widget("more", fg("Accordion")).with("title", "Mehr")),
    );
    extra(
        &mut form,
        "more",
        Widget::from_component(widget("more_label", builtin::label()).with("text", "Inhalt")),
    );
    extra(
        &mut form,
        "window",
        Widget::from_component(
            widget("day", fg("Calendar"))
                .with("date", "2026-11-27")
                .on("on_select", handler("day_selected")),
        ),
    );
    extra(
        &mut form,
        "window",
        Widget::from_component(
            widget("tag", fg("Chip"))
                .with("caption", "Neu")
                .on("on_toggle", handler("tag_toggled")),
        ),
    );
    extra(
        &mut form,
        "window",
        Widget::from_component(
            widget("tint", fg("ColorPicker")).on("on_change", handler("tint_changed")),
        ),
    );
    extra(
        &mut form,
        "window",
        Widget::from_component(widget("busy", fg("Loader"))),
    );
    extra(
        &mut form,
        "window",
        Widget::from_component(widget("placeholder", fg("Skeleton")).with("shimmer", true)),
    );
    extra(
        &mut form,
        "placeholder",
        Widget::from_component(widget("loaded_label", builtin::label()).with("text", "geladen")),
    );
    extra(
        &mut form,
        "window",
        Widget::from_component(
            widget("view", fg("SegmentedButton"))
                .with("items", texts(&["Liste", "Raster"]))
                .on("on_select", handler("view_selected")),
        ),
    );
    extra(
        &mut form,
        "window",
        Widget::from_component(
            widget("copyable", fg("SelectableText")).with("text", "markier mich"),
        ),
    );
    extra(
        &mut form,
        "window",
        Widget::from_component(
            widget("story", fg("Paragraph"))
                .with("text", "Ein längerer Text, der umbricht.")
                .with("max_lines", 2i64),
        ),
    );
    extra(
        &mut form,
        "window",
        Widget::from_component(widget("ticker", fg("Marquee")).with("text", "Laufschrift")),
    );
    extra(
        &mut form,
        "window",
        Widget::from_component(
            widget("vector", fg("Svg"))
                .with("file", "icon.svg")
                .with("color", Value::Color(Color::rgb(0x3b, 0x82, 0xf6))),
        ),
    );
    extra(
        &mut form,
        "window",
        Widget::from_component(widget("anim", fg("Gif")).with("file", "anim.gif")),
    );
    extra(
        &mut form,
        "window",
        Widget::from_component(
            widget("many", fg("VirtualList"))
                .with("items", texts(&["eins", "zwei", "drei"]))
                .on("on_select", handler("many_selected")),
        ),
    );
    extra(
        &mut form,
        "window",
        Widget::from_component(
            widget("nav", fg("SideBar"))
                .with("items", texts(&["Start", "Einstellungen"]))
                .on("on_select", handler("nav_selected")),
        ),
    );
    extra(
        &mut form,
        "window",
        Widget::from_component(
            widget("split", fg("Splitter"))
                .with("sizes", Value::List(vec![Value::Int(30), Value::Int(70)])),
        ),
    );
    extra(
        &mut form,
        "split",
        Widget::from_component(widget("left_label", builtin::label()).with("text", "links")),
    );
    extra(
        &mut form,
        "split",
        Widget::from_component(widget("right_label", builtin::label()).with("text", "rechts")),
    );
    extra(
        &mut form,
        "window",
        Widget::from_component(widget("board", fg("DragCanvas"))),
    );
    extra(
        &mut form,
        "board",
        Widget::from_component(widget("card_label", builtin::label()).with("text", "zieh mich"))
            .with_placement(Placement::absolute(20, 10, 80, 20)),
    );
    extra(
        &mut form,
        "window",
        Widget::from_component(
            widget("dialog", fg("Popup"))
                .with("title", "Frage")
                .on("on_close", handler("dialog_closing")),
        ),
    );
    extra(
        &mut form,
        "dialog",
        Widget::from_component(widget("dialog_label", builtin::label()).with("text", "Sicher?")),
    );
    extra(
        &mut form,
        "window",
        Widget::from_component(widget("source", fg("DragZone")).with("data", "Apfel")),
    );
    extra(
        &mut form,
        "source",
        Widget::from_component(widget("source_label", builtin::label()).with("text", "ziehen")),
    );
    extra(
        &mut form,
        "window",
        Widget::from_component(widget("target", fg("DropZone")).on("on_drop", handler("dropped"))),
    );
    extra(
        &mut form,
        "target",
        Widget::from_component(widget("target_label", builtin::label()).with("text", "hierher")),
    );
    extra(
        &mut form,
        "window",
        Widget::from_component(widget("later", fg("Lazy"))),
    );
    extra(
        &mut form,
        "later",
        Widget::from_component(widget("later_label", builtin::label()).with("text", "später")),
    );
    extra(
        &mut form,
        "window",
        Widget::from_component(
            widget("dock", fg("DockArea")).with("pages", texts(&["Dateien", "Suche"])),
        ),
    );
    extra(
        &mut form,
        "dock",
        Widget::from_component(widget("files_label", builtin::label()).with("text", "Dateien")),
    );
    extra(
        &mut form,
        "dock",
        Widget::from_component(widget("search_label", builtin::label()).with("text", "Suche")),
    );
    // Etappe B: Feature-Widgets von freya.
    extra(
        &mut form,
        "window",
        Widget::from_component(
            widget("readme", fg("Markdown")).with("text", "# Titel\n\n**fett** und *kursiv*"),
        ),
    );
    extra(
        &mut form,
        "window",
        Widget::from_component(
            widget("source_code", fg("CodeEditor"))
                .with("text", "fn main() {\n    println!(\"hi\");\n}\n")
                .on("on_change", handler("code_changed")),
        ),
    );
    extra(
        &mut form,
        "window",
        Widget::from_component(
            widget("json_code", fg("CodeEditor"))
                .with("language", Value::Enum("Json".into()))
                .with("text", "{\"a\": 1}"),
        ),
    );
    extra(
        &mut form,
        "window",
        Widget::from_component(
            widget("shell", fg("Terminal"))
                .with("command", "sh")
                .on("on_exit", handler("shell_exited")),
        ),
    );
    extra(
        &mut form,
        "window",
        Widget::from_component(widget("sales", fg("Chart")).with("title", "Umsatz").with(
            "values",
            Value::List(vec![
                Value::Float(1.0),
                Value::Float(3.0),
                Value::Float(2.0),
            ]),
        )),
    );
    extra(
        &mut form,
        "window",
        Widget::from_component(
            widget("bars", fg("Chart"))
                .with("kind", Value::Enum("Bar".into()))
                .with(
                    "values",
                    Value::List(vec![Value::Float(2.0), Value::Float(5.0)]),
                ),
        ),
    );
    extra(
        &mut form,
        "window",
        Widget::from_component(widget("paint", fg("Canvas")).on("on_draw", handler("paint_drawn"))),
    );
    extra(
        &mut form,
        "window",
        Widget::from_component(widget("webcam", fg("Camera"))),
    );
    extra(
        &mut form,
        "window",
        Widget::from_component(
            widget("movie", fg("Video"))
                .with("file", "movie.mp4")
                .with("autoplay", false),
        ),
    );
    extra(
        &mut form,
        "window",
        Widget::from_component(
            widget("page", fg("HtmlView")).with("html", "<h1>Hallo</h1><p>HTML</p>"),
        ),
    );
    extra(
        &mut form,
        "window",
        Widget::from_component(widget("star", fg("Icon")).with("icon", "circle-check")),
    );
    // Etappe C: Stil, Animation und Ereignisse an jedem Widget.
    extra(
        &mut form,
        "window",
        Widget::from_component(
            widget("fancy", builtin::label())
                .with("text", "Stil")
                .with("corner_radius", 8i64)
                .with("border_width", 2i64)
                .with("border_color", Value::Color(Color::rgb(0x3b, 0x82, 0xf6)))
                .with("shadow_blur", 6i64)
                .with("gradient", Value::Enum("Linear".into()))
                .with("blur", 0i64)
                .with("backdrop_blur", 4i64)
                .with("alpha", Value::Float(0.9))
                .with("rotate", 3i64)
                .with("scale", Value::Float(1.1))
                .with("cursor", Value::Enum("Pointer".into()))
                .with(
                    "hover_background",
                    Value::Color(Color::rgb(0x30, 0x30, 0x30)),
                )
                .with("appear", Value::Enum("Slide".into()))
                .with("animate_moves", true)
                .with("ripple", true)
                .on("on_hover", handler("fancy_hover"))
                .on("on_file_drop", handler("files_dropped"))
                .on("on_visible", handler("fancy_visible"))
                .on("on_secondary_click", handler("fancy_right")),
        ),
    );
    // Wunschgröße: freyas Portal (`animate_moves`) braucht eine feste Größe.
    form.widget_mut(&name("fancy")).unwrap().visual.placement = Placement::Layout {
        stretch: 0,
        align: Align::Start,
        cell: None,
        size: Size {
            width: 120,
            height: 36,
        },
        percent: Size::default(),
    };
    form.add_component(
        Component::new(name("popup_menu"), fg("ContextMenu"))
            .with("target", "greeting")
            .with("items", texts(&["Kopieren", "Mehr/Teilen", "-", "Löschen"]))
            .on("on_select", handler("menu_chosen")),
    )
    .unwrap();
    // Aus der Datenquelle `services::fruits` gefüllt.
    form.add_widget(
        &name("window"),
        Widget::from_component(data::bind(Component::new(
            name("bound"),
            TypeRef::backend("freya-gui", "Select"),
        ))),
    )
    .unwrap();
    form.add_widget(
        &name("window"),
        Widget::from_component(edit::editable(data::bind_grid(Component::new(
            name("grid"),
            TypeRef::backend("freya-gui", "DataGrid"),
        )))),
    )
    .unwrap();
    form
}

/// The user's handlers: they print what happened; the second click ends the program.
const FORM_RS: &str = r#"
include!("main_form.gen.rs");

fn say(text: &str) {
    use std::io::Write as _;
    println!("{text}");
    let _ = std::io::stdout().flush();
}

impl MainForm {
    pub fn created(&mut self) {
        say("created");
    }
    pub fn may_close(&mut self) -> bool {
        true
    }
    pub fn ok_clicked(&mut self) {
        if self.greeting.value.starts_with("Hallo ") {
            say("second click");
            std::process::exit(0);
        }
        self.bound.value = 1;
        say(&format!("bound_items={}", self.bound.items.len()));
        say(&format!("bound={:?}", self.bound_selected_value()));
        self.grid.value.selected = Some(1);
        say(&format!("grid_rows={} grid_columns={}", self.grid_rows.len(), self.grid.value.headers.len()));
        say(&format!("grid={:?} grid_value={:?}", self.grid_selected_row().map(|r| r.name.clone()), self.grid_selected_value()));
        self.greeting.value = format!("Hallo {}", *self.name_edit);
        self.window.caption = "geklickt".into();
        say(&format!("label={}", self.greeting.value));
        say(&format!(
            "gallery={} photo={}",
            self.gallery.value.sources.iter().flatten().count(),
            self.photo.value.source.is_some()
        ));
        /*EDIT_CALLS*/
    }
    /*EDIT_HANDLERS*/
    pub fn name_changed(&mut self, _text: &str) {}
    pub fn name_submitted(&mut self) {}
    pub fn agree_toggled(&mut self, _checked: bool) {}
    pub fn blue_toggled(&mut self, _checked: bool) {}
    pub fn dark_toggled(&mut self, _active: bool) {}
    pub fn level_changed(&mut self, _value: f64) {}
    pub fn fruit_selected(&mut self, _index: u32) {}
    pub fn notes_changed(&mut self, _text: &str) {}
    pub fn page_changed(&mut self, _page: u32) {}
    pub fn color_selected(&mut self, _index: i32) {}
    pub fn day_selected(&mut self, _year: i32, _month: u32, _day: u32) {}
    pub fn tag_toggled(&mut self, _selected: bool) {}
    pub fn tint_changed(&mut self, _color: freya::prelude::Color) {}
    pub fn view_selected(&mut self, _index: u32) {}
    pub fn many_selected(&mut self, _index: i32) {}
    pub fn nav_selected(&mut self, _index: u32) {}
    pub fn dialog_closing(&mut self) -> bool {
        true
    }
    pub fn dropped(&mut self, _data: &str) {}
    pub fn fancy_hover(&mut self, _inside: bool) {}
    pub fn files_dropped(&mut self, _paths: &[std::path::PathBuf]) {}
    pub fn fancy_visible(&mut self) {}
    pub fn fancy_right(&mut self) {
        crate::app::show_about_form();
    }
    pub fn code_changed(&mut self, _text: &str) {}
    pub fn shell_exited(&mut self) {}
    pub fn paint_drawn(&mut self, canvas: &freya::engine::prelude::Canvas, width: f32, height: f32) {
        let mut paint = freya::engine::prelude::Paint::default();
        paint.set_color(freya::engine::prelude::Color::from_rgb(0x3b, 0x82, 0xf6));
        canvas.draw_circle((width / 2.0, height / 2.0), height.min(width) / 3.0, &paint);
    }
    pub fn menu_chosen(&mut self, item: &str) {
        say(&format!("menu={item}"));
    }
}
"#;

fn have(program: &str, arg: &str) -> bool {
    Command::new(program)
        .arg(arg)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

fn gui_available() -> bool {
    // Nur unter Xvfb (xvfb-run) – nie Fenster und Klicks auf dem echten Bildschirm,
    // außer ausdrücklich gewünscht (FREYA_TDE_REAL_DISPLAY=1).
    let display = std::env::var_os("DISPLAY").is_some()
        && (std::env::var("XAUTHORITY").is_ok_and(|a| a.contains("xvfb-run"))
            || std::env::var_os("FREYA_TDE_REAL_DISPLAY").is_some());
    let ok = display && have("xdotool", "version");
    if !ok && std::env::var_os("FREYA_TDE_REQUIRE_GUI").is_some() {
        panic!("display or xdotool missing");
    }
    ok
}

fn xdotool(args: &[&str]) -> String {
    let out = Command::new("xdotool").args(args).output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

#[test]
fn generated_project_builds_runs_and_reacts_to_clicks() {
    let registry = registry();
    let mut form = form();
    style::apply(&mut form, "greeting", "ok_button");
    pictures::add(&mut form);
    let issues = validate(
        &form,
        &ValidationContext::for_backend(&registry, "freya-gui"),
    );
    assert!(
        issues.iter().all(|i| i.severity != Severity::Error),
        "{issues:#?}"
    );

    let dir = dirs::project("freya-gui-demo");
    let target = dirs::target("freya-gui-demo-target");
    // SAFETY: einziger Test dieser Datei, gesetzt bevor Threads starten.
    unsafe { std::env::set_var("CARGO_TARGET_DIR", &target) };
    let _ = std::fs::remove_dir_all(&dir);
    let project = ProjectFile::new("freya_gui_demo", "1.95", "freya-gui").unwrap();
    // Ein weiteres Formular öffnet sich als eigenes Fenster (`show_about_form`).
    let mut about = Form::new(
        name("about_form"),
        "freya-gui",
        Widget::from_component(widget("window", builtin::form()).with("title", "Über")),
    );
    about
        .add_widget(
            &name("window"),
            Widget::from_component(widget("about_label", builtin::label()).with("text", "freya")),
        )
        .unwrap();
    let forms = [data::services("freya-gui"), form.clone(), about];
    let generation = generate(&project, &forms, &registry, &dir);
    assert!(!generation.has_errors(), "{:#?}", generation.diagnostics);
    write(&dir, &generation).unwrap();
    style::picture(&dir);
    pictures::files(&dir);
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ide/assets/palette/calendar.svg"),
        dir.join("icon.svg"),
    )
    .or_else(|_| {
        std::fs::copy(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ide/assets/palette/form.svg"),
            dir.join("icon.svg"),
        )
    })
    .unwrap();
    let form_rs = FORM_RS
        .replace(
            "/*EDIT_CALLS*/",
            &edit::calls("say", "self.grid.value.selected = Some(1);"),
        )
        .replace(
            "/*EDIT_HANDLERS*/",
            &(edit::handlers("say") + pictures::HANDLERS),
        );
    std::fs::write(dir.join("src/main_form.rs"), form_rs).unwrap();
    std::fs::write(
        dir.join("src/about_form.rs"),
        "include!(\"about_form.gen.rs\");\n",
    )
    .unwrap();

    if !gui_available() {
        eprintln!("Bildschirm oder xdotool fehlt – Bauen und Starten übersprungen");
        return;
    }
    let report = preview::build(&dir, Some(&generation)).unwrap();
    let errors: Vec<_> = report.errors().collect();
    assert!(report.success, "{errors:#?}");
    let warnings: Vec<_> = report
        .messages
        .iter()
        .filter(|m| m.file.as_deref().is_some_and(|f| f.ends_with(".gen.rs")))
        .collect();
    assert!(
        warnings.is_empty(),
        "generated code has warnings: {warnings:#?}"
    );

    let mut child = Command::new(report.executable.expect("binary"))
        .current_dir(&dir)
        // Unter Xvfb testen, nicht auf dem Wayland-Bildschirm des Nutzers.
        .env_remove("WAYLAND_DISPLAY")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    // Auf das Fenster warten.
    let start = Instant::now();
    let window = loop {
        let found = xdotool(&["search", "--name", "freya-Test"]);
        if let Some(id) = found.lines().next() {
            break id.to_owned();
        }
        assert!(start.elapsed() < Duration::from_secs(20), "kein Fenster");
        std::thread::sleep(Duration::from_millis(200));
    };
    // Das Formular ist höher als der Xvfb-Bildschirm (640×480); zentriert
    // läge sein oberer Rand und damit der OK-Knopf außerhalb.
    xdotool(&["windowmove", &window, "0", "0"]);
    // Skia zeichnet unter Xvfb in Software, das erste Bild braucht etwas.
    std::thread::sleep(Duration::from_millis(2500));
    if let Some(dir) = std::env::var_os("FREYA_TDE_SHOT") {
        let _ = Command::new("import")
            .args(["-window", &window])
            .arg(PathBuf::from(dir).join("freya.png"))
            .status();
    }
    // OK-Knopf: unter der Menüleiste, über die ganze Breite.
    xdotool(&["mousemove", "--window", &window, "200", "64", "click", "1"]);
    std::thread::sleep(Duration::from_millis(600));
    // Kürzel des Hauptmenüs (Strg+O = „Datei/Öffnen“).
    xdotool(&["windowfocus", "--sync", &window]);
    xdotool(&["key", "ctrl+o"]);
    std::thread::sleep(Duration::from_millis(600));
    // Noch einmal OK: der Handler beendet das Programm.
    xdotool(&["mousemove", "--window", &window, "200", "64", "click", "1"]);

    let start = Instant::now();
    while child.try_wait().unwrap().is_none() && start.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let mut stdout = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut stdout)
        .unwrap();
    assert!(stdout.contains("created"), "{stdout}");
    assert!(stdout.contains("label=Hallo Freya"), "{stdout}");
    assert!(stdout.contains("gallery=3 photo=true"), "{stdout}");
    assert!(stdout.contains("bound_items=3"), "{stdout}");
    assert!(stdout.contains("bound=Some(2)"), "{stdout}");
    assert!(stdout.contains("grid_rows=3 grid_columns=2"), "{stdout}");
    for line in edit::EXPECTED {
        assert!(stdout.contains(line), "{line}: {stdout}");
    }
    assert!(
        stdout.contains("grid=Some(\"Birne\") grid_value=Some(2)"),
        "{stdout}"
    );
    assert!(stdout.contains("menu=Datei/Öffnen"), "{stdout}");
    assert!(stdout.contains("second click"), "{stdout}");

    if std::env::var_os("FREYA_TDE_KEEP_DEMO").is_none() {
        let _ = std::fs::remove_dir_all(&dir);
    } else {
        println!("demo project: {}", dir.display());
    }
}
