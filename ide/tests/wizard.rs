//! The new-project wizard without UI: preview, checks with codes, creating
//! programs and libraries (with Git).

use std::path::PathBuf;

use freya_tde::wizard::{NewProject, Starter, codes, create, plan};
use tdr_core::schema::Driver;
use tdr_core::{ProjectFile, ProjectKind, TypeRegistry};

fn registry() -> TypeRegistry {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut registry = TypeRegistry::with_builtins();
    for dir in ["components", "widgets"] {
        let (packs, _) = tdr_core::pack::load_all(&root.join(dir));
        for pack in packs {
            pack.register(&mut registry).unwrap();
        }
    }
    registry
}

fn codes_of(plan: &freya_tde::wizard::Plan) -> Vec<&str> {
    plan.errors.iter().map(|e| e.code.as_str()).collect()
}

#[test]
fn wizard() {
    let home = std::env::temp_dir().join(format!("freya-wizard-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    // Git ohne die Einstellungen des Nutzers.
    unsafe {
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
        std::env::set_var("GIT_CONFIG_GLOBAL", home.join("gitconfig"));
        for (k, v) in [
            ("GIT_AUTHOR_NAME", "Test"),
            ("GIT_AUTHOR_EMAIL", "test@example.com"),
            ("GIT_COMMITTER_NAME", "Test"),
            ("GIT_COMMITTER_EMAIL", "test@example.com"),
        ] {
            std::env::set_var(k, v);
        }
    }
    let registry = registry();

    // Programm: GTK 4, SQLite, Git mit erstem Commit.
    let mut program = NewProject::program("laden", &home, "gtk4");
    program.starters = vec![Starter::Database {
        driver: Driver::Sqlite,
        url: "sqlite://laden.db".into(),
    }];
    program.git = true;
    program.initial_commit = true;
    let preview = plan(&program, &registry);
    assert!(preview.is_ok(), "{:#?}", preview.errors);
    assert_eq!(preview.packs, ["tokio", "sqlx"]);
    assert_eq!(preview.forms, ["main_form", "services"]);
    assert_eq!(
        preview.components,
        ["runtime: pack:tokio/Runtime", "db: pack:sqlx/Pool"]
    );
    for f in [
        "laden.ptdr",
        "Cargo.toml",
        "src/main.rs",
        "src/app.gen.rs",
        "src/main_form.rs",
        "src/main_form.ftdr",
        "src/services.gen.rs",
        ".gitignore",
    ] {
        assert!(
            preview.files.contains(&f.to_owned()),
            "{f}: {:?}",
            preview.files
        );
    }
    assert!(!home.join("laden").exists(), "Vorschau schreibt nichts");
    // Als JSON (KI, Automatisierung).
    let json = serde_json::to_string(&program).unwrap();
    assert_eq!(serde_json::from_str::<NewProject>(&json).unwrap(), program);

    let created = create(&program, &registry).unwrap();
    assert!(created.warnings.is_empty(), "{:?}", created.warnings);
    let dir = home.join("laden");
    for f in &preview.files {
        assert!(dir.join(f).exists(), "{f}");
    }
    let project = ProjectFile::from_ron(&std::fs::read_to_string(&created.ptdr).unwrap()).unwrap();
    assert_eq!(project.kind, ProjectKind::Binary);
    assert_eq!(project.backend, "gtk4");
    let log = std::process::Command::new("git")
        .args(["log", "--oneline"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&log.stdout).contains("New project laden"));
    let status = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(status.stdout.is_empty(), "alles committet");
    // Der Ordner existiert jetzt: ein zweites Mal geht nicht.
    assert_eq!(
        codes_of(&plan(&program, &registry)),
        [codes::FOLDER_NOT_EMPTY]
    );

    // Bibliothek mit Redis, ohne Git.
    let mut library = NewProject::library("cachelib", &home);
    library.starters = vec![Starter::Redis {
        url: "redis://127.0.0.1/".into(),
    }];
    let created = create(&library, &registry).unwrap();
    let dir = home.join("cachelib");
    assert!(dir.join("src/lib.rs").exists());
    assert!(!dir.join("src/main.rs").exists());
    assert!(!dir.join(".git").exists());
    let project = ProjectFile::from_ron(&std::fs::read_to_string(&created.ptdr).unwrap()).unwrap();
    assert_eq!(project.kind, ProjectKind::Library);
    assert_eq!(project.packs, ["tokio", "redis"]);

    // Prüfungen mit stabilen Codes.
    let mut bad = NewProject::program("1kaputt", &home.join("fehlt"), "gibtsnicht");
    bad.rust_version = "neu".into();
    bad.starters = vec![
        Starter::Redis { url: " ".into() },
        Starter::Redis {
            url: "redis://x".into(),
        },
    ];
    let found = codes_of(&plan(&bad, &registry)).join(" ");
    for code in [
        codes::INVALID_NAME,
        codes::NO_PARENT,
        codes::UNKNOWN_BACKEND,
        codes::INVALID_RUST_VERSION,
        codes::NO_URL,
        codes::DUPLICATE_STARTER,
    ] {
        assert!(found.contains(code), "{code}: {found}");
    }
    let mut slint = NewProject::program("slintapp", &home, "slint");
    let preview = plan(&slint, &registry);
    assert_eq!(codes_of(&preview), [codes::LICENSE_NOT_ACCEPTED]);
    assert!(preview.license_notice.unwrap().contains("Royalty-free"));
    slint.license_accepted = true;
    assert!(plan(&slint, &registry).is_ok());
    // Flutter: Rust-Bibliothek plus Flutter-App, kein main.rs.
    let mut flutter = NewProject::program("flutterapp", &home, "flutter");
    flutter.starters = vec![Starter::Redis {
        url: "redis://127.0.0.1/".into(),
    }];
    let preview = plan(&flutter, &registry);
    assert!(preview.is_ok(), "{:#?}", preview.errors);
    for f in [
        "src/lib.rs",
        "src/api.gen.rs",
        "src/frb_generated.gen.rs",
        "flutter/pubspec.yaml",
        "flutter/lib/main.dart",
        "flutter/lib/freya/main_form.g.dart",
        "flutter/lib/src/rust/frb_generated.dart",
    ] {
        assert!(
            preview.files.contains(&f.to_owned()),
            "{f}: {:?}",
            preview.files
        );
    }
    assert!(!preview.files.contains(&"src/main.rs".to_owned()));
    // WebView: ein normales Programm mit eingebettetem Pico CSS.
    let webview = NewProject::program("webapp", &home, "webview");
    let preview = plan(&webview, &registry);
    assert!(preview.is_ok(), "{:#?}", preview.errors);
    for f in [
        "src/main.rs",
        "src/main_form.gen.rs",
        "src/freya_pico.gen.rs",
    ] {
        assert!(
            preview.files.contains(&f.to_owned()),
            "{f}: {:?}",
            preview.files
        );
    }
    let mut lib_ui = NewProject::library("libui", &home);
    lib_ui.backend = "gtk4".into();
    assert_eq!(
        codes_of(&plan(&lib_ui, &registry)),
        [codes::LIBRARY_WITH_BACKEND]
    );

    let _ = std::fs::remove_dir_all(&home);
}
