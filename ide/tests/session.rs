//! The IDE session without UI: placing from the palette, selecting,
//! dragging to reorder, deleting, undo, new projects.

use tdr_core::{ComponentName, ProjectFile, TypeRef, TypeRegistry, builtin};
use tdr_designer::{Command, Document, Point};

use freya_tde::Session;
use freya_tde::session::handler_line;

fn name(n: &str) -> ComponentName {
    ComponentName::new(n).unwrap()
}

fn session() -> Session {
    let mut doc = Document::new(
        ProjectFile::new("demo", "1.99", "plain").unwrap(),
        TypeRegistry::with_builtins(),
    );
    doc.apply(Command::AddForm {
        name: name("main"),
        data_module: false,
    })
    .unwrap();
    Session::new(doc)
}

/// Centre of an element on the design surface.
fn centre(s: &Session, element: &str) -> Point {
    s.layout().unwrap().rect(&name(element)).unwrap().center()
}

/// A point inside the window's client area, below all children.
fn empty_spot(s: &Session) -> Point {
    let window = s
        .layout()
        .unwrap()
        .item(&name("window"))
        .unwrap()
        .content
        .unwrap();
    Point::new(window.center().x, window.bottom() - 5)
}

#[test]
fn place_select_and_delete() {
    let mut s = session();
    assert_eq!(s.form_name(), Some(&name("main")));

    // Arm the palette, click into the form: the widget is placed and selected.
    s.arm(Some(builtin::button()));
    let at = empty_spot(&s);
    assert!(s.press(at, false).unwrap().model);
    assert_eq!(s.selection(), [name("button1")]);
    assert_eq!(s.armed(), None, "one click places one widget");

    s.arm(Some(builtin::label()));
    s.press(empty_spot(&s), false).unwrap();
    assert_eq!(s.selection(), [name("label1")]);

    // Clicking selects; clicking empty space outside the form deselects.
    let button = centre(&s, "button1");
    s.press(button, false).unwrap();
    s.release(button).unwrap();
    assert_eq!(s.selection(), [name("button1")]);
    s.press(Point::new(1, 1), false).unwrap();
    assert!(s.selection().is_empty());

    // Delete keeps the window, removes the rest; undo brings it back.
    s.select(vec![name("window"), name("button1")]);
    assert!(s.delete_selection().unwrap().model);
    let form = s.form().unwrap();
    assert!(form.contains(&name("window")));
    assert!(!form.contains(&name("button1")));
    assert!(s.undo());
    assert!(s.form().unwrap().contains(&name("button1")));
}

#[test]
fn palette_types_are_dropped_and_widgets_resized_in_layouts() {
    let mut s = session();
    // Aus der Palette gezogen: wie ein Klick mit gewähltem Typ.
    let at = empty_spot(&s);
    assert_eq!(
        s.palette_drop_preview(&builtin::button(), at),
        Some((name("window"), 0))
    );
    assert!(s.drop_type(builtin::button(), at).unwrap().model);
    assert_eq!(s.selection(), [name("button1")]);
    s.drop_type(builtin::label(), empty_spot(&s)).unwrap();
    assert_eq!(
        s.form()
            .unwrap()
            .widget(&name("window"))
            .unwrap()
            .children
            .len(),
        2
    );

    // Anfasser rechts unten am Knopf (im senkrechten Layout): Wunschgröße,
    // Fill wird zu Start, damit die Breite sichtbar ist.
    s.select(vec![name("button1")]);
    let before = s.layout().unwrap().rect(&name("button1")).unwrap();
    let corner = Point::new(before.right() - 1, before.bottom() - 1);
    s.press(corner, false).unwrap();
    let to = Point::new(corner.x - 100, corner.y + 20);
    assert!(s.drag(to));
    assert!(s.resize_preview(to).is_some());
    assert!(s.release(to).unwrap().model);
    let placement = s
        .form()
        .unwrap()
        .widget(&name("button1"))
        .unwrap()
        .visual
        .placement
        .clone();
    let tdr_core::Placement::Layout { align, size, .. } = placement else {
        panic!("still in the layout: {placement:?}");
    };
    assert_eq!(align, tdr_core::Align::Start);
    assert_eq!(
        (size.width, size.height),
        (before.width - 100, before.height + 20)
    );
    let after = s.layout().unwrap().rect(&name("button1")).unwrap();
    assert_eq!(
        (after.width, after.height),
        (before.width - 100, before.height + 20)
    );
    // Zurück zur natürlichen Größe (Ausrichtung bleibt Start).
    assert!(s.natural_size().unwrap().model);
    let natural = s.layout().unwrap().rect(&name("button1")).unwrap();
    assert_eq!(natural.height, before.height);
    assert!(!s.natural_size().unwrap().model, "nothing left to clear");
    assert!(s.undo());
    assert!(s.undo());
    assert_eq!(s.layout().unwrap().rect(&name("button1")).unwrap(), before);
}

#[test]
fn drag_reorders_widgets() {
    let mut s = session();
    for type_ref in [builtin::label(), builtin::button()] {
        s.arm(Some(type_ref));
        s.press(empty_spot(&s), false).unwrap();
    }
    // Drag the button above the label.
    let button = centre(&s, "button1");
    let label = s.layout().unwrap().rect(&name("label1")).unwrap();
    let above = Point::new(label.center().x, label.y + 1);
    s.press(button, false).unwrap();
    assert!(s.drag(above));
    assert_eq!(s.drop_preview(above), Some((name("window"), 0)));
    assert!(s.release(above).unwrap().model);
    let window = s.form().unwrap().widget(&name("window")).unwrap();
    let order: Vec<&str> = window
        .children
        .iter()
        .map(|c| c.component.name.as_str())
        .collect();
    assert_eq!(order, ["button1", "label1"]);

    // A small wobble is a click, not a drag.
    let label = centre(&s, "label1");
    s.press(label, false).unwrap();
    assert!(!s.drag(Point::new(label.x + 2, label.y + 1)));
    assert!(!s.release(label).unwrap().model);
}

#[test]
fn non_visual_components_go_to_the_tray() {
    let mut s = session();
    let mut registry = s.document().registry().clone();
    registry
        .register(tdr_core::ComponentType::new(
            TypeRef::pack("demo", "Timer"),
            tdr_core::TypeKind::NonVisual,
        ))
        .unwrap();
    *s.document_mut().registry_mut() = registry;
    s.arm(Some(TypeRef::pack("demo", "Timer")));
    s.press(Point::new(5, 5), false).unwrap();
    assert_eq!(s.selection(), [name("timer1")]);
    assert_eq!(s.form().unwrap().components.len(), 1);

    // In der Komponentenleiste, nicht auf der Formularfläche.
    assert!(s.layout().unwrap().item(&name("timer1")).is_none());
    let icon = s.tray().unwrap().rect(&name("timer1")).unwrap();
    assert_eq!(
        s.single_selection(),
        Some((&name("timer1"), tdr_designer::ItemKind::Component))
    );
    // Klick daneben hebt die Auswahl auf, Klick aufs Symbol wählt wieder.
    assert!(
        s.press_tray(Point::new(icon.right() + 50, 5), false)
            .selection
    );
    assert!(s.selection().is_empty());
    assert!(s.press_tray(icon.center(), false).selection);
    assert_eq!(s.selection(), [name("timer1")]);
    let (_, picture) = s.tray_picture().unwrap();
    assert!(
        picture
            .iter()
            .any(|p| matches!(p, tdr_designer::Primitive::Icon { .. }))
    );
}

#[test]
fn unchanged_projects_need_no_question() {
    // Das leere Startprojekt (mit dem automatischen ersten Formular).
    let mut s = Session::untitled(
        "projekt1",
        "plain",
        Vec::new(),
        TypeRegistry::with_builtins(),
    )
    .unwrap();
    assert!(!s.is_modified(), "nichts getan – nichts zu verwerfen");
    s.arm(Some(builtin::button()));
    s.press(empty_spot(&s), false).unwrap();
    assert!(s.is_modified());
    assert!(s.undo());
    assert!(!s.is_modified(), "rückgängig gemacht: wieder wie am Anfang");

    // Ein gespeichertes Projekt: frisch geöffnet unverändert, dann geändert.
    let dir = std::env::temp_dir().join(format!("freya-modified-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    Session::create(&dir, "shop", "plain", TypeRegistry::with_builtins()).unwrap();
    let (mut opened, _) = Session::open(&dir.join("shop.ptdr"), &[]).unwrap();
    assert!(!opened.is_modified());
    opened.arm(Some(builtin::button()));
    opened.press(empty_spot(&opened), false).unwrap();
    assert!(opened.is_modified());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn new_project_is_saved() {
    let dir = std::env::temp_dir().join(format!("freya-new-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let s = Session::create(&dir, "shop", "plain", TypeRegistry::with_builtins()).unwrap();
    assert_eq!(s.form_name(), Some(&name("main_form")));
    assert!(dir.join("shop.ptdr").exists());
    assert!(dir.join("src/main_form.ftdr").exists());
    let (opened, _) = Session::open(&dir.join("shop.ptdr"), &[]).unwrap();
    assert_eq!(opened.document().forms(), s.document().forms());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn handlers_and_generated_code() {
    let dir = std::env::temp_dir().join(format!("freya-code-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut s = Session::create(&dir, "shop", "none", TypeRegistry::with_builtins()).unwrap();
    s.arm(Some(builtin::button()));
    let window = s
        .layout()
        .unwrap()
        .item(&name("window"))
        .unwrap()
        .content
        .unwrap();
    s.press(Point::new(window.center().x, window.bottom() - 5), false)
        .unwrap();

    // Doppelklick-Logik: Standard-Ereignis bekommt einen vorgeschlagenen Handler.
    let handler = s.ensure_handler(&name("button1"), None).unwrap();
    assert_eq!(handler.as_str(), "button1_click");
    // Ein zweites Mal: derselbe Handler, keine Änderung.
    assert_eq!(s.ensure_handler(&name("button1"), None).unwrap(), handler);

    // Code erzeugen: Nutzerdatei mit Stub, Zeile wird gefunden.
    let (generation, written) = s.write_code().unwrap();
    assert!(!generation.has_errors(), "{:#?}", generation.diagnostics);
    assert!(written.contains(&"src/main_form.rs".to_owned()));
    let source = std::fs::read_to_string(s.form_source().unwrap()).unwrap();
    let line = handler_line(&source, &handler).unwrap();
    assert!(
        source
            .lines()
            .nth(line - 1)
            .unwrap()
            .contains("fn button1_click(")
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn anchors_follow_the_form_and_percent_follows_the_handles() {
    use tdr_core::{Anchors, Placement, Value};
    let mut s = session();
    s.drop_type(builtin::button(), empty_spot(&s)).unwrap();
    let placement = |s: &Session| {
        s.form()
            .unwrap()
            .widget(&name("button1"))
            .unwrap()
            .visual
            .placement
            .clone()
    };

    // Prozent: 50 % per Objektinspektor, dann per Anfasser auf 30 %.
    s.set_layout_row(
        &name("button1"),
        "placement.width_percent",
        Some(Value::Int(50)),
    )
    .unwrap();
    let content = s
        .layout()
        .unwrap()
        .item(&name("window"))
        .unwrap()
        .content
        .unwrap();
    let before = s.layout().unwrap().rect(&name("button1")).unwrap();
    assert_eq!(before.width, content.width / 2);
    s.select(vec![name("button1")]);
    let corner = Point::new(before.right() - 1, before.bottom() - 1);
    s.press(corner, false).unwrap();
    let to = Point::new(
        corner.x - (before.width - content.width * 30 / 100),
        corner.y,
    );
    assert!(s.drag(to));
    assert!(s.release(to).unwrap().model);
    let Placement::Layout { percent, size, .. } = placement(&s) else {
        panic!("im Layout erwartet");
    };
    assert_eq!(percent.width, 30, "Prozent bleibt Prozent");
    assert_eq!(size.width, 0);
    assert!(
        s.natural_size().unwrap().model,
        "Natürliche Größe löscht auch Prozent"
    );
    assert!(matches!(placement(&s), Placement::Layout { percent, .. } if percent.width == 0));

    // Absolut: Kinder bleiben, wo sie sind; Anker rechts unten.
    let shown = s.layout().unwrap().rect(&name("button1")).unwrap();
    s.set_layout_row(
        &name("window"),
        "layout.kind",
        Some(Value::Enum("Absolute".into())),
    )
    .unwrap();
    assert_eq!(s.layout().unwrap().rect(&name("button1")).unwrap(), shown);
    for (row, on) in [
        ("placement.anchor_left", false),
        ("placement.anchor_top", false),
        ("placement.anchor_right", true),
        ("placement.anchor_bottom", true),
    ] {
        s.set_layout_row(&name("button1"), row, Some(Value::Bool(on)))
            .unwrap();
    }
    let error = s
        .set_layout_row(&name("button1"), "placement.stretch", Some(Value::Int(1)))
        .unwrap_err();
    assert!(error.contains("placement.stretch"), "{error}");

    // Ziehen im absoluten Layout verschiebt um die Mausbewegung.
    let Placement::Absolute { position, .. } = placement(&s) else {
        panic!("absolut erwartet");
    };
    let from = centre(&s, "button1");
    s.press(from, false).unwrap();
    let to = Point::new(from.x + 30, from.y + 10);
    assert!(s.drag(to));
    assert!(s.release(to).unwrap().model);
    let Placement::Absolute {
        position: moved, ..
    } = placement(&s)
    else {
        panic!("absolut erwartet");
    };
    assert_eq!((moved.x, moved.y), (position.x + 30, position.y + 10));

    // Formular größer ziehen: der verankerte Knopf wandert mit (ein Schritt).
    let window = s.layout().unwrap().item(&name("window")).unwrap().rect;
    s.select(vec![name("window")]);
    let corner = Point::new(window.right() - 1, window.bottom() - 1);
    s.press(corner, false).unwrap();
    let to = Point::new(corner.x + 100, corner.y + 50);
    assert!(s.drag(to));
    assert!(s.release(to).unwrap().model);
    let Placement::Absolute {
        position: followed,
        size,
        anchors,
    } = placement(&s)
    else {
        panic!("absolut erwartet");
    };
    assert_eq!(anchors, Anchors::new(false, false, true, true));
    assert_eq!((followed.x, followed.y), (moved.x + 100, moved.y + 50));
    let button = s.layout().unwrap().rect(&name("button1")).unwrap();
    assert_eq!(button.width, size.width);
    assert!(s.undo());
    let Placement::Absolute { position: back, .. } = placement(&s) else {
        panic!("absolut erwartet");
    };
    assert_eq!(back, moved, "Formulargröße und Anker in einem Schritt");
}
