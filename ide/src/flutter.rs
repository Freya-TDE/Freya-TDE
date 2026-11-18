//! Flutter projects (UI backend `flutter`): the Rust side is a library, the
//! program is the Flutter app in `flutter/`. Starting it takes three steps,
//! each a [`Command`]: add the platform folder once (`flutter create`),
//! build the app (`flutter build <os> --debug`) and run the built program
//! in `flutter/` (it loads the Rust library from `../target/debug/`).
//!
//! The program is `flutter` in the `PATH` or `FREYA_TDE_FLUTTER`.

use crate::tools::NoConsole;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Folder of the Flutter app inside the project.
pub const APP_DIR: &str = "flutter";

/// The Flutter app of the project in `project_dir`, if it has one.
pub fn app_dir(project_dir: &Path) -> Option<PathBuf> {
    let dir = project_dir.join(APP_DIR);
    dir.join("pubspec.yaml").is_file().then_some(dir)
}

fn program() -> std::ffi::OsString {
    std::env::var_os("FREYA_TDE_FLUTTER").unwrap_or_else(|| "flutter".into())
}

/// Flutter's name of the system Freya TDE runs on.
pub fn host_platform() -> &'static str {
    match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "macos",
        _ => "linux",
    }
}

/// The app's name from `pubspec.yaml` (`name: …`).
pub fn app_name(app_dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(app_dir.join("pubspec.yaml")).ok()?;
    text.lines()
        .find_map(|l| l.strip_prefix("name:"))
        .map(|n| n.trim().trim_matches(['"', '\'']).to_owned())
        .filter(|n| !n.is_empty())
}

/// The steps to build the app on this system: `flutter create` while the
/// platform folder is missing (it keeps existing files), then the debug build.
pub fn build_commands(app_dir: &Path) -> Vec<Command> {
    let platform = host_platform();
    let mut steps = Vec::new();
    if !app_dir.join(platform).is_dir() {
        let mut create = Command::new(program());
        create
            .no_console()
            .args(["create", &format!("--platforms={platform}"), "."])
            .current_dir(app_dir);
        steps.push(create);
    }
    let mut build = Command::new(program());
    build
        .no_console()
        .args(["build", platform, "--debug"])
        .current_dir(app_dir);
    steps.push(build);
    steps
}

/// The built debug program of the app (after [`build_commands`]).
pub fn executable(app_dir: &Path, name: &str) -> PathBuf {
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        _ => "x64",
    };
    let build = app_dir.join("build");
    match host_platform() {
        "windows" => build.join(format!("windows/{arch}/runner/Debug/{name}.exe")),
        "macos" => build.join(format!(
            "macos/Build/Products/Debug/{name}.app/Contents/MacOS/{name}"
        )),
        _ => build.join(format!("linux/{arch}/debug/bundle/{name}")),
    }
}

/// Runs the built app in `flutter/`, so that it finds the Rust library in
/// `../target/debug/` (or `CARGO_TARGET_DIR`).
pub fn run_command(app_dir: &Path, name: &str) -> Command {
    let mut run = Command::new(executable(app_dir, name));
    run.current_dir(app_dir);
    if let Some(target) = std::env::var_os("CARGO_TARGET_DIR") {
        let mut lib = PathBuf::from(target).join("debug").into_os_string();
        lib.push("/");
        run.env("FRB_DART_LOAD_EXTERNAL_LIBRARY_NATIVE_LIB_DIR", lib);
    }
    run
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_and_paths() {
        let dir = std::env::temp_dir().join(format!("freya-flutter-steps-{}", std::process::id()));
        let app = dir.join(APP_DIR);
        std::fs::create_dir_all(&app).unwrap();
        assert_eq!(app_dir(&dir), None);
        std::fs::write(app.join("pubspec.yaml"), "# x\nname: shop_app\n").unwrap();
        assert_eq!(app_dir(&dir), Some(app.clone()));
        assert_eq!(app_name(&app).as_deref(), Some("shop_app"));

        let args = |c: &Command| -> Vec<String> {
            c.get_args()
                .map(|a| a.to_string_lossy().into_owned())
                .collect()
        };
        let steps = build_commands(&app);
        assert_eq!(steps.len(), 2);
        assert_eq!(args(&steps[0])[0], "create");
        assert_eq!(args(&steps[1]), ["build", host_platform(), "--debug"]);
        // Mit Plattform-Ordner nur noch bauen.
        std::fs::create_dir_all(app.join(host_platform())).unwrap();
        assert_eq!(build_commands(&app).len(), 1);

        let exe = executable(&app, "shop_app");
        assert!(exe.starts_with(app.join("build")));
        assert!(exe.to_string_lossy().contains("shop_app"));
        assert_eq!(
            run_command(&app, "shop_app").get_current_dir(),
            Some(app.as_path())
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
