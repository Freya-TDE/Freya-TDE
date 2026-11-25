//! The skill table of the AI: skills in the open Agent Skills format
//! (`SKILL.md` with `name` and `description` in the front matter).
//!
//! Trusted skills are built into the program ([`SkillTable::builtin`]:
//! `ide/skills`, `RUF/.agents/skills`, `CT-Editor/.agents/skills`) and
//! cannot be changed from outside. Signed skill packs bring updates between
//! releases ([`installed_pack`]: placed by the launcher in the write-protected
//! installation folder, Ed25519 signature checked against [`SKILL_PACK_KEY`]);
//! a pack skill replaces the built-in one of the same name. A project's own skills
//! (`<project>/.agents/skills/*/SKILL.md`) are used only after the
//! developer approved them; the approval holds the SHA-256 of the content,
//! so a changed skill needs a new approval. Trusted skills win on equal
//! names.
//!
//! The AI first sees only names and descriptions ([`SkillTable::catalog`])
//! and loads a skill's text when it needs it (tool `load_skill`).
//!
//! *Skill-Tabelle der KI: eingebaute (vertrauenswürdige) Skills, eigene
//! Skills eines Projekts nur nach Freigabe mit Prüfsumme.*

use crate::tr;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};

mod builtin {
    include!(concat!(env!("OUT_DIR"), "/skills.rs"));
}

/// Where a skill comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SkillOrigin {
    /// Built into Freya TDE (`source`: Freya TDE, RUF, CT-Editor).
    BuiltIn { source: String },
    /// From a signed skill pack (`version` of the pack).
    Pack { version: String },
    /// The project's own, approved by the developer, not checked.
    Project { path: PathBuf },
}

/// A usable skill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub origin: SkillOrigin,
    /// The instructions (the file without its front matter).
    pub body: String,
    /// SHA-256 of the whole file, hex.
    pub sha256: String,
}

impl Skill {
    /// Built-in and pack skills are trusted; project skills are "eigene,
    /// nicht geprüft".
    pub fn trusted(&self) -> bool {
        matches!(
            self.origin,
            SkillOrigin::BuiltIn { .. } | SkillOrigin::Pack { .. }
        )
    }
}

/// A project skill waiting for approval (new or changed since).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PendingSkill {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
    pub sha256: String,
    /// It was approved before, with other content.
    pub changed: bool,
}

/// Stable codes of skill problems.
pub mod codes {
    /// No or broken front matter.
    pub const SKILL_INVALID: &str = "skill_invalid";
    /// A project skill has the name of a trusted one and is ignored.
    pub const SKILL_SHADOWED: &str = "skill_shadowed";
    /// The file could not be read.
    pub const SKILL_UNREADABLE: &str = "skill_unreadable";
    /// A skill pack is installed, but this build has no key to check it.
    pub const SKILL_PACK_NO_KEY: &str = "skill_pack_no_key";
    /// The signature of the skill pack is missing or wrong.
    pub const SKILL_PACK_SIGNATURE: &str = "skill_pack_signature";
    /// A file of the skill pack is missing, changed or not listed.
    pub const SKILL_PACK_HASH: &str = "skill_pack_hash";
    /// The skill pack needs a newer Freya TDE.
    pub const SKILL_PACK_VERSION: &str = "skill_pack_version";
    /// `pack.json` or a skill in the pack is broken.
    pub const SKILL_PACK_INVALID: &str = "skill_pack_invalid";
}

/// A problem with a skill file (shown under "Probleme").
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillIssue {
    pub code: &'static str,
    pub detail: String,
    pub path: Option<PathBuf>,
}

/// The skills the AI may use.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SkillTable {
    skills: Vec<Skill>,
    pending: Vec<PendingSkill>,
    issues: Vec<SkillIssue>,
}

/// SHA-256 of `text`, hex.
pub fn sha256(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Name, description and body of a `SKILL.md`.
pub fn parse(text: &str) -> Result<(String, String, String), String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let rest = text
        .strip_prefix("---")
        .ok_or_else(|| tr!("skills.no_header"))?
        .trim_start_matches(['\r', '\n']);
    let end = rest
        .find("\n---")
        .ok_or_else(|| tr!("skills.no_header_end"))?;
    let (head, body) = rest.split_at(end);
    let body = body["\n---".len()..].trim_start_matches(['\r', '\n']);
    let mut fields = BTreeMap::new();
    for line in head.lines() {
        if let Some((key, value)) = line.split_once(':')
            && !key.starts_with([' ', '\t'])
        {
            let value = value.trim();
            let value = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
                .unwrap_or(value);
            fields.insert(key.trim().to_owned(), value.to_owned());
        }
    }
    let name = fields.remove("name").unwrap_or_default();
    let description = fields.remove("description").unwrap_or_default();
    let valid_name = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !valid_name {
        return Err(tr!("skills.bad_name", name = name));
    }
    if description.is_empty() || description.chars().count() > 1024 {
        return Err(tr!("skills.bad_description"));
    }
    Ok((name, description, body.to_owned()))
}

/// Public Ed25519 key of the skill packs (`skills.pub`: 307901502adabb564b8fb10555a47a102595e3b0fbf0fe20e9a2b5d6fcc0b759; made
/// with `ds-sign keygen` from DS-Launcher). `None` would refuse every
/// installed pack with [`codes::SKILL_PACK_NO_KEY`].
pub const SKILL_PACK_KEY: Option<[u8; 32]> = Some([
    0x30, 0x79, 0x01, 0x50, 0x2a, 0xda, 0xbb, 0x56, 0x4b, 0x8f, 0xb1, 0x05, 0x55, 0xa4, 0x7a, 0x10,
    0x25, 0x95, 0xe3, 0xb0, 0xfb, 0xf0, 0xfe, 0x20, 0xe9, 0xa2, 0xb5, 0xd6, 0xfc, 0xc0, 0xb7, 0x59,
]);

/// A checked skill pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillPack {
    pub version: String,
    pub dir: PathBuf,
    pub skills: Vec<Skill>,
}

/// `pack.json` of a skill pack.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PackManifest {
    format: u32,
    version: String,
    min_freya: String,
    /// Relative path (`<name>/SKILL.md`) → SHA-256, hex.
    skills: BTreeMap<String, String>,
}

/// Where an installed skill pack is looked for (first existing folder):
/// `FREYA_TDE_SKILL_PACK`, `<exe>/../share/freya-tde/skills` (Linux,
/// Homebrew), `<exe folder>/skills` (Windows).
pub fn pack_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("FREYA_TDE_SKILL_PACK").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    let exe = std::env::current_exe().ok()?;
    let bin = exe.parent()?;
    [
        bin.parent().map(|p| p.join("share/freya-tde/skills")),
        Some(bin.join("skills")),
    ]
    .into_iter()
    .flatten()
    .find(|d| d.join("pack.json").is_file())
}

/// The installed skill pack, checked once per program run: `Ok(None)`
/// without a pack, `Err` with the reason if it was refused.
pub fn installed_pack() -> &'static Result<Option<SkillPack>, SkillIssue> {
    static PACK: std::sync::OnceLock<Result<Option<SkillPack>, SkillIssue>> =
        std::sync::OnceLock::new();
    PACK.get_or_init(|| match pack_dir() {
        None => Ok(None),
        Some(dir) => match SKILL_PACK_KEY {
            None => Err(SkillIssue {
                code: codes::SKILL_PACK_NO_KEY,
                detail: tr!("skills.no_key"),
                path: Some(dir),
            }),
            Some(key) => load_pack(&dir, &key, env!("CARGO_PKG_VERSION")).map(Some),
        },
    })
}

/// Bytes of a hex string.
fn from_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok())
        .collect()
}

/// `a` ≥ `b` for dotted version numbers (`0.2` = `0.2.0`); `None` if one
/// of them is not a version.
fn version_at_least(a: &str, b: &str) -> Option<bool> {
    let parts = |v: &str| {
        v.split('.')
            .map(|p| p.parse::<u64>().ok())
            .collect::<Option<Vec<_>>>()
    };
    let (mut a, mut b) = (parts(a)?, parts(b)?);
    let len = a.len().max(b.len());
    a.resize(len, 0);
    b.resize(len, 0);
    Some(a >= b)
}

/// Checks and loads the skill pack in `dir` against the public `key` for
/// Freya TDE `freya_version`. Any problem refuses the whole pack.
pub fn load_pack(dir: &Path, key: &[u8; 32], freya_version: &str) -> Result<SkillPack, SkillIssue> {
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};
    let issue = |code, detail: String, path: Option<PathBuf>| SkillIssue {
        code,
        detail,
        path: path.or_else(|| Some(dir.to_path_buf())),
    };
    let manifest_path = dir.join("pack.json");
    let manifest = std::fs::read(&manifest_path).map_err(|e| {
        issue(
            codes::SKILL_PACK_INVALID,
            format!("pack.json: {e}"),
            Some(manifest_path.clone()),
        )
    })?;
    // Erst die Signatur, dann der Inhalt: nichts Ungeprüftes wird ausgewertet.
    let signature = std::fs::read_to_string(dir.join("pack.json.sig"))
        .ok()
        .and_then(|s| from_hex(s.trim()))
        .and_then(|b| <[u8; 64]>::try_from(b).ok())
        .map(|b| Signature::from_bytes(&b));
    let key = VerifyingKey::from_bytes(key).map_err(|e| {
        issue(
            codes::SKILL_PACK_SIGNATURE,
            tr!("skills.key_error", error = e),
            None,
        )
    })?;
    let signed = signature.is_some_and(|s| key.verify(&manifest, &s).is_ok());
    if !signed {
        return Err(issue(
            codes::SKILL_PACK_SIGNATURE,
            tr!("skills.bad_signature"),
            Some(dir.join("pack.json.sig")),
        ));
    }
    let manifest: PackManifest = serde_json::from_slice(&manifest).map_err(|e| {
        issue(
            codes::SKILL_PACK_INVALID,
            format!("pack.json: {e}"),
            Some(manifest_path.clone()),
        )
    })?;
    if manifest.format != 1 {
        return Err(issue(
            codes::SKILL_PACK_INVALID,
            format!("unbekanntes Format {} von pack.json", manifest.format),
            Some(manifest_path),
        ));
    }
    match version_at_least(freya_version, &manifest.min_freya) {
        Some(true) => {}
        Some(false) => {
            return Err(issue(
                codes::SKILL_PACK_VERSION,
                format!(
                    "Skill-Paket {} braucht Freya TDE {} (dieses: {freya_version})",
                    manifest.version, manifest.min_freya
                ),
                Some(manifest_path),
            ));
        }
        None => {
            return Err(issue(
                codes::SKILL_PACK_INVALID,
                tr!("skills.bad_version", version = manifest.min_freya),
                Some(manifest_path),
            ));
        }
    }
    // Genau die gelisteten Dateien, jede mit passender Prüfsumme.
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if path.is_dir() {
            let skill = path.join("SKILL.md");
            let rel = format!("{}/SKILL.md", entry.file_name().to_string_lossy());
            let others = std::fs::read_dir(&path)
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .any(|e| e.file_name() != "SKILL.md");
            if others || !skill.is_file() {
                return Err(issue(
                    codes::SKILL_PACK_HASH,
                    tr!("skills.unlisted_in", path = path.display()),
                    Some(path),
                ));
            }
            found.push(rel);
        } else if !matches!(
            entry.file_name().to_str(),
            Some("pack.json" | "pack.json.sig")
        ) {
            return Err(issue(
                codes::SKILL_PACK_HASH,
                tr!("skills.unlisted", path = path.display()),
                Some(path),
            ));
        }
    }
    found.sort();
    let listed: Vec<&String> = manifest.skills.keys().collect();
    if listed
        .iter()
        .map(|s| s.as_str())
        .ne(found.iter().map(String::as_str))
    {
        return Err(issue(
            codes::SKILL_PACK_HASH,
            tr!(
                "skills.mismatch",
                listed = format!("{listed:?}"),
                found = format!("{found:?}")
            ),
            None,
        ));
    }
    let mut skills = Vec::new();
    for (rel, hash) in &manifest.skills {
        let path = dir.join(rel);
        let text = std::fs::read_to_string(&path).map_err(|e| {
            issue(
                codes::SKILL_PACK_HASH,
                format!("{rel}: {e}"),
                Some(path.clone()),
            )
        })?;
        if sha256(&text) != hash.to_ascii_lowercase() {
            return Err(issue(
                codes::SKILL_PACK_HASH,
                tr!("skills.changed", path = rel),
                Some(path),
            ));
        }
        let (name, description, body) = parse(&text).map_err(|e| {
            issue(
                codes::SKILL_PACK_INVALID,
                format!("{rel}: {e}"),
                Some(path.clone()),
            )
        })?;
        skills.push(Skill {
            name,
            description,
            origin: SkillOrigin::Pack {
                version: manifest.version.clone(),
            },
            body,
            sha256: hash.to_ascii_lowercase(),
        });
    }
    Ok(SkillPack {
        version: manifest.version,
        dir: dir.to_path_buf(),
        skills,
    })
}

impl SkillTable {
    /// The built-in skills.
    pub fn builtin() -> Self {
        let mut table = Self::default();
        for (source, text) in builtin::BUILTIN {
            match parse(text) {
                Ok((name, description, body)) => table.skills.push(Skill {
                    name,
                    description,
                    origin: SkillOrigin::BuiltIn {
                        source: (*source).to_owned(),
                    },
                    body,
                    sha256: sha256(text),
                }),
                Err(e) => table.issues.push(SkillIssue {
                    code: codes::SKILL_INVALID,
                    detail: tr!("skills.builtin_invalid", source = source, error = e),
                    path: None,
                }),
            }
        }
        table.skills.sort_by(|a, b| a.name.cmp(&b.name));
        table
    }

    /// The trusted skills: built-in plus the installed skill pack, if it
    /// passed the check (problems: [`installed_pack`]).
    pub fn trusted() -> Self {
        let mut table = Self::builtin();
        if let Ok(Some(pack)) = installed_pack() {
            table.add_pack(pack);
        }
        table
    }

    /// Adds the skills of a checked pack; they replace built-in skills of
    /// the same name.
    pub fn add_pack(&mut self, pack: &SkillPack) {
        for skill in &pack.skills {
            self.skills.retain(|s| s.name != skill.name);
            self.skills.push(skill.clone());
        }
        self.skills.sort_by(|a, b| a.name.cmp(&b.name));
    }

    /// Adds the project's skills from `<project>/.agents/skills`: those in
    /// `approved` (name → SHA-256) with matching content become usable,
    /// the others wait in [`SkillTable::pending`].
    pub fn load_project(&mut self, project_dir: &Path, approved: &BTreeMap<String, String>) {
        let dir = project_dir.join(".agents").join("skills");
        let Ok(read) = std::fs::read_dir(&dir) else {
            return;
        };
        let mut files: Vec<PathBuf> = read
            .filter_map(Result::ok)
            .map(|e| e.path().join("SKILL.md"))
            .filter(|p| p.is_file())
            .collect();
        files.sort();
        for path in files {
            let text = match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(e) => {
                    self.issues.push(SkillIssue {
                        code: codes::SKILL_UNREADABLE,
                        detail: e.to_string(),
                        path: Some(path),
                    });
                    continue;
                }
            };
            let (name, description, body) = match parse(&text) {
                Ok(p) => p,
                Err(e) => {
                    self.issues.push(SkillIssue {
                        code: codes::SKILL_INVALID,
                        detail: e,
                        path: Some(path),
                    });
                    continue;
                }
            };
            if self.skills.iter().any(|s| s.name == name && s.trusted()) {
                self.issues.push(SkillIssue {
                    code: codes::SKILL_SHADOWED,
                    detail: tr!("skills.shadowed", name = name),
                    path: Some(path),
                });
                continue;
            }
            let hash = sha256(&text);
            match approved.get(&name) {
                Some(h) if *h == hash => self.skills.push(Skill {
                    name,
                    description,
                    origin: SkillOrigin::Project { path },
                    body,
                    sha256: hash,
                }),
                other => self.pending.push(PendingSkill {
                    name,
                    description,
                    path,
                    sha256: hash,
                    changed: other.is_some(),
                }),
            }
        }
    }

    pub fn skills(&self) -> &[Skill] {
        &self.skills
    }

    pub fn pending(&self) -> &[PendingSkill] {
        &self.pending
    }

    pub fn issues(&self) -> &[SkillIssue] {
        &self.issues
    }

    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.iter().find(|s| s.name == name)
    }

    /// Names and descriptions for the system prompt.
    pub fn catalog(&self) -> String {
        self.skills
            .iter()
            .map(|s| {
                let mark = if s.trusted() {
                    ""
                } else {
                    " [eigener Skill des Projekts, nicht geprüft]"
                };
                format!("- {}: {}{mark}\n", s.name, s.description)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_skills_are_valid() {
        let t = SkillTable::builtin();
        assert!(t.issues().is_empty(), "{:?}", t.issues());
        for name in ["freya-forms", "freya-project", "ruf-app", "ct-editor"] {
            let s = t.get(name).unwrap_or_else(|| panic!("{name} fehlt"));
            assert!(s.trusted() && !s.body.is_empty() && !s.body.starts_with("---"));
        }
        assert!(t.catalog().contains("- freya-forms: "));
    }

    #[test]
    fn project_skills_need_approval() {
        let dir = std::env::temp_dir().join(format!("freya-skills-{}", std::process::id()));
        let skill = |name: &str, desc: &str| {
            let d = dir.join(".agents/skills").join(name);
            std::fs::create_dir_all(&d).unwrap();
            let text =
                format!("---\nname: {name}\ndescription: \"{desc}\"\n---\n\n# {name}\nTun.\n");
            std::fs::write(d.join("SKILL.md"), &text).unwrap();
            text
        };
        let own = skill("own-style", "Unser Stil");
        skill("freya-forms", "Fremd");
        std::fs::create_dir_all(dir.join(".agents/skills/broken")).unwrap();
        std::fs::write(dir.join(".agents/skills/broken/SKILL.md"), "kein Kopf").unwrap();

        let mut t = SkillTable::builtin();
        t.load_project(&dir, &BTreeMap::new());
        assert!(t.get("own-style").is_none());
        assert_eq!(t.pending()[0].name, "own-style");
        assert!(!t.pending()[0].changed);
        let codes: Vec<_> = t.issues().iter().map(|i| i.code).collect();
        assert!(codes.contains(&codes::SKILL_SHADOWED) && codes.contains(&codes::SKILL_INVALID));
        assert!(t.get("freya-forms").unwrap().trusted());

        // Freigegeben mit passender Prüfsumme.
        let approved = BTreeMap::from([("own-style".to_owned(), sha256(&own))]);
        let mut t = SkillTable::builtin();
        t.load_project(&dir, &approved);
        let s = t.get("own-style").unwrap();
        assert!(!s.trusted());
        assert_eq!(s.description, "Unser Stil");
        assert!(t.catalog().contains("nicht geprüft"));

        // Geändert: wieder ausstehend.
        skill("own-style", "Anders");
        let mut t = SkillTable::builtin();
        t.load_project(&dir, &approved);
        assert!(t.get("own-style").is_none());
        assert!(t.pending()[0].changed);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Writes a skill pack to `dir`, signed with the test key.
    fn write_pack(dir: &Path, skills: &[(&str, &str)], min_freya: &str) -> [u8; 32] {
        use ed25519_dalek::{Signer, SigningKey};
        let _ = std::fs::remove_dir_all(dir);
        let mut listed = BTreeMap::new();
        for (name, desc) in skills {
            let text = format!("---\nname: {name}\ndescription: {desc}\n---\n\n# {name}\n");
            std::fs::create_dir_all(dir.join(name)).unwrap();
            std::fs::write(dir.join(name).join("SKILL.md"), &text).unwrap();
            listed.insert(format!("{name}/SKILL.md"), sha256(&text));
        }
        let manifest = serde_json::to_vec_pretty(&serde_json::json!({
            "format": 1, "version": "2026.11.20", "min_freya": min_freya, "skills": listed,
        }))
        .unwrap();
        let key = SigningKey::from_bytes(&[7; 32]);
        let sig: String = key
            .sign(&manifest)
            .to_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        std::fs::write(dir.join("pack.json"), &manifest).unwrap();
        std::fs::write(dir.join("pack.json.sig"), sig).unwrap();
        key.verifying_key().to_bytes()
    }

    #[test]
    fn built_in_pack_key_is_valid() {
        let key = SKILL_PACK_KEY.expect("Schlüssel der Skill-Pakete");
        assert!(ed25519_dalek::VerifyingKey::from_bytes(&key).is_ok());
    }

    #[test]
    fn signed_pack_replaces_builtin_skills() {
        let dir = std::env::temp_dir().join(format!("freya-pack-ok-{}", std::process::id()));
        let key = write_pack(
            &dir,
            &[("freya-forms", "Neue Fassung"), ("neu", "Ganz neu")],
            "0.1",
        );
        let pack = load_pack(&dir, &key, "0.1.0").unwrap();
        assert_eq!(pack.version, "2026.11.20");
        let mut t = SkillTable::builtin();
        let before = t.skills().len();
        t.add_pack(&pack);
        assert_eq!(t.skills().len(), before + 1);
        let forms = t.get("freya-forms").unwrap();
        assert_eq!(forms.description, "Neue Fassung");
        assert!(forms.trusted());
        assert_eq!(
            forms.origin,
            SkillOrigin::Pack {
                version: "2026.11.20".into()
            }
        );
        assert!(t.get("neu").unwrap().trusted());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn broken_packs_are_refused() {
        let dir = std::env::temp_dir().join(format!("freya-pack-bad-{}", std::process::id()));
        let code = |key: &[u8; 32], version: &str| load_pack(&dir, key, version).unwrap_err().code;

        // Fremder Schlüssel.
        write_pack(&dir, &[("a", "A")], "0.1");
        assert_eq!(
            code(
                &ed25519_dalek::SigningKey::from_bytes(&[8; 32])
                    .verifying_key()
                    .to_bytes(),
                "0.1.0"
            ),
            codes::SKILL_PACK_SIGNATURE
        );
        // Signatur fehlt.
        let key = write_pack(&dir, &[("a", "A")], "0.1");
        std::fs::remove_file(dir.join("pack.json.sig")).unwrap();
        assert_eq!(code(&key, "0.1.0"), codes::SKILL_PACK_SIGNATURE);
        // pack.json nach dem Signieren geändert.
        let key = write_pack(&dir, &[("a", "A")], "0.1");
        let m = std::fs::read_to_string(dir.join("pack.json")).unwrap();
        std::fs::write(dir.join("pack.json"), m.replace("0.1", "0.0")).unwrap();
        assert_eq!(code(&key, "0.1.0"), codes::SKILL_PACK_SIGNATURE);
        // Skill geändert.
        let key = write_pack(&dir, &[("a", "A")], "0.1");
        std::fs::write(
            dir.join("a/SKILL.md"),
            "---\nname: a\ndescription: B\n---\n",
        )
        .unwrap();
        assert_eq!(code(&key, "0.1.0"), codes::SKILL_PACK_HASH);
        // Zusätzliche Datei im Skill-Ordner und im Paket.
        let key = write_pack(&dir, &[("a", "A")], "0.1");
        std::fs::write(dir.join("a/run.sh"), "echo").unwrap();
        assert_eq!(code(&key, "0.1.0"), codes::SKILL_PACK_HASH);
        let key = write_pack(&dir, &[("a", "A")], "0.1");
        std::fs::create_dir_all(dir.join("b")).unwrap();
        std::fs::write(
            dir.join("b/SKILL.md"),
            "---\nname: b\ndescription: B\n---\n",
        )
        .unwrap();
        assert_eq!(code(&key, "0.1.0"), codes::SKILL_PACK_HASH);
        // Gelisteter Skill fehlt.
        let key = write_pack(&dir, &[("a", "A")], "0.1");
        std::fs::remove_dir_all(dir.join("a")).unwrap();
        assert_eq!(code(&key, "0.1.0"), codes::SKILL_PACK_HASH);
        // Zu neu bzw. keine Version.
        let key = write_pack(&dir, &[("a", "A")], "0.2");
        assert_eq!(code(&key, "0.1.9"), codes::SKILL_PACK_VERSION);
        assert!(load_pack(&dir, &key, "0.2.0").is_ok());
        let key = write_pack(&dir, &[("a", "A")], "neu");
        assert_eq!(code(&key, "0.1.0"), codes::SKILL_PACK_INVALID);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
