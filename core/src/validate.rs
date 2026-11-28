//! Design-time validation of a [`Form`] against the known component types.
//!
//! Everything that can be decided from a single form is checked here: names,
//! types, properties, events, references, required capabilities connected
//! via reference properties, and layout rules. Project-wide questions
//! (conditional requirements, cycles, initialisation order) belong to the
//! dependency graph.

use std::collections::BTreeSet;
use std::fmt;

use serde::Serialize;

use crate::capability::{Capability, Condition, RequireKind};
use crate::component_type::ComponentType;
use crate::element::Element;
use crate::form::Form;
use crate::name::{ComponentName, ComponentPath};
use crate::platform::Platform;
use crate::registry::TypeRegistry;
use crate::type_ref::TypeRef;
use crate::value::{Mismatch, PropertyType, Value};
use crate::widget::{LayoutKind, Placement, Widget};

/// What validation needs to know about the project.
#[derive(Debug, Clone, Copy)]
pub struct ValidationContext<'a> {
    pub registry: &'a TypeRegistry,
    /// Does the UI backend allow absolute positioning? (Comes from the
    /// backend descriptor later.)
    pub absolute_positioning: bool,
}

impl<'a> ValidationContext<'a> {
    pub fn new(registry: &'a TypeRegistry) -> Self {
        Self {
            registry,
            absolute_positioning: true,
        }
    }

    /// The context for forms of `backend`: absolute positioning as its
    /// descriptor allows (allowed if the backend is unknown).
    pub fn for_backend(registry: &'a TypeRegistry, backend: &str) -> Self {
        Self {
            registry,
            absolute_positioning: registry
                .backend(backend)
                .is_none_or(|b| b.absolute_positioning),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Warning,
    Error,
}

/// What is wrong. Serialized as `{"code": "unknown_type", "detail": …}`;
/// the codes are stable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum Issue {
    DuplicateName,
    UnknownType(TypeRef),
    /// A non-visual type is used as a widget.
    NotAWidgetType(TypeRef),
    /// A visual type is used as a non-visual component.
    WidgetTypeAsComponent(TypeRef),
    ChildrenNotAllowed,
    UnknownProperty(String),
    /// The property exists, but only for another backend.
    PropertyOfOtherBackend {
        property: String,
        backend: String,
    },
    TypeMismatch {
        property: String,
        mismatch: Mismatch,
    },
    PlatformNotAllowed {
        property: String,
        platform: Platform,
    },
    MissingRequired(String),
    UnknownEvent(String),
    DanglingReference {
        property: String,
        target: ComponentPath,
    },
    SelfReference(String),
    MissingCapability {
        property: String,
        target: ComponentPath,
        capability: Capability,
    },
    RequirementNotConnected {
        capability: Capability,
        property: String,
    },
    RequirementPropertyMissing {
        capability: Capability,
        property: String,
    },
    AbsoluteNotSupported,
    /// Absolute placement inside a parent whose layout is not absolute.
    AbsoluteInLayout,
    /// A grid cell outside a grid layout.
    GridCellWithoutGrid,
}

/// One finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub severity: Severity,
    /// The element concerned (`None` = the form as a whole).
    pub element: Option<ComponentName>,
    pub issue: Issue,
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Issue::DuplicateName => f.write_str("name is used more than once"),
            Issue::UnknownType(t) => write!(f, "unknown type {t}"),
            Issue::NotAWidgetType(t) => write!(f, "{t} is not a widget type"),
            Issue::WidgetTypeAsComponent(t) => {
                write!(f, "{t} is a widget type and must be placed as a widget")
            }
            Issue::ChildrenNotAllowed => f.write_str("only containers may have children"),
            Issue::UnknownProperty(p) => write!(f, "unknown property {p:?}"),
            Issue::PropertyOfOtherBackend { property, backend } => {
                write!(
                    f,
                    "property {property:?} only exists for backend {backend:?}"
                )
            }
            Issue::TypeMismatch { property, mismatch } => {
                write!(f, "property {property:?}: {mismatch}")
            }
            Issue::PlatformNotAllowed { property, platform } => {
                write!(f, "property {property:?} has no meaning on {platform}")
            }
            Issue::MissingRequired(p) => write!(f, "required property {p:?} is not set"),
            Issue::UnknownEvent(e) => write!(f, "unknown event {e:?}"),
            Issue::DanglingReference { property, target } => {
                write!(
                    f,
                    "property {property:?} refers to missing component {target}"
                )
            }
            Issue::SelfReference(p) => write!(f, "property {p:?} refers to the component itself"),
            Issue::MissingCapability {
                property,
                target,
                capability,
            } => {
                write!(
                    f,
                    "property {property:?}: {target} does not provide {capability}"
                )
            }
            Issue::RequirementNotConnected {
                capability,
                property,
            } => {
                write!(
                    f,
                    "requires {capability}, but property {property:?} is not connected"
                )
            }
            Issue::RequirementPropertyMissing {
                capability,
                property,
            } => {
                write!(
                    f,
                    "type requires {capability} via property {property:?}, which the type does not declare"
                )
            }
            Issue::AbsoluteNotSupported => {
                f.write_str("the UI backend does not support absolute positioning")
            }
            Issue::AbsoluteInLayout => {
                f.write_str("absolute placement, but the parent arranges its children by layout")
            }
            Issue::GridCellWithoutGrid => {
                f.write_str("grid cell given, but the parent has no grid layout")
            }
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let level = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        match &self.element {
            Some(name) => write!(f, "{level}: {name}: {}", self.issue),
            None => write!(f, "{level}: {}", self.issue),
        }
    }
}

struct Checker<'a> {
    form: &'a Form,
    context: ValidationContext<'a>,
    out: Vec<Diagnostic>,
}

impl<'a> Checker<'a> {
    fn report(&mut self, severity: Severity, element: &ComponentName, issue: Issue) {
        self.out.push(Diagnostic {
            severity,
            element: Some(element.clone()),
            issue,
        });
    }

    fn error(&mut self, element: &ComponentName, issue: Issue) {
        self.report(Severity::Error, element, issue);
    }

    fn names(&mut self) {
        let mut seen = BTreeSet::new();
        let mut reported = BTreeSet::new();
        for element in self.form.elements() {
            let name = element.name();
            if !seen.insert(name) && reported.insert(name) {
                self.error(name, Issue::DuplicateName);
            }
        }
    }

    /// Returns the type if it exists and matches visual/non-visual use.
    fn resolve(&mut self, element: &dyn Element) -> Option<&'a ComponentType> {
        let registry = self.context.registry;
        let name = element.name().clone();
        let Some(ty) = registry.get(element.type_ref()) else {
            self.error(&name, Issue::UnknownType(element.type_ref().clone()));
            return None;
        };
        match (element.is_widget(), ty.kind.is_widget()) {
            (true, false) => self.error(&name, Issue::NotAWidgetType(ty.type_ref.clone())),
            (false, true) => self.error(&name, Issue::WidgetTypeAsComponent(ty.type_ref.clone())),
            _ => {}
        }
        Some(ty)
    }

    fn properties(&mut self, element: &dyn Element, ty: &ComponentType) {
        let registry = self.context.registry;
        let backend = &self.form.backend;
        let component = element.component();
        let name = &component.name;

        for (property, value) in &component.properties {
            let Some(def) = registry.property_def(&ty.type_ref, backend, property) else {
                let issue = match registry.property_backend(&ty.type_ref, property) {
                    Some(other) => Issue::PropertyOfOtherBackend {
                        property: property.clone(),
                        backend: other,
                    },
                    None => Issue::UnknownProperty(property.clone()),
                };
                self.error(name, issue);
                continue;
            };
            for platform in value.platforms.keys() {
                if !def.allows_platform(platform) {
                    self.report(
                        Severity::Warning,
                        name,
                        Issue::PlatformNotAllowed {
                            property: property.clone(),
                            platform: platform.clone(),
                        },
                    );
                }
            }
            for v in value.all_values() {
                if let Err(mismatch) = v.check(&def.ty) {
                    self.error(
                        name,
                        Issue::TypeMismatch {
                            property: property.clone(),
                            mismatch,
                        },
                    );
                }
                self.references(name, property, v, &def.ty);
            }
        }

        for def in registry.property_defs(&ty.type_ref, backend) {
            if def.is_required() && !component.properties.contains_key(&def.name) {
                self.error(name, Issue::MissingRequired(def.name.clone()));
            }
        }
    }

    fn references(
        &mut self,
        name: &ComponentName,
        property: &str,
        value: &Value,
        ty: &PropertyType,
    ) {
        let capability = reference_capability(ty);
        let own = &self.form.name;
        for target in value.references() {
            // References to other forms are checked by the dependency graph.
            if target.target_form(own) != own {
                continue;
            }
            if target.points_to(own, own, name) {
                self.error(name, Issue::SelfReference(property.to_owned()));
                continue;
            }
            let Some(element) = self.form.element(&target.name) else {
                self.error(
                    name,
                    Issue::DanglingReference {
                        property: property.to_owned(),
                        target: target.clone(),
                    },
                );
                continue;
            };
            if let Some(capability) = capability {
                let provides = self
                    .context
                    .registry
                    .get(element.type_ref())
                    .is_some_and(|t| t.provides(capability));
                if !provides {
                    self.error(
                        name,
                        Issue::MissingCapability {
                            property: property.to_owned(),
                            target: target.clone(),
                            capability: capability.clone(),
                        },
                    );
                }
            }
        }
    }

    /// Mandatory requirements connected via a reference property. Conditions
    /// that need the whole project are left to the dependency graph.
    fn requirements(&mut self, element: &dyn Element, ty: &ComponentType) {
        let component = element.component();
        let backend = &self.form.backend;
        for require in &ty.requires {
            let Some(property) = &require.via else {
                continue;
            };
            if self
                .context
                .registry
                .property_def(&ty.type_ref, backend, property)
                .is_none()
            {
                self.error(
                    &component.name,
                    Issue::RequirementPropertyMissing {
                        capability: require.capability.clone(),
                        property: property.clone(),
                    },
                );
                continue;
            }
            let active = match &require.condition {
                Condition::Always => true,
                Condition::PropertySet(p) => component.get(p).is_some_and(|v| !is_empty(&v.value)),
                Condition::ReferencedBy(_) | Condition::ProjectProvides(_) => false,
            };
            let connected = component
                .get(property)
                .is_some_and(|v| !v.value.references().is_empty());
            if active && require.kind == RequireKind::Mandatory && !connected {
                self.error(
                    &component.name,
                    Issue::RequirementNotConnected {
                        capability: require.capability.clone(),
                        property: property.clone(),
                    },
                );
            }
        }
    }

    fn events(&mut self, element: &dyn Element, ty: &ComponentType) {
        let component = element.component();
        let backend = self.form.backend.as_str();
        for event in component.events.keys() {
            if self
                .context
                .registry
                .event_def(&ty.type_ref, backend, event)
                .is_none()
            {
                self.error(&component.name, Issue::UnknownEvent(event.clone()));
            }
        }
    }

    fn widget_tree(&mut self, widget: &Widget, parent_layout: Option<&LayoutKind>) {
        let name = &widget.component.name;
        let container = self
            .context
            .registry
            .get(&widget.component.type_ref)
            .is_some_and(|t| t.kind.is_container());
        if !widget.children.is_empty() && !container {
            self.error(name, Issue::ChildrenNotAllowed);
        }

        let own_layout = widget.visual.layout.as_ref().map(|l| &l.kind);
        if own_layout == Some(&LayoutKind::Absolute) && !self.context.absolute_positioning {
            self.error(name, Issue::AbsoluteNotSupported);
        }
        match &widget.visual.placement {
            Placement::Absolute { .. } => {
                if !self.context.absolute_positioning {
                    self.error(name, Issue::AbsoluteNotSupported);
                } else if parent_layout.is_some_and(|l| *l != LayoutKind::Absolute) {
                    self.report(Severity::Warning, name, Issue::AbsoluteInLayout);
                }
            }
            Placement::Layout { cell: Some(_), .. } => {
                if !matches!(parent_layout, Some(LayoutKind::Grid { .. })) {
                    self.report(Severity::Warning, name, Issue::GridCellWithoutGrid);
                }
            }
            Placement::Layout { .. } => {}
        }

        // A container without explicit layout arranges its children vertically.
        let layout = own_layout.cloned().unwrap_or_default();
        for child in &widget.children {
            self.widget_tree(child, Some(&layout));
        }
    }
}

/// The capability a reference property (or list of references) demands.
pub(crate) fn reference_capability(ty: &PropertyType) -> Option<&Capability> {
    match ty {
        PropertyType::Reference(capability) => capability.as_ref(),
        PropertyType::List(inner) => match inner.as_ref() {
            PropertyType::Reference(capability) => capability.as_ref(),
            _ => None,
        },
        _ => None,
    }
}

pub(crate) fn is_empty(value: &Value) -> bool {
    match value {
        Value::Text(t) | Value::Code(t) => t.is_empty(),
        Value::Reference(r) => r.is_none(),
        Value::List(items) => items.is_empty(),
        _ => false,
    }
}

/// Validates `form`. Errors make the form unusable for code generation;
/// warnings point at things that are probably not intended.
pub fn validate(form: &Form, context: &ValidationContext<'_>) -> Vec<Diagnostic> {
    let mut checker = Checker {
        form,
        context: *context,
        out: Vec::new(),
    };
    checker.names();
    for element in form.elements() {
        if let Some(ty) = checker.resolve(element) {
            checker.properties(element, ty);
            checker.events(element, ty);
            checker.requirements(element, ty);
        }
    }
    if let Some(root) = &form.root {
        checker.widget_tree(root, None);
    }
    checker.out.sort_by_key(|d| std::cmp::Reverse(d.severity));
    checker.out
}
