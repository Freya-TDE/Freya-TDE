//! [`Session`]: the IDE state without UI – the open project, the form shown
//! in the designer, the selection, the palette type waiting to be placed and
//! the mouse gestures on the design surface.
//!
//! The windows only forward input here and draw what [`Session::picture`]
//! returns; tests (and later an AI) drive the same functions.

use std::path::{Path, PathBuf};

use tdr_core::{
    ComponentName, Form, LayoutKind, Placement, Platform, ProjectFile, Size, TypeRef, TypeRegistry,
};
use tdr_designer::layout::drop_target;
use tdr_designer::{
    Command, CommandError, Document, DocumentError, FormLayout, ItemKind, OpenReport, Point,
    Primitive, Theme, hit_test, layout, render, render_selection,
};

/// Distance the mouse must travel before a press becomes a drag.
const DRAG_THRESHOLD: i32 = 4;

/// Default size of widgets placed into an absolute layout.
const ABSOLUTE_DEFAULT: Size = Size {
    width: 120,
    height: 30,
};

/// A press on the design surface that may become a drag.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Press {
    element: ComponentName,
    at: Point,
    dragging: bool,
}

/// What a gesture changed (the UI refreshes accordingly).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Changes {
    /// The project model changed (redraw everything, mark dirty).
    pub model: bool,
    /// Only the selection changed.
    pub selection: bool,
}

impl Changes {
    fn model() -> Self {
        Self {
            model: true,
            selection: true,
        }
    }

    fn selection() -> Self {
        Self {
            model: false,
            selection: true,
        }
    }

    pub fn any(self) -> bool {
        self.model || self.selection
    }
}

/// The IDE state.
pub struct Session {
    doc: Document,
    form: Option<ComponentName>,
    selection: Vec<ComponentName>,
    armed: Option<TypeRef>,
    platform: Platform,
    theme: Theme,
    press: Option<Press>,
}

/// The platform whose look the designer shows by default.
pub fn host_platform() -> Platform {
    if cfg!(target_os = "windows") {
        Platform::Windows
    } else if cfg!(target_os = "macos") {
        Platform::MacOs
    } else {
        Platform::Linux
    }
}

impl Session {
    pub fn new(doc: Document) -> Self {
        let platform = host_platform();
        let form = doc.forms().first().map(|f| f.name.clone());
        Self {
            theme: Theme::for_platform(&platform),
            doc,
            form,
            selection: Vec::new(),
            armed: None,
            platform,
            press: None,
        }
    }

    /// A new, unsaved project with one form `main`. `packs` are the
    /// component packs it uses (their types must be in `registry`).
    pub fn untitled(
        name: &str,
        backend: &str,
        packs: Vec<String>,
        registry: TypeRegistry,
    ) -> Result<Self, String> {
        let mut project = ProjectFile::new(name, "1.99", backend).map_err(|e| e.to_string())?;
        project.packs = packs;
        let mut doc = Document::new(project, registry);
        doc.apply(Command::AddForm {
            name: ComponentName::new("main").expect("valid name"),
            data_module: false,
        })
        .map_err(|e| e.to_string())?;
        Ok(Self::new(doc))
    }

    /// Like [`Session::untitled`], saved to `dir` right away.
    pub fn create(
        dir: &Path,
        name: &str,
        backend: &str,
        registry: TypeRegistry,
    ) -> Result<Self, String> {
        let mut session = Self::untitled(name, backend, Vec::new(), registry)?;
        session.doc.save(Some(dir)).map_err(|e| e.to_string())?;
        Ok(session)
    }

    /// Opens a `.ptdr` file, loading packs from `pack_dirs`.
    pub fn open(ptdr: &Path, pack_dirs: &[PathBuf]) -> Result<(Self, OpenReport), DocumentError> {
        let (doc, report) = Document::open(ptdr, pack_dirs)?;
        Ok((Self::new(doc), report))
    }

    pub fn document(&self) -> &Document {
        &self.doc
    }

    pub fn document_mut(&mut self) -> &mut Document {
        &mut self.doc
    }

    pub fn platform(&self) -> &Platform {
        &self.platform
    }

    /// Shows the designer in the look of another platform.
    pub fn set_platform(&mut self, platform: Platform) {
        self.theme = Theme::for_platform(&platform);
        self.platform = platform;
    }

    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    // --- Form and selection ------------------------------------------------------

    pub fn form(&self) -> Option<&Form> {
        self.doc.form(self.form.as_ref()?)
    }

    pub fn form_name(&self) -> Option<&ComponentName> {
        self.form.as_ref()
    }

    /// Shows another form in the designer.
    pub fn set_form(&mut self, name: &ComponentName) -> bool {
        if self.doc.form(name).is_none() {
            return false;
        }
        self.form = Some(name.clone());
        self.selection.clear();
        true
    }

    pub fn selection(&self) -> &[ComponentName] {
        &self.selection
    }

    /// Selects elements of the current form (unknown names are dropped).
    pub fn select(&mut self, names: Vec<ComponentName>) {
        let form = self.form();
        self.selection = names
            .into_iter()
            .filter(|n| form.is_some_and(|f| f.contains(n)))
            .collect();
    }

    /// The palette type the next click places (`None` = select mode).
    pub fn armed(&self) -> Option<&TypeRef> {
        self.armed.as_ref()
    }

    pub fn arm(&mut self, type_ref: Option<TypeRef>) {
        self.armed = type_ref;
    }

    /// Keeps form and selection valid after undo/redo or external changes.
    fn tidy(&mut self) {
        if self.form().is_none() {
            self.form = self.doc.forms().first().map(|f| f.name.clone());
        }
        let selection = std::mem::take(&mut self.selection);
        self.select(selection);
    }

    // --- Commands -------------------------------------------------------------------

    /// Applies a command; the selection follows created elements.
    pub fn apply(&mut self, command: Command) -> Result<Changes, CommandError> {
        let outcome = self.doc.apply(command)?;
        if let Some(created) = outcome.created.last() {
            match &created.form {
                Some(form) => {
                    self.form = Some(form.clone());
                    self.selection = vec![created.name.clone()];
                }
                // A new form.
                None => {
                    self.form = Some(created.name.clone());
                    self.selection.clear();
                }
            }
        }
        self.tidy();
        Ok(Changes::model())
    }

    pub fn undo(&mut self) -> bool {
        let done = self.doc.undo();
        self.tidy();
        done
    }

    pub fn redo(&mut self) -> bool {
        let done = self.doc.redo();
        self.tidy();
        done
    }

    /// Removes the selected elements (the form's window stays).
    pub fn delete_selection(&mut self) -> Result<Changes, CommandError> {
        let Some(form) = self.form.clone() else {
            return Ok(Changes::default());
        };
        let root = self
            .form()
            .and_then(|f| f.root.as_ref())
            .map(|r| r.component.name.clone());
        let commands: Vec<Command> = self
            .selection
            .iter()
            .filter(|n| Some(*n) != root.as_ref())
            .map(|n| Command::Remove {
                form: form.clone(),
                element: n.clone(),
            })
            .collect();
        if commands.is_empty() {
            return Ok(Changes::default());
        }
        self.apply(Command::Batch { commands })
    }

    // --- Design surface ------------------------------------------------------------

    pub fn layout(&self) -> Option<FormLayout> {
        Some(layout(self.form()?, self.doc.registry(), &self.theme))
    }

    /// What the design surface shows: the form and the selection frame.
    pub fn picture(&self) -> Option<(FormLayout, Vec<Primitive>)> {
        let form = self.form()?;
        let layout = layout(form, self.doc.registry(), &self.theme);
        let mut primitives = render(form, &layout, self.doc.registry(), &self.theme);
        primitives.extend(render_selection(&layout, &self.selection, &self.theme));
        Some((layout, primitives))
    }

    /// Mouse pressed on the design surface.
    pub fn press(&mut self, at: Point, add_to_selection: bool) -> Result<Changes, CommandError> {
        self.press = None;
        let Some(layout) = self.layout() else {
            return Ok(Changes::default());
        };
        if let Some(type_ref) = self.armed.take() {
            return self.place(type_ref, at, &layout);
        }
        let Some(hit) = hit_test(&layout, at, &self.selection) else {
            let changed = !self.selection.is_empty();
            self.selection.clear();
            return Ok(if changed {
                Changes::selection()
            } else {
                Changes::default()
            });
        };
        if add_to_selection {
            if let Some(i) = self.selection.iter().position(|n| n == &hit.element) {
                self.selection.remove(i);
            } else {
                self.selection.push(hit.element.clone());
            }
        } else if !self.selection.contains(&hit.element) {
            self.selection = vec![hit.element.clone()];
        }
        self.press = Some(Press {
            element: hit.element,
            at,
            dragging: false,
        });
        Ok(Changes::selection())
    }

    /// Mouse moved with the button down: `true` once it is a drag.
    pub fn drag(&mut self, to: Point) -> bool {
        let Some(press) = &mut self.press else {
            return false;
        };
        if (to.x - press.at.x).abs() > DRAG_THRESHOLD || (to.y - press.at.y).abs() > DRAG_THRESHOLD
        {
            press.dragging = true;
        }
        press.dragging
    }

    /// Where a drag would drop the dragged widget (container, index).
    pub fn drop_preview(&self, at: Point) -> Option<(ComponentName, usize)> {
        let press = self.press.as_ref().filter(|p| p.dragging)?;
        let form = self.form()?;
        let target = drop_target(form, self.doc.registry(), &self.layout()?, at)?;
        // Not into itself or its own subtree.
        let moved = form.widget(&press.element)?;
        let inside = moved
            .descendants()
            .iter()
            .any(|w| w.component.name == target.0);
        (!inside).then_some(target)
    }

    /// Mouse released: a drag moves the widget there.
    pub fn release(&mut self, at: Point) -> Result<Changes, CommandError> {
        let target = self.drop_preview(at);
        let Some(press) = self.press.take().filter(|p| p.dragging) else {
            return Ok(Changes::default());
        };
        let (Some(form), Some((parent, mut index))) = (self.form.clone(), target) else {
            return Ok(Changes::default());
        };
        let Some(current) = self.form().and_then(|f| f.parent_of(&press.element)) else {
            // The window itself or a tray component: nothing to move.
            return Ok(Changes::default());
        };
        // Moving down within the same container: the old slot disappears.
        if current.component.name == parent
            && let Some(old) = current
                .children
                .iter()
                .position(|c| c.component.name == press.element)
            && old < index
        {
            index -= 1;
        }
        self.apply(Command::Move {
            form,
            element: press.element,
            parent,
            index,
        })
    }

    fn place(
        &mut self,
        type_ref: TypeRef,
        at: Point,
        layout: &FormLayout,
    ) -> Result<Changes, CommandError> {
        let Some(form_name) = self.form.clone() else {
            return Ok(Changes::default());
        };
        let visual = self
            .doc
            .registry()
            .get(&type_ref)
            .is_some_and(|t| t.kind.is_widget());
        if !visual {
            return self.apply(Command::AddComponent {
                form: form_name,
                type_ref,
                name: None,
            });
        }
        let form = self.form().expect("current form");
        let root = form
            .root
            .as_ref()
            .map(|r| r.component.name.clone())
            .ok_or_else(|| CommandError::NotAWidgetType(type_ref.clone()))?;
        let (parent, index) =
            drop_target(form, self.doc.registry(), layout, at).unwrap_or((root, usize::MAX));
        // In an absolute layout the widget goes where it was clicked.
        let absolute = form
            .widget(&parent)
            .and_then(|w| w.visual.layout.as_ref())
            .is_some_and(|l| l.kind == LayoutKind::Absolute);
        let placement = absolute
            .then(|| layout.item(&parent).and_then(|i| i.content))
            .flatten()
            .map(|content| Placement::Absolute {
                position: tdr_core::Point {
                    x: at.x - content.x,
                    y: at.y - content.y,
                },
                size: ABSOLUTE_DEFAULT,
            });
        self.apply(Command::AddWidget {
            form: form_name,
            parent,
            type_ref,
            name: None,
            index: Some(index),
            placement,
        })
    }

    /// The kind of the selected element, if exactly one is selected.
    pub fn single_selection(&self) -> Option<(&ComponentName, ItemKind)> {
        let [name] = self.selection.as_slice() else {
            return None;
        };
        let kind = self.layout()?.item(name)?.kind;
        Some((name, kind))
    }

    /// The window root of the current form.
    pub fn root(&self) -> Option<&ComponentName> {
        Some(&self.form()?.root.as_ref()?.component.name)
    }

    pub fn is_root(&self, name: &ComponentName) -> bool {
        self.root() == Some(name)
    }
}
