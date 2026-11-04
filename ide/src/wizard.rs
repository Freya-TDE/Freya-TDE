//! The new-project wizard without UI: what to create ([`NewProject`],
//! serializable), a preview with checks ([`plan`]) and the creation
//! ([`create`]).
//!
//! A project is a program (forms with a UI backend) or a library (data
//! modules only, `src/lib.rs`). It is saved to `<parent>/<name>/` right
//! away, its code is generated (it builds at once), and optionally a Git
//! repository is created with a first commit. Starters place components in
//! the data module `services` (database, Redis, web server); the Tokio
//! runtime is added when a starter needs it.
//!
//! *Neu-Assistent ohne Oberfläche: Beschreibung, Vorschau mit Prüfungen,
//! Anlegen.*

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tdr_core::schema::Driver;
use tdr_core::{
    ComponentName, Platform, ProjectFile, ProjectKind, PropertyValue, TypeRef, TypeRegistry, Value,
    is_package_name,
};
use tdr_designer::{Command, Document};

/// Name of the first form of a program.
pub const FIRST_FORM: &str = "main_form";
/// Name of the data module the starters go into.
pub const SERVICES: &str = "services";
/// Backend name for "no UI".
pub const NO_BACKEND: &str = "none";

/// What the wizard creates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewProject {
    /// Cargo package name, also the folder name.
    pub name: String,
    /// The folder the project folder is created in.
    pub parent: PathBuf,
    pub kind: ProjectKind,
    /// Target systems (empty = the developer's system).
    #[serde(default)]
    pub platforms: Vec<Platform>,
    /// UI backend of a program (`gtk4`, `egui`, `iced`, `slint` …, or
    /// `none`); libraries have none.
    pub backend: String,
    /// `rust-version` of the crate, e.g. `1.99.1`.
    pub rust_version: String,
    /// Components placed in the data module `services`.
    #[serde(default)]
    pub starters: Vec<Starter>,
    /// Create a Git repository (`.gitignore` with `/target`).
    #[serde(default)]
    pub git: bool,
    /// … and commit the new project.
    #[serde(default)]
    pub initial_commit: bool,
    /// The developer agreed to the backend's license notice (Slint).
    #[serde(default)]
    pub license_accepted: bool,
}

/// Ready-made components for the data module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Starter {
    /// SQLx pool `db` (PostgreSQL, MySQL, SQLite).
    Database { driver: Driver, url: String },
    /// Redis client `cache`.
    Redis { url: String },
    /// Axum router `api` (HTTP).
    WebServer,
    /// Only the Tokio runtime (for own async code).
    Runtime,
}

impl Starter {
    /// (pack, type, component name).
    fn component(&self) -> (&'static str, &'static str, &'static str) {
        match self {
            Starter::Database { .. } => ("sqlx", "Pool", "db"),
            Starter::Redis { .. } => ("redis", "Client", "cache"),
            Starter::WebServer => ("axum", "Router", "api"),
            Starter::Runtime => ("tokio", "Runtime", "runtime"),
        }
    }

    fn properties(&self) -> Vec<(&'static str, Value)> {
        match self {
            Starter::Database { driver, url } => vec![
                ("driver", Value::Enum(driver.name().to_owned())),
                ("url", Value::Text(url.clone())),
            ],
            Starter::Redis { url } => vec![("url", Value::Text(url.clone()))],
            _ => Vec::new(),
        }
    }
}

/// A finding of the checks (stable `code`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WizardIssue {
    pub code: String,
    pub detail: String,
}

impl WizardIssue {
    fn new(code: &str, detail: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            detail: detail.into(),
        }
    }
}

/// Stable codes of [`WizardIssue`].
pub mod codes {
    /// Not a valid Cargo package name.
    pub const INVALID_NAME: &str = "invalid_name";
    /// The parent folder does not exist.
    pub const NO_PARENT: &str = "no_parent";
    /// The project folder exists and is not empty.
    pub const FOLDER_NOT_EMPTY: &str = "folder_not_empty";
    /// The UI backend is not installed.
    pub const UNKNOWN_BACKEND: &str = "unknown_backend";
    /// A library with a UI backend.
    pub const LIBRARY_WITH_BACKEND: &str = "library_with_backend";
    /// The backend has a license notice that was not accepted.
    pub const LICENSE_NOT_ACCEPTED: &str = "license_not_accepted";
    /// The pack a starter needs is not installed.
    pub const MISSING_PACK: &str = "missing_pack";
    /// A starter is missing its URL.
    pub const NO_URL: &str = "no_url";
    /// Not a Rust version like `1.99` or `1.99.1`.
    pub const INVALID_RUST_VERSION: &str = "invalid_rust_version";
    /// A starter was chosen twice.
    pub const DUPLICATE_STARTER: &str = "duplicate_starter";
    /// Creating failed (file system, Git).
    pub const FAILED: &str = "failed";
}

/// The preview of [`plan`]: where, what, and the problems (errors block
/// [`create`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    /// The project folder.
    pub dir: PathBuf,
    /// The `.ptdr` file.
    pub ptdr: PathBuf,
    /// Packs the project uses.
    pub packs: Vec<String>,
    /// Forms and data modules.
    pub forms: Vec<String>,
    /// Components of `services` (`name: pack/Type`).
    pub components: Vec<String>,
    /// Files that will exist afterwards (relative, sorted), generated code
    /// included.
    pub files: Vec<String>,
    /// The backend's license notice, if any.
    pub license_notice: Option<String>,
    pub errors: Vec<WizardIssue>,
}

impl Plan {
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }
}

/// The installed Rust version (`rustc --version`), e.g. `1.99.1`.
pub fn installed_rust_version() -> Option<String> {
    let output = std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.split_whitespace().nth(1).map(str::to_owned)
}

fn valid_rust_version(v: &str) -> bool {
    let parts: Vec<&str> = v.split('.').collect();
    (2..=3).contains(&parts.len())
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.parse::<u32>().is_ok())
}

impl NewProject {
    /// A program with defaults: GTK 4 if installed, installed Rust version.
    pub fn program(name: &str, parent: &Path, backend: &str) -> Self {
        Self {
            name: name.to_owned(),
            parent: parent.to_path_buf(),
            kind: ProjectKind::Binary,
            platforms: Vec::new(),
            backend: backend.to_owned(),
            rust_version: installed_rust_version().unwrap_or_else(|| "1.99".to_owned()),
            starters: Vec::new(),
            git: false,
            initial_commit: false,
            license_accepted: false,
        }
    }

    /// A library (no UI).
    pub fn library(name: &str, parent: &Path) -> Self {
        Self {
            kind: ProjectKind::Library,
            ..Self::program(name, parent, NO_BACKEND)
        }
    }

    pub fn dir(&self) -> PathBuf {
        self.parent.join(&self.name)
    }

    /// Packs the project needs: those of the starters, Tokio if one of
    /// them is async.
    pub fn packs(&self) -> Vec<String> {
        let mut packs: Vec<String> = Vec::new();
        let needs_runtime = self.starters.iter().any(|s| !matches!(s, Starter::Runtime));
        if needs_runtime || self.starters.contains(&Starter::Runtime) {
            packs.push("tokio".to_owned());
        }
        for starter in &self.starters {
            let pack = starter.component().0.to_owned();
            if !packs.contains(&pack) {
                packs.push(pack);
            }
        }
        packs
    }

    /// The starters with the runtime added when needed, in creation order.
    fn starters_with_runtime(&self) -> Vec<Starter> {
        let mut list = Vec::new();
        if self.packs().iter().any(|p| p == "tokio") {
            list.push(Starter::Runtime);
        }
        list.extend(
            self.starters
                .iter()
                .filter(|s| **s != Starter::Runtime)
                .cloned(),
        );
        list
    }

    fn check(&self, registry: &TypeRegistry) -> (Vec<WizardIssue>, Option<String>) {
        let mut errors = Vec::new();
        let mut notice = None;
        if !is_package_name(&self.name)
            || ["main", "app", "test", "core", "std"].contains(&self.name.as_str())
        {
            errors.push(WizardIssue::new(
                codes::INVALID_NAME,
                format!(
                    "„{}“: Buchstaben, Ziffern, - und _, beginnend mit einem Buchstaben",
                    self.name
                ),
            ));
        }
        if !self.parent.is_dir() {
            errors.push(WizardIssue::new(
                codes::NO_PARENT,
                self.parent.display().to_string(),
            ));
        }
        let dir = self.dir();
        if dir.exists()
            && std::fs::read_dir(&dir).map_or(true, |mut entries| entries.next().is_some())
        {
            errors.push(WizardIssue::new(
                codes::FOLDER_NOT_EMPTY,
                dir.display().to_string(),
            ));
        }
        if !valid_rust_version(&self.rust_version) {
            errors.push(WizardIssue::new(
                codes::INVALID_RUST_VERSION,
                self.rust_version.clone(),
            ));
        }
        match self.kind {
            ProjectKind::Library if self.backend != NO_BACKEND => {
                errors.push(WizardIssue::new(
                    codes::LIBRARY_WITH_BACKEND,
                    format!("Eine Bibliothek hat keine Oberfläche ({}).", self.backend),
                ));
            }
            ProjectKind::Binary if self.backend != NO_BACKEND => {
                match registry.backend(&self.backend) {
                    None => errors.push(WizardIssue::new(
                        codes::UNKNOWN_BACKEND,
                        self.backend.clone(),
                    )),
                    Some(b) => {
                        notice = b.license_notice.clone();
                        if notice.is_some() && !self.license_accepted {
                            errors.push(WizardIssue::new(
                                codes::LICENSE_NOT_ACCEPTED,
                                format!("Lizenzhinweis von {} bestätigen", self.backend),
                            ));
                        }
                    }
                }
            }
            _ => {}
        }
        let mut seen = Vec::new();
        for starter in &self.starters {
            let (pack, ty, _) = starter.component();
            if seen.contains(&(pack, ty)) {
                errors.push(WizardIssue::new(
                    codes::DUPLICATE_STARTER,
                    format!("{pack}/{ty}"),
                ));
            }
            seen.push((pack, ty));
            if registry.get(&TypeRef::pack(pack, ty)).is_none() {
                errors.push(WizardIssue::new(codes::MISSING_PACK, pack));
            }
            let url_missing = match starter {
                Starter::Database { url, .. } | Starter::Redis { url } => url.trim().is_empty(),
                _ => false,
            };
            if url_missing {
                errors.push(WizardIssue::new(codes::NO_URL, format!("{pack}/{ty}")));
            }
        }
        if self
            .starters_with_runtime().contains(&Starter::Runtime)
            && registry.get(&TypeRef::pack("tokio", "Runtime")).is_none()
        {
            errors.push(WizardIssue::new(codes::MISSING_PACK, "tokio"));
        }
        (errors, notice)
    }

    /// The document the wizard saves: project file, forms, components.
    fn document(&self, registry: &TypeRegistry) -> Result<Document, WizardIssue> {
        let fail = |e: &dyn std::fmt::Display| WizardIssue::new(codes::FAILED, e.to_string());
        let mut project = ProjectFile::new(&self.name, &self.rust_version, &self.backend)
            .map_err(|e| fail(&e))?;
        project.kind = self.kind;
        project.platforms = self.platforms.clone();
        project.packs = self.packs();
        let mut doc = Document::new(project, registry.clone());
        let name = |n: &str| ComponentName::new(n).expect("valid name");
        if self.kind == ProjectKind::Binary {
            doc.apply(Command::AddForm {
                name: name(FIRST_FORM),
                data_module: false,
            })
            .map_err(|e| fail(&e))?;
        }
        let starters = self.starters_with_runtime();
        if self.kind == ProjectKind::Library || !starters.is_empty() {
            doc.apply(Command::AddForm {
                name: name(SERVICES),
                data_module: true,
            })
            .map_err(|e| fail(&e))?;
        }
        for starter in starters {
            let (pack, ty, component) = starter.component();
            doc.apply(Command::AddComponent {
                form: name(SERVICES),
                type_ref: TypeRef::pack(pack, ty),
                name: Some(name(component)),
            })
            .map_err(|e| fail(&e))?;
            for (property, value) in starter.properties() {
                doc.apply(Command::SetProperty {
                    form: name(SERVICES),
                    element: name(component),
                    property: property.to_owned(),
                    value: Some(PropertyValue::new(value)),
                })
                .map_err(|e| fail(&e))?;
            }
        }
        Ok(doc)
    }
}

/// Checks `request` and shows what [`create`] would do. Writes nothing.
pub fn plan(request: &NewProject, registry: &TypeRegistry) -> Plan {
    let (mut errors, license_notice) = request.check(registry);
    let dir = request.dir();
    let ptdr = dir.join(format!("{}.{}", request.name, ProjectFile::EXTENSION));
    let mut forms = Vec::new();
    let mut components = Vec::new();
    let mut files = Vec::new();
    if errors.iter().all(|e| e.code != codes::INVALID_NAME) {
        match request.document(registry) {
            Ok(doc) => {
                for form in doc.forms() {
                    forms.push(form.name.to_string());
                    for c in &form.components {
                        components.push(format!("{}: {}", c.name, c.type_ref));
                    }
                }
                files.push(format!("{}.{}", request.name, ProjectFile::EXTENSION));
                for form in doc.forms() {
                    files.push(format!("src/{}.ftdr", form.name));
                }
                let generation = tdr_core::codegen::generate(
                    doc.project(),
                    doc.forms(),
                    registry,
                    &std::collections::BTreeMap::<String, String>::new(),
                );
                files.extend(generation.files.iter().map(|f| f.path.clone()));
                if request.git {
                    files.push(".gitignore".to_owned());
                }
            }
            Err(e) => errors.push(e),
        }
    }
    files.sort();
    files.dedup();
    Plan {
        dir,
        ptdr,
        packs: request.packs(),
        forms,
        components,
        files,
        license_notice,
        errors,
    }
}

/// What [`create`] did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Created {
    pub ptdr: PathBuf,
    /// Written files (relative).
    pub files: Vec<String>,
    /// Problems after creating that do not undo it (e.g. Git missing).
    pub warnings: Vec<WizardIssue>,
}

/// Creates the project: folder, project file, forms, generated code, and
/// Git if wanted. Refuses when [`plan`] reports errors.
pub fn create(request: &NewProject, registry: &TypeRegistry) -> Result<Created, Vec<WizardIssue>> {
    let plan = plan(request, registry);
    if !plan.is_ok() {
        return Err(plan.errors);
    }
    let fail = |e: &dyn std::fmt::Display| vec![WizardIssue::new(codes::FAILED, e.to_string())];
    let dir = plan.dir.clone();
    std::fs::create_dir_all(&dir).map_err(|e| fail(&e))?;
    let mut doc = request.document(registry).map_err(|e| vec![e])?;
    let mut files = doc.save(Some(&dir)).map_err(|e| fail(&e))?;
    let generation = tdr_core::codegen::generate(doc.project(), doc.forms(), registry, &dir);
    files.extend(tdr_core::codegen::write(&dir, &generation).map_err(|e| fail(&e))?);
    let mut warnings = Vec::new();
    if request.git {
        match crate::git::Git::init(&dir) {
            Ok(git) => {
                files.push(".gitignore".to_owned());
                if request.initial_commit {
                    let commit = git
                        .run(&crate::git::GitOp::Stage { paths: Vec::new() })
                        .and_then(|_| {
                            git.run(&crate::git::GitOp::Commit {
                                message: format!("New project {}", request.name),
                                amend: false,
                                then: Default::default(),
                            })
                        });
                    if let Err(e) = commit {
                        warnings.push(WizardIssue::new(e.code, e.detail));
                    }
                }
            }
            Err(e) => warnings.push(WizardIssue::new(e.code, e.detail)),
        }
    }
    files.sort();
    files.dedup();
    Ok(Created {
        ptdr: plan.ptdr,
        files,
        warnings,
    })
}
