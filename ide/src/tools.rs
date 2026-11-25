//! The Rust tools of the main window's tool bar, without UI: check, clippy,
//! format, test, update, doc and `sqlx prepare`. Each runs in the project
//! folder and returns a [`ToolReport`]; check and clippy also give Cargo's
//! messages as a [`BuildReport`] (for the problems and the editor marks).
//!
//! Formatting does not use `cargo fmt`: it would follow `mod app` into the
//! generated, write-protected `app.gen.rs`. Instead every user file is piped
//! through `rustfmt` on its own; generated files (`.gen.rs`) stay as they are.
//!
//! *Rust-Werkzeuge der Werkzeugleiste ohne Oberfläche.*

use crate::tr;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use tdr_core::codegen::Generation;
use tdr_core::preview::BuildReport;

/// A tool of the tool bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    Check,
    Clippy,
    Fmt,
    Test,
    Update,
    Doc,
    SqlxPrepare,
}

impl Tool {
    pub const ALL: [Tool; 7] = [
        Tool::Check,
        Tool::Clippy,
        Tool::Fmt,
        Tool::Test,
        Tool::Update,
        Tool::Doc,
        Tool::SqlxPrepare,
    ];

    /// Key of its texts (`tool.<key>.label`, `tool.<key>.tip`).
    fn key(self) -> &'static str {
        match self {
            Tool::Check => "check",
            Tool::Clippy => "clippy",
            Tool::Fmt => "fmt",
            Tool::Test => "test",
            Tool::Update => "update",
            Tool::Doc => "doc",
            Tool::SqlxPrepare => "sqlx",
        }
    }

    /// Text of the button.
    pub fn label(self) -> String {
        crate::tr!(&format!("tool.{}.label", self.key()))
    }

    /// Name of its icon (built-in set, `ide/assets/palette/`).
    pub fn icon(self) -> &'static str {
        match self {
            Tool::Check => "check",
            Tool::Clippy => "clippy",
            Tool::Fmt => "format",
            Tool::Test => "test",
            Tool::Update => "update",
            Tool::Doc => "doc",
            Tool::SqlxPrepare => "sqlx",
        }
    }

    /// What it does (tooltip).
    pub fn description(self) -> String {
        crate::tr!(&format!("tool.{}.tip", self.key()))
    }

    /// The command line shown in the output.
    pub fn command_line(self) -> &'static str {
        match self {
            Tool::Check => "cargo check",
            Tool::Clippy => "cargo clippy",
            Tool::Fmt => "rustfmt",
            Tool::Test => "cargo test",
            Tool::Update => "cargo update",
            Tool::Doc => "cargo doc --no-deps",
            Tool::SqlxPrepare => "cargo sqlx prepare",
        }
    }

    /// Does the tool apply to the project in `dir`? (`sqlx prepare` only
    /// with SQLx in the `Cargo.toml`.)
    pub fn applies(self, dir: &Path) -> bool {
        match self {
            Tool::SqlxPrepare => uses_sqlx(dir),
            _ => dir.join("Cargo.toml").is_file(),
        }
    }
}

/// What a tool did.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ToolReport {
    pub success: bool,
    /// Text for the output channel.
    pub output: String,
    /// Cargo's messages (check, clippy).
    pub build: Option<BuildReport>,
    /// Files rewritten by formatting.
    pub changed: Vec<PathBuf>,
    /// Page to open afterwards (doc).
    pub open: Option<PathBuf>,
}

fn cargo() -> Command {
    let mut command = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    command.no_console();
    command
}

/// Windows: Freya TDE has no console of its own, so every console program
/// it starts (cargo, rustc, rust-analyzer …) would open a console window.
/// `no_console` suppresses it (`CREATE_NO_WINDOW`); a no-op elsewhere.
pub trait NoConsole {
    fn no_console(&mut self) -> &mut Self;
}

impl NoConsole for Command {
    fn no_console(&mut self) -> &mut Self {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            self.creation_flags(0x0800_0000);
        }
        self
    }
}

fn text(output: &std::process::Output) -> String {
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    text
}

/// Runs `tool` in `dir`. `generation` lets messages in generated files name
/// their element; `env` are extra variables (e.g. `DATABASE_URL`).
pub fn run(
    tool: Tool,
    dir: &Path,
    generation: Option<&Generation>,
    env: &[(String, String)],
) -> std::io::Result<ToolReport> {
    match tool {
        Tool::Check | Tool::Clippy => {
            let output = cargo()
                .arg(if tool == Tool::Check {
                    "check"
                } else {
                    "clippy"
                })
                .arg("--message-format=json-diagnostic-rendered-ansi")
                .current_dir(dir)
                .output()?;
            let mut build =
                tdr_core::preview::parse(&String::from_utf8_lossy(&output.stdout), generation);
            build.success &= output.status.success();
            let stderr = String::from_utf8_lossy(&output.stderr);
            Ok(ToolReport {
                success: build.success,
                output: if build.success || !build.messages.is_empty() {
                    String::new()
                } else {
                    stderr.into_owned()
                },
                build: Some(build),
                ..ToolReport::default()
            })
        }
        Tool::Fmt => format_files(dir),
        Tool::Test | Tool::Update => {
            let output = cargo()
                .arg(if tool == Tool::Test { "test" } else { "update" })
                .current_dir(dir)
                .output()?;
            Ok(ToolReport {
                success: output.status.success(),
                output: text(&output),
                ..ToolReport::default()
            })
        }
        Tool::Doc => {
            let output = cargo()
                .args(["doc", "--no-deps"])
                .current_dir(dir)
                .output()?;
            let open = package_name(dir).map(|name| {
                target_dir(dir)
                    .join("doc")
                    .join(name.replace('-', "_"))
                    .join("index.html")
            });
            Ok(ToolReport {
                success: output.status.success(),
                output: text(&output),
                open: open.filter(|p| p.is_file()),
                ..ToolReport::default()
            })
        }
        Tool::SqlxPrepare => {
            let mut command = cargo();
            command.args(["sqlx", "prepare"]).current_dir(dir);
            for (key, value) in env {
                command.env(key, value);
            }
            let output = command.output()?;
            let mut text = text(&output);
            if text.contains("no such command") {
                text.push_str(&tr!("tools.sqlx_cli_missing"));
            }
            Ok(ToolReport {
                success: output.status.success(),
                output: text,
                ..ToolReport::default()
            })
        }
    }
}

/// What `cargo update` would change (`--dry-run`), for the preview.
pub fn update_preview(dir: &Path) -> std::io::Result<String> {
    let output = cargo()
        .args(["update", "--dry-run"])
        .current_dir(dir)
        .output()?;
    Ok(text(&output))
}

/// Formats the user's `.rs` files below `src/` with `rustfmt` (stdin to
/// stdout, so modules are not followed); generated files are left alone.
pub fn format_files(dir: &Path) -> std::io::Result<ToolReport> {
    let edition = edition(dir);
    let mut report = ToolReport {
        success: true,
        ..ToolReport::default()
    };
    let mut files = Vec::new();
    collect_rust(&dir.join("src"), &mut files);
    files.sort();
    let rustfmt = std::env::var_os("RUSTFMT").unwrap_or_else(|| "rustfmt".into());
    for file in files {
        let source = std::fs::read_to_string(&file)?;
        let mut child = Command::new(&rustfmt)
            .no_console()
            .args(["--edition", &edition, "--emit", "stdout"])
            .current_dir(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(source.as_bytes())?;
        }
        let output = child.wait_with_output()?;
        let relative = file
            .strip_prefix(dir)
            .unwrap_or(&file)
            .display()
            .to_string();
        if !output.status.success() {
            report.success = false;
            report.output.push_str(&format!(
                "{relative}: {}\n",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
            continue;
        }
        let formatted = String::from_utf8_lossy(&output.stdout).into_owned();
        if formatted != source {
            std::fs::write(&file, &formatted)?;
            report.output.push_str(&format!("formatiert: {relative}\n"));
            report.changed.push(file);
        }
    }
    if report.changed.is_empty() && report.success {
        report.output.push_str("Alles war schon formatiert.\n");
    }
    Ok(report)
}

fn collect_rust(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rust(&path, files);
        } else if path.extension().is_some_and(|e| e == "rs")
            && !path.to_string_lossy().ends_with(".gen.rs")
        {
            files.push(path);
        }
    }
}

fn manifest(dir: &Path) -> Option<toml::Table> {
    std::fs::read_to_string(dir.join("Cargo.toml"))
        .ok()?
        .parse()
        .ok()
}

fn package_name(dir: &Path) -> Option<String> {
    manifest(dir)?
        .get("package")?
        .get("name")?
        .as_str()
        .map(str::to_owned)
}

/// The project's edition (`[package] edition`, default 2024).
fn edition(dir: &Path) -> String {
    manifest(dir)
        .and_then(|m| {
            m.get("package")?
                .get("edition")?
                .as_str()
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "2024".to_owned())
}

fn target_dir(dir: &Path) -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| dir.join("target"))
}

/// Does the project depend on SQLx?
pub fn uses_sqlx(dir: &Path) -> bool {
    manifest(dir).is_some_and(|m| {
        m.get("dependencies")
            .and_then(|d| d.as_table())
            .is_some_and(|d| d.contains_key("sqlx"))
    })
}

/// Opens a file or address with the system's program (browser).
pub fn open_externally(target: &Path) -> std::io::Result<()> {
    let mut command = if cfg!(target_os = "windows") {
        let mut c = Command::new("cmd");
        c.no_console().args(["/C", "start", ""]);
        c
    } else if cfg!(target_os = "macos") {
        Command::new("open")
    } else {
        Command::new("xdg-open")
    };
    command
        .arg(target)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_leaves_generated_files_alone() {
        let have = Command::new("rustfmt")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success());
        if !have {
            return;
        }
        let dir = std::env::temp_dir().join(format!("freya-tools-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"probe\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nsqlx = \"0.9\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("src/main.rs"),
            "#[path = \"app.gen.rs\"]\nmod app;\nfn main(){let x=1;}\n",
        )
        .unwrap();
        let generated = "pub fn   ugly( ) {}\n";
        std::fs::write(dir.join("src/app.gen.rs"), generated).unwrap();
        let report = format_files(&dir).unwrap();
        assert!(report.success, "{}", report.output);
        assert_eq!(report.changed, [dir.join("src/main.rs")]);
        assert!(
            std::fs::read_to_string(dir.join("src/main.rs"))
                .unwrap()
                .contains("let x = 1;")
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("src/app.gen.rs")).unwrap(),
            generated
        );
        // Schon formatiert: nichts geschrieben.
        assert!(format_files(&dir).unwrap().changed.is_empty());
        assert!(uses_sqlx(&dir) && Tool::SqlxPrepare.applies(&dir));
        assert_eq!(package_name(&dir).as_deref(), Some("probe"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
