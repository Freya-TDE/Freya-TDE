//! The debugger against real adapters (lldb-dap, gdb) – each test is
//! skipped if its adapter is not installed.

use std::path::{Path, PathBuf};
use std::time::Duration;

use freya_tde::debug::{
    Adapter, AdapterKind, Breakpoint, ConsoleKind, DebugCommand, DebugSession, DebugSettings,
    LaunchConfig, Snapshot, State,
};

const PROGRAM: &str = r#"fn add(a: i32, b: i32) -> i32 {
    let sum = a + b;
    sum
}

fn main() {
    let x = 42;
    let name = String::from("Freya");
    let list = vec![1, 2, 3];
    println!("hello {name}");
    let y = add(x, 1);
    for i in 0..5 {
        let _ = i * y;
    }
    println!("y = {y} {}", list.len());
    if std::env::args().any(|a| a == "panic") {
        let v: Vec<i32> = Vec::new();
        let _ = v[3];
    }
}
"#;

const WAIT: Duration = Duration::from_secs(30);

/// Compiles the test program with debug info into a fresh folder.
fn program(name: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("freya-debug-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("prog.rs");
    std::fs::write(&source, PROGRAM).unwrap();
    let exe = dir.join(format!("prog{}", std::env::consts::EXE_SUFFIX));
    let status = std::process::Command::new("rustc")
        .args(["-g", "-C", "opt-level=0", "-o"])
        .arg(&exe)
        .arg(&source)
        .status()
        .unwrap();
    assert!(status.success());
    (dir, source)
}

fn adapter(kind: AdapterKind) -> Option<Adapter> {
    let program = Adapter::find_kind(kind)?;
    Some(Adapter { kind, program })
}

fn stopped_at(snap: &Snapshot, path: &Path, line: usize) -> bool {
    snap.is_stopped() && snap.location() == Some((path, line)) && !snap.locals().is_empty()
}

fn session(adapter: &Adapter, dir: &Path, settings: &DebugSettings, args: &[&str]) -> DebugSession {
    let mut config = LaunchConfig::new(
        dir.join(format!("prog{}", std::env::consts::EXE_SUFFIX)),
        dir,
    );
    config.args = args.iter().map(|a| a.to_string()).collect();
    // Keine Symbolserver im Netz abfragen.
    config.env.insert("DEBUGINFOD_URLS".into(), String::new());
    DebugSession::start(adapter, config, settings, || {}).unwrap()
}

fn walk_through(kind: AdapterKind) {
    let Some(adapter) = adapter(kind) else {
        eprintln!("{kind:?} nicht installiert – übersprungen");
        return;
    };
    let (dir, source) = program(&format!("{kind:?}"));
    let mut settings = DebugSettings::default();
    settings.toggle(&source, 11);
    settings.watches = vec!["x".into(), "y".into(), "gibt_es_nicht".into()];
    let debug = session(&adapter, &dir, &settings, &[]);
    let dump = |debug: &DebugSession| serde_json::to_string_pretty(&debug.snapshot()).unwrap();

    // Haltepunkt in Zeile 11, lokale Variablen und Überwachte Ausdrücke.
    assert!(
        debug.wait_for(WAIT, |s| stopped_at(s, &source, 11)),
        "{}",
        dump(&debug)
    );
    assert!(
        debug.wait_for(WAIT, |s| s.watches[0].value.is_some()),
        "{}",
        dump(&debug)
    );
    let snap = debug.snapshot();
    assert!(matches!(&snap.state, State::Stopped { reason, .. } if reason == "breakpoint"));
    assert!(snap.breakpoints[0].verified, "{}", dump(&debug));
    let x = snap.locals().iter().find(|v| v.name == "x").expect("x");
    assert_eq!(x.value, "42");
    let name = snap
        .locals()
        .iter()
        .find(|v| v.name == "name")
        .expect("name");
    assert!(name.value.contains("Freya"), "{name:?}");
    assert_eq!(snap.watches[0].value.as_deref(), Some("42"));
    assert!(snap.watches[2].error.is_some(), "{:?}", snap.watches[2]);
    assert!(
        snap.console_text().contains("hello Freya"),
        "{}",
        snap.console_text()
    );

    // Vec aufklappen.
    let list = snap
        .locals()
        .iter()
        .find(|v| v.name == "list")
        .expect("list")
        .clone();
    assert!(list.reference > 0, "{list:?}");
    debug
        .execute(&DebugCommand::LoadVariables {
            reference: list.reference,
        })
        .unwrap();
    assert!(
        debug.wait_for(WAIT, |s| s
            .variables
            .get(&list.reference)
            .is_some_and(|v| v.iter().any(|v| v.value == "3"))),
        "{}",
        dump(&debug)
    );

    // Hineinspringen in add, heraus, darüber.
    debug.execute(&DebugCommand::StepInto).unwrap();
    assert!(
        debug.wait_for(WAIT, |s| stopped_at(s, &source, 2)),
        "{}",
        dump(&debug)
    );
    assert!(debug.snapshot().frames.len() >= 2);
    debug.execute(&DebugCommand::StepOut).unwrap();
    assert!(
        debug.wait_for(WAIT, |s| s.is_stopped()
            && s.location().is_some_and(|(_, l)| l == 11)),
        "{}",
        dump(&debug)
    );
    debug.execute(&DebugCommand::StepOver).unwrap();
    assert!(
        debug.wait_for(WAIT, |s| stopped_at(s, &source, 12)),
        "{}",
        dump(&debug)
    );

    // Debug-Konsole: Ausdruck auswerten.
    debug
        .execute(&DebugCommand::Evaluate {
            expression: "y".into(),
        })
        .unwrap();
    assert!(
        debug.wait_for(WAIT, |s| s
            .console
            .iter()
            .any(|l| l.kind == ConsoleKind::Result && l.text.trim() == "43")),
        "{}",
        dump(&debug)
    );

    // Bedingter Haltepunkt in der Schleife.
    let mut conditional = Breakpoint::new(&source, 13);
    conditional.condition = Some("i == 3".into());
    debug
        .execute(&DebugCommand::SetBreakpoints {
            breakpoints: vec![conditional],
        })
        .unwrap();
    debug.execute(&DebugCommand::Continue).unwrap();
    assert!(
        debug.wait_for(WAIT, |s| stopped_at(s, &source, 13)),
        "{}",
        dump(&debug)
    );
    let i = debug
        .snapshot()
        .locals()
        .iter()
        .find(|v| v.name == "i")
        .map(|v| v.value.clone());
    assert_eq!(i.as_deref(), Some("3"), "{}", dump(&debug));

    // Bis Cursor, dann zu Ende laufen lassen.
    debug
        .execute(&DebugCommand::SetBreakpoints {
            breakpoints: vec![],
        })
        .unwrap();
    debug
        .execute(&DebugCommand::RunTo {
            path: source.clone(),
            line: 15,
        })
        .unwrap();
    assert!(
        debug.wait_for(WAIT, |s| stopped_at(s, &source, 15)),
        "{}",
        dump(&debug)
    );
    debug.execute(&DebugCommand::Continue).unwrap();
    assert!(debug.wait_for(WAIT, Snapshot::is_ended), "{}", dump(&debug));
    let snap = debug.snapshot();
    assert_eq!(snap.state, State::Ended { exit_code: Some(0) });
    assert!(
        snap.console_text().contains("y = 43 3"),
        "{}",
        snap.console_text()
    );
    assert_eq!(
        debug.execute(&DebugCommand::Continue).unwrap_err().code,
        "not_running"
    );
    let _ = std::fs::remove_dir_all(dir);
}

fn panic_stop(kind: AdapterKind) {
    let Some(adapter) = adapter(kind) else {
        return;
    };
    let (dir, source) = program(&format!("{kind:?}-panic"));
    let debug = session(&adapter, &dir, &DebugSettings::default(), &["panic"]);
    let dump = |debug: &DebugSession| serde_json::to_string_pretty(&debug.snapshot()).unwrap();
    // Angehalten im Panic, gezeigt wird die Zeile im eigenen Code.
    assert!(
        debug.wait_for(WAIT, |s| s.is_stopped() && s.location().is_some()),
        "{}",
        dump(&debug)
    );
    let snap = debug.snapshot();
    assert!(
        matches!(&snap.state, State::Stopped { reason, .. } if reason == "panic"),
        "{}",
        dump(&debug)
    );
    assert_eq!(
        snap.location(),
        Some((source.as_path(), 18)),
        "{}",
        dump(&debug)
    );
    debug.execute(&DebugCommand::Stop).unwrap();
    assert!(debug.wait_for(WAIT, Snapshot::is_ended), "{}", dump(&debug));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn lldb_walk_through() {
    walk_through(AdapterKind::Lldb);
}

#[test]
fn gdb_walk_through() {
    walk_through(AdapterKind::Gdb);
}

#[test]
fn lldb_panic() {
    panic_stop(AdapterKind::Lldb);
}

#[test]
fn gdb_panic() {
    panic_stop(AdapterKind::Gdb);
}

fn logpoint(kind: AdapterKind) {
    let Some(adapter) = adapter(kind) else {
        return;
    };
    let (dir, source) = program(&format!("{kind:?}-log"));
    let mut settings = DebugSettings::default();
    settings.set(Breakpoint {
        log_message: Some("Runde {i}".into()),
        ..Breakpoint::new(&source, 13)
    });
    let debug = session(&adapter, &dir, &settings, &[]);
    let dump = |debug: &DebugSession| serde_json::to_string_pretty(&debug.snapshot()).unwrap();
    // Ein Logpoint hält nicht an, er schreibt in die Konsole.
    assert!(debug.wait_for(WAIT, Snapshot::is_ended), "{}", dump(&debug));
    let text = debug.snapshot().console_text();
    assert!(
        text.contains("Runde 0") && text.contains("Runde 4"),
        "{text}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn lldb_logpoint() {
    logpoint(AdapterKind::Lldb);
}

#[test]
fn gdb_logpoint() {
    logpoint(AdapterKind::Gdb);
}
