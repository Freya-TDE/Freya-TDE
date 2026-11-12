//! Code completion with a real rust-analyzer (no UI): method body, own
//! field, trait members – the code actions go through `codetools::plan`.
//! Skipped without `rust-analyzer` in `PATH` (or `RUST_ANALYZER`).

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use freya_tde::codetools::{Completion, MethodOrder, apply, plan};
use freya_tde::lsp::{LspClient, LspEvent};

const MAIN: &str = r#"include!("main_form.gen.rs");

struct Kunde {
    name: String,
}

impl MainForm {
    pub fn ok_clicked(&mut self) {
        let kunde = Kunde { name: String::new() };
        self.speichern(&kunde, 3);
        self.zaehler += 1;
    }
}

trait Pruefen {
    fn gueltig(&self) -> bool;
}

impl Pruefen for Kunde {
}

fn main() {
    let _ = Kunde { name: String::new() }.name;
}
"#;

fn project() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("freya-codetools-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"codetools_probe\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .unwrap();
    std::fs::write(dir.join("src/main.rs"), MAIN).unwrap();
    std::fs::write(
        dir.join("src/main_form.gen.rs"),
        "pub struct MainForm {\n    pub title: String,\n}\n",
    )
    .unwrap();
    dir
}

fn wait(
    events: &mpsc::Receiver<LspEvent>,
    seconds: u64,
    mut f: impl FnMut(LspEvent) -> bool,
) -> bool {
    let end = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < end {
        if let Ok(event) = events.recv_timeout(Duration::from_millis(200))
            && f(event)
        {
            return true;
        }
    }
    false
}

fn position(text: &str, needle: &str) -> (usize, usize) {
    let (line, content) = text
        .lines()
        .enumerate()
        .find(|(_, l)| l.contains(needle))
        .unwrap();
    (line + 1, content.find(needle).unwrap() + 1)
}

#[test]
fn completes_method_field_and_trait() {
    let have = std::process::Command::new(freya_tde::lsp::program())
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !have {
        eprintln!("rust-analyzer fehlt – übersprungen");
        return;
    }
    let dir = project();
    let main = dir.join("src/main.rs");
    let (tx, events) = mpsc::channel();
    let client = LspClient::start(&dir, move |e| {
        let _ = tx.send(e);
    })
    .unwrap();
    assert!(
        wait(&events, 60, |e| matches!(e, LspEvent::Ready)),
        "no Ready"
    );
    client.open(&main, MAIN);
    assert!(
        wait(&events, 240, |e| matches!(
            e,
            LspEvent::Status {
                quiescent: true,
                ..
            }
        )),
        "rust-analyzer did not finish indexing"
    );
    let ask = |needle: &str| -> Result<Completion, String> {
        let (line, column) = position(MAIN, needle);
        let id = client.code_actions(&main, line, column);
        let mut actions = None;
        wait(&events, 60, |e| match e {
            LspEvent::CodeActions {
                request,
                actions: a,
            } if request == id => {
                actions = Some(a);
                true
            }
            _ => false,
        });
        let actions = actions.ok_or("no answer")?;
        plan(
            &main,
            MAIN,
            line,
            column,
            &actions,
            MethodOrder::AfterCaller,
        )
        .map_err(|e| format!("{e:?} {actions:#?}"))
    };
    let path: &Path = &main;

    // Methode: hinter ok_clicked, mit &mut self und leerem Rumpf.
    let Completion::Method { name, edit } = ask("speichern").unwrap() else {
        panic!("method")
    };
    assert_eq!(name, "speichern");
    assert_eq!(edit.path, path);
    let text = apply(MAIN, &[edit]);
    let flat: String = text.split_whitespace().collect();
    assert!(
        flat.contains("fnspeichern(&mutself,kunde:&Kunde,arg:i32){}"),
        "{text}"
    );
    assert!(syn::parse_file(&text).is_ok(), "{text}");

    // Feld: ins Formular (state), nicht ins generierte Struct.
    assert_eq!(
        ask("zaehler").unwrap(),
        Completion::Field {
            name: "zaehler".into(),
            rust_type: "i32".into()
        }
    );

    // Trait-Methoden.
    let Completion::TraitMembers { edits } = ask("Pruefen for").unwrap() else {
        panic!("trait")
    };
    let text = apply(MAIN, &edits);
    assert!(text.contains("fn gueltig(&self) -> bool"), "{text}");

    client.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}
