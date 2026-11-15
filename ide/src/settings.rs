//! Settings of Freya TDE as a whole, without UI: working directory,
//! language, appearance (also passed to the CT-Editor) and AI profiles.
//!
//! Stored per user in `<config>/freya-tde/settings.toml` (see
//! [`crate::desktop::config_dir`]), readable only by the user. API keys go
//! into the system keyring ([`SystemKeyring`]: Secret Service, macOS
//! Keychain, Windows Credential Manager); only if that fails, into the
//! settings file. `FREYA_TDE_AI_KEY` is used when a profile has no key.
//!
//! *Einstellungen von Freya TDE ohne Oberfläche: Arbeitsverzeichnis, Sprache,
//! Darstellung und KI-Profile; API-Keys im Schlüsselbund des Systems.*
//!
//! ```
//! use freya_tde::settings::{AiProfile, AiProvider, MemoryStore, Settings, Theme};
//!
//! let mut settings = Settings { theme: Theme::Dark, ..Settings::default() };
//! settings.ai.profiles.push(AiProfile::new("Claude", AiProvider::Anthropic));
//! let store = MemoryStore::default();
//! settings.set_key("Claude", "sk-test", &store).unwrap();
//! assert_eq!(settings.key("Claude", &store).as_deref(), Some("sk-test"));
//! assert!(settings.validate().is_empty());
//! ```

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Name of the settings file in the configuration directory.
pub const FILE: &str = "settings.toml";
/// Service name of the keys in the system keyring.
pub const KEYRING_SERVICE: &str = "freya-tde";
/// Environment variable with a key for profiles that have none.
pub const KEY_ENV: &str = "FREYA_TDE_AI_KEY";

/// Language of the interface.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    /// The system's language (`LANG`).
    #[default]
    System,
    German,
    English,
}

impl Language {
    pub const ALL: [Language; 3] = [Language::System, Language::German, Language::English];

    /// The code for `--lang` (`None` = the system's).
    pub fn code(self) -> Option<&'static str> {
        match self {
            Language::System => None,
            Language::German => Some("de"),
            Language::English => Some("en"),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Language::System => "System",
            Language::German => "Deutsch",
            Language::English => "English",
        }
    }
}

/// Light or dark appearance.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    /// Follows the system.
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    pub const ALL: [Theme; 3] = [Theme::System, Theme::Light, Theme::Dark];

    /// The value for the CT-Editor's `--theme`.
    pub fn arg(self) -> &'static str {
        match self {
            Theme::System => "system",
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Theme::System => "System",
            Theme::Light => "Hell",
            Theme::Dark => "Dunkel",
        }
    }
}

/// Who answers the AI requests: cloud services and local models (also in
/// the local network, e.g. `http://192.168.1.20:11434`).
///
/// Each provider speaks one of three protocols ([`AiProvider::protocol`]);
/// most services and all local servers use the OpenAI-compatible one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiProvider {
    /// Anthropic (Claude), `https://api.anthropic.com`.
    Anthropic,
    /// OpenAI, `https://api.openai.com/v1`.
    OpenAi,
    /// Google Gemini, `https://generativelanguage.googleapis.com`.
    Google,
    /// xAI (Grok), `https://api.x.ai/v1`.
    Xai,
    /// Mistral AI, `https://api.mistral.ai/v1`.
    Mistral,
    /// DeepSeek, `https://api.deepseek.com/v1`.
    DeepSeek,
    /// Groq, `https://api.groq.com/openai/v1`.
    Groq,
    /// OpenRouter (many models behind one key), `https://openrouter.ai/api/v1`.
    OpenRouter,
    /// A local model with Ollama, `http://localhost:11434`.
    Ollama,
    /// LM Studio's local server, `http://localhost:1234/v1`.
    LmStudio,
    /// Any service with an OpenAI-compatible interface (llama.cpp server,
    /// vLLM, LocalAI, Jan …); the endpoint (with `/v1`) is required.
    OpenAiCompatible,
}

/// The wire protocol of a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiProtocol {
    /// Anthropic Messages API (`/v1/messages`).
    Anthropic,
    /// OpenAI Chat Completions (`/chat/completions`).
    OpenAi,
    /// Google Gemini (`/v1beta/models/<model>:generateContent`).
    Gemini,
}

impl AiProvider {
    pub const ALL: [AiProvider; 11] = [
        AiProvider::Anthropic,
        AiProvider::OpenAi,
        AiProvider::Google,
        AiProvider::Xai,
        AiProvider::Mistral,
        AiProvider::DeepSeek,
        AiProvider::Groq,
        AiProvider::OpenRouter,
        AiProvider::Ollama,
        AiProvider::LmStudio,
        AiProvider::OpenAiCompatible,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AiProvider::Anthropic => "Anthropic (Claude)",
            AiProvider::OpenAi => "OpenAI",
            AiProvider::Google => "Google (Gemini)",
            AiProvider::Xai => "xAI (Grok)",
            AiProvider::Mistral => "Mistral AI",
            AiProvider::DeepSeek => "DeepSeek",
            AiProvider::Groq => "Groq",
            AiProvider::OpenRouter => "OpenRouter",
            AiProvider::Ollama => "Ollama (lokal/Netzwerk)",
            AiProvider::LmStudio => "LM Studio (lokal/Netzwerk)",
            AiProvider::OpenAiCompatible => "OpenAI-kompatibel (eigener Server)",
        }
    }

    /// The endpoint used when the profile gives none.
    pub fn default_endpoint(self) -> Option<&'static str> {
        match self {
            AiProvider::Anthropic => Some("https://api.anthropic.com"),
            AiProvider::OpenAi => Some("https://api.openai.com/v1"),
            AiProvider::Google => Some("https://generativelanguage.googleapis.com"),
            AiProvider::Xai => Some("https://api.x.ai/v1"),
            AiProvider::Mistral => Some("https://api.mistral.ai/v1"),
            AiProvider::DeepSeek => Some("https://api.deepseek.com/v1"),
            AiProvider::Groq => Some("https://api.groq.com/openai/v1"),
            AiProvider::OpenRouter => Some("https://openrouter.ai/api/v1"),
            AiProvider::Ollama => Some("http://localhost:11434"),
            AiProvider::LmStudio => Some("http://localhost:1234/v1"),
            AiProvider::OpenAiCompatible => None,
        }
    }

    /// Does the service need an API key? (Local servers do not; a key
    /// given anyway is sent.)
    pub fn needs_key(self) -> bool {
        !self.is_local()
    }

    /// Runs on the developer's machine or in the local network.
    pub fn is_local(self) -> bool {
        matches!(
            self,
            AiProvider::Ollama | AiProvider::LmStudio | AiProvider::OpenAiCompatible
        )
    }

    /// The wire protocol.
    pub fn protocol(self) -> AiProtocol {
        match self {
            AiProvider::Anthropic => AiProtocol::Anthropic,
            AiProvider::Google => AiProtocol::Gemini,
            _ => AiProtocol::OpenAi,
        }
    }
}

/// One set of AI access data; several let the developer pick per project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiProfile {
    /// Unique name, e.g. `Claude` or `Ollama lokal`.
    pub name: String,
    pub provider: AiProvider,
    /// Model name, e.g. `claude-opus-5-5`, `gpt-…` or `llama3.2`.
    #[serde(default)]
    pub model: String,
    /// Address of the service (empty = the provider's default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// The key is in the system keyring.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub key_in_keyring: bool,
    /// The key, only if the keyring could not take it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

impl AiProfile {
    pub fn new(name: &str, provider: AiProvider) -> Self {
        Self {
            name: name.to_owned(),
            provider,
            model: String::new(),
            endpoint: None,
            key_in_keyring: false,
            key: None,
        }
    }

    /// The endpoint that applies (own or the provider's default).
    pub fn effective_endpoint(&self) -> Option<&str> {
        self.endpoint
            .as_deref()
            .filter(|e| !e.trim().is_empty())
            .or(self.provider.default_endpoint())
    }

    /// Is a key stored (keyring or file)?
    pub fn has_key(&self) -> bool {
        self.key_in_keyring || self.key.is_some()
    }

    fn account(&self) -> String {
        account(&self.name)
    }
}

fn account(profile: &str) -> String {
    format!("ai:{profile}")
}

/// What the AI may do in a project besides reading (reading the project,
/// code, forms, problems and schema is always allowed).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiPermission {
    /// Only read; no tools that change something or run programs.
    ReadOnly,
    /// Every change and every program run shows a preview first and waits
    /// for "Übernehmen" (default).
    #[default]
    Preview,
    /// Changes and program runs happen without asking.
    Autonomous,
}

impl AiPermission {
    pub const ALL: [AiPermission; 3] = [
        AiPermission::ReadOnly,
        AiPermission::Preview,
        AiPermission::Autonomous,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AiPermission::ReadOnly => "nur lesen",
            AiPermission::Preview => "mit Vorschau",
            AiPermission::Autonomous => "selbstständig",
        }
    }
}

/// AI settings: profiles and when the AI window shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiSettings {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<AiProfile>,
    /// The profile used when a project chooses none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_profile: Option<String>,
    /// Show the AI window when a new project is created.
    #[serde(default = "yes")]
    pub show_on_new_project: bool,
    /// Always show the AI window (once an AI is connected).
    #[serde(default)]
    pub show_always: bool,
    /// Profile chosen for a project (key: the project's `.ptdr`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub projects: BTreeMap<PathBuf, String>,
    /// What the AI may do per project (key: the project's `.ptdr`). Kept
    /// here, per user, not in the project: a cloned repository must not
    /// grant the AI rights.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub permissions: BTreeMap<PathBuf, AiPermission>,
    /// Approved own skills per project (key: the project's `.ptdr`; skill
    /// name → SHA-256 of its `SKILL.md`). A changed skill needs a new
    /// approval.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub skill_approvals: BTreeMap<PathBuf, BTreeMap<String, String>>,
}

fn yes() -> bool {
    true
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            profiles: Vec::new(),
            default_profile: None,
            show_on_new_project: true,
            show_always: false,
            projects: BTreeMap::new(),
            permissions: BTreeMap::new(),
            skill_approvals: BTreeMap::new(),
        }
    }
}

impl AiSettings {
    pub fn profile(&self, name: &str) -> Option<&AiProfile> {
        self.profiles.iter().find(|p| p.name == name)
    }

    /// The profile for a project: its own choice, else the default profile.
    pub fn profile_for(&self, project: Option<&Path>) -> Option<&AiProfile> {
        project
            .and_then(|p| self.projects.get(p))
            .and_then(|name| self.profile(name))
            .or_else(|| {
                self.default_profile
                    .as_deref()
                    .and_then(|n| self.profile(n))
            })
    }

    /// What the AI may do in `project` (default: with preview).
    pub fn permission_for(&self, project: Option<&Path>) -> AiPermission {
        project
            .and_then(|p| self.permissions.get(p))
            .copied()
            .unwrap_or_default()
    }

    /// The approved own skills of `project` (name → SHA-256).
    pub fn approved_skills(&self, project: &Path) -> BTreeMap<String, String> {
        self.skill_approvals
            .get(project)
            .cloned()
            .unwrap_or_default()
    }

    /// Approves (or with `None` withdraws) an own skill of `project`.
    pub fn approve_skill(&mut self, project: &Path, name: &str, sha256: Option<&str>) {
        let entry = self
            .skill_approvals
            .entry(project.to_path_buf())
            .or_default();
        match sha256 {
            Some(hash) => {
                entry.insert(name.to_owned(), hash.to_owned());
            }
            None => {
                entry.remove(name);
            }
        }
        if entry.is_empty() {
            self.skill_approvals.remove(project);
        }
    }

    /// Is an AI connected (a profile that can be used)?
    pub fn connected(&self) -> bool {
        !self.profiles.is_empty()
    }
}

/// All settings of Freya TDE.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// Where file dialogs and new projects start (e.g. `~/DevArea/RUST`);
    /// `None` = the home directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_dir: Option<PathBuf>,
    #[serde(default)]
    pub language: Language,
    /// For Freya TDE and the CT-Editor.
    #[serde(default)]
    pub theme: Theme,
    /// Where the code completion puts new methods.
    #[serde(default)]
    pub method_order: crate::codetools::MethodOrder,
    #[serde(default)]
    pub ai: AiSettings,
}

/// A problem found by [`Settings::validate`], with a stable code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SettingsIssue {
    pub code: &'static str,
    pub detail: String,
}

/// Stable codes of [`SettingsIssue`] and [`SettingsError`].
pub mod codes {
    pub const BASE_DIR_MISSING: &str = "base_dir_missing";
    pub const PROFILE_NAME_EMPTY: &str = "profile_name_empty";
    pub const PROFILE_DUPLICATE: &str = "profile_duplicate";
    pub const PROFILE_UNKNOWN: &str = "profile_unknown";
    pub const ENDPOINT_MISSING: &str = "endpoint_missing";
    pub const KEY_MISSING: &str = "key_missing";
    /// A warning (does not block): the API key would go unencrypted
    /// (`http://`) to another machine.
    pub const INSECURE_KEY_TRANSPORT: &str = crate::ai::codes::INSECURE_KEY_TRANSPORT;
    pub const WRITE_FAILED: &str = "write_failed";
}

/// Why settings could not be saved or a key not be stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[error("{detail}")]
pub struct SettingsError {
    pub code: &'static str,
    pub detail: String,
}

impl Settings {
    /// The settings file (`None` without configuration directory).
    pub fn file() -> Option<PathBuf> {
        crate::desktop::config_dir().map(|d| d.join(FILE))
    }

    /// Reads the settings; a missing or broken file gives the defaults.
    pub fn load() -> Self {
        Self::file()
            .map(|f| Self::load_from(&f))
            .unwrap_or_default()
    }

    pub fn load_from(file: &Path) -> Self {
        std::fs::read_to_string(file)
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Writes the settings (the file is readable only by the user).
    pub fn save(&self) -> Result<PathBuf, SettingsError> {
        let file = Self::file().ok_or_else(|| SettingsError {
            code: codes::WRITE_FAILED,
            detail: "kein Konfigurationsordner".into(),
        })?;
        self.save_to(&file)?;
        Ok(file)
    }

    pub fn save_to(&self, file: &Path) -> Result<(), SettingsError> {
        let fail = |e: &dyn std::fmt::Display| SettingsError {
            code: codes::WRITE_FAILED,
            detail: format!("{}: {e}", file.display()),
        };
        let text = toml::to_string_pretty(self).map_err(|e| fail(&e))?;
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| fail(&e))?;
        }
        std::fs::write(file, text).map_err(|e| fail(&e))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| fail(&e))?;
        }
        Ok(())
    }

    /// Where file dialogs and new projects start: the working directory if
    /// it exists, else the home directory.
    pub fn start_folder(&self) -> Option<PathBuf> {
        self.base_dir
            .clone()
            .filter(|d| d.is_dir())
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
    }

    /// Checks the settings (empty = all right).
    pub fn validate(&self) -> Vec<SettingsIssue> {
        let mut issues = Vec::new();
        let mut issue = |code, detail: String| issues.push(SettingsIssue { code, detail });
        if let Some(dir) = &self.base_dir
            && !dir.is_dir()
        {
            issue(
                codes::BASE_DIR_MISSING,
                format!("Arbeitsverzeichnis fehlt: {}", dir.display()),
            );
        }
        let mut seen = std::collections::BTreeSet::new();
        for p in &self.ai.profiles {
            if p.name.trim().is_empty() {
                issue(codes::PROFILE_NAME_EMPTY, "KI-Profil ohne Namen".into());
            } else if !seen.insert(p.name.as_str()) {
                issue(
                    codes::PROFILE_DUPLICATE,
                    format!("KI-Profil „{}“ gibt es zweimal", p.name),
                );
            }
            if p.effective_endpoint().is_none() {
                issue(
                    codes::ENDPOINT_MISSING,
                    format!("KI-Profil „{}“ braucht eine Adresse", p.name),
                );
            }
            if let Some(endpoint) = p.effective_endpoint()
                && let Some(risk) = crate::ai::key_transport_risk(endpoint, p.has_key())
            {
                issue(
                    codes::INSECURE_KEY_TRANSPORT,
                    format!("KI-Profil „{}“: {}", p.name, risk.detail),
                );
            }
            if p.provider.needs_key() && !p.has_key() && std::env::var_os(KEY_ENV).is_none() {
                issue(
                    codes::KEY_MISSING,
                    format!("KI-Profil „{}“ hat keinen API-Key", p.name),
                );
            }
        }
        let names: Vec<&String> = self
            .ai
            .default_profile
            .iter()
            .chain(self.ai.projects.values())
            .collect();
        for name in names {
            if self.ai.profile(name).is_none() {
                issue(
                    codes::PROFILE_UNKNOWN,
                    format!("unbekanntes KI-Profil „{name}“"),
                );
            }
        }
        issues
    }

    /// Stores the key of profile `profile`: in `store` (the keyring), or
    /// in the settings file if the store refuses. Returns whether it went
    /// into the store. Save the settings afterwards.
    pub fn set_key(
        &mut self,
        profile: &str,
        key: &str,
        store: &dyn SecretStore,
    ) -> Result<bool, SettingsError> {
        let p = self
            .ai
            .profiles
            .iter_mut()
            .find(|p| p.name == profile)
            .ok_or_else(|| SettingsError {
                code: codes::PROFILE_UNKNOWN,
                detail: format!("unbekanntes KI-Profil „{profile}“"),
            })?;
        match store.set(&p.account(), key) {
            Ok(()) => {
                p.key_in_keyring = true;
                p.key = None;
                Ok(true)
            }
            Err(_) => {
                p.key_in_keyring = false;
                p.key = Some(key.to_owned());
                Ok(false)
            }
        }
    }

    /// The key of `profile`: keyring, settings file, else `FREYA_TDE_AI_KEY`.
    pub fn key(&self, profile: &str, store: &dyn SecretStore) -> Option<String> {
        let p = self.ai.profile(profile)?;
        let stored = if p.key_in_keyring {
            store.get(&p.account()).ok().flatten()
        } else {
            p.key.clone()
        };
        stored.or_else(|| std::env::var(KEY_ENV).ok().filter(|k| !k.is_empty()))
    }

    /// Removes the key of `profile` (keyring entry and file value).
    pub fn remove_key(&mut self, profile: &str, store: &dyn SecretStore) {
        if let Some(p) = self.ai.profiles.iter_mut().find(|p| p.name == profile) {
            if p.key_in_keyring {
                let _ = store.delete(&p.account());
            }
            p.key_in_keyring = false;
            p.key = None;
        }
    }

    /// Removes a profile with its key and every reference to it.
    pub fn remove_profile(&mut self, profile: &str, store: &dyn SecretStore) {
        self.remove_key(profile, store);
        self.ai.profiles.retain(|p| p.name != profile);
        if self.ai.default_profile.as_deref() == Some(profile) {
            self.ai.default_profile = None;
        }
        self.ai.projects.retain(|_, name| name != profile);
    }

    /// Renames a profile; its key moves along, references follow.
    pub fn rename_profile(
        &mut self,
        old: &str,
        new: &str,
        store: &dyn SecretStore,
    ) -> Result<(), SettingsError> {
        if old == new {
            return Ok(());
        }
        if self.ai.profile(new).is_some() {
            return Err(SettingsError {
                code: codes::PROFILE_DUPLICATE,
                detail: format!("KI-Profil „{new}“ gibt es schon"),
            });
        }
        let key = self
            .key(old, store)
            .filter(|_| self.ai.profile(old).is_some_and(AiProfile::has_key));
        self.remove_key(old, store);
        let p = self
            .ai
            .profiles
            .iter_mut()
            .find(|p| p.name == old)
            .ok_or_else(|| SettingsError {
                code: codes::PROFILE_UNKNOWN,
                detail: format!("unbekanntes KI-Profil „{old}“"),
            })?;
        p.name = new.to_owned();
        if self.ai.default_profile.as_deref() == Some(old) {
            self.ai.default_profile = Some(new.to_owned());
        }
        for name in self.ai.projects.values_mut() {
            if name == old {
                *name = new.to_owned();
            }
        }
        if let Some(key) = key {
            self.set_key(new, &key, store)?;
        }
        Ok(())
    }
}

/// Changes to the settings that only take effect on [`SettingsEdit::commit`]
/// (dialog with OK/Cancel): the edited settings, profile renames in order
/// and new keys (`None` = remove). Keys touch the keyring only on commit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SettingsEdit {
    pub settings: Settings,
    pub renames: Vec<(String, String)>,
    pub keys: BTreeMap<String, Option<String>>,
}

impl SettingsEdit {
    pub fn new(settings: &Settings) -> Self {
        Self {
            settings: settings.clone(),
            ..Self::default()
        }
    }

    /// Renames a profile in the edit (keys and references follow on commit).
    pub fn rename(&mut self, old: &str, new: &str) -> Result<(), SettingsError> {
        let new = new.trim();
        if new.is_empty() {
            return Err(SettingsError {
                code: codes::PROFILE_NAME_EMPTY,
                detail: "KI-Profil ohne Namen".into(),
            });
        }
        if old == new {
            return Ok(());
        }
        let ai = &mut self.settings.ai;
        if ai.profile(new).is_some() {
            return Err(SettingsError {
                code: codes::PROFILE_DUPLICATE,
                detail: format!("KI-Profil „{new}“ gibt es schon"),
            });
        }
        let p = ai
            .profiles
            .iter_mut()
            .find(|p| p.name == old)
            .ok_or_else(|| SettingsError {
                code: codes::PROFILE_UNKNOWN,
                detail: format!("unbekanntes KI-Profil „{old}“"),
            })?;
        p.name = new.to_owned();
        if ai.default_profile.as_deref() == Some(old) {
            ai.default_profile = Some(new.to_owned());
        }
        for name in ai.projects.values_mut() {
            if name == old {
                *name = new.to_owned();
            }
        }
        if let Some(key) = self.keys.remove(old) {
            self.keys.insert(new.to_owned(), key);
        }
        self.renames.push((old.to_owned(), new.to_owned()));
        Ok(())
    }

    /// The key `profile` has in this edit: a new key typed in, none if it
    /// is to be removed, else the stored one (keyring under the name before
    /// any rename, settings file); `FREYA_TDE_AI_KEY` as fallback.
    pub fn key(&self, profile: &str, store: &dyn SecretStore) -> Option<String> {
        let env = || std::env::var(KEY_ENV).ok().filter(|k| !k.is_empty());
        if let Some(key) = self.keys.get(profile) {
            return key
                .as_deref()
                .map(str::trim)
                .filter(|k| !k.is_empty())
                .map(str::to_owned)
                .or_else(env);
        }
        let p = self.settings.ai.profile(profile)?;
        let mut original = profile.to_owned();
        for (old, new) in self.renames.iter().rev() {
            if *new == original {
                original.clone_from(old);
            }
        }
        let stored = if p.key_in_keyring {
            store.get(&account(&original)).ok().flatten()
        } else {
            p.key.clone()
        };
        stored.or_else(env)
    }

    /// Whether `profile` has a key in this edit (without asking the keyring).
    pub fn has_key(&self, profile: &str) -> bool {
        match self.keys.get(profile) {
            Some(key) => key.as_deref().is_some_and(|k| !k.trim().is_empty()),
            None => self
                .settings
                .ai
                .profile(profile)
                .is_some_and(AiProfile::has_key),
        }
    }

    /// A warning if the key of `profile` would go unencrypted to another
    /// machine (code `insecure_key_transport`).
    pub fn key_warning(&self, profile: &str) -> Option<crate::ai::AiIssue> {
        let p = self.settings.ai.profile(profile)?;
        crate::ai::key_transport_risk(p.effective_endpoint()?, self.has_key(profile))
    }

    /// A client for `profile` as edited, e.g. for "Modelle abrufen" before
    /// "OK".
    pub fn client(
        &self,
        profile: &str,
        store: &dyn SecretStore,
    ) -> Result<crate::ai::Client, crate::ai::AiError> {
        let p = self.settings.ai.profile(profile).ok_or_else(|| {
            crate::ai::AiError::new(
                crate::ai::codes::PROFILE_UNKNOWN,
                format!("unbekanntes KI-Profil „{profile}“"),
            )
        })?;
        crate::ai::Client::new(p, self.key(profile, store))
    }

    /// Removes a profile from the edit (its key goes on commit).
    pub fn remove(&mut self, profile: &str) {
        let ai = &mut self.settings.ai;
        ai.profiles.retain(|p| p.name != profile);
        if ai.default_profile.as_deref() == Some(profile) {
            ai.default_profile = None;
        }
        ai.projects.retain(|_, name| name != profile);
        self.keys.remove(profile);
    }

    /// Applies the edit to `saved`: renames and removes keys in `store`,
    /// stores new keys and returns the new settings (not yet saved).
    pub fn commit(&self, saved: &Settings, store: &dyn SecretStore) -> Settings {
        let mut base = saved.clone();
        for (old, new) in &self.renames {
            if base.ai.profile(old).is_some() {
                let _ = base.rename_profile(old, new, store);
            }
        }
        let gone: Vec<String> = base
            .ai
            .profiles
            .iter()
            .filter(|p| self.settings.ai.profile(&p.name).is_none())
            .map(|p| p.name.clone())
            .collect();
        for name in gone {
            base.remove_profile(&name, store);
        }
        let mut result = self.settings.clone();
        for p in &mut result.ai.profiles {
            match base.ai.profile(&p.name) {
                Some(old) => {
                    p.key_in_keyring = old.key_in_keyring;
                    p.key = old.key.clone();
                }
                None => {
                    p.key_in_keyring = false;
                    p.key = None;
                }
            }
        }
        for (name, key) in &self.keys {
            match key.as_deref().map(str::trim).filter(|k| !k.is_empty()) {
                Some(key) => {
                    let _ = result.set_key(name, key, store);
                }
                None => result.remove_key(name, store),
            }
        }
        result
    }
}

/// Where API keys are kept.
pub trait SecretStore {
    fn set(&self, account: &str, secret: &str) -> Result<(), String>;
    fn get(&self, account: &str) -> Result<Option<String>, String>;
    fn delete(&self, account: &str) -> Result<(), String>;
}

/// The system keyring (Secret Service, macOS Keychain, Windows Credential
/// Manager), service [`KEYRING_SERVICE`].
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemKeyring;

impl SecretStore for SystemKeyring {
    fn set(&self, account: &str, secret: &str) -> Result<(), String> {
        keyring::Entry::new(KEYRING_SERVICE, account)
            .and_then(|e| e.set_password(secret))
            .map_err(|e| e.to_string())
    }

    fn get(&self, account: &str) -> Result<Option<String>, String> {
        match keyring::Entry::new(KEYRING_SERVICE, account).and_then(|e| e.get_password()) {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        match keyring::Entry::new(KEYRING_SERVICE, account).and_then(|e| e.delete_credential()) {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// Keys in memory (tests, automation without keyring).
#[derive(Debug, Default)]
pub struct MemoryStore {
    secrets: RefCell<BTreeMap<String, String>>,
    /// Refuse every write (like a missing keyring).
    pub refuse: bool,
}

impl SecretStore for MemoryStore {
    fn set(&self, account: &str, secret: &str) -> Result<(), String> {
        if self.refuse {
            return Err("kein Schlüsselbund".into());
        }
        self.secrets
            .borrow_mut()
            .insert(account.to_owned(), secret.to_owned());
        Ok(())
    }

    fn get(&self, account: &str) -> Result<Option<String>, String> {
        Ok(self.secrets.borrow().get(account).cloned())
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        self.secrets.borrow_mut().remove(account);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("freya-settings-{name}-{}", std::process::id()))
    }

    #[test]
    fn round_trip_and_private_file() {
        let file = temp("rt").join(FILE);
        let mut s = Settings {
            base_dir: Some(std::env::temp_dir()),
            language: Language::English,
            theme: Theme::Dark,
            ..Settings::default()
        };
        s.ai.profiles
            .push(AiProfile::new("Ollama", AiProvider::Ollama));
        s.ai.default_profile = Some("Ollama".into());
        s.save_to(&file).unwrap();
        assert_eq!(Settings::load_from(&file), s);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&file).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        // Fehlt oder ist kaputt: Vorgaben.
        std::fs::write(&file, "language = [").unwrap();
        assert_eq!(Settings::load_from(&file), Settings::default());
        assert!(Settings::default().ai.show_on_new_project);
        let _ = std::fs::remove_dir_all(file.parent().unwrap());
    }

    #[test]
    fn keys_go_to_the_keyring_else_to_the_file() {
        let mut s = Settings::default();
        s.ai.profiles
            .push(AiProfile::new("Claude", AiProvider::Anthropic));
        let keyring = MemoryStore::default();
        assert!(s.set_key("Claude", "k1", &keyring).unwrap());
        assert!(s.ai.profiles[0].key_in_keyring && s.ai.profiles[0].key.is_none());
        assert_eq!(s.key("Claude", &keyring).as_deref(), Some("k1"));
        // Ändern
        s.set_key("Claude", "k2", &keyring).unwrap();
        assert_eq!(s.key("Claude", &keyring).as_deref(), Some("k2"));
        // Ohne Schlüsselbund: in die Datei.
        let refusing = MemoryStore {
            refuse: true,
            ..MemoryStore::default()
        };
        assert!(!s.set_key("Claude", "k3", &refusing).unwrap());
        assert_eq!(s.ai.profiles[0].key.as_deref(), Some("k3"));
        assert_eq!(s.key("Claude", &refusing).as_deref(), Some("k3"));
        // Umbenennen nimmt den Key mit, Entfernen löscht ihn.
        s.set_key("Claude", "k4", &keyring).unwrap();
        s.ai.default_profile = Some("Claude".into());
        s.ai.projects.insert("/p/x.ptdr".into(), "Claude".into());
        s.rename_profile("Claude", "Claude Opus", &keyring).unwrap();
        assert_eq!(s.key("Claude Opus", &keyring).as_deref(), Some("k4"));
        assert_eq!(keyring.get("ai:Claude").unwrap(), None);
        assert_eq!(s.ai.default_profile.as_deref(), Some("Claude Opus"));
        assert_eq!(
            s.ai.profile_for(Some(Path::new("/p/x.ptdr"))).unwrap().name,
            "Claude Opus"
        );
        s.remove_profile("Claude Opus", &keyring);
        assert!(s.ai.profiles.is_empty() && s.ai.default_profile.is_none());
        assert!(s.ai.projects.is_empty());
        assert_eq!(keyring.get("ai:Claude Opus").unwrap(), None);
    }

    #[test]
    fn edit_touches_keys_only_on_commit() {
        let store = MemoryStore::default();
        let mut saved = Settings::default();
        saved
            .ai
            .profiles
            .push(AiProfile::new("A", AiProvider::Anthropic));
        saved
            .ai
            .profiles
            .push(AiProfile::new("B", AiProvider::OpenAi));
        saved.set_key("A", "ka", &store).unwrap();
        saved.set_key("B", "kb", &store).unwrap();
        saved.ai.default_profile = Some("A".into());

        let mut edit = SettingsEdit::new(&saved);
        edit.rename("A", "A2").unwrap();
        assert_eq!(
            edit.rename("B", "A2").unwrap_err().code,
            codes::PROFILE_DUPLICATE
        );
        assert_eq!(
            edit.rename("B", " ").unwrap_err().code,
            codes::PROFILE_NAME_EMPTY
        );
        edit.remove("B");
        edit.settings
            .ai
            .profiles
            .push(AiProfile::new("C", AiProvider::Ollama));
        edit.keys.insert("C".into(), Some("kc".into()));
        // Abbrechen = nichts committen: Schlüsselbund unverändert.
        assert_eq!(store.get("ai:A").unwrap().as_deref(), Some("ka"));
        assert_eq!(store.get("ai:C").unwrap(), None);
        // Vor „OK“: der Key folgt der Umbenennung, neue Keys gelten sofort.
        assert_eq!(edit.key("A2", &store).as_deref(), Some("ka"));
        assert_eq!(edit.key("C", &store).as_deref(), Some("kc"));
        assert!(edit.has_key("C"));
        assert_eq!(edit.key_warning("C"), None, "Ollama auf localhost");
        edit.settings.ai.profiles[1].endpoint = Some("http://192.168.1.20:11434".into());
        assert_eq!(
            edit.key_warning("C").unwrap().code,
            codes::INSECURE_KEY_TRANSPORT
        );
        assert_eq!(edit.client("C", &store).unwrap().model(), "");
        assert_eq!(
            edit.client("X", &store).unwrap_err().code,
            crate::ai::codes::PROFILE_UNKNOWN
        );

        let s = edit.commit(&saved, &store);
        assert_eq!(s.key("A2", &store).as_deref(), Some("ka"));
        assert_eq!(s.key("C", &store).as_deref(), Some("kc"));
        assert_eq!(store.get("ai:A").unwrap(), None);
        assert_eq!(store.get("ai:B").unwrap(), None);
        assert_eq!(s.ai.default_profile.as_deref(), Some("A2"));

        // Key entfernen
        let mut edit = SettingsEdit::new(&s);
        edit.keys.insert("A2".into(), None);
        let s = edit.commit(&s, &store);
        assert!(!s.ai.profile("A2").unwrap().has_key());
        assert_eq!(store.get("ai:A2").unwrap(), None);
    }

    #[test]
    fn validation_codes() {
        let mut s = Settings {
            base_dir: Some(PathBuf::from("/gibt/es/nicht")),
            ..Settings::default()
        };
        s.ai.profiles
            .push(AiProfile::new("A", AiProvider::OpenAiCompatible));
        s.ai.profiles.push(AiProfile::new("A", AiProvider::Ollama));
        s.ai.default_profile = Some("B".into());
        let codes: Vec<&str> = s.validate().iter().map(|i| i.code).collect();
        for code in [
            codes::BASE_DIR_MISSING,
            codes::ENDPOINT_MISSING,
            codes::PROFILE_DUPLICATE,
            codes::PROFILE_UNKNOWN,
        ] {
            assert!(codes.contains(&code), "{code}: {codes:?}");
        }
        let project = Some(Path::new("/p/y.ptdr"));
        s.ai.default_profile = Some("A".into());
        assert_eq!(s.ai.profile_for(project).unwrap().name, "A");
        assert_eq!(Language::German.code(), Some("de"));
        assert_eq!(Theme::Light.arg(), "light");
    }
}
