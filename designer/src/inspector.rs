//! Palette and object inspector as data: which types can be placed, and
//! which properties and events an element has.

use std::collections::BTreeSet;

use serde::Serialize;
use tdr_core::{
    ComponentName, EventParam, Form, HandlerName, Origin, Platform, PropertyType, PropertyValue,
    TypeRef, TypeRegistry, Value, builtin,
};

/// One entry of the component palette.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PaletteItem {
    #[serde(rename = "type")]
    pub type_ref: TypeRef,
    pub category: String,
    pub description: String,
    pub icon: Option<String>,
    /// Placed as a widget (else as a non-visual component).
    pub visual: bool,
    pub container: bool,
}

/// The types a form of `backend` can use: the core widgets (except the
/// form root), the backend's palette and all component packs – sorted by
/// category, then name.
pub fn palette(registry: &TypeRegistry, backend: &str) -> Vec<PaletteItem> {
    let mut items: Vec<PaletteItem> = registry
        .types()
        .filter(|t| t.type_ref != builtin::form())
        .filter(|t| match &t.type_ref.origin {
            Origin::Backend(b) => b == backend,
            Origin::Core | Origin::Pack(_) => true,
        })
        .map(|t| PaletteItem {
            type_ref: t.type_ref.clone(),
            category: if t.category.is_empty() {
                "Other".to_owned()
            } else {
                t.category.clone()
            },
            description: t.description.clone(),
            icon: t.icon.clone(),
            visual: t.kind.is_widget(),
            container: t.kind.is_container(),
        })
        .collect();
    items.sort_by(|a, b| (&a.category, &a.type_ref.name).cmp(&(&b.category, &b.type_ref.name)));
    items
}

/// One row of the object inspector's property page.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PropertyRow {
    pub name: String,
    pub category: String,
    pub description: String,
    #[serde(rename = "type")]
    pub ty: PropertyType,
    /// The type's default (`None` = required).
    pub default: Option<Value>,
    /// The value set on the element (`None` = default applies).
    pub value: Option<PropertyValue>,
    /// Exists only for the project's UI backend.
    pub backend_specific: bool,
    /// Only on these platforms (empty = all).
    pub platforms: BTreeSet<Platform>,
}

impl PropertyRow {
    /// The value that applies (set value, else default).
    pub fn effective(&self) -> Option<&Value> {
        self.value
            .as_ref()
            .map(|v| &v.value)
            .or(self.default.as_ref())
    }
}

/// One row of the inspector's event page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EventRow {
    pub name: String,
    pub description: String,
    pub params: Vec<EventParam>,
    pub returns: Option<String>,
    pub handler: Option<HandlerName>,
}

/// The properties of `element` on `form`, in declaration order.
pub fn properties(
    form: &Form,
    element: &ComponentName,
    registry: &TypeRegistry,
) -> Option<Vec<PropertyRow>> {
    let component = form.element(element)?.component();
    let rows = registry
        .property_defs(&component.type_ref, &form.backend)
        .into_iter()
        .map(|def| PropertyRow {
            name: def.name.clone(),
            category: def.category.clone(),
            description: def.description.clone(),
            ty: def.ty.clone(),
            default: def.default.clone(),
            value: component.get(&def.name).cloned(),
            backend_specific: def.backend.is_some(),
            platforms: def.platforms.clone(),
        })
        .collect();
    Some(rows)
}

/// The events of `element` on `form`; `forms` (the whole project) resolves
/// parameter types that follow referenced components (SQLx row type).
pub fn events(
    forms: &[Form],
    form: &Form,
    element: &ComponentName,
    registry: &TypeRegistry,
) -> Option<Vec<EventRow>> {
    let el = form.element(element)?;
    let component = el.component();
    let ty = registry.get(&component.type_ref)?;
    Some(
        ty.events
            .iter()
            .map(|e| EventRow {
                name: e.name.clone(),
                description: e.description.clone(),
                params: tdr_core::codegen::event_params(forms, form, el, registry, e),
                returns: e.returns.clone(),
                handler: component.events.get(&e.name).cloned(),
            })
            .collect(),
    )
}

/// The handler name the designer proposes when an event is double-clicked:
/// `ok_button` + `on_click` → `ok_button_click` (like Delphi's
/// `OkButtonClick`).
pub fn suggest_handler(element: &ComponentName, event: &str) -> HandlerName {
    let event = event.strip_prefix("on_").unwrap_or(event);
    HandlerName::new(format!("{element}_{event}"))
        .unwrap_or_else(|_| HandlerName::new(format!("on_{element}")).expect("valid identifier"))
}
