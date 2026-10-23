//! Editing commands: every change the designer makes to a project.
//!
//! Commands are plain data (serde), so the UI, a script or an AI send the
//! same thing:
//!
//! ```json
//! {"command": "add_widget", "form": "main", "parent": "window", "type": "core/Button"}
//! {"command": "set_property", "form": "main", "element": "button1",
//!  "property": "caption", "value": {"Text": "OK"}}
//! ```
//!
//! [`crate::Document::apply`] checks and executes them; each successful
//! command (a [`Command::Batch`] as a whole) is one undo step.

use serde::{Deserialize, Serialize};
use tdr_core::{
    ComponentName, ComponentPath, ContainerLayout, HandlerName, Mismatch, ModelError, Placement,
    PropertyValue, TypeRef,
};

/// A change to the project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    /// A new form (window root `window`) or data module.
    AddForm {
        name: ComponentName,
        #[serde(default)]
        data_module: bool,
    },
    RemoveForm {
        form: ComponentName,
    },
    /// Renames a form and updates qualified references on all forms.
    RenameForm {
        form: ComponentName,
        new_name: ComponentName,
    },
    /// Adds a widget to a container. Without `name` a free one is chosen
    /// (`button1`, `button2` …); without `index` it goes last.
    AddWidget {
        form: ComponentName,
        parent: ComponentName,
        #[serde(rename = "type")]
        type_ref: TypeRef,
        #[serde(default)]
        name: Option<ComponentName>,
        #[serde(default)]
        index: Option<usize>,
        #[serde(default)]
        placement: Option<Placement>,
    },
    /// Adds a non-visual component.
    AddComponent {
        form: ComponentName,
        #[serde(rename = "type")]
        type_ref: TypeRef,
        #[serde(default)]
        name: Option<ComponentName>,
    },
    /// Removes an element (a widget with its subtree); references to it on
    /// all forms are cleared.
    Remove {
        form: ComponentName,
        element: ComponentName,
    },
    /// Renames an element and updates references on all forms.
    Rename {
        form: ComponentName,
        element: ComponentName,
        new_name: ComponentName,
    },
    /// Moves a widget to another container or position.
    Move {
        form: ComponentName,
        element: ComponentName,
        parent: ComponentName,
        index: usize,
    },
    /// Sets a property; `null` resets it to the type's default.
    SetProperty {
        form: ComponentName,
        element: ComponentName,
        property: String,
        #[serde(default)]
        value: Option<PropertyValue>,
    },
    /// Connects an event to a handler; `null` disconnects it.
    SetEvent {
        form: ComponentName,
        element: ComponentName,
        event: String,
        #[serde(default)]
        handler: Option<HandlerName>,
    },
    SetPlacement {
        form: ComponentName,
        element: ComponentName,
        placement: Placement,
    },
    /// Sets the layout of a container's children; `null` = default.
    SetLayout {
        form: ComponentName,
        element: ComponentName,
        #[serde(default)]
        layout: Option<ContainerLayout>,
    },
    /// Several commands as one undo step; all or nothing.
    Batch {
        commands: Vec<Command>,
    },
}

/// What a successful command did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Outcome {
    /// Forms and elements that were created (with the chosen names).
    pub created: Vec<ComponentPath>,
}

/// Why a command was refused. Serialized with a stable `code`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum CommandError {
    #[error(transparent)]
    Model(#[from] ModelError),
    #[error("there is no form {0}")]
    UnknownForm(ComponentName),
    #[error("a form named {0} already exists")]
    DuplicateForm(ComponentName),
    #[error("{0} not found")]
    UnknownElement(ComponentPath),
    #[error("unknown type {0}")]
    UnknownType(TypeRef),
    #[error("{0} is not a widget type; add it as a component")]
    NotAWidgetType(TypeRef),
    #[error("{0} is a widget type; add it to a container")]
    NotAComponentType(TypeRef),
    #[error("{0} cannot have children")]
    NotAContainer(ComponentPath),
    #[error("{0} is not a widget")]
    NotAWidget(ComponentPath),
    #[error("{element} has no property {property:?}")]
    UnknownProperty {
        element: ComponentPath,
        property: String,
    },
    #[error("property {property:?}: {mismatch}")]
    TypeMismatch {
        property: String,
        mismatch: Mismatch,
    },
    #[error("{element} has no event {event:?}")]
    UnknownEvent {
        element: ComponentPath,
        event: String,
    },
}

/// `CheckBox` → `check_box`: the prefix of automatic names.
pub(crate) fn name_prefix(type_name: &str) -> String {
    let chars: Vec<char> = type_name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            let prev = chars[i - 1];
            let next_lower = chars.get(i + 1).is_some_and(|n| n.is_ascii_lowercase());
            // Word boundary: aB, 1B, or the last capital of an acronym (HTTPRouter).
            if prev.is_ascii_lowercase()
                || prev.is_ascii_digit()
                || (prev.is_ascii_uppercase() && next_lower)
            {
                out.push('_');
            }
        }
        out.push(c.to_ascii_lowercase());
    }
    if out.is_empty() || out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert_str(0, "item");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes() {
        assert_eq!(name_prefix("CheckBox"), "check_box");
        assert_eq!(name_prefix("Button"), "button");
        assert_eq!(name_prefix("HTTPRouter"), "http_router");
    }

    #[test]
    fn json_round_trip() {
        let json = r#"{"command": "add_widget", "form": "main", "parent": "window", "type": "core/Button"}"#;
        let command: Command = serde_json::from_str(json).unwrap();
        assert!(matches!(&command, Command::AddWidget { name: None, .. }));
        let bad =
            r#"{"command": "add_widget", "form": "main", "parent": "window", "type": "Button"}"#;
        assert!(serde_json::from_str::<Command>(bad).is_err());
    }
}
