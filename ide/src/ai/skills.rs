//! The skill table of the AI: skills in the open Agent Skills format
//! (`SKILL.md` with `name` and `description` in the front matter).
//!
//! Trusted skills are built into the program ([`SkillTable::builtin`]:
//! `ide/skills`, `RUF/.agents/skills`, `CT-Editor/.agents/skills`) and
//! cannot be changed from outside. Signed skill packs for updates between
//! releases come with the launcher. A project's own skills
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
    /// Built-in skills are trusted; project skills are "eigene, nicht geprüft".
    pub fn trusted(&self) -> bool {
        matches!(self.origin, SkillOrigin::BuiltIn { .. })
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
        .ok_or("Kopf (---) fehlt")?
        .trim_start_matches(['\r', '\n']);
    let end = rest.find("\n---").ok_or("Ende des Kopfs (---) fehlt")?;
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
        return Err(format!(
            "ungültiger Name „{name}“ (Kleinbuchstaben, Ziffern, -; höchstens 64 Zeichen)"
        ));
    }
    if description.is_empty() || description.chars().count() > 1024 {
        return Err("description fehlt oder ist länger als 1024 Zeichen".into());
    }
    Ok((name, description, body.to_owned()))
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
                    detail: format!("eingebauter Skill ({source}): {e}"),
                    path: None,
                }),
            }
        }
        table.skills.sort_by(|a, b| a.name.cmp(&b.name));
        table
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
                    detail: format!(
                        "Projekt-Skill „{name}“ heißt wie ein geprüfter Skill und wird nicht verwendet"
                    ),
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
}
