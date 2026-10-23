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

fn main() {
    // Eigene Konfiguration, damit die Fensteranordnung des Nutzers unberührt bleibt.
    let home = std::env::temp_dir().join(format!("freya-ui-{}", std::process::id()));
    // SAFETY: am Programmanfang, vor weiteren Threads.
    unsafe {
        std::env::set_var("HOME", &home);
        std::env::set_var("XDG_CONFIG_HOME", home.join("config"));
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
        Some("main::button1"),
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
            r#"[{"command": "add_widget", "form": "main", "parent": "window", "type": "core/Label", "name": "hint"}]"#,
        )
        .unwrap();
    assert_eq!(count, 1);
    assert!(ide.session().form().unwrap().contains(&name("hint")));
    println!("test run_commands ... ok");

    Application::process_events();
    let _ = std::fs::remove_dir_all(&home);
    println!("\ntest result: ok. 4 passed");
}
