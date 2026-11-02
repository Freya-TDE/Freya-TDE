//! The GTK 4 backend pack (`widgets/gtk4`) end to end: generate a project
//! from a form, build it with Cargo and run it – a click on the button calls
//! the handler in the user's code, which changes the label.
//!
//! Needs GTK 4 development files and a display (`xvfb-run` on Linux); the
//! first build fetches and compiles gtk4-rs. Without GTK the test is
//! skipped (set `FREYA_TDE_REQUIRE_GTK=1` to make that an error).

use std::path::PathBuf;

use tdr_core::codegen::{generate, write};
use tdr_core::pack::load_all;
use tdr_core::*;

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
    registry
}

fn widget(n: &str, type_ref: TypeRef) -> Component {
    Component::new(name(n), type_ref)
}

/// A form with every widget of the pack, in a vertical layout with a grid
/// and an absolute container inside.
fn form() -> Form {
    let gtk = |t: &str| TypeRef::backend("gtk4", t);
    let mut form = Form::new(
        name("main_form"),
        "gtk4",
        Widget::from_component(
            widget("window", builtin::form())
                .with("title", "GTK-Test")
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
        Widget::from_component(widget("greeting", builtin::label()).with("text", "Hallo")),
    );
    add(
        &mut form,
        &window,
        Widget::from_component(
            widget("name_edit", builtin::edit())
                .with("placeholder", "Name")
                .on("on_change", handler("name_changed")),
        ),
    );
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
            widget("red", gtk("RadioButton"))
                .with("caption", "Rot")
                .with("checked", true),
        ),
    );
    add(
        &mut form,
        &options,
        Widget::from_component(
            widget("blue", gtk("RadioButton"))
                .with("caption", "Blau")
                .on("on_toggle", handler("blue_toggled")),
        ),
    );
    add(
        &mut form,
        &options,
        Widget::from_component(
            widget("dark", gtk("Switch")).on("on_toggle", handler("dark_toggled")),
        ),
    );
    add(
        &mut form,
        &options,
        Widget::from_component(
            widget("count", gtk("SpinButton"))
                .with("value", 3.0)
                .on("on_change", handler("count_changed")),
        ),
    );
    add(
        &mut form,
        &window,
        Widget::from_component(
            widget("level", gtk("Scale"))
                .with("value", 40.0)
                .on("on_change", handler("level_changed")),
        ),
    );
    add(
        &mut form,
        &window,
        Widget::from_component(
            widget("progress", gtk("ProgressBar"))
                .with("fraction", 0.5)
                .with("text", "50 %"),
        ),
    );
    add(
        &mut form,
        &window,
        Widget::from_component(
            widget("fruit", gtk("DropDown"))
                .with(
                    "items",
                    Value::List(vec![
                        Value::Text("Apfel".into()),
                        Value::Text("Birne".into()),
                    ]),
                )
                .on("on_select", handler("fruit_selected")),
        ),
    );
    add(
        &mut form,
        &window,
        Widget::from_component(widget("box_frame", gtk("Frame")).with("title", "Notizen")),
    );
    add(
        &mut form,
        &name("box_frame"),
        Widget::from_component(
            widget("notes", gtk("TextView")).on("on_change", handler("notes_changed")),
        )
        .with_placement(Placement::Layout {
            stretch: 1,
            align: Align::Fill,
            cell: None,
        }),
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
        Widget::from_component(widget("logo", gtk("Image")).with("file", "logo.png"))
            .with_placement(Placement::absolute(10, 5, 32, 32)),
    );
    add(
        &mut form,
        &window,
        Widget::from_component(widget("line", gtk("Separator"))),
    );
    add(
        &mut form,
        &window,
        Widget::from_component(widget("home", gtk("LinkButton")).with("caption", "Freya TDE")),
    );
    form
}

/// The user's code: handlers and a `started` that clicks the button once the
/// window is up, prints the label and ends the program.
const MAIN_RS: &str = r#"
#[path = "app.gen.rs"]
mod app;

fn main() {
    if let Err(error) = app::run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn started(app: &mut app::App) -> Result<(), app::Error> {
    use gtk4::prelude::*;
    let button = app.main_form.ok_button.clone();
    let edit = app.main_form.name_edit.clone();
    let label = app.main_form.greeting.clone();
    let blue = app.main_form.blue.clone();
    let red = app.main_form.red.clone();
    let window = app.main_form.window.clone();
    gtk4::glib::timeout_add_local_once(std::time::Duration::from_millis(300), move || {
        edit.set_text("Freya");
        button.emit_clicked();
        blue.set_active(true);
        println!("label={}", label.text());
        println!("visible={}", window.is_visible());
        println!("radio_group={}", !red.is_active());
        std::process::exit(0);
    });
    Ok(())
}
"#;

const FORM_RS: &str = r#"
include!("main_form.gen.rs");

impl MainForm {
    pub fn created(&mut self) {
        println!("created");
    }
    pub fn may_close(&mut self) -> bool {
        true
    }
    pub fn name_changed(&mut self, text: &str) {
        println!("name={text}");
    }
    pub fn ok_clicked(&mut self) {
        let name = self.name_edit.text();
        self.greeting.set_text(&format!("Hallo {name}"));
    }
    pub fn agree_toggled(&mut self, _checked: bool) {}
    pub fn blue_toggled(&mut self, checked: bool) {
        println!("blue={checked}");
    }
    pub fn dark_toggled(&mut self, _active: bool) {}
    pub fn count_changed(&mut self, _value: f64) {}
    pub fn level_changed(&mut self, _value: f64) {}
    pub fn fruit_selected(&mut self, _index: u32) {}
    pub fn notes_changed(&mut self, _text: &str) {}
}
"#;

fn gtk_available() -> bool {
    let found = std::process::Command::new("pkg-config")
        .args(["--exists", "gtk4"])
        .status()
        .is_ok_and(|s| s.success());
    let display =
        std::env::var_os("DISPLAY").is_some() || std::env::var_os("WAYLAND_DISPLAY").is_some();
    if !(found && display) && std::env::var_os("FREYA_TDE_REQUIRE_GTK").is_some() {
        panic!("GTK 4 or a display is missing");
    }
    found && display
}

#[test]
fn generated_project_builds_and_runs() {
    let registry = registry();
    let form = form();
    let issues = validate(&form, &ValidationContext::for_backend(&registry, "gtk4"));
    assert!(
        issues.iter().all(|i| i.severity != Severity::Error),
        "{issues:#?}"
    );

    // Außerhalb des Workspaces; gebaut wird in ein festes Verzeichnis, damit
    // gtk4-rs nur beim ersten Lauf kompiliert wird.
    let dir = std::env::temp_dir().join(format!("freya-gtk4-demo-{}", std::process::id()));
    let target = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("gtk4-demo-target");
    // SAFETY: einziger Test dieser Datei, gesetzt bevor Threads starten.
    unsafe { std::env::set_var("CARGO_TARGET_DIR", &target) };
    let _ = std::fs::remove_dir_all(&dir);
    let project = ProjectFile::new("gtk4_demo", "1.85", "gtk4").unwrap();
    let generation = generate(&project, std::slice::from_ref(&form), &registry, &dir);
    assert!(!generation.has_errors(), "{:#?}", generation.diagnostics);
    write(&dir, &generation).unwrap();
    // Nutzercode wie nach dem Ausfüllen der Handler.
    std::fs::write(dir.join("src/main.rs"), MAIN_RS).unwrap();
    std::fs::write(dir.join("src/main_form.rs"), FORM_RS).unwrap();
    // Ein zweiter Lauf ändert die Nutzerdateien nicht (alle Handler da).
    let again = generate(&project, std::slice::from_ref(&form), &registry, &dir);
    for file in &again.files {
        if file.path == "src/main_form.rs" || file.path == "src/main.rs" {
            assert_eq!(file.change, codegen::Change::Unchanged, "{}", file.path);
        }
    }

    if !gtk_available() {
        eprintln!("GTK 4 oder Bildschirm fehlt – Bauen und Starten übersprungen");
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

    let output = std::process::Command::new(report.executable.expect("binary"))
        .current_dir(&dir)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("created"), "{stdout}");
    assert!(stdout.contains("name=Freya"), "{stdout}");
    assert!(stdout.contains("label=Hallo Freya"), "{stdout}");
    assert!(stdout.contains("blue=true"), "{stdout}");
    assert!(stdout.contains("radio_group=true"), "{stdout}");
    assert!(stdout.contains("visible=true"), "{stdout}");
    // FREYA_TDE_KEEP_DEMO=1 lässt das Projekt zum Ansehen liegen.
    if std::env::var_os("FREYA_TDE_KEEP_DEMO").is_none() {
        let _ = std::fs::remove_dir_all(&dir);
    } else {
        println!("demo project: {}", dir.display());
    }
}
