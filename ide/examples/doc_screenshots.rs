//! Screenshots of Freya TDE's windows for the handbook (`docs/images/*-<lang>.png`).
//!
//! Runs in its own empty configuration (the user's settings and keys stay untouched), creates a
//! demo project with the new-project wizard logic, designs a small sign-in form, generates the
//! code and saves every window as a PNG.
//!
//! ```bash
//! env -u WAYLAND_DISPLAY GDK_BACKEND=x11 xvfb-run -a -s "-screen 0 1920x1200x24" \
//!     cargo run -p freya-tde --example doc_screenshots -- docs/images de
//! ```
//!
//! *Bildschirmfotos der Fenster für das Handbuch; zweites Argument: `de` oder `en`.*

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use freya_tde::desktop::window;
use freya_tde::ui::panel::Tab;
use freya_tde::wizard::{NewProject, Starter, create};
use rncl::prelude::*;
use tdr_core::{ProjectKind, TypeRegistry, schema::Driver};

fn pump(millis: u64) {
    let end = Instant::now() + Duration::from_millis(millis);
    while Instant::now() < end {
        Application::process_events();
        std::thread::sleep(Duration::from_millis(15));
    }
}

/// Packs of the repository (components and UI backends).
fn registry(root: &Path) -> TypeRegistry {
    let mut registry = TypeRegistry::with_builtins();
    for dir in ["components", "widgets"] {
        let (packs, errors) = tdr_core::pack::load_all(&root.join(dir));
        assert!(errors.is_empty(), "{errors:#?}");
        for pack in packs {
            pack.register(&mut registry).unwrap();
        }
    }
    registry
}

fn main() {
    let mut args = std::env::args().skip(1);
    let out = PathBuf::from(args.next().unwrap_or_else(|| "docs/images".into()));
    let lang = args.next().unwrap_or_else(|| "de".into());
    let de = lang == "de";
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let home = std::env::temp_dir().join(format!("freya-doc-shots-{}-{lang}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    // SAFETY: am Programmanfang, vor weiteren Threads.
    unsafe {
        std::env::set_var("HOME", &home);
        std::env::set_var("XDG_CONFIG_HOME", home.join("config"));
        std::env::set_var("FREYA_TDE_LANG", &lang);
        // GTK-eigene Texte (z. B. „(keine)“) folgen der Systemsprache.
        let locale = if lang == "de" {
            "de_DE.UTF-8"
        } else {
            "en_US.UTF-8"
        };
        std::env::set_var("LANG", locale);
        std::env::set_var("LC_ALL", locale);
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
        std::env::set_var("GIT_CONFIG_GLOBAL", home.join("gitconfig"));
        for (key, value) in [
            ("GIT_AUTHOR_NAME", "Freya TDE"),
            ("GIT_AUTHOR_EMAIL", "docs@freya-tde.dev"),
            ("GIT_COMMITTER_NAME", "Freya TDE"),
            ("GIT_COMMITTER_EMAIL", "docs@freya-tde.dev"),
        ] {
            std::env::set_var(key, value);
        }
        // Neutrale Shell ohne Benutzer- und Rechnernamen im Prompt.
        std::env::set_var("SHELL", "/bin/sh");
        std::env::set_var("PS1", "contacts $ ");
        std::env::remove_var("ENV");
        if std::env::var_os("GSK_RENDERER").is_none() {
            std::env::set_var("GSK_RENDERER", "cairo");
        }
    }
    std::fs::create_dir_all(&out).unwrap();

    // Demo-Projekt wie mit dem Neu-Assistenten: GTK 4, SQLite-Datenmodul, Git.
    let request = NewProject {
        name: "contacts".into(),
        parent: home.join("projects"),
        kind: ProjectKind::Binary,
        platforms: Vec::new(),
        backend: "gtk4".into(),
        rust_version: "1.99.1".into(),
        starters: vec![Starter::Database {
            driver: Driver::Sqlite,
            url: "sqlite://contacts.db".into(),
        }],
        git: true,
        initial_commit: true,
        license_accepted: false,
    };
    std::fs::create_dir_all(&request.parent).unwrap();
    let created = create(&request, &registry(&root)).expect("demo project");
    // Eine kleine SQLite-Datenbank für den Schema-Browser.
    let project_dir = created.ptdr.parent().unwrap().to_path_buf();
    let _ = std::process::Command::new("sqlite3")
        .arg(project_dir.join("contacts.db"))
        .arg(
            "CREATE TABLE contact (id INTEGER PRIMARY KEY, name TEXT NOT NULL, email TEXT, birthday DATE);\
             CREATE TABLE phone (id INTEGER PRIMARY KEY, contact_id INTEGER NOT NULL REFERENCES contact(id), number TEXT NOT NULL);\
             CREATE VIEW contact_phones AS SELECT name, number FROM contact JOIN phone ON phone.contact_id = contact.id;",
        )
        .status();

    let _app = Application::new();
    ruf::dialog::set_automatic(Some(true));
    freya_tde::i18n::init(Default::default());
    let ide = freya_tde::Ide::start(Some(&created.ptdr));
    pump(800);

    // Ein kleines Anmeldeformular.
    let t = |de_text: &str, en_text: &str| if de { de_text } else { en_text }.to_owned();
    let text = |s: String| serde_json::json!({ "Text": s });
    let mut commands = vec![
        serde_json::json!({"command": "set_property", "form": "main_form", "element": "window", "property": "title", "value": text(t("Kontakte", "Contacts"))}),
        serde_json::json!({"command": "set_property", "form": "main_form", "element": "window", "property": "size", "value": {"Size": {"width": 380, "height": 300}}}),
    ];
    let widgets = [
        (
            "core/Label",
            "hint",
            "text",
            t("Bitte anmelden", "Please sign in"),
        ),
        (
            "core/Edit",
            "user",
            "placeholder",
            t("Benutzername", "User name"),
        ),
        (
            "core/Edit",
            "password",
            "placeholder",
            t("Passwort", "Password"),
        ),
        (
            "core/CheckBox",
            "remember",
            "caption",
            t("Angemeldet bleiben", "Remember me"),
        ),
        (
            "core/Button",
            "sign_in",
            "caption",
            t("Anmelden", "Sign in"),
        ),
    ];
    for (ty, name, property, value) in widgets {
        commands.push(serde_json::json!({"command": "add_widget", "form": "main_form", "parent": "window", "type": ty, "name": name}));
        commands.push(serde_json::json!({"command": "set_property", "form": "main_form", "element": name, "property": property, "value": text(value)}));
    }
    commands.push(serde_json::json!({"command": "set_property", "form": "main_form", "element": "password", "property": "password", "value": {"Bool": true}}));
    commands.push(serde_json::json!({"command": "set_event", "form": "main_form", "element": "sign_in", "event": "on_click", "handler": "sign_in_clicked"}));
    ide.run_commands(&serde_json::Value::Array(commands).to_string())
        .expect("design commands");
    ide.select(vec![tdr_core::ComponentName::new("sign_in").unwrap()]);
    ide.save();
    ide.generate_code();
    ide.show_code();
    // Der Handler bekommt Inhalt.
    let form_rs = project_dir.join("src/main_form.rs");
    let code = ide.code_text(&form_rs).unwrap_or_default();
    let body = if de {
        "pub fn sign_in_clicked(&mut self) {\n        let user = self.user.text();\n        self.hint.set_text(&format!(\"Willkommen, {user}!\"));\n    }"
    } else {
        "pub fn sign_in_clicked(&mut self) {\n        let user = self.user.text();\n        self.hint.set_text(&format!(\"Welcome, {user}!\"));\n    }"
    };
    ide.set_code_text(
        &form_rs,
        &code.replace("pub fn sign_in_clicked(&mut self) {}", body),
    );
    let _ = ide.save_all_code();
    ide.git_refresh(true);
    pump(1500);

    let shot = |widget: &Widget, name: &str| {
        let path = out.join(format!("{name}-{lang}.png"));
        if widget.is_visible() && widget.save_screenshot(&path) {
            println!("{}", path.display());
        } else {
            eprintln!("not saved: {name}");
        }
    };

    // Projekt-Manager als eigenes Fenster (sonst Reiter im Projektfenster).
    ide.undock_view(window::PROJECT_MANAGER);
    // Fenster der Grundausstattung.
    for name in [
        window::MAIN,
        window::DESIGNER,
        window::INSPECTOR,
        window::STRUCTURE,
        window::CODE,
        window::PROJECT,
        window::PROJECT_MANAGER,
    ] {
        ide.show_window(name);
    }
    pump(1000);
    let dir = home.join("shots");
    for path in ide.screenshots(&dir) {
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let target = out.join(format!("{stem}-{lang}.png"));
        std::fs::copy(&path, &target).unwrap();
        println!("{}", target.display());
    }

    // Meldungen: Probleme, dann Terminal.
    ide.show_panel(Tab::Problems);
    pump(800);
    if let Some((_, w)) = ide_window(&ide, window::MESSAGES) {
        shot(&w, "messages");
    }
    ide.show_terminal();
    pump(2500);
    if let Some((_, w)) = ide_window(&ide, window::MESSAGES) {
        shot(&w, "terminal");
    }

    // Werkzeugfenster einzeln.
    ide.show_window(window::GIT);
    pump(1200);
    if let Some((_, w)) = ide_window(&ide, window::GIT) {
        shot(&w, "git");
    }
    ide.debug_menu("dbg.show");
    pump(800);
    if let Some((_, w)) = ide_window(&ide, window::DEBUGGER) {
        shot(&w, "debugger");
    }
    ide.show_database();
    pump(4000);
    if let Some((_, w)) = ide_window(&ide, window::DATABASE) {
        shot(&w, "database");
    }
    ide.show_ai();
    pump(800);
    if let Some((_, w)) = ide_window(&ide, window::AI) {
        shot(&w, "ai");
    }
    ide.show_shortcuts();
    pump(800);
    if let Some((_, w)) = ide_window(&ide, window::SHORTCUTS) {
        shot(&w, "shortcuts");
    }
    ide.show_settings();
    pump(800);
    shot(ide.settings_window().widget(), "settings");
    ide.settings_window().hide();

    // Neu-Assistent auf der Seite „Oberfläche“.
    ide.show_wizard();
    let mut wizard = request.clone();
    wizard.name = "my_app".into();
    wizard.starters.clear();
    wizard.backend = "freya-gui".into();
    ide.set_wizard_request(&wizard);
    for _ in 0..3 {
        ide.wizard_step(true);
    }
    pump(800);
    shot(ide.wizard_window().widget(), "wizard");
    ide.wizard_window().hide();

    let _ = std::fs::remove_dir_all(&home);
}

/// A window of the IDE by its name.
fn ide_window(ide: &freya_tde::Ide, name: &str) -> Option<(String, Widget)> {
    ide.window_widget(name).map(|w| (name.to_owned(), w))
}
