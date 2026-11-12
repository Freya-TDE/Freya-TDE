//! The IDE windows with real (simulated) mouse and keyboard input.
//!
//! GTK allows one application per process in the main thread, hence
//! `harness = false`. Linux without display: `xvfb-run cargo test`.

use rncl::prelude::*;
use ruf::{Key, Modifiers};
use tdr_core::{ComponentName, ComponentPath, PropertyValue, TypeRef, Value, builtin};

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

    // Größe ziehen: Formular unten rechts, Widget in absolutem Layout oben links.
    let size_of = |ide: &freya_tde::Ide| match ide
        .session()
        .form()
        .unwrap()
        .widget(&name("window"))
        .unwrap()
        .component
        .get("size")
        .map(|v| v.value.clone())
    {
        Some(tdr_core::Value::Size(s)) => (s.width, s.height),
        _ => (640, 480),
    };
    let before = size_of(&ide);
    ide.select(vec![name("window")]);
    let window = ide
        .session()
        .layout()
        .unwrap()
        .rect(&name("window"))
        .unwrap();
    let corner = tdr_designer::Point::new(window.right(), window.bottom());
    mouse(&surface, MouseAction::Pressed, corner);
    mouse(
        &surface,
        MouseAction::Moved,
        tdr_designer::Point::new(corner.x + 40, corner.y + 30),
    );
    mouse(
        &surface,
        MouseAction::Released,
        tdr_designer::Point::new(corner.x + 40, corner.y + 30),
    );
    assert_eq!(size_of(&ide), (before.0 + 40, before.1 + 30));
    ide.undo();
    assert_eq!(size_of(&ide), before);
    ide.run_commands(
        r#"[
        {"command": "add_widget", "form": "main_form", "parent": "window", "type": "core/Container", "name": "free"},
        {"command": "set_layout", "form": "main_form", "element": "free", "layout": {"kind": "Absolute", "spacing": 0, "padding": {"left": 0, "top": 0, "right": 0, "bottom": 0}}},
        {"command": "add_widget", "form": "main_form", "parent": "free", "type": "core/Button", "name": "moved"},
        {"command": "set_placement", "form": "main_form", "element": "moved", "placement": {"Absolute": {"position": {"x": 20, "y": 10}, "size": {"width": 100, "height": 30}}}}
    ]"#,
    )
    .unwrap();
    ide.select(vec![name("moved")]);
    let button = ide
        .session()
        .layout()
        .unwrap()
        .rect(&name("moved"))
        .unwrap();
    let top_left = tdr_designer::Point::new(button.x, button.y);
    mouse(&surface, MouseAction::Pressed, top_left);
    mouse(
        &surface,
        MouseAction::Moved,
        tdr_designer::Point::new(top_left.x - 10, top_left.y - 5),
    );
    mouse(
        &surface,
        MouseAction::Released,
        tdr_designer::Point::new(top_left.x - 10, top_left.y - 5),
    );
    let placement = ide
        .session()
        .form()
        .unwrap()
        .widget(&name("moved"))
        .unwrap()
        .visual
        .placement
        .clone();
    assert_eq!(placement, tdr_core::Placement::absolute(10, 5, 110, 35));
    ide.run_commands(r#"[{"command": "remove", "form": "main_form", "element": "free"}]"#)
        .unwrap();
    println!("test resize ... ok");

    // Aus der Palette ziehen und im Designer ablegen; danach im Layout die
    // Größe über den Anfasser rechts unten ändern.
    let drop_at = {
        let session = ide.session();
        let window = session
            .layout()
            .unwrap()
            .item(&name("window"))
            .unwrap()
            .content
            .unwrap();
        tdr_designer::Point::new(window.center().x, window.bottom() - 4)
    };
    let before: Vec<_> = ide
        .session()
        .form()
        .unwrap()
        .widget(&name("window"))
        .unwrap()
        .children
        .iter()
        .map(|c| c.component.name.clone())
        .collect();
    for action in [ruf::DropAction::Motion, ruf::DropAction::Drop] {
        surface.simulate_drop(ruf::DropEvent {
            action,
            x: drop_at.x,
            y: drop_at.y,
            text: Some("core/CheckBox".into()),
        });
    }
    let dropped = ide.session().selection().to_vec();
    assert_eq!(dropped.len(), 1, "{dropped:?}");
    assert!(!before.contains(&dropped[0]));
    assert_eq!(
        ide.session()
            .form()
            .unwrap()
            .widget(&dropped[0])
            .unwrap()
            .component
            .type_ref,
        builtin::check_box()
    );
    let r = ide.session().layout().unwrap().rect(&dropped[0]).unwrap();
    let corner = tdr_designer::Point::new(r.right() - 1, r.bottom() - 1);
    let to = tdr_designer::Point::new(corner.x - 60, corner.y + 10);
    mouse(&surface, MouseAction::Pressed, corner);
    mouse(&surface, MouseAction::Moved, to);
    mouse(&surface, MouseAction::Released, to);
    let after = ide.session().layout().unwrap().rect(&dropped[0]).unwrap();
    assert_eq!((after.width, after.height), (r.width - 60, r.height + 10));
    // Objektinspektor: Breite in Prozent (Zeile „width_percent“ der Gruppe Platzierung).
    ide.set_layout_row(
        &dropped[0],
        "placement.width_percent",
        Some(tdr_core::Value::Int(50)),
    );
    let content = ide
        .session()
        .layout()
        .unwrap()
        .item(&name("window"))
        .unwrap()
        .content
        .unwrap();
    let half = ide.session().layout().unwrap().rect(&dropped[0]).unwrap();
    assert_eq!(half.width, content.width / 2);
    pump_until(1, || false);
    if let Some(dir) = std::env::var_os("FREYA_TDE_SHOT") {
        ide.screenshots(&std::path::Path::new(&dir).join("layout-rows"));
    }
    ide.undo();
    ide.undo();
    ide.undo();
    assert!(ide.session().form().unwrap().widget(&dropped[0]).is_none());
    println!("test palette_drop_and_layout_resize ... ok");

    // Nicht-visuelle Komponente: auf die Komponentenleiste gezogen, dort
    // per Klick gewählt (Objektinspektor), nicht auf der Formularfläche.
    let bar = ide.component_bar();
    bar.simulate_drop(ruf::DropEvent {
        action: ruf::DropAction::Drop,
        x: 10,
        y: 10,
        text: Some("pack:ws/Client".into()),
    });
    let socket = ide.session().selection().to_vec();
    assert_eq!(socket.len(), 1, "{socket:?}");
    assert!(
        ide.session()
            .form()
            .unwrap()
            .component(&socket[0])
            .is_some()
    );
    assert!(ide.session().layout().unwrap().item(&socket[0]).is_none());
    let icon = ide.session().tray().unwrap().rect(&socket[0]).unwrap();
    ide.select(Vec::new());
    mouse(&bar, MouseAction::Pressed, icon.center());
    mouse(&bar, MouseAction::Released, icon.center());
    assert_eq!(ide.session().selection(), socket.as_slice());
    pump_until(1, || false);
    if let Some(dir) = std::env::var_os("FREYA_TDE_SHOT") {
        ide.screenshots(&std::path::Path::new(&dir).join("component-bar"));
    }
    ide.undo();
    assert!(
        ide.session()
            .form()
            .unwrap()
            .component(&socket[0])
            .is_none()
    );
    println!("test component_bar ... ok");

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
        // Der Editor gilt nach set_code_text als geändert; ohne Speichern
        // fragte das nächste neue Projekt (unsichtbar unter Xvfb) nach.
        ide.save_all_code().unwrap();
        println!("test debugger ... ok");
        passed += 1;
    } else {
        println!("test debugger ... übersprungen (kein lldb-dap/gdb)");
    }

    // Neues Projekt mit egui: Palette mit dessen Widgets, Platzieren klappt.
    ide.new_project_with("egui");
    {
        let session = ide.session();
        assert_eq!(session.document().project().backend, "egui");
        let items = tdr_designer::palette(session.document().registry(), "egui");
        assert!(
            items
                .iter()
                .any(|i| i.type_ref == TypeRef::backend("egui", "Slider"))
        );
        assert!(
            !items
                .iter()
                .any(|i| i.type_ref == TypeRef::backend("gtk4", "Switch"))
        );
    }
    ide.run_commands(
        r#"[{"command": "add_widget", "form": "main_form", "parent": "window", "type": "backend:egui/Slider", "name": "level"}]"#,
    )
    .unwrap();
    assert!(
        ide.session()
            .layout()
            .unwrap()
            .rect(&name("level"))
            .is_some()
    );
    ide.new_project_with("iced");
    assert_eq!(ide.session().document().project().backend, "iced");
    assert!(
        tdr_designer::palette(ide.session().document().registry(), "iced")
            .iter()
            .any(|i| i.type_ref == TypeRef::backend("iced", "PickList"))
    );
    // Slint: erst der Lizenzhinweis (die Oberfläche fragt), dann das Projekt.
    let notice = ide
        .backend_notice("slint")
        .expect("Slint hat einen Lizenzhinweis");
    assert!(
        notice.contains("Royalty-free") && notice.contains("GPLv3"),
        "{notice}"
    );
    assert_eq!(ide.backend_notice("gtk4"), None);
    ide.new_project_with_notice_accepted("slint");
    assert_eq!(ide.session().document().project().backend, "slint");
    assert!(
        tdr_designer::palette(ide.session().document().registry(), "slint")
            .iter()
            .any(|i| i.type_ref == TypeRef::backend("slint", "AboutSlint"))
    );
    ide.new_project_with_notice_accepted("iced");
    ide.new_project_with("kein_backend");
    assert_eq!(
        ide.session().document().project().backend,
        "iced",
        "unbekanntes Backend ändert nichts"
    );
    println!("test egui_iced_slint_projects ... ok");
    passed += 1;

    // Datenbank: Spalten aus der Datenbank, Verbindung testen, Fenster
    // „Datenbank“ – mit dem echten Helfer und einer SQLite-Datei.
    let helper = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../target/debug")
        .join(format!("freya-tde-db{}", std::env::consts::EXE_SUFFIX));
    let lager = home.join("lager");
    let created = helper.is_file() && {
        drop(
            freya_tde::Session::create(
                &lager,
                "lager",
                "none",
                tdr_core::TypeRegistry::with_builtins(),
            )
            .unwrap(),
        );
        // Ein SQLx-Projekt: das Pack steht in der Projektdatei.
        let ptdr = lager.join("lager.ptdr");
        let mut project =
            tdr_core::ProjectFile::from_ron(&std::fs::read_to_string(&ptdr).unwrap()).unwrap();
        project.packs = vec!["tokio".into(), "sqlx".into()];
        std::fs::write(&ptdr, project.to_ron().unwrap()).unwrap();
        std::process::Command::new("sqlite3")
            .arg(lager.join("lager.db"))
            .arg(
                "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL, price REAL);\
                     CREATE VIEW cheap AS SELECT id, name FROM items WHERE price < 10;",
            )
            .status()
            .is_ok_and(|s| s.success())
    };
    if created {
        use tdr_core::schema::codes;
        use tdr_designer::Command;
        let ide = freya_tde::Ide::start(Some(&lager.join("lager.ptdr")));
        let form = name("main_form");
        let set = |element: &str, property: &str, value: Value| Command::SetProperty {
            form: form.clone(),
            element: name(element),
            property: property.into(),
            value: Some(PropertyValue::new(value)),
        };
        for (type_name, element) in [("Pool", "db"), ("Query", "items")] {
            ide.apply(Command::AddComponent {
                form: form.clone(),
                type_ref: TypeRef::pack("sqlx", type_name),
                name: Some(name(element)),
            });
        }
        ide.apply(set("db", "driver", Value::Enum("Sqlite".into())));
        ide.apply(set("db", "url", Value::Text("sqlite://lager.db".into())));
        ide.apply(set("items", "pool", Value::reference(name("db"))));
        ide.apply(set(
            "items",
            "sql",
            Value::Code("\"SELECT id, name, price FROM items\"".into()),
        ));
        let pool: ComponentPath = "main_form::db".parse().unwrap();
        let items: ComponentPath = "main_form::items".parse().unwrap();
        let columns = |ide: &freya_tde::Ide| {
            ide.session()
                .document()
                .form(&form)
                .unwrap()
                .element(&name("items"))
                .unwrap()
                .component()
                .get(tdr_core::data::COLUMNS)
                .map(|v| v.value.clone())
        };

        assert!(ide.database_test(&pool).unwrap().starts_with("SQLite "));
        let (_, described) = ide.query_columns_preview(&items).unwrap();
        assert_eq!(columns(&ide), None, "Vorschau ändert nichts");
        assert_eq!(described.columns.len(), 3);
        ide.take_query_columns(&items).unwrap();
        let expected = Value::List(
            ["id: i64", "name: String", "price: Option<f64>"]
                .map(|t| Value::Text(t.into()))
                .to_vec(),
        );
        assert_eq!(columns(&ide), Some(expected.clone()));
        ide.undo();
        assert_eq!(columns(&ide), None, "rückgängig machbar");

        // Wie der Knopf im Objektinspektor: im Hintergrund.
        ide.database_job(freya_tde::ui::database_window::DbJob::Columns, &items);
        assert!(
            pump_until(20, || columns(&ide) == Some(expected.clone())),
            "Spalten im Hintergrund übernommen"
        );

        ide.show_database();
        let has = |ide: &freya_tde::Ide, id: &str| {
            fn find(items: &[TreeItem], id: &str) -> bool {
                items.iter().any(|i| i.id == id || find(&i.children, id))
            }
            find(&ide.database_tree(), id)
        };
        assert!(
            pump_until(20, || has(&ide, "table:main_form::db:main.items")),
            "{:#?}",
            ide.database_tree()
        );
        assert!(has(&ide, "table:main_form::db:main.cheap"));
        assert!(has(&ide, "table:main_form::db:main.items:price"));
        if let Some(dir) = std::env::var_os("FREYA_TDE_SHOT") {
            ide.select(vec![name("items")]);
            ide.show_window(freya_tde::desktop::window::INSPECTOR);
            pump_until(1, || false);
            ide.screenshots(std::path::Path::new(&dir));
        }

        // Fehler mit stabilem Code: Datei fehlt.
        ide.apply(set("db", "url", Value::Text("sqlite://fehlt.db".into())));
        assert_eq!(
            ide.database_test(&pool).unwrap_err().code,
            codes::CONNECT_FAILED
        );
        assert!(!lager.join("fehlt.db").exists());
        println!("test database ... ok");
        passed += 1;
    } else {
        println!("test database ... übersprungen (freya-tde-db nicht gebaut oder kein sqlite3)");
    }

    // Redis: Schlüssel im Fenster „Datenbank“, Spalten aus der Art.
    let redis_url = std::env::var("FREYA_TDE_TEST_REDIS").ok();
    let cli = |url: &str, args: &[&str]| {
        std::process::Command::new("redis-cli")
            .arg("-u")
            .arg(url)
            .args(args)
            .output()
            .is_ok_and(|o| o.status.success())
    };
    match redis_url {
        Some(url) if helper.is_file() && cli(&url, &["HSET", "freya:ui:user", "name", "Freya"]) => {
            use tdr_designer::Command;
            let dir = home.join("cache");
            drop(
                freya_tde::Session::create(
                    &dir,
                    "cache",
                    "none",
                    tdr_core::TypeRegistry::with_builtins(),
                )
                .unwrap(),
            );
            let ptdr = dir.join("cache.ptdr");
            let mut project =
                tdr_core::ProjectFile::from_ron(&std::fs::read_to_string(&ptdr).unwrap()).unwrap();
            project.packs = vec!["tokio".into(), "redis".into()];
            std::fs::write(&ptdr, project.to_ron().unwrap()).unwrap();
            let ide = freya_tde::Ide::start(Some(&ptdr));
            let form = name("main_form");
            let set = |element: &str, property: &str, value: Value| Command::SetProperty {
                form: form.clone(),
                element: name(element),
                property: property.into(),
                value: Some(PropertyValue::new(value)),
            };
            for (type_name, element) in [("Client", "redis"), ("Read", "user")] {
                ide.apply(Command::AddComponent {
                    form: form.clone(),
                    type_ref: TypeRef::pack("redis", type_name),
                    name: Some(name(element)),
                });
            }
            ide.apply(set("redis", "url", Value::Text(url.clone())));
            ide.apply(set("user", "client", Value::reference(name("redis"))));
            ide.apply(set("user", "kind", Value::Enum("Hash".into())));
            ide.apply(set("user", "key", Value::Text("freya:ui:user".into())));
            let client: ComponentPath = "main_form::redis".parse().unwrap();
            let read: ComponentPath = "main_form::user".parse().unwrap();
            let server = ide.database_test(&read).unwrap();
            assert!(
                server.starts_with("Redis ") || server.starts_with("Valkey "),
                "{server}"
            );
            let keys = ide.database_keys(&client, "freya:ui:*", 10).unwrap();
            assert_eq!(keys.keys[0].key, "freya:ui:user");
            assert_eq!(keys.keys[0].preview, ["name = Freya"]);
            ide.take_query_columns(&read).unwrap();
            let columns = ide
                .session()
                .document()
                .form(&form)
                .unwrap()
                .element(&name("user"))
                .unwrap()
                .component()
                .get(tdr_core::data::COLUMNS)
                .map(|v| v.value.clone());
            assert_eq!(
                columns,
                Some(Value::List(vec![
                    Value::Text("field: String".into()),
                    Value::Text("value: String".into())
                ]))
            );
            ide.show_database();
            let has = |ide: &freya_tde::Ide, id: &str| {
                fn find(items: &[TreeItem], id: &str) -> bool {
                    items.iter().any(|i| i.id == id || find(&i.children, id))
                }
                find(&ide.database_tree(), id)
            };
            assert!(
                pump_until(20, || has(&ide, "key:main_form::redis:freya:ui:user")),
                "{:#?}",
                ide.database_tree()
            );
            if let Some(dir) = std::env::var_os("FREYA_TDE_SHOT") {
                ide.screenshots(std::path::Path::new(&dir).join("redis").as_path());
            }
            cli(&url, &["DEL", "freya:ui:user"]);
            println!("test redis ... ok");
            passed += 1;
        }
        _ => {
            println!("test redis ... übersprungen (FREYA_TDE_TEST_REDIS, freya-tde-db, redis-cli)")
        }
    }

    // Neu-Assistent: Seiten, Vorschau, Anlegen (Programm mit SQLite und Git).
    {
        use freya_tde::wizard::{NewProject, Starter};
        let ide = freya_tde::Ide::start(None);
        ide.show_wizard();
        assert!(ide.wizard_window().widget().is_visible());
        let mut request = NewProject::program("werkstatt", &home, "gtk4");
        request.starters = vec![Starter::Database {
            driver: tdr_core::schema::Driver::Sqlite,
            url: "sqlite://werkstatt.db".into(),
        }];
        request.git = true;
        request.initial_commit = true;
        request.platforms = vec![tdr_core::Platform::Linux];
        ide.set_wizard_request(&request);
        assert_eq!(
            ide.wizard_request(),
            request,
            "Seiten geben die Beschreibung wieder"
        );
        let shots = std::env::var_os("FREYA_TDE_SHOT").map(std::path::PathBuf::from);
        for page in 0..6 {
            if let Some(dir) = &shots {
                pump_until(1, || false);
                let _ = std::fs::create_dir_all(dir);
                ide.wizard_window()
                    .widget()
                    .save_screenshot(dir.join(format!("wizard-{page}.png")));
            }
            ide.wizard_step(true);
        }
        // Größeres Fenster: der Lizenzhinweis (Seite „Oberfläche“) wächst mit.
        ide.wizard_step(false);
        ide.wizard_step(false);
        pump_until(1, || false);
        let before = ide.wizard_notice_size();
        ide.wizard_window().widget().set_size(900, 700);
        assert!(
            pump_until(5, || {
                let now = ide.wizard_notice_size();
                now.0 > before.0 + 200 && now.1 > before.1 + 150
            }),
            "Hinweis wächst mit dem Fenster: {before:?} → {:?}",
            ide.wizard_notice_size()
        );
        if let Some(dir) = &shots {
            ide.wizard_window()
                .widget()
                .save_screenshot(dir.join("wizard-large.png"));
        }
        // Bibliothek: die Seite „Oberfläche“ entfällt.
        let mut library = NewProject::library("werkzeug", &home);
        library.rust_version = request.rust_version.clone();
        ide.set_wizard_request(&library);
        assert_eq!(ide.wizard_request().kind, tdr_core::ProjectKind::Library);
        ide.set_wizard_request(&request);

        let created = ide.create_project(&ide.wizard_request()).unwrap();
        assert_eq!(created.ptdr, home.join("werkstatt/werkstatt.ptdr"));
        assert_eq!(
            ide.session().project_dir().unwrap().canonicalize().unwrap(),
            home.join("werkstatt").canonicalize().unwrap()
        );
        let forms: Vec<String> = ide
            .session()
            .document()
            .forms()
            .iter()
            .map(|f| f.name.to_string())
            .collect();
        assert_eq!(forms, ["main_form", "services"]);
        assert!(home.join("werkstatt/.git").is_dir());
        assert!(home.join("werkstatt/src/main.rs").is_file());
        ide.wizard_window().hide();
        println!("test wizard ... ok");
        passed += 1;
    }

    // Einstellungen: Arbeitsverzeichnis, Darstellung, KI-Profil mit Key
    // (Schlüsselbund im Speicher); der Assistent beginnt im Arbeitsverzeichnis.
    {
        use freya_tde::settings::{MemoryStore, Settings, Theme, codes};
        let ide = freya_tde::Ide::start(None);
        ide.set_secret_store(Box::new(MemoryStore::default()));
        ide.show_settings();
        assert!(ide.settings_window().widget().is_visible());
        let work = home.join("arbeit");
        std::fs::create_dir_all(&work).unwrap();
        let mut edit = ide.settings_edit();
        edit.settings.base_dir = Some(work.clone());
        edit.settings.theme = Theme::Dark;
        edit.settings.language = freya_tde::settings::Language::English;
        ide.set_settings_edit(edit.clone());
        assert_eq!(ide.settings_edit(), edit, "Füllen ändert nichts");
        ide.settings_add_profile();
        let mut edit = ide.settings_edit();
        assert_eq!(
            edit.settings.ai.default_profile.as_deref(),
            Some("Profil 1")
        );
        edit.rename("Profil 1", "Claude").unwrap();
        edit.keys.insert("Claude".into(), Some("sk-test".into()));
        ide.set_settings_edit(edit.clone());
        let shots = std::env::var_os("FREYA_TDE_SHOT").map(std::path::PathBuf::from);
        if let Some(dir) = &shots {
            for tab in 0..2 {
                ide.settings_tab(tab);
                pump_until(1, || false);
                let _ = std::fs::create_dir_all(dir);
                ide.settings_window()
                    .widget()
                    .save_screenshot(dir.join(format!("settings-{tab}.png")));
            }
        }
        // Fehlendes Arbeitsverzeichnis blockiert.
        let mut wrong = edit.clone();
        wrong.settings.base_dir = Some(home.join("gibt-es-nicht"));
        let errors = ide.update_settings(&wrong).unwrap_err();
        assert_eq!(errors[0].code, codes::BASE_DIR_MISSING);

        let settings = ide.update_settings(&edit).unwrap();
        assert_eq!(ide.settings(), settings);
        assert!(settings.ai.profile("Claude").unwrap().key_in_keyring);
        assert_eq!(ide.ai_key().as_deref(), Some("sk-test"));
        let file = Settings::file().unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(!text.contains("sk-test"), "Key nicht in der Datei: {text}");
        assert_eq!(Settings::load_from(&file), settings);
        assert_eq!(
            ruf::dialog::default_folder().as_deref(),
            Some(work.as_path())
        );
        ide.settings_window().hide();
        ide.show_wizard();
        assert_eq!(ide.wizard_request().parent, work);
        ide.wizard_window().hide();
        let mut back = freya_tde::settings::SettingsEdit::new(&ide.settings());
        back.settings.theme = Theme::System;
        ide.update_settings(&back).unwrap();
        println!("test settings ... ok");
        passed += 1;
    }

    Application::process_events();
    let _ = std::fs::remove_dir_all(&home);
    println!("\ntest result: ok. {passed} passed");
}
