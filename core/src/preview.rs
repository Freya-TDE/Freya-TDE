//! Preview: the generated project, built and run in debug mode.
//!
//! There is no live preview host – the designer draws the forms itself, and
//! the real look comes from the real program. [`build`] runs
//! `cargo build --message-format=json` in the project directory and reads
//! Cargo's messages (JSON lines on stdout). Compiler messages in generated
//! files are traced back to the element that produced the code
//! ([`crate::codegen::Generation::source_map`]), so the designer can select
//! it. [`BuildReport::executable`] is the debug binary to start.
//!
//! [`command`] and [`parse`] are separate, so a tool can run Cargo itself
//! (e.g. to stream progress) and still get the same report.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

use crate::codegen::Generation;
use crate::name::ComponentPath;
use crate::validate::Severity;

/// One compiler message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BuildMessage {
    pub severity: Severity,
    pub message: String,
    /// Compiler error code, e.g. `E0425`.
    pub code: Option<String>,
    /// File relative to the project directory, `/`-separated.
    pub file: Option<String>,
    pub line: Option<usize>,
    pub column: Option<usize>,
    /// The element whose generated code is concerned.
    pub element: Option<ComponentPath>,
    /// The full message as the compiler prints it.
    pub rendered: Option<String>,
}

/// The result of a debug build.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct BuildReport {
    pub success: bool,
    pub messages: Vec<BuildMessage>,
    /// The debug binary, if one was built.
    pub executable: Option<PathBuf>,
}

impl BuildReport {
    pub fn errors(&self) -> impl Iterator<Item = &BuildMessage> {
        self.messages
            .iter()
            .filter(|m| m.severity == Severity::Error)
    }
}

/// The Cargo command for a debug build of the project in `dir`, reporting
/// in JSON.
pub fn command(dir: &Path) -> Command {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut command = Command::new(cargo);
    command
        .arg("build")
        .arg("--message-format=json-diagnostic-rendered-ansi")
        .current_dir(dir);
    command
}

/// Reads Cargo's JSON messages. With the [`Generation`] the project was
/// generated from, messages in generated files name their element.
pub fn parse(output: &str, generation: Option<&Generation>) -> BuildReport {
    let mut report = BuildReport::default();
    for line in output.lines() {
        let Ok(message) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        match message["reason"].as_str() {
            Some("compiler-message") => {
                if let Some(m) = compiler_message(&message["message"], generation) {
                    report.messages.push(m);
                }
            }
            Some("compiler-artifact") => {
                if let Some(exe) = message["executable"].as_str() {
                    report.executable = Some(PathBuf::from(exe));
                }
            }
            Some("build-finished") => {
                report.success = message["success"].as_bool().unwrap_or(false);
            }
            _ => {}
        }
    }
    report
}

fn compiler_message(
    message: &serde_json::Value,
    generation: Option<&Generation>,
) -> Option<BuildMessage> {
    let severity = match message["level"].as_str()? {
        "error" | "error: internal compiler error" => Severity::Error,
        "warning" => Severity::Warning,
        _ => return None,
    };
    let text = message["message"].as_str()?.to_owned();
    let spans = message["spans"].as_array();
    let primary = spans.and_then(|spans| {
        spans
            .iter()
            .find(|s| s["is_primary"].as_bool() == Some(true))
            .or_else(|| spans.first())
    });
    if primary.is_none() && text.starts_with("aborting due to") {
        return None;
    }
    let file = primary
        .and_then(|s| s["file_name"].as_str())
        .map(|f| f.replace('\\', "/"));
    let line = primary
        .and_then(|s| s["line_start"].as_u64())
        .and_then(|l| usize::try_from(l).ok());
    let column = primary
        .and_then(|s| s["column_start"].as_u64())
        .and_then(|c| usize::try_from(c).ok());
    let element = match (generation, &file, line) {
        (Some(g), Some(file), Some(line)) => g.element_at(file, line).cloned(),
        _ => None,
    };
    Some(BuildMessage {
        severity,
        message: text,
        code: message["code"]["code"].as_str().map(str::to_owned),
        file,
        line,
        column,
        element,
        rendered: message["rendered"].as_str().map(str::to_owned),
    })
}

/// Builds the project in `dir` in debug mode and reports the result.
/// Fails only if Cargo cannot be started.
pub fn build(dir: &Path, generation: Option<&Generation>) -> std::io::Result<BuildReport> {
    let output = command(dir).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut report = parse(&stdout, generation);
    // Cargo errors before compiling (bad manifest …) arrive on stderr only.
    if !output.status.success() && report.messages.is_empty() {
        report.messages.push(BuildMessage {
            severity: Severity::Error,
            message: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            code: None,
            file: None,
            line: None,
            column: None,
            element: None,
            rendered: None,
        });
    }
    report.success &= output.status.success();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cargo_messages() {
        let output = r#"{"reason":"compiler-message","package_id":"x","message":{"level":"error","message":"cannot find function `nope`","code":{"code":"E0425"},"spans":[{"file_name":"src/app.gen.rs","line_start":12,"column_start":9,"is_primary":true}],"rendered":"error[E0425]: …"}}
{"reason":"compiler-message","package_id":"x","message":{"level":"error","message":"aborting due to 1 previous error","code":null,"spans":[]}}
not json
{"reason":"build-finished","success":false}"#;
        let report = parse(output, None);
        assert!(!report.success);
        assert_eq!(report.messages.len(), 1);
        let m = &report.messages[0];
        assert_eq!(m.code.as_deref(), Some("E0425"));
        assert_eq!(
            (m.file.as_deref(), m.line, m.column),
            (Some("src/app.gen.rs"), Some(12), Some(9))
        );
    }
}
