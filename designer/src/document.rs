//! [`Document`]: an open project in the designer – forms, known types,
//! undo/redo and saving.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tdr_core::graph::{self, ProjectDiagnostic};
use tdr_core::pack::{MANIFEST, Pack, PackError};
use tdr_core::{
    Component, ComponentName, ComponentPath, Diagnostic, Form, ProjectFile, TypeRegistry,
    ValidationContext, Widget, builtin, project,
};

use crate::command::{Command, CommandError, Outcome, name_prefix};

/// How many steps can be undone.
pub const UNDO_LIMIT: usize = 200;

/// Name of the root widget of new forms.
pub const WINDOW: &str = "window";

/// Problems while opening a project that did not stop it from opening.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct OpenReport {
    /// Packs (or the backend) named by the project but not found.
    pub missing_packs: Vec<String>,
    pub pack_errors: Vec<PackError>,
}

/// Why a project could not be opened or saved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum DocumentError {
    #[error("{path}: {message}")]
    Io { path: String, message: String },
    #[error("{path}: {message}")]
    Format { path: String, message: String },
}

/// Findings of the whole project: per form and across forms.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Findings {
    pub forms: Vec<(ComponentName, Vec<Diagnostic>)>,
    pub project: Vec<ProjectDiagnostic>,
}

impl Findings {
    pub fn is_empty(&self) -> bool {
        self.project.is_empty() && self.forms.iter().all(|(_, d)| d.is_empty())
    }
}

/// An open project.
#[derive(Debug, Clone)]
pub struct Document {
    project: ProjectFile,
    forms: Vec<Form>,
    registry: TypeRegistry,
    /// Project directory; `None` until saved somewhere.
    dir: Option<PathBuf>,
    undo: Vec<Vec<Form>>,
    redo: Vec<Vec<Form>>,
    /// Forms as last saved or opened.
    saved: Vec<Form>,
}

fn io(path: &Path, e: std::io::Error) -> DocumentError {
    DocumentError::Io {
        path: path.display().to_string(),
        message: e.to_string(),
    }
}

/// Path of a form file relative to the project directory.
pub fn form_path(form: &ComponentName) -> String {
    format!("src/{form}.ftdr")
}

impl Document {
    /// A new, unsaved project.
    pub fn new(project: ProjectFile, registry: TypeRegistry) -> Self {
        Self {
            project,
            forms: Vec::new(),
            registry,
            dir: None,
            undo: Vec::new(),
            redo: Vec::new(),
            saved: Vec::new(),
        }
    }

    /// Opens a `.ptdr` file: reads the forms and loads the packs it names
    /// (and its backend) from the first of `pack_dirs` that has them.
    pub fn open(ptdr: &Path, pack_dirs: &[PathBuf]) -> Result<(Self, OpenReport), DocumentError> {
        let text = std::fs::read_to_string(ptdr).map_err(|e| io(ptdr, e))?;
        let project = ProjectFile::from_ron(&text).map_err(|e| DocumentError::Format {
            path: ptdr.display().to_string(),
            message: e.to_string(),
        })?;
        let dir = ptdr.parent().map(Path::to_path_buf).unwrap_or_default();

        let mut report = OpenReport::default();
        let mut registry = TypeRegistry::with_builtins();
        let names = project.packs.iter().chain([&project.backend]);
        for name in names {
            let Some(pack_dir) = pack_dirs
                .iter()
                .map(|d| d.join(name))
                .find(|d| d.join(MANIFEST).is_file())
            else {
                report.missing_packs.push(name.clone());
                continue;
            };
            match Pack::load(&pack_dir) {
                Ok(pack) => {
                    if let Err(e) = pack.register(&mut registry) {
                        report.pack_errors.push(PackError::Invalid {
                            file: Some(pack_dir.join(MANIFEST)),
                            problems: vec![tdr_core::pack::PackProblem {
                                location: String::new(),
                                message: e.to_string(),
                            }],
                        });
                    }
                }
                Err(e) => report.pack_errors.push(e),
            }
        }

        let mut forms = Vec::new();
        for path in &project.forms {
            let file = dir.join(path);
            let text = std::fs::read_to_string(&file).map_err(|e| io(&file, e))?;
            let form = Form::from_ron(&text).map_err(|e| DocumentError::Format {
                path: file.display().to_string(),
                message: e.to_string(),
            })?;
            forms.push(form);
        }
        let document = Self {
            project,
            saved: forms.clone(),
            forms,
            registry,
            dir: Some(dir),
            undo: Vec::new(),
            redo: Vec::new(),
        };
        Ok((document, report))
    }

    pub fn project(&self) -> &ProjectFile {
        &self.project
    }

    pub fn forms(&self) -> &[Form] {
        &self.forms
    }

    pub fn form(&self, name: &ComponentName) -> Option<&Form> {
        self.forms.iter().find(|f| &f.name == name)
    }

    pub fn registry(&self) -> &TypeRegistry {
        &self.registry
    }

    pub fn registry_mut(&mut self) -> &mut TypeRegistry {
        &mut self.registry
    }

    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    /// Changed since the last save?
    pub fn is_dirty(&self) -> bool {
        self.forms != self.saved
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(std::mem::replace(&mut self.forms, previous));
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(std::mem::replace(&mut self.forms, next));
        true
    }

    /// Executes a command. On success it is one undo step; on failure
    /// nothing changes.
    pub fn apply(&mut self, command: Command) -> Result<Outcome, CommandError> {
        let before = self.forms.clone();
        let mut outcome = Outcome::default();
        match self.execute(command, &mut outcome) {
            Ok(()) => {
                if self.forms != before {
                    self.undo.push(before);
                    if self.undo.len() > UNDO_LIMIT {
                        self.undo.remove(0);
                    }
                    self.redo.clear();
                }
                Ok(outcome)
            }
            Err(e) => {
                self.forms = before;
                Err(e)
            }
        }
    }

    fn form_mut(&mut self, name: &ComponentName) -> Result<&mut Form, CommandError> {
        self.forms
            .iter_mut()
            .find(|f| &f.name == name)
            .ok_or_else(|| CommandError::UnknownForm(name.clone()))
    }

    fn element_type(
        &self,
        form: &ComponentName,
        element: &ComponentName,
    ) -> Result<(ComponentPath, &tdr_core::ComponentType), CommandError> {
        let path = ComponentPath::on(form.clone(), element.clone());
        let f = self
            .form(form)
            .ok_or_else(|| CommandError::UnknownForm(form.clone()))?;
        let e = f
            .element(element)
            .ok_or_else(|| CommandError::UnknownElement(path.clone()))?;
        let ty = self
            .registry
            .get(e.type_ref())
            .ok_or_else(|| CommandError::UnknownType(e.type_ref().clone()))?;
        Ok((path, ty))
    }

    fn execute(&mut self, command: Command, outcome: &mut Outcome) -> Result<(), CommandError> {
        match command {
            Command::AddForm { name, data_module } => {
                if self.form(&name).is_some() {
                    return Err(CommandError::DuplicateForm(name));
                }
                let form = if data_module {
                    Form::data_module(name.clone(), self.project.backend.clone())
                } else {
                    let window = Widget::from_component(
                        Component::new(
                            ComponentName::new(WINDOW).expect("valid name"),
                            builtin::form(),
                        )
                        .with("title", name.as_str()),
                    );
                    Form::new(name.clone(), self.project.backend.clone(), window)
                };
                self.forms.push(form);
                outcome.created.push(ComponentPath::local(name));
            }
            Command::RemoveForm { form } => {
                if self.form(&form).is_none() {
                    return Err(CommandError::UnknownForm(form));
                }
                project::remove_form(&mut self.forms, &form)?;
            }
            Command::RenameForm { form, new_name } => {
                if self.form(&new_name).is_some() {
                    return Err(CommandError::DuplicateForm(new_name));
                }
                project::rename_form(&mut self.forms, &form, new_name)?;
            }
            Command::AddWidget {
                form,
                parent,
                type_ref,
                name,
                index,
                placement,
            } => {
                let ty = self
                    .registry
                    .get(&type_ref)
                    .ok_or_else(|| CommandError::UnknownType(type_ref.clone()))?;
                if !ty.kind.is_widget() {
                    return Err(CommandError::NotAWidgetType(type_ref));
                }
                let (parent_path, parent_ty) = self.element_type(&form, &parent)?;
                if !parent_ty.kind.is_container() {
                    return Err(CommandError::NotAContainer(parent_path));
                }
                let f = self.form_mut(&form)?;
                if f.widget(&parent).is_none() {
                    return Err(CommandError::NotAWidget(parent_path));
                }
                let name = match name {
                    Some(n) => n,
                    None => f.unique_name(&name_prefix(&type_ref.name))?,
                };
                let mut widget = Widget::new(name.clone(), type_ref);
                if let Some(placement) = placement {
                    widget.visual.placement = placement;
                }
                f.insert_widget(&parent, index.unwrap_or(usize::MAX), widget)?;
                outcome.created.push(ComponentPath::on(form, name));
            }
            Command::AddComponent {
                form,
                type_ref,
                name,
            } => {
                let ty = self
                    .registry
                    .get(&type_ref)
                    .ok_or_else(|| CommandError::UnknownType(type_ref.clone()))?;
                if ty.kind.is_widget() {
                    return Err(CommandError::NotAComponentType(type_ref));
                }
                let f = self.form_mut(&form)?;
                let name = match name {
                    Some(n) => n,
                    None => f.unique_name(&name_prefix(&type_ref.name))?,
                };
                f.add_component(Component::new(name.clone(), type_ref))?;
                outcome.created.push(ComponentPath::on(form, name));
            }
            Command::Remove { form, element } => {
                self.form_mut(&form)?;
                project::remove_component(&mut self.forms, &form, &element)?;
            }
            Command::Rename {
                form,
                element,
                new_name,
            } => {
                self.form_mut(&form)?;
                project::rename_component(&mut self.forms, &form, &element, new_name)?;
            }
            Command::Move {
                form,
                element,
                parent,
                index,
            } => {
                let (parent_path, parent_ty) = self.element_type(&form, &parent)?;
                if !parent_ty.kind.is_container() {
                    return Err(CommandError::NotAContainer(parent_path));
                }
                self.form_mut(&form)?
                    .move_widget(&element, &parent, index)?;
            }
            Command::SetProperty {
                form,
                element,
                property,
                value,
            } => {
                let (path, ty) = self.element_type(&form, &element)?;
                let backend = self
                    .form(&form)
                    .map(|f| f.backend.clone())
                    .unwrap_or_default();
                let def = self
                    .registry
                    .property_def(&ty.type_ref, &backend, &property)
                    .ok_or_else(|| CommandError::UnknownProperty {
                        element: path.clone(),
                        property: property.clone(),
                    })?;
                if let Some(value) = &value {
                    for v in value.all_values() {
                        v.check(&def.ty)
                            .map_err(|mismatch| CommandError::TypeMismatch {
                                property: property.clone(),
                                mismatch,
                            })?;
                    }
                }
                let component = self
                    .form_mut(&form)?
                    .element_mut(&element)
                    .ok_or(CommandError::UnknownElement(path))?;
                match value {
                    Some(value) => component.set(property, value),
                    None => {
                        component.reset(&property);
                    }
                }
            }
            Command::SetEvent {
                form,
                element,
                event,
                handler,
            } => {
                let (path, ty) = self.element_type(&form, &element)?;
                if ty.event(&event).is_none() {
                    return Err(CommandError::UnknownEvent {
                        element: path,
                        event,
                    });
                }
                let component = self
                    .form_mut(&form)?
                    .element_mut(&element)
                    .ok_or(CommandError::UnknownElement(path))?;
                match handler {
                    Some(handler) => {
                        component.events.insert(event, handler);
                    }
                    None => {
                        component.events.remove(&event);
                    }
                }
            }
            Command::SetPlacement {
                form,
                element,
                placement,
            } => {
                let path = ComponentPath::on(form.clone(), element.clone());
                let widget = self
                    .form_mut(&form)?
                    .widget_mut(&element)
                    .ok_or(CommandError::NotAWidget(path))?;
                widget.visual.placement = placement;
            }
            Command::SetLayout {
                form,
                element,
                layout,
            } => {
                let (path, ty) = self.element_type(&form, &element)?;
                if !ty.kind.is_container() {
                    return Err(CommandError::NotAContainer(path));
                }
                let widget = self
                    .form_mut(&form)?
                    .widget_mut(&element)
                    .ok_or(CommandError::NotAWidget(path))?;
                widget.visual.layout = layout;
            }
            Command::Batch { commands } => {
                for command in commands {
                    self.execute(command, outcome)?;
                }
            }
        }
        Ok(())
    }

    /// Validation of every form plus the project-wide analysis.
    pub fn findings(&self) -> Findings {
        let context = ValidationContext::for_backend(&self.registry, &self.project.backend);
        Findings {
            forms: self
                .forms
                .iter()
                .map(|f| (f.name.clone(), f.validate(&context)))
                .collect(),
            project: graph::analyze(&self.forms, &self.registry).diagnostics,
        }
    }

    /// The project's files as they would be saved (dry run): the `.ptdr`
    /// and one `.ftdr` per form, relative to the project directory.
    pub fn files(&self) -> Result<Vec<(String, String)>, DocumentError> {
        let format = |path: &str, e: tdr_core::ModelError| DocumentError::Format {
            path: path.to_owned(),
            message: e.to_string(),
        };
        let mut project = self.project.clone();
        project.forms = self.forms.iter().map(|f| form_path(&f.name)).collect();
        let ptdr = format!("{}.{}", project.name, ProjectFile::EXTENSION);
        let mut files = vec![(
            ptdr.clone(),
            project.to_ron().map_err(|e| format(&ptdr, e))?,
        )];
        for form in &self.forms {
            let path = form_path(&form.name);
            let text = form.to_ron().map_err(|e| format(&path, e))?;
            files.push((path, text));
        }
        Ok(files)
    }

    /// Saves into `dir` (or the directory it was opened from). Form files
    /// of removed or renamed forms are deleted. Returns the written paths.
    pub fn save(&mut self, dir: Option<&Path>) -> Result<Vec<String>, DocumentError> {
        let dir = match (dir, &self.dir) {
            (Some(d), _) => d.to_path_buf(),
            (None, Some(d)) => d.clone(),
            (None, None) => {
                return Err(DocumentError::Io {
                    path: String::new(),
                    message: "the project has no directory yet".to_owned(),
                });
            }
        };
        let files = self.files()?;
        let mut written = Vec::new();
        for (path, content) in &files {
            let target = dir.join(path);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| io(parent, e))?;
            }
            if std::fs::read_to_string(&target).ok().as_deref() != Some(content) {
                std::fs::write(&target, content).map_err(|e| io(&target, e))?;
                written.push(path.clone());
            }
        }
        let current: BTreeSet<&ComponentName> = self.forms.iter().map(|f| &f.name).collect();
        for gone in self.saved.iter().filter(|f| !current.contains(&f.name)) {
            let target = dir.join(form_path(&gone.name));
            if target.exists() {
                std::fs::remove_file(&target).map_err(|e| io(&target, e))?;
            }
        }
        self.project.forms = self.forms.iter().map(|f| form_path(&f.name)).collect();
        self.saved = self.forms.clone();
        self.dir = Some(dir);
        Ok(written)
    }
}
