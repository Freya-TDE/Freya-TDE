//! The egui backend pack (`widgets/egui`) end to end: generate a project
//! with every widget, build it with Cargo, run it and click with the mouse –
//! the button's handler changes the label, the main menu calls its handler.
//!
//! Needs a display (`xvfb-run` on Linux) and `xdotool` for the clicks; the
//! first build fetches and compiles eframe. Without them the test only
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
#[path = "support/edit.rs"]
mod edit;
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
    let egui = |t: &str| TypeRef::backend("egui", t);
    let mut form = Form::new(
        name("main_form"),
        "egui",
        Widget::from_component(
            widget("window", builtin::form())
                .with("title", "egui-Test")
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
            widget("red", egui("RadioButton"))
                .with("caption", "Rot")
                .with("checked", true),
        ),
    );
    add(
        &mut form,
        &options,
        Widget::from_component(
            widget("blue", egui("RadioButton"))
                .with("caption", "Blau")
                .on("on_toggle", handler("blue_toggled")),
        ),
    );
    add(
        &mut form,
        &options,
        Widget::from_component(
            widget("dark", egui("Switch")).on("on_toggle", handler("dark_toggled")),
        ),
    );
    add(
        &mut form,
        &options,
        Widget::from_component(
            widget("count", egui("DragValue"))
                .with("value", 3.0)
                .on("on_change", handler("count_changed")),
        ),
    );
    add(
        &mut form,
        &window,
        Widget::from_component(
            widget("level", egui("Slider"))
                .with("value", 40.0)
                .on("on_change", handler("level_changed")),
        ),
    );
    add(
        &mut form,
        &window,
        Widget::from_component(
            widget("progress", egui("ProgressBar"))
                .with("fraction", 0.5)
                .with("text", "50 %"),
        ),
    );
    add(
        &mut form,
        &window,
        Widget::from_component(
            widget("fruit", egui("ComboBox"))
                .with("items", texts(&["Apfel", "Birne"]))
                .on("on_select", handler("fruit_selected")),
        ),
    );
    add(
        &mut form,
        &window,
        Widget::from_component(widget("empty", egui("ComboBox"))),
    );
    add(
        &mut form,
        &window,
        Widget::from_component(
            widget("details", egui("CollapsingHeader")).with("title", "Notizen"),
        ),
    );
    add(
        &mut form,
        &name("details"),
        Widget::from_component(
            widget("notes", egui("TextEdit"))
                .with("rows", 2i64)
                .on("on_change", handler("notes_changed")),
        ),
    );
    add(
        &mut form,
        &window,
        Widget::from_component(
            widget("pages", egui("Tabs"))
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
            widget("colors", egui("ListBox"))
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
        Widget::from_component(widget("line", egui("Separator"))),
    );
    add(
        &mut form,
        &window,
        Widget::from_component(widget("home", egui("Hyperlink")).with("caption", "Freya TDE")),
    );
    form.add_component(
        widget("main_menu", egui("MainMenu"))
            .with(
                "items",
                texts(&[
                    "Datei/Neu | Ctrl+N",
                    "Datei/-",
                    "Datei/Export/PDF",
                    "Datei/Beenden | Ctrl+Q",
                    "Hilfe/Über",
                ]),
            )
            .on("on_select", handler("menu_selected")),
    )
    .unwrap();
    // Aus der Datenquelle `services::fruits` gefüllt.
    form.add_widget(
        &name("window"),
        Widget::from_component(data::bind(Component::new(
            name("bound"),
            TypeRef::backend("egui", "ComboBox"),
        ))),
    )
    .unwrap();
    form.add_widget(
        &name("window"),
        Widget::from_component(edit::editable(data::bind_grid(Component::new(
            name("grid"),
            TypeRef::backend("egui", "DataGrid"),
        )))),
    )
    .unwrap();
    form
}

/// The user's handlers: they print what happened; the menu ends the program.
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
        self.bound.value = 1;
        say(&format!("bound_items={}", self.bound.items.len()));
        say(&format!("bound={:?}", self.bound_selected_value()));
        self.grid.value.selected = Some(1);
        say(&format!("grid_rows={} grid_columns={}", self.grid_rows.len(), self.grid.value.headers.len()));
        say(&format!("grid={:?} grid_value={:?}", self.grid_selected_row().map(|r| r.name.clone()), self.grid_selected_value()));
        self.greeting.value = format!("Hallo {}", *self.name_edit);
        self.ok_button.enabled = false;
        self.window.caption = "geklickt".into();
        say(&format!("label={}", self.greeting.value));
        /*EDIT_CALLS*/
    }
    /*EDIT_HANDLERS*/
    pub fn name_changed(&mut self, _text: &str) {}
    pub fn name_submitted(&mut self) {}
    pub fn agree_toggled(&mut self, _checked: bool) {}
    pub fn blue_toggled(&mut self, _checked: bool) {}
    pub fn dark_toggled(&mut self, _active: bool) {}
    pub fn count_changed(&mut self, _value: f64) {}
    pub fn level_changed(&mut self, _value: f64) {}
    pub fn fruit_selected(&mut self, _index: u32) {}
    pub fn notes_changed(&mut self, _text: &str) {}
    pub fn page_changed(&mut self, _page: u32) {}
    pub fn color_selected(&mut self, _index: i32) {}
    pub fn menu_selected(&mut self, item: &str) {
        say(&format!("menu={item}"));
        std::process::exit(0);
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
    let issues = validate(&form, &ValidationContext::for_backend(&registry, "egui"));
    assert!(
        issues.iter().all(|i| i.severity != Severity::Error),
        "{issues:#?}"
    );

    let dir = std::env::temp_dir().join(format!("freya-egui-demo-{}", std::process::id()));
    let target = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("egui-demo-target");
    // SAFETY: einziger Test dieser Datei, gesetzt bevor Threads starten.
    unsafe { std::env::set_var("CARGO_TARGET_DIR", &target) };
    let _ = std::fs::remove_dir_all(&dir);
    let project = ProjectFile::new("egui_demo", "1.95", "egui").unwrap();
    let forms = [data::services("egui"), form.clone()];
    let generation = generate(&project, &forms, &registry, &dir);
    assert!(!generation.has_errors(), "{:#?}", generation.diagnostics);
    write(&dir, &generation).unwrap();
    style::picture(&dir);
    let form_rs = FORM_RS
        .replace(
            "/*EDIT_CALLS*/",
            &edit::calls("say", "self.grid.value.selected = Some(1);"),
        )
        .replace("/*EDIT_HANDLERS*/", &edit::handlers("say"));
    std::fs::write(dir.join("src/main_form.rs"), form_rs).unwrap();

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
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Auf das Fenster warten; ohne Fenster die Fehlerausgabe des Programms zeigen.
    let start = Instant::now();
    let window = loop {
        let found = xdotool(&["search", "--name", "egui-Test"]);
        if let Some(id) = found.lines().next() {
            break id.to_owned();
        }
        if start.elapsed() > Duration::from_secs(40) || child.try_wait().unwrap().is_some() {
            let _ = child.kill();
            let output = child.wait_with_output().unwrap();
            panic!(
                "kein Fenster ({}):\n{}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        std::thread::sleep(Duration::from_millis(200));
    };
    // Das Formular ist höher als der Xvfb-Bildschirm (640×480); zentriert
    // läge sein oberer Rand und damit der erste Knopf außerhalb.
    xdotool(&["windowmove", &window, "0", "0"]);
    std::thread::sleep(Duration::from_millis(1500));
    if let Some(dir) = std::env::var_os("FREYA_TDE_SHOT") {
        let _ = Command::new("import")
            .args(["-window", &window])
            .arg(PathBuf::from(dir).join("egui.png"))
            .status();
    }
    // OK-Knopf: oben unter der Menüleiste, über die ganze Breite.
    xdotool(&["mousemove", "--window", &window, "200", "52", "click", "1"]);
    std::thread::sleep(Duration::from_millis(600));
    // Menü „Datei“, dann „Neu“.
    xdotool(&["mousemove", "--window", &window, "20", "11", "click", "1"]);
    std::thread::sleep(Duration::from_millis(600));
    xdotool(&["mousemove", "--window", &window, "40", "38", "click", "1"]);

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
    assert!(stdout.contains("menu=Datei/Neu"), "{stdout}");

    if std::env::var_os("FREYA_TDE_KEEP_DEMO").is_none() {
        let _ = std::fs::remove_dir_all(&dir);
    } else {
        println!("demo project: {}", dir.display());
    }
}
