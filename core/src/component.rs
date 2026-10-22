//! [`Component`]: an instance of a (non-visual) component type – and the
//! common part every [`crate::Widget`] contains.

use serde::{Deserialize, Serialize};

use crate::event::Events;
use crate::name::{ComponentName, ComponentPath, HandlerName};
use crate::property::{Properties, PropertyValue};
use crate::type_ref::TypeRef;
use crate::value::Value;

/// An instance on a form: name, type, property values and event handlers.
///
/// Non-visual components (pools, timers, routers …) are used directly;
/// widgets wrap a `Component` and add their visual part.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Component {
    pub name: ComponentName,
    #[serde(rename = "type")]
    pub type_ref: TypeRef,
    /// Only values that differ from the type's defaults.
    #[serde(default, skip_serializing_if = "Properties::is_empty")]
    pub properties: Properties,
    #[serde(default, skip_serializing_if = "Events::is_empty")]
    pub events: Events,
}

impl Component {
    pub fn new(name: ComponentName, type_ref: TypeRef) -> Self {
        Self {
            name,
            type_ref,
            properties: Properties::new(),
            events: Events::new(),
        }
    }

    /// Sets a property (builder style).
    pub fn with(mut self, property: impl Into<String>, value: impl Into<PropertyValue>) -> Self {
        self.set(property, value);
        self
    }

    /// Connects an event to a handler (builder style).
    pub fn on(mut self, event: impl Into<String>, handler: HandlerName) -> Self {
        self.events.insert(event.into(), handler);
        self
    }

    pub fn set(&mut self, property: impl Into<String>, value: impl Into<PropertyValue>) {
        self.properties.insert(property.into(), value.into());
    }

    /// Removes the value, so the type's default applies again.
    pub fn reset(&mut self, property: &str) -> Option<PropertyValue> {
        self.properties.remove(property)
    }

    pub fn get(&self, property: &str) -> Option<&PropertyValue> {
        self.properties.get(property)
    }

    /// All components this one references (property values, including
    /// platform overrides and lists), with the property name.
    pub fn references(&self) -> Vec<(&str, &ComponentPath)> {
        self.properties
            .iter()
            .flat_map(|(name, value)| {
                value
                    .all_values()
                    .flat_map(Value::references)
                    .map(move |target| (name.as_str(), target))
            })
            .collect()
    }

    /// Updates references after `form::old` was renamed to `new`;
    /// `current` is the form this component lives on.
    pub(crate) fn rename_references(
        &mut self,
        current: &ComponentName,
        form: &ComponentName,
        old: &ComponentName,
        new: &ComponentName,
    ) {
        for value in self.properties.values_mut() {
            for v in value.all_values_mut() {
                v.rename_reference(current, form, old, new);
            }
        }
    }

    /// Clears references to `form::gone`; `current` is this component's form.
    pub(crate) fn clear_references(
        &mut self,
        current: &ComponentName,
        form: &ComponentName,
        gone: &ComponentName,
    ) {
        for value in self.properties.values_mut() {
            for v in value.all_values_mut() {
                v.clear_reference(current, form, gone);
            }
        }
    }

    /// Updates qualified references after the form `old` was renamed.
    pub(crate) fn rename_form_references(&mut self, old: &ComponentName, new: &ComponentName) {
        for value in self.properties.values_mut() {
            for v in value.all_values_mut() {
                v.rename_form_reference(old, new);
            }
        }
    }
}
