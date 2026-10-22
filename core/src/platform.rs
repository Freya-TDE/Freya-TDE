//! Target platforms – for platform-specific properties and values.
//!
//! The code generator turns platform differences into `cfg` branches;
//! [`Platform::cfg`] gives the predicate.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A target platform.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Platform {
    Windows,
    Linux,
    MacOs,
    Android,
    Ios,
    /// WebAssembly in the browser.
    Web,
    /// Any other target, given as a Rust `cfg` predicate, e.g.
    /// `target_os = "freebsd"`.
    Other(String),
}

impl Platform {
    /// The `cfg` predicate for generated code.
    pub fn cfg(&self) -> String {
        match self {
            Platform::Windows => r#"target_os = "windows""#.into(),
            Platform::Linux => r#"target_os = "linux""#.into(),
            Platform::MacOs => r#"target_os = "macos""#.into(),
            Platform::Android => r#"target_os = "android""#.into(),
            Platform::Ios => r#"target_os = "ios""#.into(),
            Platform::Web => r#"target_family = "wasm""#.into(),
            Platform::Other(predicate) => predicate.clone(),
        }
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Platform::Windows => f.write_str("Windows"),
            Platform::Linux => f.write_str("Linux"),
            Platform::MacOs => f.write_str("macOS"),
            Platform::Android => f.write_str("Android"),
            Platform::Ios => f.write_str("iOS"),
            Platform::Web => f.write_str("Web"),
            Platform::Other(p) => write!(f, "cfg({p})"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cfg_predicates() {
        assert_eq!(Platform::MacOs.cfg(), r#"target_os = "macos""#);
        assert_eq!(Platform::Other("unix".into()).cfg(), "unix");
    }
}
