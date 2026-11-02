//! The IDE windows with real (simulated) mouse and keyboard input.
//!
//! GTK allows one application per process in the main thread, hence
//! `harness = false`. Linux without display: `xvfb-run cargo test`.

use rncl::prelude::*;
use ruf::{Key, Modifiers};
use tdr_core::{ComponentName, TypeRef, builtin};

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
        // Git ohne die Konfiguration des Nutzers.
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
        std::env::set_var("GIT_CONFIG_GLOBAL", home.join("gitconfig"));
        std::env::set_var("GIT_AUTHOR_NAME", "Test");
        std::env::set_var("GIT_AUTHOR_EMAIL", "test@example.com");
        std::env::set_var("GIT_COMMITTER_NAME", "Test");
        std::env::set_var("GIT_COMMITTER_EMAIL", "test@example.com");
        // Ohne Bildschirm (Xvfb) ist Software-Vulkan unzuverlässig.
        if std::env::var_os("GSK_RENDERER").is_none() {
            std::env::set_var("GSK_RENDERER", "cairo");
        }
    }
    let _app = Application::new();
    let ide = freya_tde::Ide::start(None);
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
        text: None,
    });
    assert!(!ide.session().form().unwrap().contains(&name("button1")));
    surface.simulate_key(KeyEvent {
        key: Key::Char('z'),
        modifiers: Modifiers {
            ctrl: true,
            ..Modifiers::default()
        },
        text: None,
    });
    assert!(ide.session().form().unwrap().contains(&name("button1")));
    println!("test delete_and_undo ... ok");

    // Neue Projekte nutzen GTK 4: die Palette hat dessen Widgets, ein
    // Schalter lässt sich platzieren und erscheint mit Symbol im Designer.
    let switch = TypeRef::backend("gtk4", "Switch");
    {
        let session = ide.session();
        assert_eq!(session.document().project().backend, "gtk4");
        let items = tdr_designer::palette(session.document().registry(), "gtk4");
        assert!(
            items
                .iter()
                .any(|i| i.type_ref == switch && i.icon.as_deref() == Some("switch"))
        );
    }
    ide.arm(Some(switch.clone()));
    mouse(&surface, MouseAction::Pressed, spot);
    mouse(&surface, MouseAction::Released, spot);
    assert_eq!(ide.session().selection(), [name("switch1")]);
    // Gezeichnet wie ein Schalter: Spur und Knopf (look = "switch").
    {
        let session = ide.session();
        let (layout, primitives) = session.picture().unwrap();
        let area = layout.rect(&name("switch1")).unwrap();
        let parts = primitives
            .iter()
            .filter(|p| matches!(p, tdr_designer::Primitive::Rect { rect, radius, .. } if *radius > 4 && area.contains(rect.center())))
            .count();
        assert!(parts >= 2, "Schalter im Designer");
    }
    surface.simulate_key(KeyEvent {
        key: Key::Delete,
        modifiers: Modifiers::default(),
        text: None,
    });
    assert!(!ide.session().form().unwrap().contains(&name("switch1")));
    println!("test gtk4_palette ... ok");

    // Reiter: ein Klick auf den zweiten zeigt dessen Seite (rückgängig machbar).
    ide.run_commands(
        r#"[
        {"command": "add_widget", "form": "main_form", "parent": "window", "type": "backend:gtk4/Notebook", "name": "pages"},
        {"command": "set_property", "form": "main_form", "element": "pages", "property": "pages", "value": {"List": [{"Text": "Eins"}, {"Text": "Zwei"}]}},
        {"command": "add_widget", "form": "main_form", "parent": "pages", "type": "core/Label", "name": "first"},
        {"command": "add_widget", "form": "main_form", "parent": "pages", "type": "core/Label", "name": "second"}
    ]"#,
    )
    .unwrap();
    let visible = |n: &str| ide.session().layout().unwrap().rect(&name(n)).is_some();
    assert!(visible("first") && !visible("second"));
    let notebook = ide
        .session()
        .layout()
        .unwrap()
        .rect(&name("pages"))
        .unwrap();
    let theme = ide.session().theme().clone();
    let second_tab = tdr_designer::Point::new(
        notebook.x + theme.text_width("Eins") + 24 + 10,
        notebook.y + 10,
    );
    mouse(&surface, MouseAction::Pressed, second_tab);
    mouse(&surface, MouseAction::Released, second_tab);
    assert!(
        !visible("first") && visible("second"),
        "zweite Seite sichtbar"
    );
    assert_eq!(ide.session().selection(), [name("pages")]);
    ide.undo();
    assert!(visible("first"));
    ide.run_commands(r#"[{"command": "remove", "form": "main_form", "element": "pages"}]"#)
        .unwrap();
    println!("test notebook_tabs ... ok");

    // Befehle als JSON (Skripte, KI).
    let count = ide
        .run_commands(
            r#"[{"command": "add_widget", "form": "main_form", "parent": "window", "type": "core/Label", "name": "hint"}]"#,
        )
        .unwrap();
    assert_eq!(count, 1);
    assert!(ide.session().form().unwrap().contains(&name("hint")));
    println!("test run_commands ... ok");

    // Hilfe → Über.
    ide.show_about();
    assert!(ide.about_window().widget().is_visible());
    assert!(freya_tde::ui::about::AboutWindow::lines()[0] == "Freya TDE");
    ide.about_window().hide();
    println!("test about ... ok");

    // Doppelklick im Designer: Handler anlegen, Code erzeugen, hinspringen.
    let project_dir = home.join("shop");
    let session = freya_tde::Session::create(
        &project_dir,
        "shop",
        "none",
        tdr_core::TypeRegistry::with_builtins(),
    )
    .unwrap();
    drop(session);
    let ide = freya_tde::Ide::start(Some(&project_dir.join("shop.ptdr")));
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

    let mut passed = 6;

    // Projektfenster: Dateibaum und Dateioperationen (wie über das Kontextmenü).
    use freya_tde::files::{Entry, FileOp};
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

    // Git: Repository anlegen, Status mit Markierungen, stagen, Commit.
    use freya_tde::git::{After, Change, GitOp};
    if std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_ok()
    {
        assert!(ide.repo().is_none());
        ide.git_init();
        assert!(ide.repo().is_some());
        let changes = ide.git_changes();
        assert_eq!(changes.get("src/main_form.rs"), Some(&Change::Untracked));
        assert!(!changes.contains_key("target"));
        ide.git_run(&GitOp::Stage { paths: vec![] }).unwrap();
        ide.git_run(&GitOp::Commit {
            message: "Initial commit".into(),
            amend: false,
            then: After::Nothing,
        })
        .unwrap();
        let status = ide.git_status().unwrap();
        assert!(status.files.is_empty(), "{:?}", status.files);
        assert!(status.head.is_some());
        let log = ide.repo().unwrap().log(10).unwrap();
        assert_eq!(log[0].subject, "Initial commit");
        // Änderungsmarken im Editor: verglichen mit dem Git-Index.
        use freya_tde::git::LineChangeKind;
        let form_rs = project_dir.join("src/main_form.rs");
        ide.open_source(&form_rs, 1);
        let open = ide.code_file().unwrap();
        assert!(ide.code_line_changes(&open).is_empty());
        let text = ide.code_text(&open).unwrap();
        let mut lines: Vec<&str> = text.lines().collect();
        // Neue Zeile oben, unveränderte Zeile, dann eine geänderte Zeile.
        lines[1] = "//! geändert";
        lines.insert(0, "// neu");
        let changed = lines.join("\n") + "\n";
        assert!(ide.set_code_text(&open, &changed));
        assert!(pump_until(5, || !ide.code_line_changes(&open).is_empty()));
        let kinds: Vec<(LineChangeKind, usize)> = ide
            .code_line_changes(&open)
            .iter()
            .map(|c| (c.kind, c.line))
            .collect();
        assert_eq!(
            kinds,
            [(LineChangeKind::Added, 1), (LineChangeKind::Modified, 3)],
            "{:?}",
            ide.code_line_changes(&open)
        );
        assert!(ide.set_code_text(&open, &text));
        assert!(pump_until(5, || ide.code_line_changes(&open).is_empty()));
        println!("test git_line_changes ... ok");
        passed += 1;
        std::fs::write(project_dir.join("README.md"), "# Shop\n").unwrap();
        ide.git_refresh(false);
        assert_eq!(ide.git_changes().get("README.md"), Some(&Change::Untracked));
        ide.git_show_diff("README.md", false);
        assert!(
            ide.code_file()
                .is_some_and(|p| p.to_string_lossy().contains("diff:u:README.md"))
        );
        ide.set_graph_refs(freya_tde::git::LogRefs::All);
        assert!(
            ide.git_output().contains("> git commit -F -"),
            "{}",
            ide.git_output()
        );
        assert_eq!(
            ide.git_output(),
            ide.output().text(freya_tde::output::channel::GIT)
        );
        ide.git_show_output();
        ide.open_project_file("src/main_form.ftdr");
        println!("test git ... ok");
        passed += 1;
    } else {
        println!("test git ... übersprungen (kein git)");
    }

    // rust-analyzer: ein Fehler im Editor erscheint als Marke am Rand.
    if std::process::Command::new(freya_tde::lsp::program())
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
        // Dieselben Fehler stehen in den Problemen (Panel „Probleme“).
        let problems = ide.problems();
        assert!(
            problems.iter().any(|p| p.line == Some(line)
                && p.source == freya_tde::problems::ProblemSource::RustAnalyzer
                && p.file.as_deref() == Some(file.as_path())),
            "{problems:?}"
        );
        assert!(
            serde_json::to_string(&problems)
                .unwrap()
                .contains("rust_analyzer")
        );
        println!("test lsp_marks ... ok");
        passed += 1;
    } else {
        println!("test lsp_marks ... übersprungen (kein rust-analyzer)");
    }

    // Projekt-Manager: Unterprojekt aus Cargo.toml, öffnen, aktivieren, schließen.
    let shop = ide.active_project().expect("shop ist gespeichert");
    assert_eq!(ide.main_project().as_ref(), Some(&shop));
    let helper_dir = project_dir.join("tools/helper");
    std::fs::create_dir_all(&helper_dir).unwrap();
    std::fs::write(
        helper_dir.join("Cargo.toml"),
        "[package]\nname = \"helper\"\nrust-version = \"1.95\"\n",
    )
    .unwrap();
    let plan = ide.plan_projects_from_cargo(&helper_dir, false).unwrap();
    assert_eq!(plan[0].name, "helper");
    let written = ide.create_projects_from_cargo(&plan).unwrap();
    let helper = written[0].clone();
    ide.edit_projects(|list| list.set_parent(&helper, Some(&shop)))
        .unwrap();
    ide.open_project(&helper);
    assert_eq!(ide.active_project().as_ref(), Some(&helper));
    assert_eq!(ide.main_project().as_ref(), Some(&shop));
    let tree = ide.project_tree();
    let shop_node = tree.iter().find(|n| n.path == shop).expect("shop im Baum");
    assert!(shop_node.open);
    let helper_node = shop_node
        .children
        .iter()
        .find(|n| n.path == helper)
        .expect("helper unter shop");
    assert!(helper_node.open);
    // Nur verwaltet: Code erzeugen verweigert, die Projektdatei bleibt ohne Code.
    assert!(ide.generate_code().is_none());
    assert!(!helper_dir.join("src/main.rs").exists());
    ide.activate_project(&shop);
    assert_eq!(ide.active_project().as_ref(), Some(&shop));
    assert!(ide.open_projects().contains(&helper));
    ide.close_project(&helper);
    assert_eq!(ide.open_projects(), std::slice::from_ref(&shop));
    assert!(ide.session().document().project().generate);
    println!("test project_manager ... ok");
    passed += 1;

    // Terminal im Panel: Befehl schicken, Ergebnis lesen.
    if cfg!(unix) {
        ide.terminal_run("echo freya-$((40+2))");
        assert!(
            pump_until(20, || ide.terminal_text().contains("freya-42")),
            "{}",
            ide.terminal_text()
        );
        println!("test terminal ... ok");
        passed += 1;

        // Zweites Terminal: eigene Shell, eigener Bildschirm.
        assert_eq!(ide.terminal_count(), 1);
        assert_eq!(ide.new_terminal(), 1);
        ide.terminal_run("echo zwei-$((1+1))");
        assert!(
            pump_until(20, || ide.terminal_text().contains("zwei-2")),
            "{}",
            ide.terminal_text()
        );
        assert!(!ide.terminal_text().contains("freya-42"));
        assert!(ide.select_terminal(0));
        assert!(ide.terminal_text().contains("freya-42"));
        assert!(!ide.select_terminal(5));
        ide.close_terminal();
        assert_eq!(ide.terminal_count(), 1);
        assert!(ide.terminal_text().contains("zwei-2"));
        println!("test terminals ... ok");
        passed += 1;
    }

    // Debugger: Haltepunkt per Klick in den Rand, F9, Schritt, Ausdruck, Ende.
    if freya_tde::debug::Adapter::find().is_ok() {
        use freya_tde::debug::State;
        let demo_dir = project_dir.join("tools/dbgdemo");
        std::fs::create_dir_all(demo_dir.join("src")).unwrap();
        std::fs::write(
            demo_dir.join("Cargo.toml"),
            "[package]\nname = \"dbgdemo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .unwrap();
        let main_rs = demo_dir.join("src/main.rs");
        std::fs::write(
            &main_rs,
            "fn main() {\n    let x = 41;\n    let y = x + 1;\n    println!(\"y = {y}\");\n}\n",
        )
        .unwrap();
        let main_rs = main_rs.canonicalize().unwrap();
        let plan = ide.plan_projects_from_cargo(&demo_dir, false).unwrap();
        let demo = ide.create_projects_from_cargo(&plan).unwrap()[0].clone();
        ide.open_project(&demo);
        ide.open_source(&main_rs, 1);
        ide.click_gutter(&main_rs, 3, 1);
        assert_eq!(ide.debug_settings().breakpoints.len(), 1);
        assert_eq!(ide.code_breakpoint_marks(&main_rs)[0].line, 3);
        // Zeile davor einfügen: der Haltepunkt wandert mit.
        let text = ide.code_text(&main_rs).unwrap();
        ide.set_code_text(
            &main_rs,
            &text.replacen("fn main() {\n", "fn main() {\n    // neu\n", 1),
        );
        assert_eq!(ide.debug_settings().breakpoints[0].line, 4);
        ide.set_code_text(&main_rs, &text);
        assert_eq!(ide.debug_settings().breakpoints[0].line, 3);
        ide.add_watch("x");
        ide.debug_menu("start.run");
        let stopped_at = |line| {
            ide.debug_snapshot()
                .is_some_and(|s| s.is_stopped() && s.location().is_some_and(|(_, l)| l == line))
        };
        assert!(
            pump_until(180, || stopped_at(3)),
            "{:?}",
            ide.debug_snapshot()
        );
        assert!(pump_until(20, || ide.code_execution_line(&main_rs) == Some(3)));
        assert!(pump_until(20, || ide.debug_snapshot().is_some_and(|s| s
            .watches[0]
            .value
            .as_deref()
            == Some("41"))));
        ide.debug_menu("start.step_over");
        assert!(
            pump_until(30, || stopped_at(4)),
            "{:?}",
            ide.debug_snapshot()
        );
        assert!(pump_until(20, || ide.code_execution_line(&main_rs) == Some(4)));
        ide.debug_evaluate("y");
        assert!(
            pump_until(20, || ide.debug_console().contains("42")),
            "{}",
            ide.debug_console()
        );
        // Bildschirmfotos zum Ansehen: FREYA_UI_SHOTS=<ordner>.
        if let Some(dir) = std::env::var_os("FREYA_UI_SHOTS") {
            ide.show_shortcuts();
            ide.show_about();
            pump_until(1, || false);
            ide.screenshots(std::path::Path::new(&dir));
            ide.about_window()
                .widget()
                .save_screenshot(std::path::Path::new(&dir).join("about.png"));
        }
        ide.debug_menu("start.run");
        assert!(pump_until(30, || ide
            .debug_snapshot()
            .is_some_and(|s| s.is_ended())));
        assert_eq!(
            ide.debug_snapshot().map(|s| s.state),
            Some(State::Ended { exit_code: Some(0) })
        );
        assert!(pump_until(20, || ide.debug_console().contains("y = 42")));
        assert!(pump_until(10, || ide
            .code_execution_line(&main_rs)
            .is_none()));
        // Die Haltepunkte bleiben für das Projekt gespeichert.
        let store = freya_tde::debug::DebugStore::load(
            &freya_tde::debug::DebugStore::default_path().unwrap(),
        );
        assert_eq!(store.get(&demo).breakpoints.len(), 1);
        println!("test debugger ... ok");
        passed += 1;
    } else {
        println!("test debugger ... übersprungen (kein lldb-dap/gdb)");
    }

    Application::process_events();
    let _ = std::fs::remove_dir_all(&home);
    println!("\ntest result: ok. {passed} passed");
}
