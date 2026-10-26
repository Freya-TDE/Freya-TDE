//! Freya-TD – starts the development environment.
//!
//! ```bash
//! freya-td                                  # neues, ungespeichertes Projekt
//! freya-td pfad/zu/app.ptdr                 # Projekt öffnen
//! freya-td --commands befehle.json          # Designer-Befehle anwenden (JSON)
//! freya-td --generate                      # Code erzeugen (nach --commands)
//! freya-td --show-code                     # Code des Formulars im Editor öffnen
//! freya-td --screenshots ordner             # alle Fenster als PNG speichern, beenden
//! ```

use std::path::PathBuf;

use rncl::prelude::*;

struct Args {
    project: Option<PathBuf>,
    commands: Option<PathBuf>,
    screenshots: Option<PathBuf>,
    generate: bool,
    show_code: bool,
}

fn args() -> Args {
    let mut args = Args {
        project: None,
        commands: None,
        screenshots: None,
        generate: false,
        show_code: false,
    };
    let mut it = std::env::args_os().skip(1);
    while let Some(arg) = it.next() {
        match arg.to_str() {
            Some("--commands") => args.commands = it.next().map(PathBuf::from),
            Some("--screenshots") => args.screenshots = it.next().map(PathBuf::from),
            Some("--generate") => args.generate = true,
            Some("--show-code") => args.show_code = true,
            _ => args.project = Some(PathBuf::from(arg)),
        }
    }
    args
}

fn main() {
    // Freya-TD stellt seine Fensteranordnung wieder her. Unter Wayland dürfen
    // Programme Fenster nicht positionieren – deshalb unter Linux über
    // X11/XWayland (Entscheidung vom 23.10.2026), wenn nichts anderes gesetzt ist.
    #[cfg(all(unix, not(target_os = "macos")))]
    if std::env::var_os("GDK_BACKEND").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_some() {
        // SAFETY: noch vor jedem weiteren Thread, ganz am Programmanfang.
        unsafe { std::env::set_var("GDK_BACKEND", "x11") };
    }

    let args = args();
    let app = Application::new();
    app.set_name("Freya-TD");
    let ide = freya_td::Ide::start(args.project.as_deref());
    if let Some(file) = &args.commands {
        let result = std::fs::read_to_string(file)
            .map_err(|e| e.to_string())
            .and_then(|json| ide.run_commands(&json));
        if let Err(e) = result {
            eprintln!("freya-td: {}: {e}", file.display());
        }
    }
    if args.generate {
        ide.generate_code();
    }
    if args.show_code {
        ide.show_code();
    }
    if let Some(dir) = &args.screenshots {
        for _ in 0..30 {
            Application::process_events();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        for path in ide.screenshots(dir) {
            println!("{}", path.display());
        }
        return;
    }
    std::process::exit(app.exec());
}
