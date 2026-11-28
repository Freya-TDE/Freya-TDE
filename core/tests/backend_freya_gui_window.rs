//! The window switches of the freya backend (`widgets/freya-gui`): a borderless,
//! transparent window with freya's title bar, an icon, zoom shortcuts and a
//! tray icon. Generates the project and checks the Cargo features the switches
//! turn on; with `FREYA_TDE_REQUIRE_GUI=1` (or under `xvfb-run`) it also builds
//! it. Running is left out: a tray needs a desktop with a notification area.

use std::path::PathBuf;

use tdr_core::codegen::{generate, write};
use tdr_core::pack::load_all;
use tdr_core::*;

#[path = "support/dirs.rs"]
mod dirs;

fn name(n: &str) -> ComponentName {
    ComponentName::new(n).unwrap()
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

fn form() -> Form {
    let mut form = Form::new(
        name("main_form"),
        "freya-gui",
        Widget::from_component(
            Component::new(name("window"), builtin::form())
                .with("title", "freya-Fenster")
                .with("decorations", false)
                .with("custom_titlebar", true)
                .with("transparent", true)
                .with("corner_radius", 12i64)
                .with("icon", "icon.png")
                .with("scale_factor", Value::Float(1.25))
                .with("app_id", "dev.freya-tde.demo")
                .with("zoom_shortcuts", true),
        ),
    );
    form.add_widget(
        &name("window"),
        Widget::from_component(
            Component::new(name("hello"), builtin::label()).with("text", "Hallo"),
        ),
    )
    .unwrap();
    form.add_widget(
        &name("window"),
        Widget::from_component(
            Component::new(name("logo"), builtin::image_box())
                .with("file", "https://freyaui.dev/logo.png"),
        ),
    )
    .unwrap();
    form.add_component(
        Component::new(name("tray"), TypeRef::backend("freya-gui", "Tray"))
            .with("icon", "icon.png")
            .with("tooltip", "Demo")
            .with(
                "items",
                Value::List(vec![
                    Value::Text("Öffnen".into()),
                    Value::Text("-".into()),
                    Value::Text("Beenden".into()),
                ]),
            )
            .on("on_select", HandlerName::new("tray_chosen").unwrap()),
    )
    .unwrap();
    form
}

const FORM_RS: &str = r#"
include!("main_form.gen.rs");

impl MainForm {
    pub fn tray_chosen(&mut self, item: &str) {
        if item == "Beenden" {
            std::process::exit(0);
        }
    }
}
"#;

#[test]
fn window_switches_turn_on_freya_features_and_build() {
    let registry = registry();
    let form = form();
    let issues = validate(
        &form,
        &ValidationContext::for_backend(&registry, "freya-gui"),
    );
    assert!(
        issues.iter().all(|i| i.severity != Severity::Error),
        "{issues:#?}"
    );
    let dir = dirs::project("freya-gui-window");
    let target = dirs::target("freya-gui-window-target");
    // SAFETY: einziger Test dieser Datei, gesetzt bevor Threads starten.
    unsafe { std::env::set_var("CARGO_TARGET_DIR", &target) };
    let _ = std::fs::remove_dir_all(&dir);
    let project = ProjectFile::new("freya_window_demo", "1.95", "freya-gui").unwrap();
    let generation = generate(&project, std::slice::from_ref(&form), &registry, &dir);
    assert!(!generation.has_errors(), "{:#?}", generation.diagnostics);
    write(&dir, &generation).unwrap();
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ide/assets/freya-tde-icon.png"),
        dir.join("icon.png"),
    )
    .or_else(|_| {
        std::fs::copy(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ide/assets/palette/form.png"),
            dir.join("icon.png"),
        )
    })
    .unwrap();
    std::fs::write(dir.join("src/main_form.rs"), FORM_RS).unwrap();

    // Die Schalter schalten genau ihre Features von freya ein.
    let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
    for feature in [
        "borderless",
        "titlebar",
        "zoom-shortcuts",
        "tray",
        "remote-asset",
    ] {
        assert!(
            manifest.contains(&format!("\"{feature}\"")),
            "{feature}: {manifest}"
        );
    }
    for feature in ["devtools", "metrics", "hotreload", "material-design"] {
        assert!(
            !manifest.contains(&format!("\"{feature}\"")),
            "{feature}: {manifest}"
        );
    }

    let build = std::env::var_os("FREYA_TDE_REQUIRE_GUI").is_some()
        || std::env::var("XAUTHORITY").is_ok_and(|a| a.contains("xvfb-run"));
    if !build {
        eprintln!("FREYA_TDE_REQUIRE_GUI nicht gesetzt – Bauen übersprungen");
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
    if std::env::var_os("FREYA_TDE_KEEP_DEMO").is_none() {
        let _ = std::fs::remove_dir_all(&dir);
    }
}
