//! Dependency graph: references across forms, requirements with project-wide
//! conditions, suggestions, cycles, initialisation order, Cargo dependencies
//! and editing across forms.

use tdr_core::graph::{EdgeKind, Graph, ProjectIssue, Suggestion, analyze};
use tdr_core::*;

fn name(n: &str) -> ComponentName {
    ComponentName::new(n).unwrap()
}

fn cap(c: &str) -> Capability {
    Capability::new(c).unwrap()
}

fn path(p: &str) -> ComponentPath {
    p.parse().unwrap()
}

fn reference(capability: Option<&str>) -> PropertyType {
    PropertyType::Reference(capability.map(cap))
}

/// Example packs: tokio, sqlx, axum and a generic "Node" for cycles.
fn registry() -> TypeRegistry {
    let mut r = TypeRegistry::with_builtins();
    r.register(
        ComponentType::new(TypeRef::pack("tokio", "Runtime"), TypeKind::NonVisual)
            .with_provide(Provide {
                capability: cap("tokio.runtime"),
                description: String::new(),
            })
            .with_cargo(CargoDependency::new("tokio", "1").with_features(["rt-multi-thread"])),
    )
    .unwrap();
    r.register(
        ComponentType::new(TypeRef::pack("sqlx", "Pool"), TypeKind::NonVisual)
            .with_property(PropertyDef::new(
                "url",
                PropertyType::Text,
                "postgres://localhost",
            ))
            .with_provide(Provide {
                capability: cap("sqlx.pool"),
                description: String::new(),
            })
            // Needs a runtime somewhere in the project (no property).
            .with_require(Require {
                capability: cap("tokio.runtime"),
                kind: RequireKind::Mandatory,
                via: None,
                condition: Condition::Always,
                description: String::new(),
            })
            .with_cargo(
                CargoDependency::new("sqlx", "0.8").with_features(["runtime-tokio", "postgres"]),
            ),
    )
    .unwrap();
    r.register(
        ComponentType::new(TypeRef::pack("sqlx", "Query"), TypeKind::NonVisual)
            .with_property(PropertyDef::new(
                "pool",
                reference(Some("sqlx.pool")),
                Value::Reference(None),
            ))
            .with_property(PropertyDef::new(
                "router",
                reference(Some("http.router")),
                Value::Reference(None),
            ))
            .with_provide(Provide {
                capability: cap("sqlx.query"),
                description: String::new(),
            })
            .with_require(Require::via(cap("sqlx.pool"), "pool"))
            .with_cargo(CargoDependency::new("sqlx", "0.8").with_features(["macros"])),
    )
    .unwrap();
    r.register(
        ComponentType::new(TypeRef::pack("axum", "Router"), TypeKind::NonVisual)
            .with_property(PropertyDef::new(
                "pool",
                reference(Some("sqlx.pool")),
                Value::Reference(None),
            ))
            .with_provide(Provide {
                capability: cap("http.router"),
                description: String::new(),
            })
            // Needs a pool only if a query is attached to it.
            .with_require(Require {
                capability: cap("sqlx.pool"),
                kind: RequireKind::Mandatory,
                via: Some("pool".into()),
                condition: Condition::ReferencedBy(cap("sqlx.query")),
                description: String::new(),
            })
            .with_cargo(CargoDependency::new("axum", "0.8")),
    )
    .unwrap();
    r.register(
        ComponentType::new(TypeRef::pack("test", "Node"), TypeKind::NonVisual).with_property(
            PropertyDef::new("next", reference(None), Value::Reference(None)),
        ),
    )
    .unwrap();
    r
}

/// services (data module): runtime, db_pool
/// main (form):            main_window, router, load_customers → services::db_pool
fn project() -> Vec<Form> {
    let mut services = Form::data_module(name("services"), "egui");
    services
        .add_component(Component::new(
            name("runtime"),
            TypeRef::pack("tokio", "Runtime"),
        ))
        .unwrap();
    services
        .add_component(Component::new(
            name("db_pool"),
            TypeRef::pack("sqlx", "Pool"),
        ))
        .unwrap();

    let mut main = Form::new(
        name("main"),
        "egui",
        Widget::new(name("main_window"), builtin::form()),
    );
    main.add_component(Component::new(
        name("router"),
        TypeRef::pack("axum", "Router"),
    ))
    .unwrap();
    main.add_component(
        Component::new(name("load_customers"), TypeRef::pack("sqlx", "Query")).with(
            "pool",
            Value::reference_on(name("services"), name("db_pool")),
        ),
    )
    .unwrap();
    vec![services, main]
}

fn issues(forms: &[Form]) -> Vec<ProjectIssue> {
    analyze(forms, &registry())
        .diagnostics
        .into_iter()
        .map(|d| d.issue)
        .collect()
}

fn order(forms: &[Form]) -> Vec<String> {
    analyze(forms, &registry())
        .order
        .iter()
        .map(ToString::to_string)
        .collect()
}

#[test]
fn valid_project() {
    let forms = project();
    let registry = registry();
    let analysis = analyze(&forms, &registry);
    assert!(
        analysis.diagnostics.is_empty(),
        "{:#?}",
        analysis.diagnostics
    );
    for form in &forms {
        assert!(form.validate(&ValidationContext::new(&registry)).is_empty());
    }
    // Dependencies first, otherwise project order.
    assert_eq!(
        order(&forms),
        [
            "services::runtime",
            "services::db_pool",
            "main::main_window",
            "main::router",
            "main::load_customers"
        ]
    );
}

#[test]
fn edges_and_queries() {
    let forms = project();
    let registry = registry();
    let graph = Graph::build(&forms, &registry);
    let query = graph.find(&path("main::load_customers")).unwrap();
    let pool = graph.find(&path("services::db_pool")).unwrap();
    let runtime = graph.find(&path("services::runtime")).unwrap();
    assert_eq!(graph.dependencies(query), [pool]);
    assert_eq!(graph.dependencies(pool), [runtime]);
    assert_eq!(graph.dependents(pool), [query]);
    assert!(graph.edges().iter().any(|e| e.from == pool
        && e.to == runtime
        && e.kind
            == EdgeKind::Implicit {
                capability: cap("tokio.runtime")
            }));
    assert_eq!(
        graph.providers(&cap("sqlx.pool")).collect::<Vec<_>>(),
        [pool]
    );
}

#[test]
fn cargo_dependencies_are_merged() {
    let analysis = analyze(&project(), &registry());
    let summary: Vec<(String, Vec<String>)> = analysis
        .cargo
        .iter()
        .map(|d| {
            (
                format!("{} {}", d.crate_name, d.version),
                d.features.clone(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("axum 0.8".to_owned(), vec![]),
            (
                "sqlx 0.8".to_owned(),
                vec![
                    "macros".to_owned(),
                    "postgres".to_owned(),
                    "runtime-tokio".to_owned()
                ]
            ),
            ("tokio 1".to_owned(), vec!["rt-multi-thread".to_owned()]),
        ]
    );

    let mut registry = registry();
    registry
        .register(
            ComponentType::new(TypeRef::pack("old", "Thing"), TypeKind::NonVisual)
                .with_cargo(CargoDependency::new("tokio", "0.2")),
        )
        .unwrap();
    let mut forms = project();
    forms[0]
        .add_component(Component::new(name("thing"), TypeRef::pack("old", "Thing")))
        .unwrap();
    let analysis = analyze(&forms, &registry);
    assert!(analysis.diagnostics.iter().any(|d| d.issue
        == ProjectIssue::CargoVersionConflict {
            crate_name: "tokio".into(),
            versions: vec!["0.2".into(), "1".into()],
        }));
}

#[test]
fn references_across_forms() {
    let mut forms = project();
    let query = forms[1].component_mut(&name("load_customers")).unwrap();
    query.set(
        "pool",
        Value::reference_on(name("nowhere"), name("db_pool")),
    );
    assert!(matches!(
        &issues(&forms)[..],
        [ProjectIssue::UnknownForm { .. }, ..]
    ));

    let query = forms[1].component_mut(&name("load_customers")).unwrap();
    query.set(
        "pool",
        Value::reference_on(name("services"), name("old_pool")),
    );
    assert!(issues(&forms).contains(&ProjectIssue::DanglingReference {
        property: "pool".into(),
        target: path("services::old_pool")
    }));

    let query = forms[1].component_mut(&name("load_customers")).unwrap();
    query.set(
        "pool",
        Value::reference_on(name("services"), name("runtime")),
    );
    assert!(issues(&forms).contains(&ProjectIssue::MissingCapability {
        property: "pool".into(),
        target: path("services::runtime"),
        capability: cap("sqlx.pool")
    }));
}

#[test]
fn implicit_requirement_suggests_creating_a_provider() {
    let mut forms = project();
    forms[0].remove(&name("runtime")).unwrap();
    assert_eq!(
        issues(&forms),
        [ProjectIssue::Unsatisfied {
            capability: cap("tokio.runtime"),
            via: None,
            suggestion: Suggestion::Create(vec![TypeRef::pack("tokio", "Runtime")]),
        }]
    );
}

#[test]
fn conditional_requirement() {
    // The router needs a pool only once a query references it.
    let mut forms = project();
    assert!(issues(&forms).is_empty());
    forms[1]
        .component_mut(&name("load_customers"))
        .unwrap()
        .set("router", Value::reference(name("router")));
    let analysis = analyze(&forms, &registry());
    assert_eq!(analysis.diagnostics.len(), 1, "{:#?}", analysis.diagnostics);
    let diagnostic = &analysis.diagnostics[0];
    assert_eq!(diagnostic.element, Some(path("main::router")));
    assert_eq!(
        diagnostic.issue,
        ProjectIssue::Unsatisfied {
            capability: cap("sqlx.pool"),
            via: Some("pool".into()),
            suggestion: Suggestion::Connect(vec![path("services::db_pool")]),
        }
    );
    assert_eq!(
        diagnostic.to_string(),
        "error: main::router: requires sqlx.pool via property \"pool\" – connect services::db_pool"
    );

    // Connected: fine again, and the router now comes after the pool.
    forms[1].component_mut(&name("router")).unwrap().set(
        "pool",
        Value::reference_on(name("services"), name("db_pool")),
    );
    assert!(issues(&forms).is_empty());
    let order = order(&forms);
    let position = |p: &str| order.iter().position(|o| o == p).unwrap();
    assert!(position("services::db_pool") < position("main::router"));
    // The query references the router, so it comes after it.
    assert!(position("main::router") < position("main::load_customers"));
}

#[test]
fn cycles_are_detected() {
    let mut forms = project();
    let node = |n: &str, next: &str| {
        Component::new(name(n), TypeRef::pack("test", "Node"))
            .with("next", Value::reference(name(next)))
    };
    forms[1].add_component(node("a", "b")).unwrap();
    forms[1].add_component(node("b", "c")).unwrap();
    forms[1].add_component(node("c", "a")).unwrap();
    forms[1].add_component(node("d", "a")).unwrap();
    let analysis = analyze(&forms, &registry());
    assert_eq!(
        analysis.cycles,
        [vec![path("main::a"), path("main::b"), path("main::c")]]
    );
    assert!(analysis.has_errors());
    // Cycle members and everything depending on them cannot be ordered.
    let ordered: Vec<String> = analysis.order.iter().map(ToString::to_string).collect();
    assert!(
        !ordered
            .iter()
            .any(|p| ["main::a", "main::b", "main::c", "main::d"].contains(&p.as_str()))
    );
    assert_eq!(ordered.len(), 5);
}

#[test]
fn duplicate_form_names() {
    let mut forms = project();
    forms.push(Form::data_module(name("services"), "egui"));
    assert!(issues(&forms).contains(&ProjectIssue::DuplicateForm(name("services"))));
}

#[test]
fn editing_across_forms() {
    let mut forms = project();

    // Renaming a component updates qualified references on other forms.
    project::rename_component(
        &mut forms,
        &name("services"),
        &name("db_pool"),
        name("main_db"),
    )
    .unwrap();
    let refs = |forms: &[Form]| -> Vec<String> {
        forms[1]
            .component(&name("load_customers"))
            .unwrap()
            .references()
            .iter()
            .map(|(_, p)| p.to_string())
            .collect()
    };
    assert_eq!(refs(&forms), ["services::main_db"]);

    // Renaming the form updates the qualifier.
    project::rename_form(&mut forms, &name("services"), name("data")).unwrap();
    assert_eq!(refs(&forms), ["data::main_db"]);
    assert_eq!(
        project::rename_form(&mut forms, &name("data"), name("main")),
        Err(ModelError::DuplicateName(name("main")))
    );

    // Removing it clears the reference everywhere.
    project::remove_component(&mut forms, &name("data"), &name("main_db")).unwrap();
    assert!(refs(&forms).is_empty());
    assert!(analyze(&forms, &registry()).diagnostics.is_empty());
    // … but the query's own form validation now reports the missing pool.
    let registry = registry();
    let diagnostics = forms[1].validate(&ValidationContext::new(&registry));
    assert!(
        diagnostics
            .iter()
            .any(|d| matches!(d.issue, Issue::RequirementNotConnected { .. }))
    );
}

#[test]
fn data_module_round_trip() {
    let forms = project();
    let text = forms[0].to_ron().unwrap();
    assert!(!text.contains("root"), "{text}");
    let back = Form::from_ron(&text).unwrap();
    assert!(back.is_data_module());
    assert_eq!(back, forms[0]);
    let text = forms[1].to_ron().unwrap();
    assert!(
        text.contains(r#""pool": Reference(Some("services::db_pool"))"#),
        "{text}"
    );
    assert_eq!(Form::from_ron(&text).unwrap(), forms[1]);
}
