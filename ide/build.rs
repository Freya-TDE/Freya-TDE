//! Embeds the built-in AI skills: every `SKILL.md` below
//! `RUF/.agents/skills`, `CT-Editor/.agents/skills`, `ide/skills` and
//! `ide/skills-third-party` (skills of other projects, with their license).
//! They are part of the program and cannot be changed from outside.
//!
//! *Bettet die mitgelieferten Skills der KI ein (Grundausstattung).*

use std::path::{Path, PathBuf};

fn main() {
    windows_icon();
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let sources = [
        ("Freya TDE", manifest.join("skills")),
        ("RUF", manifest.join("../RUF/.agents/skills")),
        ("CT-Editor", manifest.join("../CT-Editor/.agents/skills")),
        // Fremde Skills mit eigener Lizenz (LICENSE.md daneben), z. B. der von freya.
        ("freya (freyaui.dev)", manifest.join("skills-third-party")),
    ];
    let mut entries = Vec::new();
    for (origin, dir) in &sources {
        println!("cargo:rerun-if-changed={}", dir.display());
        let Ok(read) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut skills: Vec<PathBuf> = read
            .filter_map(Result::ok)
            .map(|e| e.path().join("SKILL.md"))
            .filter(|p| p.is_file())
            .collect();
        skills.sort();
        for file in skills {
            println!("cargo:rerun-if-changed={}", file.display());
            entries.push(format!(
                "    ({origin:?}, include_str!({:?})),\n",
                canonical(&file)
            ));
        }
    }
    let code = format!(
        "/// Built-in skills: (origin, content of `SKILL.md`).\npub const BUILTIN: &[(&str, &str)] = &[\n{}];\n",
        entries.concat()
    );
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("skills.rs");
    std::fs::write(out, code).unwrap();
}

fn canonical(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

/// Windows: embeds the program icon in the `.exe` (Explorer, start menu,
/// desktop shortcuts); nothing on other systems.
fn windows_icon() {
    println!("cargo:rerun-if-changed=assets/freya-tde.rc");
    println!("cargo:rerun-if-changed=assets/freya-tde.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("assets/freya-tde.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("Windows-Ressource (Symbol) nicht übersetzbar");
    }
}
