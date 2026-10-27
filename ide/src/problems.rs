//! Problems (like VS Code's „Problems“): findings of the model, the code
//! generation, the last build and rust-analyzer in one list – serializable,
//! sorted deterministically, with where they lead (file and line or the
//! element in the designer).

use std::path::PathBuf;

use serde::Serialize;
use tdr_core::ComponentPath;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProblemSeverity {
    Error,
    Warning,
    Info,
}

impl ProblemSeverity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Error => "Fehler",
            Self::Warning => "Warnung",
            Self::Info => "Hinweis",
        }
    }
}

impl From<tdr_core::Severity> for ProblemSeverity {
    fn from(severity: tdr_core::Severity) -> Self {
        match severity {
            tdr_core::Severity::Error => Self::Error,
            tdr_core::Severity::Warning => Self::Warning,
        }
    }
}

/// Who found the problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProblemSource {
    /// Form validation and project analysis.
    Model,
    Codegen,
    Build,
    RustAnalyzer,
}

impl ProblemSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Model => "Modell",
            Self::Codegen => "Code",
            Self::Build => "Build",
            Self::RustAnalyzer => "rust-analyzer",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Problem {
    pub source: ProblemSource,
    pub severity: ProblemSeverity,
    pub message: String,
    /// Compiler or lint code, e.g. `E0425`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Absolute path of the file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<PathBuf>,
    /// Line (1-based) and column (0-based, in characters).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
    /// The element concerned (form findings, errors in generated code).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element: Option<ComponentPath>,
}

impl Problem {
    pub fn new(source: ProblemSource, severity: ProblemSeverity, message: &str) -> Self {
        Self {
            source,
            severity,
            message: message.to_owned(),
            code: None,
            file: None,
            line: None,
            column: None,
            element: None,
        }
    }

    /// Where the problem belongs for grouping: the file, else the form of
    /// the element, else the project.
    pub fn group(&self) -> Group {
        match (&self.file, &self.element) {
            (Some(file), _) => Group::File(file.clone()),
            (None, Some(element)) => Group::Form(
                element
                    .form
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default(),
            ),
            (None, None) => Group::Project,
        }
    }
}

/// Groups of the problem list.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    Project,
    Form(String),
    File(PathBuf),
}

/// Sorts by group, then line, column, severity, source and message.
pub fn sort(problems: &mut [Problem]) {
    problems.sort_by(|a, b| {
        (
            a.group(),
            a.line,
            a.column,
            a.severity,
            a.source,
            &a.message,
        )
            .cmp(&(
                b.group(),
                b.line,
                b.column,
                b.severity,
                b.source,
                &b.message,
            ))
    });
}

/// Number of errors, warnings and hints.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
}

pub fn counts(problems: &[Problem]) -> Counts {
    let mut counts = Counts::default();
    for p in problems {
        match p.severity {
            ProblemSeverity::Error => counts.errors += 1,
            ProblemSeverity::Warning => counts.warnings += 1,
            ProblemSeverity::Info => counts.infos += 1,
        }
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorted_and_counted() {
        let at = |file: &str, line: usize, severity| Problem {
            file: Some(PathBuf::from(file)),
            line: Some(line),
            ..Problem::new(ProblemSource::RustAnalyzer, severity, "m")
        };
        let mut problems = vec![
            at("/p/src/b.rs", 3, ProblemSeverity::Warning),
            at("/p/src/a.rs", 9, ProblemSeverity::Error),
            Problem::new(ProblemSource::Model, ProblemSeverity::Error, "Projekt"),
            at("/p/src/a.rs", 2, ProblemSeverity::Info),
        ];
        sort(&mut problems);
        let lines: Vec<Option<usize>> = problems.iter().map(|p| p.line).collect();
        assert_eq!(lines, [None, Some(2), Some(9), Some(3)]);
        assert_eq!(
            counts(&problems),
            Counts {
                errors: 2,
                warnings: 1,
                infos: 1
            }
        );
        let json = serde_json::to_value(&problems[1]).unwrap();
        assert_eq!(json["source"], "rust_analyzer");
        assert_eq!(json["severity"], "info");
    }
}
