//! [`Element`]: uniform access to components and widgets.
//!
//! Components and widgets are separate types, but dependency graph, code
//! generator and form need to treat them alike (name, type, properties,
//! references). They do so through this trait instead of duplicating every
//! code path.

use crate::component::Component;
use crate::name::ComponentName;
use crate::type_ref::TypeRef;
use crate::widget::Widget;

/// Something on a form: a [`Component`] or a [`Widget`].
pub trait Element {
    /// The component part (name, type, properties, events).
    fn component(&self) -> &Component;

    fn component_mut(&mut self) -> &mut Component;

    /// The widget, if this element is visual.
    fn as_widget(&self) -> Option<&Widget> {
        None
    }

    fn name(&self) -> &ComponentName {
        &self.component().name
    }

    fn type_ref(&self) -> &TypeRef {
        &self.component().type_ref
    }

    fn is_widget(&self) -> bool {
        self.as_widget().is_some()
    }
}

impl Element for Component {
    fn component(&self) -> &Component {
        self
    }

    fn component_mut(&mut self) -> &mut Component {
        self
    }
}

impl Element for Widget {
    fn component(&self) -> &Component {
        &self.component
    }

    fn component_mut(&mut self) -> &mut Component {
        &mut self.component
    }

    fn as_widget(&self) -> Option<&Widget> {
        Some(self)
    }
}
