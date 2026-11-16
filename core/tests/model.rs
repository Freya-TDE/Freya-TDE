//! Integration tests: forms with widgets and non-visual components from a
//! small example pack, validation, editing operations and `.ftdr` files.

use tdr_core::*;

fn name(n: &str) -> ComponentName {
    ComponentName::new(n).unwrap()
}

fn handler(n: &str) -> HandlerName {
    HandlerName::new(n).unwrap()
}

fn cap(c: &str) -> Capability {
    Capability::new(c).unwrap()
}

/// Core types plus an example "sqlx" pack and an "egui" extension.
fn registry() -> TypeRegistry {
    let mut registry = TypeRegistry::with_builtins();
    registry
        .register(
            ComponentType::new(TypeRef::pack("sqlx", "Pool"), TypeKind::NonVisual)
                .with_category("Database")
                .with_property(PropertyDef::required("url", PropertyType::Text))
                .with_property(PropertyDef::new(
                    "max_connections",
                    PropertyType::Int {
                        min: Some(1),
                        max: Some(1000),
                    },
                    5,
                ))
                .with_provide(Provide {
                    capability: cap("sqlx.pool"),
                    description: "A database connection pool".into(),
                })
                .with_cargo(
                    CargoDependency::new("sqlx", "0.8")
                        .with_features(["runtime-tokio", "postgres"]),
                ),
        )
        .unwrap();
    registry
        .register(
            ComponentType::new(TypeRef::pack("sqlx", "Query"), TypeKind::NonVisual)
                .with_property(PropertyDef::new(
                    "pool",
                    PropertyType::Reference(Some(cap("sqlx.pool"))),
                    Value::Reference(None),
                ))
                .with_property(PropertyDef::new(
                    "sql",
                    PropertyType::Code,
                    Value::Code(String::new()),
                ))
                .with_event(EventDef::new("on_row").with_param("row", "&sqlx::postgres::PgRow"))
                .with_require(Require::via(cap("sqlx.pool"), "pool")),
        )
        .unwrap();
    registry
        .extend(
            "egui",
            &builtin::button(),
            [PropertyDef::new("small", PropertyType::Bool, false)],
        )
        .unwrap();
    registry
}

/// main_form
/// ├── toolbar (Container, horizontal)
/// │   ├── save_button
/// │   └── quit_button
/// └── name_edit
/// + db_pool, load_query (non-visual)
fn sample_form() -> Form {
    let toolbar = Widget::new(name("toolbar"), builtin::container())
        .with_layout(ContainerLayout {
            kind: LayoutKind::Horizontal,
            spacing: 6,
            padding: Insets::all(4),
        })
        .with_child(Widget::from_component(
            Component::new(name("save_button"), builtin::button())
                .with(
                    "caption",
                    PropertyValue::new("Save").on(Platform::MacOs, "Sichern"),
                )
                .with("small", true)
                .on("on_click", handler("save_clicked")),
        ))
        .with_child(Widget::from_component(
            Component::new(name("quit_button"), builtin::button()).with("caption", "Quit"),
        ));
    let root = Widget::from_component(
        Component::new(name("main_form"), builtin::form())
            .with("title", "Customers")
            .with(
                "size",
                Size {
                    width: 800,
                    height: 600,
                },
            ),
    )
    .with_layout(ContainerLayout::default())
    .with_child(toolbar)
    .with_child(
        Widget::from_component(
            Component::new(name("name_edit"), builtin::edit()).with("placeholder", "Name"),
        )
        .with_placement(Placement::Layout {
            stretch: 1,
            align: Align::Fill,
            cell: None,
            size: Size::default(),
            percent: Size::default(),
        }),
    );
    let mut form = Form::new(name("customers"), "egui", root);
    form.add_component(
        Component::new(name("db_pool"), TypeRef::pack("sqlx", "Pool"))
            .with("url", "postgres://localhost/app")
            .with("max_connections", 10),
    )
    .unwrap();
    form.add_component(
        Component::new(name("load_query"), TypeRef::pack("sqlx", "Query"))
            .with("pool", Value::reference(name("db_pool")))
            .with("sql", Value::Code("\"SELECT * FROM customers\"".into()))
            .on("on_row", handler("customer_loaded")),
    )
    .unwrap();
    form
}

fn issues(form: &Form) -> Vec<Issue> {
    let registry = registry();
    form.validate(&ValidationContext::new(&registry))
        .into_iter()
        .map(|d| d.issue)
        .collect()
}

#[test]
fn sample_form_is_valid() {
    let registry = registry();
    let diagnostics = sample_form().validate(&ValidationContext::new(&registry));
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}

#[test]
fn elements_give_uniform_access() {
    let form = sample_form();
    let names: Vec<&str> = form.elements().iter().map(|e| e.name().as_str()).collect();
    assert_eq!(
        names,
        [
            "main_form",
            "toolbar",
            "save_button",
            "quit_button",
            "name_edit",
            "db_pool",
            "load_query"
        ]
    );
    let widgets = form.elements().iter().filter(|e| e.is_widget()).count();
    assert_eq!(widgets, 5);
    assert_eq!(
        form.parent_of(&name("save_button")).unwrap().component.name,
        name("toolbar")
    );
    assert_eq!(
        form.component(&name("load_query")).unwrap().references(),
        [("pool", &ComponentPath::local(name("db_pool")))]
    );
}

#[test]
fn ftdr_round_trip() {
    let form = sample_form();
    let text = form.to_ron().unwrap();
    assert!(text.contains(r#"type: "pack:sqlx/Pool""#), "{text}");
    assert!(text.contains("MacOs: Text(\"Sichern\")"), "{text}");
    assert_eq!(Form::from_ron(&text).unwrap(), form);
}

#[test]
fn reading_rejects_bad_files() {
    let text = sample_form().to_ron().unwrap();
    let bad_name = text.replace("\"quit_button\"", "\"quit-button\"");
    assert!(matches!(Form::from_ron(&bad_name), Err(ModelError::Ron(_))));
    let future = text.replacen("format: 1", "format: 99", 1);
    assert_eq!(
        Form::from_ron(&future),
        Err(ModelError::UnsupportedFormat(99))
    );
}

#[test]
fn unknown_types_and_wrong_kinds() {
    let mut form = sample_form();
    form.add_component(Component::new(
        name("ghost"),
        TypeRef::pack("nope", "Ghost"),
    ))
    .unwrap();
    form.add_component(Component::new(name("loose_button"), builtin::button()))
        .unwrap();
    form.add_widget(
        &name("main_form"),
        Widget::new(name("pool_widget"), TypeRef::pack("sqlx", "Pool")),
    )
    .unwrap();
    let found = issues(&form);
    assert!(found.contains(&Issue::UnknownType(TypeRef::pack("nope", "Ghost"))));
    assert!(found.contains(&Issue::WidgetTypeAsComponent(builtin::button())));
    assert!(found.contains(&Issue::NotAWidgetType(TypeRef::pack("sqlx", "Pool"))));
}

#[test]
fn property_checks() {
    let mut form = sample_form();
    let pool = form.component_mut(&name("db_pool")).unwrap();
    pool.set("max_connections", 0);
    pool.set("colour", "red");
    pool.reset("url");
    form.widget_mut(&name("quit_button"))
        .unwrap()
        .component
        .set("caption", PropertyValue::new("Quit").on(Platform::Linux, 5));
    let found = issues(&form);
    assert!(found.iter().any(|i| matches!(i, Issue::TypeMismatch { property, mismatch: Mismatch::OutOfRange { .. } } if property == "max_connections")));
    assert!(found.contains(&Issue::UnknownProperty("colour".into())));
    assert!(found.contains(&Issue::MissingRequired("url".into())));
    // A platform override is type-checked as well.
    assert!(
        found
            .iter()
            .any(|i| matches!(i, Issue::TypeMismatch { property, .. } if property == "caption"))
    );
}

#[test]
fn backend_specific_properties() {
    let mut form = sample_form();
    form.backend = "slint".into();
    assert_eq!(
        issues(&form),
        [Issue::PropertyOfOtherBackend {
            property: "small".into(),
            backend: "egui".into()
        }]
    );
}

#[test]
fn platform_scoped_properties() {
    let mut registry = registry();
    registry
        .extend(
            "egui",
            &builtin::form(),
            [PropertyDef::new(
                "taskbar_progress",
                PropertyType::Int {
                    min: Some(0),
                    max: Some(100),
                },
                0,
            )
            .for_platforms([Platform::Windows])],
        )
        .unwrap();
    let mut form = sample_form();
    form.root.as_mut().unwrap().component.set(
        "taskbar_progress",
        PropertyValue::new(0)
            .on(Platform::Windows, 50)
            .on(Platform::Linux, 10),
    );
    let diagnostics = form.validate(&ValidationContext::new(&registry));
    assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
    assert_eq!(diagnostics[0].severity, Severity::Warning);
    assert!(matches!(
        &diagnostics[0].issue,
        Issue::PlatformNotAllowed {
            platform: Platform::Linux,
            ..
        }
    ));
}

#[test]
fn events_and_handlers() {
    let mut form = sample_form();
    form.widget_mut(&name("name_edit"))
        .unwrap()
        .component
        .events
        .insert("on_double_click".into(), handler("x"));
    assert_eq!(
        issues(&form),
        [Issue::UnknownEvent("on_double_click".into())]
    );
}

#[test]
fn references_and_capabilities() {
    let mut form = sample_form();
    let query = form.component_mut(&name("load_query")).unwrap();
    query.set("pool", Value::reference(name("save_button")));
    let found = issues(&form);
    assert!(found.iter().any(|i| matches!(i, Issue::MissingCapability { capability, .. } if capability.as_str() == "sqlx.pool")));

    let query = form.component_mut(&name("load_query")).unwrap();
    query.set("pool", Value::reference(name("missing")));
    assert!(
        issues(&form)
            .iter()
            .any(|i| matches!(i, Issue::DanglingReference { .. }))
    );

    let query = form.component_mut(&name("load_query")).unwrap();
    query.set("pool", Value::reference(name("load_query")));
    assert!(issues(&form).contains(&Issue::SelfReference("pool".into())));

    let query = form.component_mut(&name("load_query")).unwrap();
    query.reset("pool");
    assert_eq!(
        issues(&form),
        [Issue::RequirementNotConnected {
            capability: cap("sqlx.pool"),
            property: "pool".into()
        }]
    );
}

#[test]
fn layout_rules() {
    let mut form = sample_form();
    form.widget_mut(&name("quit_button"))
        .unwrap()
        .visual
        .placement = Placement::absolute(10, 10, 80, 24);
    form.widget_mut(&name("name_edit"))
        .unwrap()
        .visual
        .placement = Placement::Layout {
        stretch: 0,
        align: Align::Start,
        cell: Some(GridCell {
            row: 0,
            column: 0,
            row_span: 1,
            column_span: 1,
        }),
        size: Size::default(),
        percent: Size::default(),
    };
    form.widget_mut(&name("name_edit"))
        .unwrap()
        .children
        .push(Widget::new(name("inner"), builtin::label()));
    let found = issues(&form);
    assert!(found.contains(&Issue::AbsoluteInLayout));
    assert!(found.contains(&Issue::GridCellWithoutGrid));
    assert!(found.contains(&Issue::ChildrenNotAllowed));

    let registry = registry();
    let strict = ValidationContext {
        registry: &registry,
        absolute_positioning: false,
    };
    assert!(
        form.validate(&strict)
            .iter()
            .any(|d| d.issue == Issue::AbsoluteNotSupported)
    );
}

#[test]
fn editing_operations() {
    let mut form = sample_form();

    // Names are unique across widgets and components, also within added subtrees.
    assert_eq!(
        form.add_component(Component::new(
            name("save_button"),
            TypeRef::pack("sqlx", "Pool")
        )),
        Err(ModelError::DuplicateName(name("save_button")))
    );
    let twin = Widget::new(name("panel"), builtin::container())
        .with_child(Widget::new(name("panel"), builtin::label()));
    assert_eq!(
        form.add_widget(&name("main_form"), twin),
        Err(ModelError::DuplicateName(name("panel")))
    );
    assert_eq!(
        form.add_widget(&name("nowhere"), Widget::new(name("x"), builtin::label())),
        Err(ModelError::NotFound(name("nowhere")))
    );
    assert_eq!(form.unique_name("button").unwrap(), name("button1"));
    form.add_widget(
        &name("main_form"),
        Widget::new(name("button1"), builtin::button()),
    )
    .unwrap();
    assert_eq!(form.unique_name("button").unwrap(), name("button2"));

    // Renaming updates references.
    form.rename(&name("db_pool"), name("main_db")).unwrap();
    assert_eq!(
        form.component(&name("load_query")).unwrap().references(),
        [("pool", &ComponentPath::local(name("main_db")))]
    );
    assert_eq!(
        form.rename(&name("main_db"), name("toolbar")),
        Err(ModelError::DuplicateName(name("toolbar")))
    );

    // Removing a subtree removes its widgets; removing a provider clears references.
    let removed = form.remove(&name("toolbar")).unwrap();
    assert_eq!(
        removed,
        [name("toolbar"), name("save_button"), name("quit_button")]
    );
    assert!(!form.contains(&name("save_button")));
    form.remove(&name("main_db")).unwrap();
    assert!(
        form.component(&name("load_query"))
            .unwrap()
            .references()
            .is_empty()
    );
    assert_eq!(
        form.remove(&name("main_form")),
        Err(ModelError::CannotRemoveRoot)
    );
}

#[test]
fn builtin_types_are_consistent() {
    let registry = TypeRegistry::with_builtins();
    assert_eq!(registry.len(), 8);
    for ty in registry.types() {
        assert!(ty.kind.is_widget(), "{}", ty.type_ref);
        for def in &ty.properties {
            if let Some(default) = &def.default {
                assert!(
                    default.check(&def.ty).is_ok(),
                    "{}.{}",
                    ty.type_ref,
                    def.name
                );
            }
        }
        // Component type descriptions are serialisable too (pack files later).
        let text = ron::to_string(ty).unwrap();
        assert_eq!(&ron::from_str::<ComponentType>(&text).unwrap(), ty);
    }
}
