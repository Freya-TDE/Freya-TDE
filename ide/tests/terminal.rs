//! The terminal against a real shell in a pseudo terminal.

use std::time::{Duration, Instant};

use freya_tde::terminal::{TermColor, Terminal};

fn wait_for(terminal: &Terminal, seconds: u64, done: impl Fn(&Terminal) -> bool) -> bool {
    let end = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < end {
        if done(terminal) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn shell_round_trip() {
    let dir = std::env::temp_dir();
    let terminal = Terminal::start(&dir, 24, 80).unwrap();
    assert!(terminal.is_running());
    // Die Shell rechnet selbst: kommt das Ergebnis, lief der Befehl.
    if cfg!(windows) {
        terminal.write_text("echo freya-%COMSPEC:~0,0%2\r");
    } else {
        terminal.write_text("echo freya-$((1+1))\r");
    }
    assert!(
        wait_for(&terminal, 20, |t| t.text().contains("freya-2")),
        "{}",
        terminal.text()
    );
    terminal.resize(10, 40);
    assert_eq!(terminal.size(), (10, 40));
    let screen = terminal.screen();
    assert_eq!((screen.rows, screen.cols), (10, 40));
    assert!(screen.cursor.is_some());
    let json = serde_json::to_string(&screen).unwrap();
    assert!(json.contains("\"lines\""));
    if !cfg!(windows) {
        terminal.write_text("exit\r");
        assert!(wait_for(&terminal, 20, |t| !t.is_running()));
    }
}

#[cfg(unix)]
#[test]
fn colors_and_scrollback() {
    let dir = std::env::temp_dir();
    let script = "printf '\\033[1;31mrot\\033[0m normal\\n'; for i in $(seq 1 40); do echo zeile$i; done; sleep 5";
    let terminal = Terminal::start_command(&dir, "sh", &["-c", script], 10, 40).unwrap();
    assert!(wait_for(&terminal, 20, |t| t.text().contains("zeile40")));
    // „rot“ ist in die Rückschau gerutscht: zurückscrollen.
    terminal.scroll(100);
    let screen = terminal.screen();
    assert!(screen.scrolled > 0);
    assert_eq!(screen.cursor, None);
    let red = screen
        .lines
        .iter()
        .flatten()
        .find(|run| run.text == "rot")
        .expect("rot sichtbar");
    assert_eq!(red.style.fg, TermColor::Idx(1));
    assert!(red.style.bold);
    terminal.write_text("x");
    assert_eq!(terminal.screen().scrolled, 0);
    let copied = terminal.text_between((0, 0), (0, 2));
    assert!(!copied.is_empty());
}
