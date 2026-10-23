//! The designer without UI: commands, undo/redo, layout, display list, hit
//! testing, palette, inspector, saving and opening.

use std::path::PathBuf;

use tdr_core::{
    Align, ComponentName, ContainerLayout, LayoutKind, Placement, Platform, ProjectFile,
    PropertyValue, TypeRegistry, Value, builtin,
};
use tdr_designer::layout::drop_target;
use tdr_designer::*;

fn name(n: &str) -> ComponentName {
    ComponentName::new(n).unwrap()
}

fn document() -> Document {
    Document::new(
        ProjectFile::new("demo", "1.99", "plain").unwrap(),
        TypeRegistry::with_builtins(),
    )
}

/// A form "main" with a label and a button (caption "OK").
fn sample() -> Document {
    let mut doc = document();
    doc.apply(Command::AddForm {
        name: name("main"),
        data_module: false,
    })
    .unwrap();
    for type_ref in [builtin::label(), builtin::button()] {
        doc.apply(Command::AddWidget {
            form: name("main"),
            parent: name("window"),
            type_ref,
            name: None,
            index: None,
            placement: None,
        })
        .unwrap();
    }
    doc.apply(Command::SetProperty {
        form: name("main"),
        element: name("button1"),
        property: "caption".into(),
        value: Some(PropertyValue::new("OK")),
    })
    .unwrap();
    doc
}

#[test]
fn commands_choose_names_and_check_types() {
    let mut doc = sample();
    let form = doc.form(&name("main")).unwrap();
    let names: Vec<&str> = form
        .widgets()
        .iter()
        .map(|w| w.component.name.as_str())
        .collect();
    assert_eq!(names, ["window", "label1", "button1"]);
    assert_eq!(
        form.widget(&name("button1"))
            .unwrap()
            .component
            .get("caption"),
        Some(&PropertyValue::new("OK"))
    );

    // Wrong type, unknown property, unknown event, leaf as parent.
    let set = |property: &str, value: Value| Command::SetProperty {
        form: name("main"),
        element: name("button1"),
        property: property.into(),
        value: Some(PropertyValue::new(value)),
    };
    assert!(matches!(
        doc.apply(set("caption", Value::Int(1))),
        Err(CommandError::TypeMismatch { .. })
    ));
    assert!(matches!(
        doc.apply(set("nope", Value::Int(1))),
        Err(CommandError::UnknownProperty { .. })
    ));
    assert!(matches!(
        doc.apply(Command::SetEvent {
            form: name("main"),
            element: name("label1"),
            event: "on_click".into(),
            handler: None,
        }),
        Err(CommandError::UnknownEvent { .. })
    ));
    assert!(matches!(
        doc.apply(Command::AddWidget {
            form: name("main"),
            parent: name("button1"),
            type_ref: builtin::label(),
            name: None,
            index: None,
            placement: None,
        }),
        Err(CommandError::NotAContainer(_))
    ));
}

#[test]
fn undo_redo_and_atomic_batches() {
    let mut doc = sample();
    assert!(doc.is_dirty());
    let before = doc.forms().to_vec();

    // A batch that fails halfway changes nothing.
    let result = doc.apply(Command::Batch {
        commands: vec![
            Command::Rename {
                form: name("main"),
                element: name("button1"),
                new_name: name("ok_button"),
            },
            Command::Remove {
                form: name("main"),
                element: name("missing"),
            },
        ],
    });
    assert!(result.is_err());
    assert_eq!(doc.forms(), before.as_slice());

    doc.apply(Command::Rename {
        form: name("main"),
        element: name("button1"),
        new_name: name("ok_button"),
    })
    .unwrap();
    assert!(
        doc.form(&name("main"))
            .unwrap()
            .contains(&name("ok_button"))
    );
    assert!(doc.undo());
    assert_eq!(doc.forms(), before.as_slice());
    assert!(doc.redo());
    assert!(
        doc.form(&name("main"))
            .unwrap()
            .contains(&name("ok_button"))
    );
    assert!(!doc.redo());
}

#[test]
fn commands_from_json() {
    let mut doc = document();
    let script = r#"[
        {"command": "add_form", "name": "main"},
        {"command": "add_widget", "form": "main", "parent": "window", "type": "core/Edit", "name": "customer"},
        {"command": "set_property", "form": "main", "element": "customer", "property": "placeholder", "value": {"Text": "Name"}},
        {"command": "add_widget", "form": "main", "parent": "window", "type": "core/Button"},
        {"command": "set_event", "form": "main", "element": "button1", "event": "on_click", "handler": "save_clicked"},
        {"command": "move", "form": "main", "element": "button1", "parent": "window", "index": 0}
    ]"#;
    let commands: Vec<Command> = serde_json::from_str(script).unwrap();
    for command in commands {
        doc.apply(command).unwrap();
    }
    let form = doc.form(&name("main")).unwrap();
    let window = form.widget(&name("window")).unwrap();
    assert_eq!(window.children[0].component.name.as_str(), "button1");
    assert!(doc.findings().is_empty(), "{:#?}", doc.findings());

    // Errors serialize with a stable code.
    let error = doc
        .apply(serde_json::from_str(r#"{"command": "remove_form", "form": "nope"}"#).unwrap())
        .unwrap_err();
    let json = serde_json::to_value(&error).unwrap();
    assert_eq!(json["code"], "unknown_form");
}

#[test]
fn layout_stacks_children_and_stretches() {
    let mut doc = sample();
    let theme = Theme::for_platform(&Platform::Linux);
    let form = doc.form(&name("main")).unwrap();
    let layout = layout(form, doc.registry(), &theme);

    let window = layout.item(&name("window")).unwrap();
    assert_eq!(window.kind, ItemKind::Window);
    assert_eq!(
        window.rect,
        Rect::new(24, 24, 640, 480 + theme.title_bar_height)
    );
    let label = layout.rect(&name("label1")).unwrap();
    let button = layout.rect(&name("button1")).unwrap();
    // Vertical layout, theme padding and spacing, full width.
    assert_eq!(label.x, 24 + theme.padding);
    assert_eq!(label.y, 24 + theme.title_bar_height + theme.padding);
    assert_eq!(button.y, label.bottom() + theme.spacing);
    assert_eq!(label.width, 640 - 2 * theme.padding);
    assert_eq!(button.height, theme.metrics(&builtin::button()).height);

    // Stretch: the button takes the remaining height; centered alignment.
    doc.apply(Command::SetPlacement {
        form: name("main"),
        element: name("button1"),
        placement: Placement::Layout {
            stretch: 1,
            align: Align::Center,
            cell: None,
        },
    })
    .unwrap();
    let layout = tdr_designer::layout(doc.form(&name("main")).unwrap(), doc.registry(), &theme);
    let window = layout.item(&name("window")).unwrap().content.unwrap();
    let button = layout.rect(&name("button1")).unwrap();
    assert_eq!(button.bottom(), window.bottom());
    assert!(button.width < window.width);
    assert_eq!(button.center().x, window.center().x);
}

#[test]
fn horizontal_and_grid_layouts() {
    let mut doc = sample();
    doc.apply(Command::SetLayout {
        form: name("main"),
        element: name("window"),
        layout: Some(ContainerLayout {
            kind: LayoutKind::Horizontal,
            spacing: 4,
            padding: tdr_core::Insets::all(0),
        }),
    })
    .unwrap();
    let theme = Theme::for_platform(&Platform::Windows);
    let layout = tdr_designer::layout(doc.form(&name("main")).unwrap(), doc.registry(), &theme);
    let label = layout.rect(&name("label1")).unwrap();
    let button = layout.rect(&name("button1")).unwrap();
    assert_eq!(button.x, label.right() + 4);
    assert_eq!(label.y, button.y);

    doc.apply(Command::SetLayout {
        form: name("main"),
        element: name("window"),
        layout: Some(ContainerLayout {
            kind: LayoutKind::Grid { columns: 2 },
            spacing: 0,
            padding: tdr_core::Insets::all(0),
        }),
    })
    .unwrap();
    let layout = tdr_designer::layout(doc.form(&name("main")).unwrap(), doc.registry(), &theme);
    let label = layout.rect(&name("label1")).unwrap();
    let button = layout.rect(&name("button1")).unwrap();
    // Two columns share the width.
    assert_eq!(label.width + button.width, 640);
    assert_eq!(button.x, label.right());
}

#[test]
fn display_list_hit_test_and_drop_target() {
    let doc = sample();
    let theme = Theme::for_platform(&Platform::Linux);
    let form = doc.form(&name("main")).unwrap();
    let layout = layout(form, doc.registry(), &theme);
    let primitives = render(form, &layout, doc.registry(), &theme);
    let texts: Vec<&str> = primitives
        .iter()
        .filter_map(|p| match p {
            Primitive::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["main", "OK"]);
    // The display list is data an AI can read.
    let json = serde_json::to_value(&primitives).unwrap();
    assert_eq!(json[0]["kind"], "rect");

    let button = layout.rect(&name("button1")).unwrap();
    let hit = hit_test(&layout, button.center(), &[]).unwrap();
    assert_eq!(hit.element.as_str(), "button1");
    assert_eq!(hit.handle, None);
    // Handles of the selected widget win.
    let corner = Point::new(button.right(), button.bottom());
    let hit = hit_test(&layout, corner, &[name("button1")]).unwrap();
    assert_eq!(hit.handle, Some(Handle::BottomRight));
    assert_eq!(
        render_selection(&layout, &[name("button1")], &theme).len(),
        9
    );

    // Dropping between label and button inserts at index 1.
    let label = layout.rect(&name("label1")).unwrap();
    let between = Point::new(label.center().x, (label.bottom() + button.y) / 2);
    assert_eq!(
        drop_target(form, doc.registry(), &layout, between),
        Some((name("window"), 1))
    );
    assert_eq!(
        drop_target(form, doc.registry(), &layout, Point::new(1, 1)),
        None
    );
}

#[test]
fn tray_palette_and_inspector() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../components");
    let (packs, errors) = tdr_core::pack::load_all(&dir);
    assert!(errors.is_empty());
    let mut doc = sample();
    for pack in &packs {
        pack.register(doc.registry_mut()).unwrap();
    }
    let created = doc
        .apply(Command::AddComponent {
            form: name("main"),
            type_ref: tdr_core::TypeRef::pack("sqlx", "Pool"),
            name: None,
        })
        .unwrap();
    assert_eq!(created.created[0].to_string(), "main::pool1");

    let theme = Theme::for_platform(&Platform::Linux);
    let form = doc.form(&name("main")).unwrap();
    let layout = layout(form, doc.registry(), &theme);
    let pool = layout.item(&name("pool1")).unwrap();
    assert_eq!(pool.kind, ItemKind::Component);
    assert!(pool.rect.y > layout.rect(&name("window")).unwrap().bottom());
    assert!(
        render(form, &layout, doc.registry(), &theme)
            .iter()
            .any(|p| matches!(p, Primitive::Icon { name, .. } if name == "database"))
    );

    let items = palette(doc.registry(), "plain");
    assert!(items.iter().all(|i| i.type_ref != builtin::form()));
    assert!(
        items
            .iter()
            .any(|i| i.type_ref.to_string() == "pack:sqlx/Pool" && !i.visual)
    );

    let rows = properties(form, &name("pool1"), doc.registry()).unwrap();
    let url = rows.iter().find(|r| r.name == "url").unwrap();
    assert!(url.default.is_none() && url.value.is_none());
    let rows = properties(form, &name("button1"), doc.registry()).unwrap();
    let caption = rows.iter().find(|r| r.name == "caption").unwrap();
    assert_eq!(caption.effective(), Some(&Value::Text("OK".into())));
    let events = events(form, &name("button1"), doc.registry()).unwrap();
    assert_eq!(events[0].name, "on_click");
    assert_eq!(
        suggest_handler(&name("button1"), "on_click").as_str(),
        "button1_click"
    );
}

#[test]
fn save_and_open() {
    let dir = std::env::temp_dir().join(format!("tdr-designer-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut doc = sample();
    // Dry run first: what would be written.
    let files: Vec<String> = doc.files().unwrap().into_iter().map(|(p, _)| p).collect();
    assert_eq!(files, ["demo.ptdr", "src/main.ftdr"]);
    assert_eq!(doc.save(Some(&dir)).unwrap(), files);
    assert!(!doc.is_dirty());
    assert!(doc.save(None).unwrap().is_empty());

    let (opened, report) = Document::open(&dir.join("demo.ptdr"), &[]).unwrap();
    assert_eq!(opened.forms(), doc.forms());
    // The backend "plain" is not installed anywhere.
    assert_eq!(report.missing_packs, ["plain"]);

    // Renaming a form removes its old file on save.
    doc.apply(Command::RenameForm {
        form: name("main"),
        new_name: name("start"),
    })
    .unwrap();
    doc.save(None).unwrap();
    assert!(dir.join("src/start.ftdr").exists());
    assert!(!dir.join("src/main.ftdr").exists());
    std::fs::remove_dir_all(&dir).unwrap();
}
