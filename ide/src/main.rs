//! Freya TDE – starts the development environment.
//!
//! ```bash
//! freya-tde                                 # neues, ungespeichertes Projekt
//! freya-tde pfad/zu/app.ptdr                # Projekt öffnen
//! freya-tde --commands befehle.json         # Designer-Befehle anwenden (JSON)
//! freya-tde --generate                      # Code erzeugen (nach --commands)
//! freya-tde --show-code                     # Code des Formulars im Editor öffnen
//! freya-tde --screenshots ordner            # alle Fenster als PNG speichern, beenden
//! freya-tde --no-splash                     # ohne Startbildschirm
//! ```

// Unter Windows ein GUI-Programm ohne eigenes Konsolenfenster (Release-Builds);
// aus einer Eingabeaufforderung gestartet, schreibt es in deren Konsole.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::path::PathBuf;

use freya_tde::ui::about;
use rncl::prelude::*;

struct Args {
    project: Option<PathBuf>,
    commands: Option<PathBuf>,
    screenshots: Option<PathBuf>,
    generate: bool,
    show_code: bool,
    splash: bool,
}

fn args() -> Args {
    let mut args = Args {
        project: None,
        commands: None,
        screenshots: None,
        generate: false,
        show_code: false,
        splash: true,
    };
    let mut it = std::env::args_os().skip(1);
    while let Some(arg) = it.next() {
        match arg.to_str() {
            Some("--commands") => args.commands = it.next().map(PathBuf::from),
            Some("--screenshots") => args.screenshots = it.next().map(PathBuf::from),
            Some("--generate") => args.generate = true,
            Some("--show-code") => args.show_code = true,
            Some("--no-splash") => args.splash = false,
            _ => args.project = Some(PathBuf::from(arg)),
        }
    }
    args
}

fn main() {
    attach_parent_console();
    // Als GIT_ASKPASS/SSH_ASKPASS gestartet: Frage weiterreichen, sonst nichts.
    if let Some(code) = freya_tde::git::askpass::helper_main() {
        std::process::exit(code);
    }

    // Freya TDE stellt seine Fensteranordnung wieder her. Unter Wayland dürfen
    // Programme Fenster nicht positionieren – deshalb unter Linux über
    // X11/XWayland (Entscheidung vom 23.10.2026), wenn nichts anderes gesetzt ist.
    #[cfg(all(unix, not(target_os = "macos")))]
    if std::env::var_os("GDK_BACKEND").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_some() {
        // SAFETY: noch vor jedem weiteren Thread, ganz am Programmanfang.
        unsafe { std::env::set_var("GDK_BACKEND", "x11") };
    }

    let args = args();
    let app = Application::new();
    app.set_name(about::PRODUCT);
    about::set_program_icon();
    // Kein Startbildschirm bei Automatisierung (Befehle, Screenshots).
    let splash = (args.splash && args.commands.is_none() && args.screenshots.is_none())
        .then(about::Splash::show);
    let ide = freya_tde::Ide::start(args.project.as_deref());
    if let Some(splash) = splash {
        splash.finish();
    }
    if let Some(file) = &args.commands {
        let result = std::fs::read_to_string(file)
            .map_err(|e| e.to_string())
            .and_then(|json| ide.run_commands(&json));
        if let Err(e) = result {
            eprintln!("freya-tde: {}: {e}", file.display());
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

/// Windows (Release): Ausgaben in die Konsole schreiben, aus der das Programm
/// gestartet wurde (ohne Konsole: nichts).
fn attach_parent_console() {
    #[cfg(all(windows, not(debug_assertions)))]
    // SAFETY: einfacher Systemaufruf ohne Zeiger, ganz am Programmanfang.
    unsafe {
        windows_sys::Win32::System::Console::AttachConsole(
            windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS,
        );
    }
}
