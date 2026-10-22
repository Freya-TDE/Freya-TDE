//! [`ProjectFile`]: the project file (`name.ptdr`, RON).
//!
//! It names the crate, fixes the Rust version, the one UI backend and the
//! component packs in use, and lists the forms (`.ftdr` files, each next to
//! its `.rs` file).

use serde::{Deserialize, Serialize};

use crate::error::ModelError;

/// A Freya-TD project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectFile {
    /// File format version of the `.ptdr` file.
    #[serde(default = "current_format")]
    pub format: u32,
    /// Cargo package name of the generated crate.
    pub name: String,
    /// Rust version the project is built with (`rust-version` in
    /// `Cargo.toml`), e.g. `"1.99"`.
    pub rust_version: String,
    /// The project's UI backend (e.g. `"egui"`); exactly one per project.
    pub backend: String,
    /// Names of the component packs in use (e.g. `["tokio", "sqlx"]`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub packs: Vec<String>,
    /// The forms and data modules, as paths relative to the project
    /// directory (e.g. `"src/customers.ftdr"`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub forms: Vec<String>,
}

fn current_format() -> u32 {
    ProjectFile::FORMAT
}

/// Is `name` usable as a Cargo package name?
pub fn is_package_name(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_alphabetic())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

impl ProjectFile {
    /// Current `.ptdr` format version.
    pub const FORMAT: u32 = 1;
    /// File extension of project files.
    pub const EXTENSION: &str = "ptdr";

    pub fn new(
        name: impl Into<String>,
        rust_version: impl Into<String>,
        backend: impl Into<String>,
    ) -> Result<Self, ModelError> {
        let name = name.into();
        if !is_package_name(&name) {
            return Err(ModelError::InvalidPackageName(name));
        }
        Ok(Self {
            format: Self::FORMAT,
            name,
            rust_version: rust_version.into(),
            backend: backend.into(),
            packs: Vec::new(),
            forms: Vec::new(),
        })
    }

    /// The project as `.ptdr` text (pretty-printed RON).
    pub fn to_ron(&self) -> Result<String, ModelError> {
        let config = ron::ser::PrettyConfig::new().indentor("    ".to_owned());
        ron::ser::to_string_pretty(self, config).map_err(|e| ModelError::Ron(e.to_string()))
    }

    /// Reads `.ptdr` text.
    pub fn from_ron(text: &str) -> Result<Self, ModelError> {
        let project: Self = ron::from_str(text).map_err(|e| ModelError::Ron(e.to_string()))?;
        if project.format > Self::FORMAT {
            return Err(ModelError::UnsupportedFormat(project.format));
        }
        if !is_package_name(&project.name) {
            return Err(ModelError::InvalidPackageName(project.name));
        }
        Ok(project)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_names() {
        let mut project = ProjectFile::new("crm-app", "1.99", "egui").unwrap();
        project.packs = vec!["tokio".into(), "sqlx".into()];
        project.forms = vec!["src/services.ftdr".into()];
        let text = project.to_ron().unwrap();
        assert_eq!(ProjectFile::from_ron(&text).unwrap(), project);
        assert!(ProjectFile::new("2fast", "1.99", "egui").is_err());
        assert!(ProjectFile::new("my app", "1.99", "egui").is_err());
    }
}
