//! The Slint backend pack (`widgets/slint`) end to end: generate a project
//! with every widget, build it with Cargo, run it and click with the mouse –
//! the button's handler changes a property, the main menu ends the program.
//!
//! Needs a display (`xvfb-run` on Linux) and `xdotool` for the clicks; the
//! first build fetches and compiles Slint. Without them the test only
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

/// The menu bar comes first, then the OK button (the test clicks both).
fn form() -> Form {
    let slint = |t: &str| TypeRef::backend("slint", t);
    let mut form = Form::new(
        name("main_form"),
        "slint",
        Widget::from_component(
            widget("window", builtin::form())
                .with("title", "slint-Test")
                .with(
                    "size",
                    Value::Size(Size {
                        width: 400,
                        height: 700,
                    }),
                )
                .on("on_create", handler("created"))
                .on("on_close", handler("may_close")),
        ),
    );
    let window = name("window");
    let add = |form: &mut Form, parent: &ComponentName, w: Component| {
        form.add_widget(parent, Widget::from_component(w)).unwrap();
    };
    add(
        &mut form,
        &window,
        widget("ok_button", builtin::button())
            .with("caption", "OK")
            .on("on_click", handler("ok_clicked")),
    );
    add(
        &mut form,
        &window,
        widget("greeting", builtin::label()).with("text", "Hallo \"Welt\""),
    );
    add(
        &mut form,
        &window,
        widget("name_edit", builtin::edit())
            .with("text", "Freya")
            .with("placeholder", "Name")
            .on("on_change", handler("name_changed"))
            .on("on_submit", handler("name_submitted")),
    );
    add(
        &mut form,
        &window,
        widget("agree", builtin::check_box())
            .with("caption", "Einverstanden")
            .on("on_toggle", handler("agree_toggled")),
    );
    form.add_widget(
        &window,
        Widget::new(name("options"), builtin::container()).with_layout(ContainerLayout {
            kind: LayoutKind::Grid { columns: 2 },
            spacing: 4,
            padding: Insets::all(0),
        }),
    )
    .unwrap();
    let options = name("options");
    add(
        &mut form,
        &options,
        widget("red", slint("RadioButton"))
            .with("caption", "Rot")
            .with("checked", true),
    );
    add(
        &mut form,
        &options,
        widget("blue", slint("RadioButton"))
            .with("caption", "Blau")
            .on("on_toggle", handler("blue_toggled")),
    );
    add(
        &mut form,
        &options,
        widget("dark", slint("Switch"))
            .with("caption", "Dunkel")
            .on("on_toggle", handler("dark_toggled")),
    );
    add(
        &mut form,
        &options,
        widget("count", slint("SpinBox"))
            .with("value", 3i64)
            .on("on_change", handler("count_changed")),
    );
    add(
        &mut form,
        &window,
        widget("level", slint("Slider"))
            .with("value", 40.0)
            .on("on_change", handler("level_changed")),
    );
    add(
        &mut form,
        &window,
        widget("progress", slint("ProgressIndicator")).with("progress", 0.5),
    );
    add(
        &mut form,
        &window,
        widget("fruit", slint("ComboBox"))
            .with("items", texts(&["Apfel", "Birne"]))
            .on("on_select", handler("fruit_selected")),
    );
    add(&mut form, &window, widget("empty", slint("ComboBox")));
    form.add_widget(&window, Widget::new(name("details"), slint("GroupBox")))
        .unwrap();
    add(
        &mut form,
        &name("details"),
        widget("notes", slint("TextEdit")).on("on_change", handler("notes_changed")),
    );
    form.add_widget(
        &window,
        Widget::from_component(
            widget("pages", slint("TabWidget"))
                .with("pages", texts(&["Allgemein"]))
                .on("on_page", handler("page_changed")),
        ),
    )
    .unwrap();
    for (page, text) in [("general", "Seite 1"), ("extra", "Seite 2")] {
        form.add_widget(
            &name("pages"),
            Widget::new(name(page), builtin::container()),
        )
        .unwrap();
        add(
            &mut form,
            &name(page),
            widget(&format!("{page}_label"), builtin::label()).with("text", text),
        );
    }
    add(
        &mut form,
        &window,
        widget("colors", slint("ListView"))
            .with("items", texts(&["Rot", "Grün", "Blau"]))
            .on("on_select", handler("color_selected")),
    );
    form.add_widget(
        &window,
        Widget::new(name("canvas"), builtin::container()).with_layout(ContainerLayout {
            kind: LayoutKind::Absolute,
            spacing: 0,
            padding: Insets::all(0),
        }),
    )
    .unwrap();
    form.add_widget(
        &name("canvas"),
        Widget::from_component(
            widget("logo", slint("Image"))
                .with("file", "logo.png")
                .with("pixel_size", 24i64),
        )
        .with_placement(Placement::absolute(10, 2, 24, 24)),
    )
    .unwrap();
    add(&mut form, &window, widget("line", slint("Separator")));
    add(&mut form, &window, widget("about", slint("AboutSlint")));
    form.add_component(
        widget("main_menu", slint("MainMenu"))
            .with(
                "items",
                texts(&[
                    "Datei/Neu | Ctrl+N",
                    "Datei/-",
                    "Datei/Export/PDF",
                    "Datei/Beenden | Ctrl+Q",
                    "Hilfe/Über | F1",
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
            TypeRef::backend("slint", "ComboBox"),
        ))),
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
        use slint::Model as _;
        self.window.set_bound_selected(1);
        say(&format!("bound_items={}", self.window.get_bound_items().row_count()));
        say(&format!("bound={:?}", self.bound_selected_value()));
        let name = self.window.get_name_edit_text();
        self.window.set_greeting_text(format!("Hallo {name}").into());
        self.window.set_window_title("geklickt".into());
        say(&format!("label={}", self.window.get_greeting_text()));
    }
    pub fn name_changed(&mut self, _text: &str) {}
    pub fn name_submitted(&mut self) {}
    pub fn agree_toggled(&mut self, _checked: bool) {}
    pub fn blue_toggled(&mut self, _checked: bool) {}
    pub fn dark_toggled(&mut self, _checked: bool) {}
    pub fn count_changed(&mut self, _value: i32) {}
    pub fn level_changed(&mut self, _value: f32) {}
    pub fn fruit_selected(&mut self, _index: i32) {}
    pub fn notes_changed(&mut self, _text: &str) {}
    pub fn page_changed(&mut self, _page: i32) {}
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
    let display = std::env::var_os("DISPLAY").is_some();
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
    let form = form();
    let issues = validate(&form, &ValidationContext::for_backend(&registry, "slint"));
    assert!(
        issues.iter().all(|i| i.severity != Severity::Error),
        "{issues:#?}"
    );

    let dir = std::env::temp_dir().join(format!("freya-slint-demo-{}", std::process::id()));
    let target = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("slint-demo-target");
    // SAFETY: einziger Test dieser Datei, gesetzt bevor Threads starten.
    unsafe { std::env::set_var("CARGO_TARGET_DIR", &target) };
    let _ = std::fs::remove_dir_all(&dir);
    let project = ProjectFile::new("slint_demo", "1.92", "slint").unwrap();
    let forms = [data::services("slint"), form.clone()];
    let generation = generate(&project, &forms, &registry, &dir);
    assert!(!generation.has_errors(), "{:#?}", generation.diagnostics);
    write(&dir, &generation).unwrap();
    std::fs::write(dir.join("src/main_form.rs"), FORM_RS).unwrap();
    // Ein Bild für das Image-Widget.
    std::fs::write(
        dir.join("logo.png"),
        include_bytes!("../../ide/assets/palette/image.png"),
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
        let found = xdotool(&["search", "--name", "slint-Test"]);
        if let Some(id) = found.lines().next() {
            break id.to_owned();
        }
        assert!(start.elapsed() < Duration::from_secs(20), "kein Fenster");
        std::thread::sleep(Duration::from_millis(200));
    };
    std::thread::sleep(Duration::from_millis(1500));
    if let Some(dir) = std::env::var_os("FREYA_TDE_SHOT") {
        let _ = Command::new("import")
            .args(["-window", &window])
            .arg(PathBuf::from(dir).join("slint.png"))
            .status();
    }
    // OK-Knopf: unter der Menüleiste, über die ganze Breite.
    xdotool(&["mousemove", "--window", &window, "200", "68", "click", "1"]);
    std::thread::sleep(Duration::from_millis(600));
    // Menü „Datei“, dann „Neu“: der Handler beendet das Programm.
    xdotool(&["mousemove", "--window", &window, "27", "18", "click", "1"]);
    std::thread::sleep(Duration::from_millis(600));
    if let Some(dir) = std::env::var_os("FREYA_TDE_SHOT") {
        let _ = Command::new("import")
            .args(["-window", "root"])
            .arg(PathBuf::from(dir).join("slint-menu.png"))
            .status();
    }
    xdotool(&["mousemove", "--window", &window, "50", "52", "click", "1"]);

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
    assert!(stdout.contains("menu=Datei/Neu"), "{stdout}");

    if std::env::var_os("FREYA_TDE_KEEP_DEMO").is_none() {
        let _ = std::fs::remove_dir_all(&dir);
    } else {
        println!("demo project: {}", dir.display());
    }
}
