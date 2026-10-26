//! The LSP client against the real rust-analyzer (skipped if it is not
//! installed). Uses a tiny Cargo project without dependencies.

use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use freya_td::lsp::{DiagnosticSeverity, LspClient, LspEvent};

const MAIN: &str = r#"fn helper() -> String {
    String::from("x")
}

fn main() {
    let text = helper();
    let n = text.len();
    println!("{n}");
    undefined_function();
}
"#;

fn wait_for(
    events: &mpsc::Receiver<LspEvent>,
    seconds: u64,
    mut matches: impl FnMut(&LspEvent) -> bool,
) -> Option<LspEvent> {
    let end = Instant::now() + Duration::from_secs(seconds);
    while let Some(left) = end.checked_duration_since(Instant::now()) {
        match events.recv_timeout(left) {
            Ok(event) if matches(&event) => return Some(event),
            Ok(_) => {}
            Err(_) => return None,
        }
    }
    None
}

#[test]
fn rust_analyzer_round_trip() {
    if std::process::Command::new(freya_td::lsp::program())
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("rust-analyzer nicht installiert – Test übersprungen");
        return;
    }
    let dir = std::env::temp_dir().join(format!("freya-lsp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"lsp_demo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .unwrap();
    let main = dir.join("src/main.rs");
    std::fs::write(&main, MAIN).unwrap();
    let main = main.canonicalize().unwrap();

    let (tx, events) = mpsc::channel();
    let client = LspClient::start(&dir.canonicalize().unwrap(), move |e| {
        let _ = tx.send(e);
    })
    .unwrap();
    assert!(wait_for(&events, 60, |e| *e == LspEvent::Ready).is_some());
    client.open(&main, MAIN);
    client.save(&main);
    // Bereit: Indexierung abgeschlossen.
    wait_for(&events, 180, |e| {
        matches!(
            e,
            LspEvent::Status {
                quiescent: true,
                ..
            }
        )
    });

    // Gehe zu Definition: `helper` in Zeile 6, Spalte 15.
    let request = client.definition(&main, 6, 16);
    let found = wait_for(
        &events,
        60,
        |e| matches!(e, LspEvent::Definition { request: r, .. } if *r == request),
    );
    let Some(LspEvent::Definition { locations, .. }) = found else {
        panic!("keine Definition");
    };
    assert_eq!(locations[0].path, main);
    assert_eq!(locations[0].line, 1);

    // Vervollständigung nach `text.` in Zeile 7.
    let changed = MAIN.replace("text.len()", "text.");
    client.change(&main, &changed);
    let line = changed.lines().nth(6).unwrap();
    let request = client.completion(&main, 7, line.find("text.").unwrap() + 5);
    let found = wait_for(
        &events,
        60,
        |e| matches!(e, LspEvent::Completion { request: r, .. } if *r == request),
    );
    let Some(LspEvent::Completion { items, .. }) = found else {
        panic!("keine Vervollständigung");
    };
    assert!(
        items.iter().any(|i| i.label.starts_with("len")),
        "{:?}",
        &items[..items.len().min(10)]
    );

    // Fehler aus rust-analyzer bzw. cargo check: der unbekannte Funktionsaufruf.
    client.change(&main, MAIN);
    client.save(&main);
    let error = wait_for(&events, 180, |e| match e {
        LspEvent::Diagnostics { path, items } => {
            path == &main
                && items
                    .iter()
                    .any(|d| d.line == 9 && d.severity == DiagnosticSeverity::Error)
        }
        _ => false,
    });
    assert!(
        error.is_some(),
        "kein Fehler für undefined_function gemeldet"
    );

    drop(client);
    let _ = std::fs::remove_dir_all(Path::new(&dir));
}
