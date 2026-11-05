//! [`Session`]: the IDE state without UI – the open project, the form shown
//! in the designer, the selection, the palette type waiting to be placed and
//! the mouse gestures on the design surface.
//!
//! The windows only forward input here and draw what [`Session::picture`]
//! returns; tests (and later an AI) drive the same functions.

use std::path::{Path, PathBuf};

use tdr_core::codegen::{self, Generation};
use tdr_core::{
    ComponentName, Form, HandlerName, LayoutKind, Placement, Platform, ProjectFile, Size, TypeRef,
    TypeRegistry, Widget,
};
use tdr_designer::layout::drop_target;
use tdr_designer::{
    Command, CommandError, Document, DocumentError, FormLayout, ItemKind, OpenReport, Point,
    Primitive, Theme, hit_test, layout, render, render_selection, suggest_handler, tab_at,
};

/// Name of the first form of new projects (`main` and `app` are reserved
/// for `src/main.rs` and `src/app.gen.rs`).
pub const FIRST_FORM: &str = "main_form";

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
    /// A resize handle was grabbed: which one and the element's rectangle.
    resize: Option<(tdr_designer::Handle, tdr_designer::Rect)>,
}

/// Smallest size a resize leaves.
const MIN_RESIZE: i32 = 16;

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
    /// The forms of a new, never saved project as created (the automatic
    /// first form is no change of the user's).
    untitled_forms: Option<Vec<Form>>,
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
        // Zielsystem des Projekts, sonst das System des Entwicklers.
        let platform = doc
            .project()
            .platforms
            .first()
            .cloned()
            .unwrap_or_else(host_platform);
        let form = doc.forms().first().map(|f| f.name.clone());
        Self {
            theme: Theme::for_platform(&platform),
            doc,
            form,
            selection: Vec::new(),
            armed: None,
            platform,
            press: None,
            untitled_forms: None,
        }
    }

    /// Changes worth asking about before closing: a saved project changed
    /// since its last save, a new one changed since it was created.
    pub fn is_modified(&self) -> bool {
        if !self.doc.is_dirty() {
            return false;
        }
        match &self.untitled_forms {
            Some(forms) if self.doc.dir().is_none() => self.doc.forms() != forms.as_slice(),
            _ => true,
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
            name: ComponentName::new(FIRST_FORM).expect("valid name"),
            data_module: false,
        })
        .map_err(|e| e.to_string())?;
        let mut session = Self::new(doc);
        session.untitled_forms = Some(session.doc.forms().to_vec());
        Ok(session)
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

    /// The inspector's layout rows of `element` on the current form
    /// (placement in the parent, layout of a container's children).
    pub fn layout_rows(&self, element: &ComponentName) -> Vec<tdr_designer::PropertyRow> {
        let Some(form) = self.form() else {
            return Vec::new();
        };
        let absolute = self
            .doc
            .registry()
            .backend(&form.backend)
            .is_none_or(|b| b.absolute_positioning);
        tdr_designer::layout_rows(form, element, absolute)
    }

    /// Sets the layout row `name` (`placement.…`, `layout.…`) of `element`;
    /// `None` = default. Errors as text (unknown row, invalid value) or from
    /// the command.
    pub fn set_layout_row(
        &mut self,
        element: &ComponentName,
        name: &str,
        value: Option<tdr_core::Value>,
    ) -> Result<Changes, String> {
        let Some(form) = self.form() else {
            return Ok(Changes::default());
        };
        let command =
            tdr_designer::layout_command(form, element, name, value, self.layout().as_ref())
                .map_err(|e| e.to_string())?;
        self.apply(command).map_err(|e| e.to_string())
    }

    /// Back to the natural size: clears the requested size of the selected
    /// widgets in layouts (stretch and alignment stay).
    pub fn natural_size(&mut self) -> Result<Changes, CommandError> {
        let Some(form) = self.form.clone() else {
            return Ok(Changes::default());
        };
        let commands: Vec<Command> = self
            .selection
            .iter()
            .filter_map(|n| {
                let widget = self.form()?.widget(n)?;
                match widget.visual.placement.clone() {
                    Placement::Layout {
                        stretch,
                        align,
                        cell,
                        size,
                        percent,
                    } if size != Size::default() || percent != Size::default() => {
                        Some(Command::SetPlacement {
                            form: form.clone(),
                            element: n.clone(),
                            placement: Placement::Layout {
                                stretch,
                                align,
                                cell,
                                size: Size::default(),
                                percent: Size::default(),
                            },
                        })
                    }
                    _ => None,
                }
            })
            .collect();
        if commands.is_empty() {
            return Ok(Changes::default());
        }
        self.apply(Command::Batch { commands })
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
        // Klick auf einen Reiter: Seite wechseln (wie Delphi), Notebook wählen.
        let tab = self
            .form()
            .and_then(|form| tab_at(form, &layout, self.doc.registry(), &self.theme, at));
        if let (Some((notebook, page)), Some(form)) = (tab, self.form.clone()) {
            self.selection = vec![notebook.clone()];
            let current = self
                .form()
                .and_then(|f| f.widget(&notebook))
                .and_then(|w| w.component.get("page"))
                .map(|v| v.value.clone());
            let page = tdr_core::Value::Int(page as i64);
            if current.as_ref() == Some(&page)
                || (current.is_none() && page == tdr_core::Value::Int(0))
            {
                return Ok(Changes::selection());
            }
            return self.apply(Command::SetProperty {
                form,
                element: notebook,
                property: "page".into(),
                value: Some(tdr_core::PropertyValue::new(page)),
            });
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
        // Anfasser: Formular und frei platzierte Widgets ändern ihre Größe.
        let resize = hit
            .handle
            .filter(|_| self.resizable(&hit.element))
            .zip(layout.rect(&hit.element));
        self.press = Some(Press {
            element: hit.element,
            at,
            dragging: false,
            resize,
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

    /// Can `element` be resized with the mouse? The form (its size) and
    /// every widget: absolute ones get position and size, ones in a layout
    /// a requested size (like Delphi's Width/Height).
    fn resizable(&self, element: &ComponentName) -> bool {
        self.form().and_then(|f| f.widget(element)).is_some()
    }

    /// The rectangle a resize drag to `at` would give (`None` if no resize
    /// is going on).
    pub fn resize_preview(&self, at: Point) -> Option<tdr_designer::Rect> {
        use tdr_designer::Handle as H;
        let press = self.press.as_ref().filter(|p| p.dragging)?;
        let (handle, r) = press.resize?;
        let (dx, dy) = (at.x - press.at.x, at.y - press.at.y);
        let (left, right) = match handle {
            H::TopLeft | H::Left | H::BottomLeft => (dx, 0),
            H::TopRight | H::Right | H::BottomRight => (0, dx),
            H::Top | H::Bottom => (0, 0),
        };
        let (top, bottom) = match handle {
            H::TopLeft | H::Top | H::TopRight => (dy, 0),
            H::BottomLeft | H::Bottom | H::BottomRight => (0, dy),
            H::Left | H::Right => (0, 0),
        };
        // Das Formular wächst nur nach rechts und unten (seine Lage ist kein Entwurf).
        let (left, top) = if self.is_root(&press.element) {
            (0, 0)
        } else {
            (
                left.min(r.width - MIN_RESIZE),
                top.min(r.height - MIN_RESIZE),
            )
        };
        let width = (r.width - left + right).max(MIN_RESIZE);
        let height = (r.height - top + bottom).max(MIN_RESIZE);
        Some(tdr_designer::Rect::new(
            r.x + left,
            r.y + top,
            width,
            height,
        ))
    }

    /// Where a drag would drop the dragged widget (container, index).
    pub fn drop_preview(&self, at: Point) -> Option<(ComponentName, usize)> {
        let press = self
            .press
            .as_ref()
            .filter(|p| p.dragging && p.resize.is_none())?;
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
        if let Some(rect) = self.resize_preview(at) {
            return self.finish_resize(rect);
        }
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
        // Absolute layout: the widget goes where it was dropped.
        if let Some(placement) = self.dropped_placement(&press, &parent, at) {
            let same = current.component.name == parent;
            let set = Command::SetPlacement {
                form: form.clone(),
                element: press.element.clone(),
                placement,
            };
            return self.apply(if same {
                set
            } else {
                Command::Batch {
                    commands: vec![
                        Command::Move {
                            form,
                            element: press.element,
                            parent,
                            index,
                        },
                        set,
                    ],
                }
            });
        }
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

    /// Placement of a widget dragged into `parent` and dropped at `at`: in an
    /// absolute layout at the drop position, out of one back into the
    /// parent's layout; `None` = placement stays.
    fn dropped_placement(
        &self,
        press: &Press,
        parent: &ComponentName,
        at: Point,
    ) -> Option<Placement> {
        let form = self.form()?;
        let widget = form.widget(&press.element)?;
        let into_absolute = form
            .widget(parent)?
            .visual
            .layout
            .as_ref()
            .is_some_and(|l| l.kind == LayoutKind::Absolute);
        let same = form.parent_of(&press.element)?.component.name == *parent;
        match (widget.visual.placement.clone(), into_absolute) {
            (
                Placement::Absolute {
                    position,
                    size,
                    anchors,
                },
                true,
            ) if same => Some(Placement::Absolute {
                position: tdr_core::Point {
                    x: position.x + at.x - press.at.x,
                    y: position.y + at.y - press.at.y,
                },
                size,
                anchors,
            }),
            (placement, true) => {
                let layout = self.layout()?;
                let content = layout.item(parent)?.content?;
                let shown = layout.rect(&press.element)?;
                let size = match placement {
                    Placement::Absolute { size, .. } => size,
                    Placement::Layout { .. } => Size {
                        width: shown.width,
                        height: shown.height,
                    },
                };
                Some(Placement::Absolute {
                    position: tdr_core::Point {
                        x: shown.x + at.x - press.at.x - content.x,
                        y: shown.y + at.y - press.at.y - content.y,
                    },
                    size,
                    anchors: tdr_core::Anchors::TOP_LEFT,
                })
            }
            (Placement::Absolute { .. }, false) => Some(Placement::default()),
            (Placement::Layout { .. }, false) => None,
        }
    }

    /// Applies a resize: the form's `size`, position and size of an
    /// absolutely placed widget, or the requested size (pixels or percent)
    /// of one in a layout. Anchored children of a resized container follow
    /// like in Delphi (one undo step).
    fn finish_resize(&mut self, rect: tdr_designer::Rect) -> Result<Changes, CommandError> {
        let Some(press) = self.press.take() else {
            return Ok(Changes::default());
        };
        let (Some(form), Some((handle, old))) = (self.form.clone(), press.resize) else {
            return Ok(Changes::default());
        };
        if rect == old {
            return Ok(Changes::default());
        }
        let Some(widget) = self.form().and_then(|f| f.widget(&press.element)).cloned() else {
            return Ok(Changes::default());
        };
        let mut resized = widget.clone();
        let main = if self.is_root(&press.element) {
            // Titel- und Menüleiste gehören nicht zur Formulargröße.
            let size = match widget.component.get("size").map(|v| &v.value) {
                Some(tdr_core::Value::Size(s)) => *s,
                _ => Size {
                    width: 640,
                    height: 480,
                },
            };
            let chrome = old.height - size.height;
            let value = tdr_core::Value::Size(Size {
                width: rect.width,
                height: (rect.height - chrome).max(MIN_RESIZE),
            });
            resized.component.set("size", value.clone());
            Command::SetProperty {
                form: form.clone(),
                element: press.element.clone(),
                property: "size".into(),
                value: Some(tdr_core::PropertyValue::new(value)),
            }
        } else {
            let placement = match widget.visual.placement.clone() {
                Placement::Absolute {
                    position,
                    size,
                    anchors,
                } => Placement::Absolute {
                    // Um die Änderung verschieben: die Anzeige kann wegen
                    // der Verankerung vom Gespeicherten abweichen.
                    position: tdr_core::Point {
                        x: position.x + rect.x - old.x,
                        y: position.y + rect.y - old.y,
                    },
                    size: Size {
                        width: (size.width + rect.width - old.width).max(MIN_RESIZE),
                        height: (size.height + rect.height - old.height).max(MIN_RESIZE),
                    },
                    anchors,
                },
                Placement::Layout {
                    stretch,
                    align,
                    cell,
                    size,
                    percent,
                } => self.layout_resize(
                    &press.element,
                    handle,
                    rect,
                    stretch,
                    align,
                    cell,
                    size,
                    percent,
                ),
            };
            resized.visual.placement = placement.clone();
            Command::SetPlacement {
                form: form.clone(),
                element: press.element.clone(),
                placement,
            }
        };
        let mut commands = vec![main];
        commands.extend(anchored_children(&form, &widget, &resized));
        self.apply(match commands.len() {
            1 => commands.remove(0),
            _ => Command::Batch { commands },
        })
    }

    /// New placement of a widget in a layout resized to `rect` with `handle`:
    /// axes in percent stay in percent (of the parent's content), the others
    /// get a requested size in pixels.
    #[allow(clippy::too_many_arguments)]
    fn layout_resize(
        &self,
        element: &ComponentName,
        handle: tdr_designer::Handle,
        rect: tdr_designer::Rect,
        mut stretch: u32,
        mut align: tdr_core::Align,
        cell: Option<tdr_core::GridCell>,
        mut size: Size,
        mut percent: Size,
    ) -> Placement {
        use tdr_designer::Handle as H;
        let horizontal = !matches!(handle, H::Top | H::Bottom);
        let vertical = !matches!(handle, H::Left | H::Right);
        let parent = self.form().and_then(|f| f.parent_of(element));
        let content = parent
            .and_then(|p| self.layout()?.item(&p.component.name)?.content)
            .unwrap_or_default();
        let share = |len: i32, space: i32| ((len * 100 + space / 2) / space.max(1)).clamp(1, 100);
        if horizontal {
            if percent.width > 0 {
                percent.width = share(rect.width, content.width);
            } else {
                size.width = rect.width;
            }
        }
        if vertical {
            if percent.height > 0 {
                percent.height = share(rect.height, content.height);
            } else {
                size.height = rect.height;
            }
        }
        // Damit die neue Größe sichtbar wird: Fill und Dehnen der
        // geänderten Achse aufheben (rückgängig machbar).
        let kind = parent
            .and_then(|p| p.visual.layout.as_ref().map(|l| l.kind.clone()))
            .unwrap_or(LayoutKind::Vertical);
        let (cross, main) = match kind {
            LayoutKind::Horizontal => (vertical, horizontal),
            LayoutKind::Grid { .. } => (horizontal || vertical, false),
            _ => (horizontal, vertical),
        };
        if cross && align == tdr_core::Align::Fill {
            align = tdr_core::Align::Start;
        }
        if main {
            stretch = 0;
        }
        Placement::Layout {
            stretch,
            align,
            cell,
            size,
            percent,
        }
    }

    /// A palette type dragged over the design surface: the container and
    /// index it would land in (visual types only).
    pub fn palette_drop_preview(
        &self,
        type_ref: &TypeRef,
        at: Point,
    ) -> Option<(ComponentName, usize)> {
        let visual = self
            .doc
            .registry()
            .get(type_ref)
            .is_some_and(|t| t.kind.is_widget());
        let form = self.form().filter(|_| visual)?;
        drop_target(form, self.doc.registry(), &self.layout()?, at)
    }

    /// A palette type dropped onto the design surface (drag and drop):
    /// placed there like a click with the armed type.
    pub fn drop_type(&mut self, type_ref: TypeRef, at: Point) -> Result<Changes, CommandError> {
        self.press = None;
        self.armed = None;
        let Some(layout) = self.layout() else {
            return Ok(Changes::default());
        };
        self.place(type_ref, at, &layout)
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
                anchors: tdr_core::Anchors::TOP_LEFT,
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

    // --- Code ------------------------------------------------------------------------

    /// The project directory (`None` while the project is unsaved).
    pub fn project_dir(&self) -> Option<PathBuf> {
        self.doc.dir().map(Path::to_path_buf)
    }

    /// What code generation would write (dry run).
    pub fn generation(&self) -> Option<Generation> {
        let dir = self.project_dir()?;
        Some(codegen::generate(
            self.doc.project(),
            self.doc.forms(),
            self.doc.registry(),
            &dir,
        ))
    }

    /// Saves the project and writes the generated code. Returns the
    /// generation (with its diagnostics) and the written files.
    pub fn write_code(&mut self) -> Result<(Generation, Vec<String>), String> {
        let dir = self
            .project_dir()
            .ok_or("Das Projekt ist noch nicht gespeichert.")?;
        if !self.doc.project().generate {
            return Err(
                "Dieses Projekt wird nur verwaltet (aus einer Cargo.toml erstellt); Freya TDE erzeugt dafür keinen Code."
                    .to_owned(),
            );
        }
        self.doc.save(Some(&dir)).map_err(|e| e.to_string())?;
        let generation = self.generation().expect("saved project");
        let written = codegen::write(&dir, &generation).map_err(|e| e.to_string())?;
        Ok((generation, written))
    }

    /// The handler of `event` on `element` (default: its first event); a
    /// missing one is connected with a suggested name (like Delphi's
    /// double-click on an event).
    pub fn ensure_handler(
        &mut self,
        element: &ComponentName,
        event: Option<&str>,
    ) -> Result<HandlerName, String> {
        let form = self.form.clone().ok_or("Kein Formular geöffnet.")?;
        let (event, existing) = {
            let f = self.form().ok_or("Kein Formular geöffnet.")?;
            let component = f
                .element(element)
                .ok_or_else(|| format!("{element} gibt es nicht."))?
                .component();
            let ty = self
                .doc
                .registry()
                .get(&component.type_ref)
                .ok_or_else(|| format!("Unbekannter Typ {}", component.type_ref))?;
            let event = match event {
                Some(e) => e.to_owned(),
                None => ty
                    .events
                    .first()
                    .map(|e| e.name.clone())
                    .ok_or_else(|| format!("{} hat keine Ereignisse.", component.type_ref))?,
            };
            let existing = component.events.get(&event).cloned();
            (event, existing)
        };
        if let Some(handler) = existing {
            return Ok(handler);
        }
        let handler = suggest_handler(element, &event);
        self.apply(Command::SetEvent {
            form,
            element: element.clone(),
            event,
            handler: Some(handler.clone()),
        })
        .map_err(|e| e.to_string())?;
        Ok(handler)
    }

    /// The user file of the current form.
    pub fn form_source(&self) -> Option<PathBuf> {
        Some(
            self.project_dir()?
                .join(format!("src/{}.rs", self.form_name()?)),
        )
    }
}

/// Line (1-based) of `fn <handler>(` in a source text.
pub fn handler_line(source: &str, handler: &HandlerName) -> Option<usize> {
    let pattern = format!("fn {handler}(");
    source
        .lines()
        .position(|line| line.contains(&pattern))
        .map(|i| i + 1)
}

/// Commands that move and resize the anchored, absolutely placed children
/// of `before` for its new size `after` (Delphi: resizing a form or panel
/// in the designer changes the anchored controls with it).
fn anchored_children(form: &ComponentName, before: &Widget, after: &Widget) -> Vec<Command> {
    let (old, new) = (before.design_area(), after.design_area());
    if old == new {
        return Vec::new();
    }
    before
        .children
        .iter()
        .filter_map(|child| match child.visual.placement {
            Placement::Absolute {
                position,
                size,
                anchors,
            } if !anchors.is_top_left() => {
                let (position, size) = anchors.apply(position, size, old, new);
                Some(Command::SetPlacement {
                    form: form.clone(),
                    element: child.component.name.clone(),
                    placement: Placement::Absolute {
                        position,
                        size,
                        anchors,
                    },
                })
            }
            _ => None,
        })
        .collect()
}
