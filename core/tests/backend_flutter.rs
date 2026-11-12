//! The Flutter backend pack (`widgets/flutter`) end to end: generate a
//! project with every widget (Rust library plus Flutter app in `flutter/`),
//! build the library with Cargo and the app with Flutter, run it and click –
//! the button's handler in Rust changes the label and, on the second click,
//! ends the program.
//!
//! Without a display (`xvfb-run`), `xdotool` and `flutter` the test only
//! generates and checks the files (set `FREYA_TDE_REQUIRE_GUI=1` to make that
//! an error). The first run fetches flutter_rust_bridge.

use std::collections::BTreeMap;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use tdr_core::codegen::{Generation, generate, write};
use tdr_core::pack::load_all;
use tdr_core::*;

#[path = "support/data.rs"]
mod data;
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
    let flutter = |t: &str| TypeRef::backend("flutter", t);
    let mut form = Form::new(
        name("main_form"),
        "flutter",
        Widget::from_component(
            widget("window", builtin::form())
                .with("title", "Flutter-Test")
                .on("on_create", handler("created"))
                .on("on_close", handler("may_close")),
        ),
    );
    let add = |form: &mut Form, parent: &str, w: Widget| {
        form.add_widget(&name(parent), w).unwrap();
    };
    add(
        &mut form,
        "window",
        Widget::from_component(
            widget("ok_button", builtin::button())
                .with("caption", "OK")
                .on("on_click", handler("ok_clicked")),
        ),
    );
    add(
        &mut form,
        "window",
        Widget::from_component(widget("greeting", builtin::label()).with("text", "Hallo")),
    );
    add(
        &mut form,
        "window",
        Widget::from_component(
            widget("name_edit", builtin::edit())
                .with("text", "Freya")
                .with("placeholder", "Name ($)")
                .on("on_change", handler("name_changed"))
                .on("on_submit", handler("name_submitted")),
        ),
    );
    add(
        &mut form,
        "window",
        Widget::from_component(
            widget("agree", builtin::check_box())
                .with("caption", "Einverstanden")
                .on("on_toggle", handler("agree_toggled")),
        ),
    );
    add(
        &mut form,
        "window",
        Widget::new(name("options"), builtin::container()).with_layout(ContainerLayout {
            kind: LayoutKind::Grid { columns: 3 },
            spacing: 4,
            padding: Insets::all(0),
        }),
    );
    add(
        &mut form,
        "options",
        Widget::from_component(
            widget("red", flutter("RadioButton"))
                .with("caption", "Rot")
                .with("checked", true),
        ),
    );
    add(
        &mut form,
        "options",
        Widget::from_component(
            widget("blue", flutter("RadioButton"))
                .with("caption", "Blau")
                .on("on_toggle", handler("blue_toggled")),
        ),
    );
    add(
        &mut form,
        "options",
        Widget::from_component(
            widget("dark", flutter("Switch")).on("on_toggle", handler("dark_toggled")),
        ),
    );
    add(
        &mut form,
        "window",
        Widget::new(name("row"), builtin::container()).with_layout(ContainerLayout {
            kind: LayoutKind::Horizontal,
            spacing: 8,
            padding: Insets::all(0),
        }),
    );
    add(
        &mut form,
        "row",
        Widget::from_component(
            widget("level", flutter("Slider"))
                .with("value", 40.0)
                .with("divisions", 10i64)
                .on("on_change", handler("level_changed")),
        ),
    );
    add(
        &mut form,
        "row",
        Widget::from_component(widget("progress", flutter("ProgressBar")).with("fraction", 0.5)),
    );
    add(
        &mut form,
        "row",
        Widget::from_component(
            widget("fruit", flutter("ComboBox"))
                .with("items", texts(&["Apfel", "Birne"]))
                .with("selected", 0i64)
                .on("on_select", handler("fruit_selected")),
        ),
    );
    add(
        &mut form,
        "window",
        Widget::from_component(widget("details", flutter("Card")).with("title", "Notizen")),
    );
    add(
        &mut form,
        "details",
        Widget::from_component(
            widget("notes", flutter("TextEdit"))
                .with("rows", 2i64)
                .on("on_change", handler("notes_changed")),
        ),
    );
    add(
        &mut form,
        "window",
        Widget::from_component(
            widget("pages", flutter("Tabs"))
                .with("pages", texts(&["Allgemein"]))
                .on("on_page", handler("page_changed")),
        ),
    );
    for (page, text) in [("general", "Seite 1"), ("extra", "Seite 2")] {
        add(
            &mut form,
            "pages",
            Widget::new(name(page), builtin::container()),
        );
        add(
            &mut form,
            page,
            Widget::from_component(
                widget(&format!("{page}_label"), builtin::label()).with("text", text),
            ),
        );
    }
    add(
        &mut form,
        "window",
        Widget::from_component(
            widget("colors", flutter("ListBox"))
                .with("items", texts(&["Rot", "Grün", "Blau"]))
                .on("on_select", handler("color_selected")),
        ),
    );
    add(
        &mut form,
        "window",
        Widget::new(name("canvas"), builtin::container()).with_layout(ContainerLayout {
            kind: LayoutKind::Absolute,
            spacing: 0,
            padding: Insets::all(0),
        }),
    );
    add(
        &mut form,
        "canvas",
        Widget::from_component(widget("corner", builtin::label()).with("text", "absolut"))
            .with_placement(Placement::absolute(10, 2, 80, 20)),
    );
    add(
        &mut form,
        "window",
        Widget::from_component(widget("line", flutter("Separator"))),
    );
    // Aus der Datenquelle `services::fruits` gefüllt.
    add(
        &mut form,
        "window",
        Widget::from_component(data::bind(Component::new(
            name("bound"),
            flutter("ComboBox"),
        ))),
    );
    add(
        &mut form,
        "window",
        Widget::from_component(data::bind_grid(Component::new(
            name("grid"),
            flutter("DataGrid"),
        ))),
    );
    form.add_component(
        widget("menu", flutter("MainMenu"))
            .with(
                "items",
                texts(&["Datei/Öffnen | Ctrl+O", "Datei/-", "Datei/Beenden", "Hilfe"]),
            )
            .on("on_select", handler("menu_chosen")),
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
    }
    pub fn name_changed(&mut self, _text: &str) {}
    pub fn name_submitted(&mut self) {}
    pub fn agree_toggled(&mut self, _checked: bool) {}
    pub fn blue_toggled(&mut self, _checked: bool) {}
    pub fn dark_toggled(&mut self, _active: bool) {}
    pub fn level_changed(&mut self, _value: f64) {}
    pub fn fruit_selected(&mut self, _index: i32) {}
    pub fn notes_changed(&mut self, _text: &str) {}
    pub fn page_changed(&mut self, _page: u32) {}
    pub fn color_selected(&mut self, _index: i32) {}
    pub fn menu_chosen(&mut self, item: &str) {
        say(&format!("menu={item}"));
    }
}
"#;

fn file<'g>(generation: &'g Generation, path: &str) -> &'g str {
    &generation
        .file(path)
        .unwrap_or_else(|| panic!("{path} missing"))
        .content
}

fn have(program: &str, arg: &str) -> bool {
    Command::new(program)
        .arg(arg)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn gui_available() -> bool {
    // Nur unter Xvfb (xvfb-run) – nie Fenster und Klicks auf dem echten Bildschirm,
    // außer ausdrücklich gewünscht (FREYA_TDE_REAL_DISPLAY=1).
    let display = std::env::var_os("DISPLAY").is_some()
        && (std::env::var("XAUTHORITY").is_ok_and(|a| a.contains("xvfb-run"))
            || std::env::var_os("FREYA_TDE_REAL_DISPLAY").is_some());
    let ok = display && have("xdotool", "version") && have("flutter", "--version");
    if !ok && std::env::var_os("FREYA_TDE_REQUIRE_GUI").is_some() {
        panic!("display, xdotool or flutter missing");
    }
    ok
}

fn xdotool(args: &[&str]) -> String {
    let out = Command::new("xdotool").args(args).output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn run(dir: &Path, program: &str, args: &[&str]) {
    let status = Command::new(program)
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "{program} {args:?} failed");
}

#[test]
fn generated_project_has_rust_library_and_flutter_app() {
    let registry = registry();
    let mut project = ProjectFile::new("flutter_demo", "1.95", "flutter").unwrap();
    project.platforms = vec![Platform::Linux];
    let forms = [data::services("flutter"), form()];
    let generation = generate(
        &project,
        &forms,
        &registry,
        &BTreeMap::<String, String>::new(),
    );
    assert!(!generation.has_errors(), "{:#?}", generation.diagnostics);

    // Rust: Bibliothek statt Programm.
    assert!(generation.file("src/main.rs").is_none());
    let cargo = file(&generation, "Cargo.toml");
    assert!(
        cargo.contains("crate-type = [\"cdylib\", \"staticlib\"]"),
        "{cargo}"
    );
    assert!(
        cargo.contains("flutter_rust_bridge = \"=2.13.0\""),
        "{cargo}"
    );
    let lib = file(&generation, "src/lib.rs");
    assert!(lib.contains("fn started(app: &mut app::App)"), "{lib}");
    let app = file(&generation, "src/app.gen.rs");
    assert!(
        app.contains("crate::app::freya_keep(app, runtime);"),
        "{app}"
    );
    assert!(
        app.contains("\"main_form\" => app.main_form.freya_snapshot()"),
        "{app}"
    );
    assert!(!app.contains("\"services\" =>"), "{app}");
    for path in ["src/api.gen.rs", "src/frb_generated.gen.rs"] {
        assert!(
            file(&generation, path).starts_with(codegen::GENERATED_MARK),
            "{path}"
        );
    }

    // Flutter: App, Formular, Brücke.
    let pubspec = file(&generation, "flutter/pubspec.yaml");
    assert!(pubspec.contains("name: flutter_demo_app"), "{pubspec}");
    assert!(pubspec.contains("path: rust_builder"), "{pubspec}");
    assert!(pubspec.contains("window_manager:"), "{pubspec}");
    assert!(pubspec.contains("flutter_rust_bridge: 2.13.0"), "{pubspec}");
    let main = file(&generation, "flutter/lib/main.dart");
    assert!(main.contains("stem: 'flutter_demo'"), "{main}");
    assert!(main.contains("home: const MainFormView()"), "{main}");
    assert!(main.contains("await MainFormView.setupWindow();"), "{main}");
    let view = file(&generation, "flutter/lib/freya/main_form.g.dart");
    assert!(view.starts_with(codegen::GENERATED_MARK), "{view}");
    assert!(
        view.contains("class MainFormView extends StatefulWidget"),
        "{view}"
    );
    assert!(view.contains("hintText: 'Name (\\$)'"), "{view}");
    assert!(view.contains("title: 'Flutter-Test'"), "{view}");
    assert!(view.contains("center: true"), "{view}");
    assert!(view.contains("freyaMenuBar(_get('menu')"), "{view}");
    assert!(
        view.contains("Timer.periodic"),
        "data binding polls: {view}"
    );
    assert!(
        generation
            .file("flutter/lib/freya/services.g.dart")
            .is_none()
    );
    for path in [
        "flutter/lib/freya/freya.g.dart",
        "flutter/lib/src/rust/api.dart",
        "flutter/lib/src/rust/frb_generated.dart",
        "flutter/README.md",
        // cargokit: baut und packt die Bibliothek bei `flutter build`.
        "flutter/rust_builder/cargokit/cmake/cargokit.cmake",
        "flutter/rust_builder/cargokit/run_build_tool.sh",
        "flutter/rust_builder/cargokit/LICENSE",
    ] {
        file(&generation, path);
    }
    let builder = file(&generation, "flutter/rust_builder/pubspec.yaml");
    assert!(builder.contains("name: flutter_demo\n"), "{builder}");
    let cmake = file(&generation, "flutter/rust_builder/linux/CMakeLists.txt");
    assert!(
        cmake.contains("apply_cargokit(${PROJECT_NAME} ../../.. flutter_demo \"\")"),
        "{cmake}"
    );
    assert!(cmake.contains("flutter_demo_bundled_libraries"), "{cmake}");
    file(
        &generation,
        "flutter/rust_builder/macos/flutter_demo.podspec",
    );
}

#[test]
fn generated_project_builds_runs_and_reacts_to_clicks() {
    let registry = registry();
    let mut form = form();
    style::apply(&mut form, "greeting", "ok_button");
    let issues = validate(&form, &ValidationContext::for_backend(&registry, "flutter"));
    assert!(
        issues.iter().all(|i| i.severity != Severity::Error),
        "{issues:#?}"
    );
    if !gui_available() {
        eprintln!("Bildschirm, xdotool oder flutter fehlt – Bauen und Starten übersprungen");
        return;
    }

    let dir = std::env::temp_dir().join(format!("freya-flutter-demo-{}", std::process::id()));
    let target = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("flutter-demo-target");
    // SAFETY: gesetzt bevor Threads starten; der andere Test liest es nicht.
    unsafe { std::env::set_var("CARGO_TARGET_DIR", &target) };
    let _ = std::fs::remove_dir_all(&dir);
    let mut project = ProjectFile::new("flutter_demo", "1.95", "flutter").unwrap();
    project.platforms = vec![Platform::Linux];
    let forms = [data::services("flutter"), form];
    let generation = generate(&project, &forms, &registry, &dir);
    assert!(!generation.has_errors(), "{:#?}", generation.diagnostics);
    write(&dir, &generation).unwrap();
    style::picture(&dir);
    std::fs::write(dir.join("src/main_form.rs"), FORM_RS).unwrap();

    // Rust-Bibliothek.
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

    // Ohne Datenmodul und Laufzeit: die Bibliothek baut ebenso.
    let plain_dir = dir.with_file_name(format!("freya-flutter-plain-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&plain_dir);
    let plain = Form::new(
        name("main_form"),
        "flutter",
        Widget::from_component(widget("window", builtin::form())),
    );
    let plain_project = ProjectFile::new("flutter_plain", "1.95", "flutter").unwrap();
    let generation = generate(&plain_project, &[plain], &registry, &plain_dir);
    assert!(!generation.has_errors(), "{:#?}", generation.diagnostics);
    write(&plain_dir, &generation).unwrap();
    let report = preview::build(&plain_dir, Some(&generation)).unwrap();
    let errors: Vec<_> = report.errors().collect();
    assert!(report.success, "{errors:#?}");
    assert!(
        report
            .messages
            .iter()
            .all(|m| !m.file.as_deref().is_some_and(|f| f.ends_with(".gen.rs"))),
        "{:#?}",
        report.messages
    );

    // Ereignisse aus dem Hintergrund (WebSocket, HTTP): die Brücke bedient
    // das Datenmodul (die App bleibt beim Host), das Formular ihre Warteschlange.
    let web_dir = dir.with_file_name(format!("freya-flutter-web-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&web_dir);
    let mut services = Form::data_module(name("services"), "flutter");
    let pack = |p: &str, t: &str| TypeRef::pack(p, t);
    for component in [
        Component::new(name("runtime"), pack("tokio", "Runtime")),
        Component::new(name("api"), pack("axum", "Router")).on("on_start", handler("served")),
        Component::new(name("echo"), pack("ws", "Endpoint"))
            .with("router", Value::reference(name("api")))
            .on("on_message", handler("echoed")),
        Component::new(name("web"), pack("http", "Client")),
    ] {
        services.add_component(component).unwrap();
    }
    let mut web = Form::new(
        name("main_form"),
        "flutter",
        Widget::from_component(widget("window", builtin::form())),
    );
    web.add_widget(
        &name("window"),
        Widget::from_component(widget("status", builtin::label())),
    )
    .unwrap();
    for component in [
        Component::new(name("socket"), pack("ws", "Client"))
            .with("url", "wss://example.org/ws")
            .on("on_message", handler("received")),
        Component::new(name("ping"), pack("http", "Request"))
            .with("client", Value::reference_on(name("services"), name("web")))
            .with("url", "https://example.org/")
            .on("on_response", handler("answered")),
    ] {
        web.add_component(component).unwrap();
    }
    let web_project = ProjectFile::new("flutter_web", "1.95", "flutter").unwrap();
    let generation = generate(&web_project, &[services, web], &registry, &web_dir);
    assert!(!generation.has_errors(), "{:#?}", generation.diagnostics);
    let app = generation
        .files
        .iter()
        .find(|f| f.path == "src/app.gen.rs")
        .unwrap();
    assert!(app.content.contains("app.services.freya_start_data();"));
    assert!(!app.content.contains("app.services.freya_run();"));
    write(&web_dir, &generation).unwrap();
    let report = preview::build(&web_dir, Some(&generation)).unwrap();
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
    let _ = std::fs::remove_dir_all(&web_dir);
    let _ = std::fs::remove_dir_all(&plain_dir);

    // Flutter-App: Plattform-Ordner anlegen, prüfen, bauen.
    let app = dir.join("flutter");
    run(&app, "flutter", &["create", "--platforms=linux", "."]);
    // `flutter create` lässt vorhandene Dateien stehen.
    assert!(
        std::fs::read_to_string(app.join("lib/main.dart"))
            .unwrap()
            .contains("freyaStart()")
    );
    let analyze = Command::new("flutter")
        .args(["analyze", "--no-fatal-infos", "lib"])
        .current_dir(&app)
        .output()
        .unwrap();
    assert!(
        analyze.status.success(),
        "flutter analyze:\n{}",
        String::from_utf8_lossy(&analyze.stdout)
    );
    run(&app, "flutter", &["build", "linux", "--debug"]);
    let bundle = app.join("build/linux/x64/debug/bundle/flutter_demo");
    assert!(bundle.exists(), "{}", bundle.display());

    let mut child = Command::new(&bundle)
        .current_dir(&app)
        // Die Bibliothek aus dem Zielordner dieses Tests.
        .env(
            "FRB_DART_LOAD_EXTERNAL_LIBRARY_NATIVE_LIB_DIR",
            format!("{}/debug/", target.display()),
        )
        // Unter Xvfb testen, nicht auf dem Wayland-Bildschirm des Nutzers.
        .env_remove("WAYLAND_DISPLAY")
        .env("GDK_BACKEND", "x11")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let start = Instant::now();
    let window = loop {
        let found = xdotool(&["search", "--onlyvisible", "--name", "flutter_demo"]);
        if let Some(id) = found.lines().next() {
            break id.to_owned();
        }
        assert!(start.elapsed() < Duration::from_secs(30), "kein Fenster");
        std::thread::sleep(Duration::from_millis(200));
    };
    std::thread::sleep(Duration::from_millis(3000));
    if let Some(dir) = std::env::var_os("FREYA_TDE_SHOT") {
        let _ = Command::new("import")
            .args(["-window", &window])
            .arg(PathBuf::from(dir).join("flutter.png"))
            .status();
    }
    // OK-Knopf: unter der Menüleiste, über die ganze Breite.
    let click = |x: &str, y: &str| {
        xdotool(&["mousemove", "--window", &window, x, y, "click", "1"]);
        std::thread::sleep(Duration::from_millis(800));
    };
    click("300", "70");
    if let Some(dir) = std::env::var_os("FREYA_TDE_SHOT") {
        let _ = Command::new("import")
            .args(["-window", &window])
            .arg(PathBuf::from(dir).join("flutter-clicked.png"))
            .status();
    }
    // Noch einmal: der Handler beendet das Programm.
    click("300", "70");

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
    assert!(
        stdout.contains("grid=Some(\"Birne\") grid_value=Some(2)"),
        "{stdout}"
    );
    assert!(stdout.contains("second click"), "{stdout}");

    if std::env::var_os("FREYA_TDE_KEEP_DEMO").is_none() {
        let _ = std::fs::remove_dir_all(&dir);
    } else {
        println!("demo project: {}", dir.display());
    }
}
