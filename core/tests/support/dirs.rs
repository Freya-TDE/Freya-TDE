//! Where the backend tests put their demo projects and build folders:
//! below `FREYA_TDE_TEST_DIR` if set (e.g. a separate disk for the large
//! builds), otherwise the temp folder and Cargo's target tmp folder.

use std::path::PathBuf;

fn base() -> Option<PathBuf> {
    std::env::var_os("FREYA_TDE_TEST_DIR")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
}

/// The demo project folder `<name>-<pid>`.
pub fn project(name: &str) -> PathBuf {
    let folder = format!("{name}-{}", std::process::id());
    match base() {
        Some(base) => base.join("projects").join(folder),
        None => std::env::temp_dir().join(folder),
    }
}

/// The Cargo target folder `<name>` (kept between runs).
pub fn target(name: &str) -> PathBuf {
    match base() {
        Some(base) => base.join("targets").join(name),
        None => PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name),
    }
}
