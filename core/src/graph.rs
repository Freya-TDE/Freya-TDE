//! The project-wide dependency graph.
//!
//! Nodes are all elements of all forms (widgets and components, including
//! data modules). An edge points from a component to something it depends
//! on: a referenced component, or the provider of an implicit requirement
//! (a [`crate::Require`] without `via`, e.g. a runtime).
//!
//! [`analyze`] uses the graph to
//!
//! * check references across forms (`services::db_pool`),
//! * evaluate requirements including project-wide conditions and suggest how
//!   to fulfil missing ones (connect an existing provider or create one),
//! * detect cycles,
//! * derive the initialisation order for the generated code,
//! * collect the Cargo dependencies of all used types.
//!
//! Checks that need only one form are done by [`crate::validate()`]; run both.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

use serde::Serialize;

use crate::capability::{Capability, Condition, RequireKind};
use crate::component_type::{CargoDependency, ComponentType};
use crate::element::Element;
use crate::form::Form;
use crate::name::{ComponentName, ComponentPath};
use crate::registry::TypeRegistry;
use crate::type_ref::TypeRef;
use crate::validate::{Severity, is_empty, reference_capability};

/// One element of the project.
#[derive(Clone, Copy)]
pub struct Node<'a> {
    pub form: &'a Form,
    pub element: &'a dyn Element,
    /// `None` if the type is unknown (reported by form validation).
    pub ty: Option<&'a ComponentType>,
}

impl Node<'_> {
    /// The fully qualified path (`form::name`).
    pub fn path(&self) -> ComponentPath {
        ComponentPath::on(self.form.name.clone(), self.element.name().clone())
    }

    fn provides(&self, capability: &Capability) -> bool {
        self.ty.is_some_and(|t| t.provides(capability))
    }
}

impl fmt::Debug for Node<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Node({})", self.path())
    }
}

/// Why one node depends on another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EdgeKind {
    /// A reference property points to the dependency.
    Reference { property: String },
    /// An implicit requirement (`via: None`) is fulfilled by the dependency.
    Implicit { capability: Capability },
}

/// `from` depends on `to` (indices into [`Graph::nodes`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub kind: EdgeKind,
}

/// How a missing requirement could be fulfilled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum Suggestion {
    /// Connect one of these existing providers.
    Connect(Vec<ComponentPath>),
    /// Create a component of one of these types.
    Create(Vec<TypeRef>),
    /// No known component type provides the capability.
    Nothing,
}

/// A project-level finding. Serialized with a stable `code` like
/// [`crate::Issue`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum ProjectIssue {
    DuplicateForm(ComponentName),
    UnknownForm {
        property: String,
        target: ComponentPath,
    },
    DanglingReference {
        property: String,
        target: ComponentPath,
    },
    MissingCapability {
        property: String,
        target: ComponentPath,
        capability: Capability,
    },
    /// A mandatory, active requirement is not fulfilled.
    Unsatisfied {
        capability: Capability,
        via: Option<String>,
        suggestion: Suggestion,
    },
    /// These elements depend on each other in a circle.
    Cycle(Vec<ComponentPath>),
    /// Two used types need different versions of the same crate.
    CargoVersionConflict {
        crate_name: String,
        versions: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectDiagnostic {
    pub severity: Severity,
    /// The element concerned, fully qualified (`None` = the project).
    pub element: Option<ComponentPath>,
    pub issue: ProjectIssue,
}

fn list<T: fmt::Display>(items: &[T]) -> String {
    items
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

impl fmt::Display for ProjectIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProjectIssue::DuplicateForm(n) => write!(f, "form name {n} is used more than once"),
            ProjectIssue::UnknownForm { property, target } => {
                write!(
                    f,
                    "property {property:?} refers to {target}, but the project has no such form"
                )
            }
            ProjectIssue::DanglingReference { property, target } => {
                write!(
                    f,
                    "property {property:?} refers to missing component {target}"
                )
            }
            ProjectIssue::MissingCapability {
                property,
                target,
                capability,
            } => {
                write!(
                    f,
                    "property {property:?}: {target} does not provide {capability}"
                )
            }
            ProjectIssue::Unsatisfied {
                capability,
                via,
                suggestion,
            } => {
                match via {
                    Some(p) => write!(f, "requires {capability} via property {p:?}")?,
                    None => write!(f, "requires {capability} in the project")?,
                }
                match suggestion {
                    Suggestion::Connect(paths) => write!(f, " – connect {}", list(paths)),
                    Suggestion::Create(types) => {
                        write!(f, " – add a component of type {}", list(types))
                    }
                    Suggestion::Nothing => f.write_str(" – no known component provides it"),
                }
            }
            ProjectIssue::Cycle(paths) => write!(f, "circular dependency: {}", list(paths)),
            ProjectIssue::CargoVersionConflict {
                crate_name,
                versions,
            } => {
                write!(
                    f,
                    "crate {crate_name} is required in different versions: {}",
                    versions.join(", ")
                )
            }
        }
    }
}

impl fmt::Display for ProjectDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let level = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        match &self.element {
            Some(path) => write!(f, "{level}: {path}: {}", self.issue),
            None => write!(f, "{level}: {}", self.issue),
        }
    }
}

/// The dependency graph of a project.
pub struct Graph<'a> {
    nodes: Vec<Node<'a>>,
    edges: Vec<Edge>,
    index: HashMap<(&'a ComponentName, &'a ComponentName), usize>,
}

impl<'a> Graph<'a> {
    /// Builds the graph from all forms of a project. Only references that
    /// resolve become edges; [`analyze`] reports the others.
    pub fn build(forms: &'a [Form], registry: &'a TypeRegistry) -> Self {
        let mut nodes = Vec::new();
        let mut index = HashMap::new();
        for form in forms {
            for element in form.elements() {
                // First occurrence wins on duplicates (reported elsewhere).
                index
                    .entry((&form.name, element.name()))
                    .or_insert(nodes.len());
                nodes.push(Node {
                    form,
                    element,
                    ty: registry.get(element.type_ref()),
                });
            }
        }
        let mut graph = Self {
            nodes,
            edges: Vec::new(),
            index,
        };

        // Reference edges.
        for (from, node) in graph.nodes.iter().enumerate() {
            for (property, target) in node.element.component().references() {
                if let Some(to) = graph.resolve(node.form, target)
                    && to != from
                {
                    graph.edges.push(Edge {
                        from,
                        to,
                        kind: EdgeKind::Reference {
                            property: property.to_owned(),
                        },
                    });
                }
            }
        }
        // Implicit requirement edges (to every provider in the project).
        let mut implicit = Vec::new();
        for (from, node) in graph.nodes.iter().enumerate() {
            for require in node.ty.into_iter().flat_map(|t| &t.requires) {
                if require.via.is_none() {
                    for to in graph
                        .providers(&require.capability)
                        .filter(|&to| to != from)
                    {
                        implicit.push(Edge {
                            from,
                            to,
                            kind: EdgeKind::Implicit {
                                capability: require.capability.clone(),
                            },
                        });
                    }
                }
            }
        }
        graph.edges.extend(implicit);
        graph
    }

    pub fn nodes(&self) -> &[Node<'a>] {
        &self.nodes
    }

    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    /// The node a reference written on `form` points to.
    pub fn resolve(&self, form: &Form, target: &ComponentPath) -> Option<usize> {
        let form_name = target.target_form(&form.name);
        self.index.get(&(form_name, &target.name)).copied()
    }

    /// Looks up a node by its fully qualified path.
    pub fn find(&self, path: &ComponentPath) -> Option<usize> {
        let form = path.form.as_ref()?;
        self.index.get(&(form, &path.name)).copied()
    }

    /// All nodes whose type provides `capability`.
    pub fn providers<'s>(&'s self, capability: &'s Capability) -> impl Iterator<Item = usize> + 's {
        self.nodes
            .iter()
            .enumerate()
            .filter(move |(_, n)| n.provides(capability))
            .map(|(i, _)| i)
    }

    /// What `node` depends on (distinct, in edge order).
    pub fn dependencies(&self, node: usize) -> Vec<usize> {
        let mut out = Vec::new();
        for e in self.edges.iter().filter(|e| e.from == node) {
            if !out.contains(&e.to) {
                out.push(e.to);
            }
        }
        out
    }

    /// What depends on `node` (distinct, in edge order).
    pub fn dependents(&self, node: usize) -> Vec<usize> {
        let mut out = Vec::new();
        for e in self.edges.iter().filter(|e| e.to == node) {
            if !out.contains(&e.from) {
                out.push(e.from);
            }
        }
        out
    }

    /// Groups of nodes that depend on each other in a circle (strongly
    /// connected components with more than one node), Tarjan's algorithm.
    pub fn cycles(&self) -> Vec<Vec<usize>> {
        struct Tarjan<'g, 'a> {
            graph: &'g Graph<'a>,
            index: Vec<Option<usize>>,
            low: Vec<usize>,
            on_stack: Vec<bool>,
            stack: Vec<usize>,
            next: usize,
            out: Vec<Vec<usize>>,
        }
        impl Tarjan<'_, '_> {
            fn visit(&mut self, v: usize) {
                self.index[v] = Some(self.next);
                self.low[v] = self.next;
                self.next += 1;
                self.stack.push(v);
                self.on_stack[v] = true;
                for w in self.graph.dependencies(v) {
                    match self.index[w] {
                        None => {
                            self.visit(w);
                            self.low[v] = self.low[v].min(self.low[w]);
                        }
                        Some(iw) if self.on_stack[w] => self.low[v] = self.low[v].min(iw),
                        Some(_) => {}
                    }
                }
                if Some(self.low[v]) == self.index[v] {
                    let mut group = Vec::new();
                    while let Some(w) = self.stack.pop() {
                        self.on_stack[w] = false;
                        group.push(w);
                        if w == v {
                            break;
                        }
                    }
                    if group.len() > 1 {
                        group.sort_unstable();
                        self.out.push(group);
                    }
                }
            }
        }
        let n = self.nodes.len();
        let mut tarjan = Tarjan {
            graph: self,
            index: vec![None; n],
            low: vec![0; n],
            on_stack: vec![false; n],
            stack: Vec::new(),
            next: 0,
            out: Vec::new(),
        };
        for v in 0..n {
            if tarjan.index[v].is_none() {
                tarjan.visit(v);
            }
        }
        let mut cycles = tarjan.out;
        cycles.sort();
        cycles
    }

    /// Initialisation order: every node after its dependencies; otherwise in
    /// project order (forms, then elements). Nodes on or behind a cycle are
    /// left out – they cannot be ordered.
    pub fn init_order(&self) -> Vec<usize> {
        let n = self.nodes.len();
        let mut missing: Vec<usize> = (0..n).map(|v| self.dependencies(v).len()).collect();
        let mut ready: BTreeSet<usize> = (0..n).filter(|&v| missing[v] == 0).collect();
        let mut order = Vec::with_capacity(n);
        while let Some(v) = ready.pop_first() {
            order.push(v);
            for d in self.dependents(v) {
                missing[d] -= 1;
                if missing[d] == 0 {
                    ready.insert(d);
                }
            }
        }
        order
    }
}

/// The result of [`analyze`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct Analysis {
    /// Initialisation order, dependencies first (all elements not on or
    /// behind a cycle).
    pub order: Vec<ComponentPath>,
    pub cycles: Vec<Vec<ComponentPath>>,
    /// Merged Cargo dependencies of all used types, sorted by crate name.
    pub cargo: Vec<CargoDependency>,
    pub diagnostics: Vec<ProjectDiagnostic>,
}

impl Analysis {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
}

/// Analyses all forms of a project (including data modules).
pub fn analyze(forms: &[Form], registry: &TypeRegistry) -> Analysis {
    let graph = Graph::build(forms, registry);
    let mut diagnostics = Vec::new();
    let mut report = |severity, element: Option<ComponentPath>, issue| {
        diagnostics.push(ProjectDiagnostic {
            severity,
            element,
            issue,
        });
    };

    // Form names must be unique – qualified references depend on it.
    let mut seen = BTreeSet::new();
    for form in forms {
        if !seen.insert(&form.name) {
            report(
                Severity::Error,
                None,
                ProjectIssue::DuplicateForm(form.name.clone()),
            );
        }
    }

    // References to other forms.
    let form_names: BTreeSet<&ComponentName> = forms.iter().map(|f| &f.name).collect();
    for node in graph.nodes() {
        let form = node.form;
        for (property, target) in node.element.component().references() {
            let target_form = target.target_form(&form.name);
            if target_form == &form.name {
                continue; // checked by form validation
            }
            let issue = if !form_names.contains(target_form) {
                Some(ProjectIssue::UnknownForm {
                    property: property.to_owned(),
                    target: target.clone(),
                })
            } else if let Some(to) = graph.resolve(form, target) {
                let capability = node
                    .ty
                    .and_then(|t| registry.property_def(&t.type_ref, &form.backend, property))
                    .and_then(|def| reference_capability(&def.ty));
                capability
                    .filter(|c| !graph.nodes()[to].provides(c))
                    .map(|c| ProjectIssue::MissingCapability {
                        property: property.to_owned(),
                        target: target.clone(),
                        capability: c.clone(),
                    })
            } else {
                Some(ProjectIssue::DanglingReference {
                    property: property.to_owned(),
                    target: target.clone(),
                })
            };
            if let Some(issue) = issue {
                report(Severity::Error, Some(node.path()), issue);
            }
        }
    }

    // Requirements with project-wide conditions.
    for (i, node) in graph.nodes().iter().enumerate() {
        let Some(ty) = node.ty else { continue };
        let component = node.element.component();
        for require in &ty.requires {
            if require.kind == RequireKind::Optional {
                continue;
            }
            let active = match &require.condition {
                Condition::Always => true,
                Condition::PropertySet(p) => component.get(p).is_some_and(|v| !is_empty(&v.value)),
                Condition::ReferencedBy(c) => graph
                    .dependents(i)
                    .iter()
                    .any(|&d| graph.nodes()[d].provides(c)),
                Condition::ProjectProvides(c) => graph.providers(c).next().is_some(),
            };
            if !active {
                continue;
            }
            let satisfied = match &require.via {
                Some(property) => graph.edges().iter().any(|e| {
                    e.from == i
                        && matches!(&e.kind, EdgeKind::Reference { property: p } if p == property)
                        && graph.nodes()[e.to].provides(&require.capability)
                }),
                None => graph.providers(&require.capability).any(|p| p != i),
            };
            // Unconnected `via` properties under `Always` are already reported
            // by form validation; report them here only for project conditions.
            let already_reported = require.via.is_some()
                && matches!(
                    require.condition,
                    Condition::Always | Condition::PropertySet(_)
                );
            if satisfied || already_reported {
                continue;
            }
            let existing: Vec<ComponentPath> = graph
                .providers(&require.capability)
                .filter(|&p| p != i)
                .map(|p| graph.nodes()[p].path())
                .collect();
            let suggestion = if !existing.is_empty() {
                Suggestion::Connect(existing)
            } else {
                let types: Vec<TypeRef> = registry
                    .types()
                    .filter(|t| t.provides(&require.capability))
                    .map(|t| t.type_ref.clone())
                    .collect();
                if types.is_empty() {
                    Suggestion::Nothing
                } else {
                    Suggestion::Create(types)
                }
            };
            report(
                Severity::Error,
                Some(node.path()),
                ProjectIssue::Unsatisfied {
                    capability: require.capability.clone(),
                    via: require.via.clone(),
                    suggestion,
                },
            );
        }
    }

    // Cycles.
    let cycles: Vec<Vec<ComponentPath>> = graph
        .cycles()
        .into_iter()
        .map(|group| group.into_iter().map(|v| graph.nodes()[v].path()).collect())
        .collect();
    for cycle in &cycles {
        report(Severity::Error, None, ProjectIssue::Cycle(cycle.clone()));
    }

    // Cargo dependencies of all used components; a dependency with `when`
    // only if the component's properties match.
    let mut cargo: BTreeMap<String, CargoDependency> = BTreeMap::new();
    let mut versions: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for node in graph.nodes() {
        let Some(ty) = node.ty else { continue };
        let value_of = |name: &str| {
            let component = node.element.component();
            let value = component.get(name).map(|v| v.value.clone()).or_else(|| {
                registry
                    .property_defs(&ty.type_ref, &node.form.backend)
                    .into_iter()
                    .find(|d| d.name == name)
                    .and_then(|d| d.default.clone())
            })?;
            value.as_condition_text()
        };
        for dep in ty.cargo.iter().filter(|d| d.applies(value_of)) {
            versions
                .entry(dep.crate_name.clone())
                .or_default()
                .insert(dep.version.clone());
            let merged = cargo
                .entry(dep.crate_name.clone())
                .or_insert_with(|| CargoDependency {
                    features: Vec::new(),
                    when: BTreeMap::new(),
                    ..dep.clone()
                });
            merged.default_features |= dep.default_features;
            for feature in &dep.features {
                if !merged.features.contains(feature) {
                    merged.features.push(feature.clone());
                }
            }
            merged.features.sort();
        }
    }
    for (crate_name, set) in versions {
        if set.len() > 1 {
            report(
                Severity::Error,
                None,
                ProjectIssue::CargoVersionConflict {
                    crate_name,
                    versions: set.into_iter().collect(),
                },
            );
        }
    }

    let order = graph
        .init_order()
        .into_iter()
        .map(|v| graph.nodes()[v].path())
        .collect();
    Analysis {
        order,
        cycles,
        cargo: cargo.into_values().collect(),
        diagnostics,
    }
}
