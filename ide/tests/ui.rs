//! The IDE windows with real (simulated) mouse and keyboard input.
//!
//! GTK allows one application per process in the main thread, hence
//! `harness = false`. Linux without display: `xvfb-run cargo test`.

use rncl::prelude::*;
use ruf::{Key, Modifiers};
use tdr_core::{ComponentName, builtin};

fn name(n: &str) -> ComponentName {
    ComponentName::new(n).unwrap()
}

fn mouse(surface: &Widget, action: MouseAction, at: tdr_designer::Point) {
    surface.simulate_mouse(MouseEvent {
        action,
        x: at.x,
        y: at.y,
        button: 1,
        modifiers: Modifiers::default(),
    });
}

/// Runs the event loop until `done` holds or `seconds` have passed.
fn pump_until(seconds: u64, mut done: impl FnMut() -> bool) -> bool {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
    while std::time::Instant::now() < end {
        Application::process_events();
        if done() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    false
}

fn main() {
    // Eigene Konfiguration, damit die Fensteranordnung des Nutzers unberührt bleibt.
    let home = std::env::temp_dir().join(format!("freya-ui-{}", std::process::id()));
    // SAFETY: am Programmanfang, vor weiteren Threads.
    unsafe {
        std::env::set_var("HOME", &home);
        std::env::set_var("XDG_CONFIG_HOME", home.join("config"));
        // Ohne Bildschirm (Xvfb) ist Software-Vulkan unzuverlässig.
        if std::env::var_os("GSK_RENDERER").is_none() {
            std::env::set_var("GSK_RENDERER", "cairo");
        }
    }
    let _app = Application::new();
    let ide = freya_td::Ide::start(None);
    let surface = ide.design_surface();

    // Palette bewaffnen und in das Formular klicken: Button wird platziert.
    ide.arm(Some(builtin::button()));
    let client = {
        let session = ide.session();
        let layout = session.layout().unwrap();
        layout.item(&name("window")).unwrap().content.unwrap()
    };
    let spot = tdr_designer::Point::new(client.center().x, client.bottom() - 5);
    mouse(&surface, MouseAction::Pressed, spot);
    mouse(&surface, MouseAction::Released, spot);
    assert_eq!(ide.session().selection(), [name("button1")]);
    assert_eq!(
        ide.inspected().map(|p| p.to_string()).as_deref(),
        Some("main_form::button1"),
        "Objektinspektor folgt der Auswahl"
    );
    println!("test place_from_palette ... ok");

    // Klick ins Leere hebt die Auswahl auf, Klick auf den Button wählt ihn.
    mouse(
        &surface,
        MouseAction::Pressed,
        tdr_designer::Point::new(2, 2),
    );
    assert!(ide.session().selection().is_empty());
    let button = ide
        .session()
        .layout()
        .unwrap()
        .rect(&name("button1"))
        .unwrap();
    mouse(&surface, MouseAction::Pressed, button.center());
    mouse(&surface, MouseAction::Released, button.center());
    assert_eq!(ide.session().selection(), [name("button1")]);
    println!("test click_selects ... ok");

    // Entf löscht, Strg+Z holt zurück.
    surface.simulate_key(KeyEvent {
        key: Key::Delete,
        modifiers: Modifiers::default(),
    });
    assert!(!ide.session().form().unwrap().contains(&name("button1")));
    surface.simulate_key(KeyEvent {
        key: Key::Char('z'),
        modifiers: Modifiers {
            ctrl: true,
            ..Modifiers::default()
        },
    });
    assert!(ide.session().form().unwrap().contains(&name("button1")));
    println!("test delete_and_undo ... ok");

    // Befehle als JSON (Skripte, KI).
    let count = ide
        .run_commands(
            r#"[{"command": "add_widget", "form": "main_form", "parent": "window", "type": "core/Label", "name": "hint"}]"#,
        )
        .unwrap();
    assert_eq!(count, 1);
    assert!(ide.session().form().unwrap().contains(&name("hint")));
    println!("test run_commands ... ok");

    // Doppelklick im Designer: Handler anlegen, Code erzeugen, hinspringen.
    let project_dir = home.join("shop");
    let session = freya_td::Session::create(
        &project_dir,
        "shop",
        "none",
        tdr_core::TypeRegistry::with_builtins(),
    )
    .unwrap();
    drop(session);
    let ide = freya_td::Ide::start(Some(&project_dir.join("shop.ptdr")));
    let surface = ide.design_surface();
    ide.arm(Some(builtin::button()));
    let client = {
        let session = ide.session();
        let layout = session.layout().unwrap();
        layout.item(&name("window")).unwrap().content.unwrap()
    };
    let spot = tdr_designer::Point::new(client.center().x, client.bottom() - 5);
    mouse(&surface, MouseAction::Pressed, spot);
    mouse(&surface, MouseAction::Released, spot);
    let button = ide
        .session()
        .layout()
        .unwrap()
        .rect(&name("button1"))
        .unwrap();
    mouse(&surface, MouseAction::DoubleClicked, button.center());
    assert_eq!(
        ide.code_file(),
        Some(project_dir.join("src/main_form.rs").canonicalize().unwrap()),
        "Code-Editor zeigt die Formulardatei"
    );
    let source = std::fs::read_to_string(project_dir.join("src/main_form.rs")).unwrap();
    assert!(source.contains("pub fn button1_click(&mut self)"));
    println!("test double_click_opens_handler ... ok");

    // Projektfenster: Dateibaum und Dateioperationen (wie über das Kontextmenü).
    use freya_td::files::{Entry, FileOp};
    fn paths(entries: &[Entry]) -> Vec<String> {
        entries
            .iter()
            .flat_map(|e| std::iter::once(e.path.clone()).chain(paths(&e.children)))
            .collect()
    }
    let tree = paths(&ide.project_files());
    for path in [
        "src",
        "src/main_form.rs",
        "src/main_form.ftdr",
        "Cargo.toml",
        "shop.ptdr",
    ] {
        assert!(tree.contains(&path.to_owned()), "{path} fehlt in {tree:?}");
    }
    assert!(
        !tree
            .iter()
            .any(|p| p.ends_with(".gen.rs") || p.starts_with("target"))
    );
    ide.file_op(&FileOp::NewFile {
        path: "src/util.rs".into(),
        text: "pub fn util() {}\n".into(),
    })
    .unwrap();
    assert!(paths(&ide.project_files()).contains(&"src/util.rs".to_owned()));
    ide.open_project_file("src/util.rs");
    let util = project_dir.join("src/util.rs").canonicalize().unwrap();
    assert_eq!(ide.code_file().as_ref(), Some(&util));
    // Umbenennen: der offene Reiter zieht mit.
    ide.file_op(&FileOp::Rename {
        from: "src/util.rs".into(),
        to: "src/helpers.rs".into(),
    })
    .unwrap();
    let helpers = project_dir.join("src/helpers.rs").canonicalize().unwrap();
    assert_eq!(ide.code_file().as_ref(), Some(&helpers));
    assert_eq!(ide.code_text(&helpers).unwrap(), "pub fn util() {}\n");
    // Projektdatei und Formulare sind geschützt.
    let refused = ide.file_op(&FileOp::Delete {
        path: "shop.ptdr".into(),
        permanent: true,
    });
    assert!(refused.is_err());
    assert!(project_dir.join("shop.ptdr").is_file());
    ide.file_op(&FileOp::Delete {
        path: "src/helpers.rs".into(),
        permanent: true,
    })
    .unwrap();
    assert_ne!(ide.code_file().as_ref(), Some(&helpers));
    assert!(!project_dir.join("src/helpers.rs").exists());
    // Eine .ftdr öffnet das Formular samt seiner .rs.
    ide.open_project_file("src/main_form.ftdr");
    assert_eq!(
        ide.code_file(),
        Some(project_dir.join("src/main_form.rs").canonicalize().unwrap())
    );
    println!("test project_files ... ok");

    // rust-analyzer: ein Fehler im Editor erscheint als Marke am Rand.
    let mut passed = 6;
    if std::process::Command::new(freya_td::lsp::program())
        .arg("--version")
        .output()
        .is_ok()
    {
        let file = ide.code_file().unwrap();
        assert!(pump_until(120, || ide.lsp_ready()), "rust-analyzer startet");
        let text = ide.code_text(&file).unwrap();
        let broken = format!("{text}\nfn broken() {{\n    let _x = ;\n}}\n");
        let line = broken.lines().count() - 1;
        assert!(ide.set_code_text(&file, &broken));
        assert!(
            pump_until(180, || ide
                .code_marks(&file)
                .iter()
                .any(|m| m.line == line && m.kind == MarkKind::Error)),
            "Fehlermarke in Zeile {line}"
        );
        println!("test lsp_marks ... ok");
        passed += 1;
    } else {
        println!("test lsp_marks ... übersprungen (kein rust-analyzer)");
    }

    Application::process_events();
    let _ = std::fs::remove_dir_all(&home);
    println!("\ntest result: ok. {passed} passed");
}
