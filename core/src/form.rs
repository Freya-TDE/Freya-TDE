//! [`Form`]: a designer form – an optional widget tree plus the non-visual
//! components placed on it. Stored as `name.ftdr` (RON) next to `name.rs`.
//!
//! A form without widgets is a *data module*: the project's service
//! container (Delphi's `TDataModule`), holding pools, routers, runtimes …
//! that other forms reference as `services::db_pool`.

use serde::{Deserialize, Serialize};

use crate::component::Component;
use crate::element::Element;
use crate::error::ModelError;
use crate::name::ComponentName;
use crate::validate::{Diagnostic, ValidationContext};
use crate::widget::Widget;

/// A form (with widgets) or data module (without).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Form {
    /// File format version of the `.ftdr` file.
    #[serde(default = "current_format")]
    pub format: u32,
    /// The form's name; qualified references use it (`services::db_pool`).
    pub name: ComponentName,
    /// The UI backend this form is designed for (e.g. `"egui"`).
    pub backend: String,
    /// The root widget (usually `core/Form`); `None` for a data module.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<Widget>,
    /// Non-visual components (icons on the design surface, like Delphi's
    /// `TTimer` or `TQuery`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<Component>,
}

fn current_format() -> u32 {
    Form::FORMAT
}

impl Form {
    /// Current `.ftdr` format version.
    pub const FORMAT: u32 = 1;

    /// A form with a widget tree.
    pub fn new(name: ComponentName, backend: impl Into<String>, root: Widget) -> Self {
        Self {
            format: Self::FORMAT,
            name,
            backend: backend.into(),
            root: Some(root),
            components: Vec::new(),
        }
    }

    /// A data module: only non-visual components (the service container).
    pub fn data_module(name: ComponentName, backend: impl Into<String>) -> Self {
        Self {
            format: Self::FORMAT,
            name,
            backend: backend.into(),
            root: None,
            components: Vec::new(),
        }
    }

    pub fn is_data_module(&self) -> bool {
        self.root.is_none()
    }

    pub fn name(&self) -> &ComponentName {
        &self.name
    }

    /// All widgets, depth-first.
    pub fn widgets(&self) -> Vec<&Widget> {
        self.root
            .as_ref()
            .map(Widget::descendants)
            .unwrap_or_default()
    }

    /// All elements: widgets depth-first, then non-visual components.
    pub fn elements(&self) -> Vec<&dyn Element> {
        let widgets = self.widgets().into_iter().map(|w| w as &dyn Element);
        let components = self.components.iter().map(|c| c as &dyn Element);
        widgets.chain(components).collect()
    }

    pub fn element(&self, name: &ComponentName) -> Option<&dyn Element> {
        self.elements().into_iter().find(|e| e.name() == name)
    }

    pub fn contains(&self, name: &ComponentName) -> bool {
        self.element(name).is_some()
    }

    pub fn widget(&self, name: &ComponentName) -> Option<&Widget> {
        self.widgets()
            .into_iter()
            .find(|w| &w.component.name == name)
    }

    pub fn widget_mut(&mut self, name: &ComponentName) -> Option<&mut Widget> {
        self.root.as_mut().and_then(|r| r.find_mut(name))
    }

    pub fn component(&self, name: &ComponentName) -> Option<&Component> {
        self.components.iter().find(|c| &c.name == name)
    }

    pub fn component_mut(&mut self, name: &ComponentName) -> Option<&mut Component> {
        self.components.iter_mut().find(|c| &c.name == name)
    }

    /// The component part of any element (widget or component), mutable.
    pub fn element_mut(&mut self, name: &ComponentName) -> Option<&mut Component> {
        if self.components.iter().any(|c| &c.name == name) {
            return self.component_mut(name);
        }
        self.widget_mut(name).map(|w| &mut w.component)
    }

    /// The parent of a widget (`None` for the root and for non-visual components).
    pub fn parent_of(&self, name: &ComponentName) -> Option<&Widget> {
        self.widgets()
            .into_iter()
            .find(|w| w.children.iter().any(|c| &c.component.name == name))
    }

    /// Fails if any name in `names` already exists on the form, or if
    /// `names` contains duplicates itself.
    fn ensure_free<'a>(
        &self,
        names: impl IntoIterator<Item = &'a ComponentName>,
    ) -> Result<(), ModelError> {
        let mut seen = Vec::new();
        for name in names {
            if self.contains(name) || seen.contains(&name) {
                return Err(ModelError::DuplicateName(name.clone()));
            }
            seen.push(name);
        }
        Ok(())
    }

    /// Adds a widget (with its subtree) as last child of `parent`.
    pub fn add_widget(&mut self, parent: &ComponentName, widget: Widget) -> Result<(), ModelError> {
        self.ensure_free(widget.descendants().into_iter().map(|w| &w.component.name))?;
        let parent = self
            .widget_mut(parent)
            .ok_or_else(|| ModelError::NotFound(parent.clone()))?;
        parent.children.push(widget);
        Ok(())
    }

    /// Inserts a widget (with its subtree) as child `index` of `parent`
    /// (clamped to the number of children).
    pub fn insert_widget(
        &mut self,
        parent: &ComponentName,
        index: usize,
        widget: Widget,
    ) -> Result<(), ModelError> {
        self.ensure_free(widget.descendants().into_iter().map(|w| &w.component.name))?;
        let parent = self
            .widget_mut(parent)
            .ok_or_else(|| ModelError::NotFound(parent.clone()))?;
        let index = index.min(parent.children.len());
        parent.children.insert(index, widget);
        Ok(())
    }

    /// Moves a widget (with its subtree) to child `index` of `new_parent` –
    /// another container or a new position in the same one. References stay
    /// valid, names do not change.
    pub fn move_widget(
        &mut self,
        name: &ComponentName,
        new_parent: &ComponentName,
        index: usize,
    ) -> Result<(), ModelError> {
        let root = self
            .root
            .as_mut()
            .ok_or_else(|| ModelError::NotFound(name.clone()))?;
        if &root.component.name == name {
            return Err(ModelError::CannotRemoveRoot);
        }
        let moved = root
            .find_mut(name)
            .ok_or_else(|| ModelError::NotFound(name.clone()))?;
        if moved.find_mut(new_parent).is_some() {
            return Err(ModelError::MoveIntoItself(name.clone()));
        }
        if root.find_mut(new_parent).is_none() {
            return Err(ModelError::NotFound(new_parent.clone()));
        }
        let widget = root.remove_descendant(name).expect("found above");
        let parent = root.find_mut(new_parent).expect("found above");
        let index = index.min(parent.children.len());
        parent.children.insert(index, widget);
        Ok(())
    }

    /// Adds a non-visual component.
    pub fn add_component(&mut self, component: Component) -> Result<(), ModelError> {
        self.ensure_free([&component.name])?;
        self.components.push(component);
        Ok(())
    }

    /// Removes an element (a widget with its whole subtree) and clears
    /// references to it *on this form*. [`crate::project::remove_component`]
    /// also clears references on other forms. Returns the removed names.
    pub fn remove(&mut self, name: &ComponentName) -> Result<Vec<ComponentName>, ModelError> {
        if self
            .root
            .as_ref()
            .is_some_and(|r| &r.component.name == name)
        {
            return Err(ModelError::CannotRemoveRoot);
        }
        let removed: Vec<ComponentName> =
            if let Some(i) = self.components.iter().position(|c| &c.name == name) {
                vec![self.components.remove(i).name]
            } else {
                let widget = self
                    .root
                    .as_mut()
                    .and_then(|r| r.remove_descendant(name))
                    .ok_or_else(|| ModelError::NotFound(name.clone()))?;
                widget
                    .descendants()
                    .into_iter()
                    .map(|w| w.component.name.clone())
                    .collect()
            };
        let own = self.name.clone();
        self.for_each_component_mut(|c| {
            for gone in &removed {
                c.clear_references(&own, &own, gone);
            }
        });
        Ok(removed)
    }

    /// Renames an element and updates references to it *on this form*.
    /// [`crate::project::rename_component`] also updates other forms.
    pub fn rename(&mut self, old: &ComponentName, new: ComponentName) -> Result<(), ModelError> {
        if !self.contains(old) {
            return Err(ModelError::NotFound(old.clone()));
        }
        if old == &new {
            return Ok(());
        }
        self.ensure_free([&new])?;
        let own = self.name.clone();
        self.for_each_component_mut(|c| {
            if &c.name == old {
                c.name = new.clone();
            }
            c.rename_references(&own, &own, old, &new);
        });
        Ok(())
    }

    /// A free name `prefix1`, `prefix2`, … (like Delphi's `Button1`).
    pub fn unique_name(&self, prefix: &str) -> Result<ComponentName, ModelError> {
        (1..)
            .map(|n| ComponentName::new(format!("{prefix}{n}")))
            .find(|candidate| !matches!(candidate, Ok(name) if self.contains(name)))
            .expect("infinite sequence")
    }

    pub(crate) fn for_each_component_mut(&mut self, mut f: impl FnMut(&mut Component)) {
        if let Some(root) = &mut self.root {
            root.for_each_mut(&mut |w| f(&mut w.component));
        }
        self.components.iter_mut().for_each(f);
    }

    /// Checks the form on its own against the known types; see
    /// [`crate::validate()`]. References to other forms are checked by
    /// [`crate::graph::analyze`].
    pub fn validate(&self, context: &ValidationContext<'_>) -> Vec<Diagnostic> {
        crate::validate::validate(self, context)
    }

    /// The form as `.ftdr` text (pretty-printed RON).
    pub fn to_ron(&self) -> Result<String, ModelError> {
        // No struct names: property values use an untagged representation,
        // which serde cannot buffer when struct names are written.
        let config = ron::ser::PrettyConfig::new().indentor("    ".to_owned());
        ron::ser::to_string_pretty(self, config).map_err(|e| ModelError::Ron(e.to_string()))
    }

    /// Reads `.ftdr` text. Names, type references and capabilities are
    /// validated while reading; use [`Form::validate`] for the rest.
    pub fn from_ron(text: &str) -> Result<Self, ModelError> {
        let form: Form = ron::from_str(text).map_err(|e| ModelError::Ron(e.to_string()))?;
        if form.format > Self::FORMAT {
            return Err(ModelError::UnsupportedFormat(form.format));
        }
        Ok(form)
    }
}
