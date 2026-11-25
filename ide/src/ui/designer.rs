//! Designer window: the form on a canvas. Click selects (Ctrl/Shift adds),
//! a click with an armed palette type places it, so does dropping a palette
//! button here; dragging reorders or moves into another container, the
//! handles of the selection resize, a double click opens the element's event
//! handler, Delete removes, Esc cancels.
//!
//! Non-visual components (database, WebSocket, timer …) are not drawn on
//! the form: they sit as icons in the component bar at the bottom of the
//! window (like Delphi's non-visual components); a click selects one for
//! the object inspector, a double click opens its event handler. A data
//! module has no bar – its components are its surface.
//!
//! No form or platform selector: a form is opened together with its `.rs`
//! file, and the look is the project's target system.

use crate::tr;
use std::cell::Cell;
use std::rc::Rc;

use rncl::prelude::*;
use ruf::{Color, Key};
use tdr_core::ComponentName;
use tdr_designer::{ItemKind, Point};

use super::{Ide, convert, with};

/// Height of the component bar.
const BAR: i32 = 72;

pub struct DesignerWindow {
    pub window: RnclWindow,
    scroll: RnclScrollView,
    canvas: RnclCanvas,
    /// The component bar (non-visual components of a form).
    tray_scroll: RnclScrollView,
    tray: RnclCanvas,
    /// Is the bar shown (forms yes, data modules no)?
    tray_shown: Cell<bool>,
}

impl DesignerWindow {
    pub fn build() -> Self {
        let window = RnclWindow::new(&tr!("window.designer"));
        window.set_menu_bar(menu_bar());
        let scroll = RnclScrollView::new(&window);
        let canvas = RnclCanvas::new(scroll.content());
        let tray_scroll = RnclScrollView::new(&window);
        let tray = RnclCanvas::new(tray_scroll.content());
        tray.widget().set_tool_tip(
            "Nicht-visuelle Komponenten: klicken zum Bearbeiten im Objektinspektor, \
             Doppelklick öffnet den Handler",
        );
        Self {
            window,
            scroll,
            canvas,
            tray_scroll,
            tray,
            tray_shown: Cell::new(true),
        }
    }

    fn layout(&self, width: i32, height: i32) {
        let bar = if self.tray_shown.get() { BAR } else { 0 };
        self.scroll.set_geometry(0, 0, width, (height - bar).max(0));
        self.tray_scroll
            .set_geometry(0, (height - bar).max(0), width, bar);
        self.tray_scroll.widget().set_visible(bar > 0);
    }

    pub fn tray_widget(&self) -> Widget {
        self.tray.widget().clone()
    }

    /// Redraws the component bar (shown for forms, hidden for data modules).
    fn draw_tray(&self, ide: &Ide) {
        let session = ide.session();
        let shown = session.form().is_some_and(|f| f.root.is_some());
        if shown != self.tray_shown.get() {
            self.tray_shown.set(shown);
            let size = self.window.size();
            self.layout(size.width, size.height);
        }
        let Some((tray, primitives)) = session.tray_picture().filter(|_| shown) else {
            self.tray.set_shapes(Vec::new());
            return;
        };
        let theme = session.theme();
        let rgb = |c: tdr_core::Color| ruf::Color::rgb(c.r, c.g, c.b);
        let visible = self.tray_scroll.size();
        let width = tray.width.max(visible.width - 4);
        let mut shapes = vec![
            Shape::Rect {
                rect: ruf::Rect::new(0, 0, width, BAR),
                fill: Some(rgb(theme.colors.tray)),
                stroke: None,
                radius: 0,
                dashed: false,
            },
            Shape::Line {
                from: ruf::Point::new(0, 0),
                to: ruf::Point::new(width, 0),
                color: rgb(theme.colors.selection),
            },
        ];
        if tray.items.is_empty() {
            shapes.push(Shape::Text {
                x: 12,
                y: BAR / 2 - 9,
                text: tr!("designer.tray_empty"),
                color: rgb(theme.colors.text),
                size: 13,
                bold: false,
                italic: true,
                family: None,
            });
        }
        let project = session.project_dir();
        shapes.extend(convert::shapes(
            &primitives,
            theme.colors.tray,
            project.as_deref(),
        ));
        self.tray.set_geometry(0, 0, width, BAR - 4);
        self.tray_scroll.set_content_size(tray.width, BAR - 4);
        self.tray.set_shapes(shapes);
    }

    pub fn canvas_widget(&self) -> Widget {
        self.canvas.widget().clone()
    }

    /// Redraws the form; `drop` marks where a dragged widget would land.
    fn draw(&self, ide: &Ide, drop: Option<(ComponentName, usize)>) {
        self.draw_with(ide, drop, None);
    }

    /// Like `draw`, with the outline of a resize in progress.
    fn draw_with(
        &self,
        ide: &Ide,
        drop: Option<(ComponentName, usize)>,
        resize: Option<tdr_designer::Rect>,
    ) {
        let session = ide.session();
        let Some((layout, primitives)) = session.picture() else {
            self.canvas.set_shapes(Vec::new());
            return;
        };
        let theme = session.theme();
        let project = session.project_dir();
        let mut shapes = convert::shapes(&primitives, theme.colors.window, project.as_deref());
        if let Some(r) = resize {
            shapes.push(Shape::Rect {
                rect: ruf::Rect::new(r.x, r.y, r.width, r.height),
                fill: None,
                stroke: Some(ruf::Color::rgb(
                    theme.colors.selection.r,
                    theme.colors.selection.g,
                    theme.colors.selection.b,
                )),
                radius: 0,
                dashed: true,
            });
        }
        if let Some((parent, index)) = drop
            && let Some(marker) = drop_marker(&session, &layout, &parent, index)
        {
            shapes.push(marker);
        }
        // Die Fläche füllt mindestens das Fenster (Hintergrund wie der Designer).
        let visible = self.scroll.size();
        let (w, h) = (
            layout.width.max(visible.width - 4),
            layout.height.max(visible.height - 4),
        );
        shapes.insert(
            0,
            Shape::Rect {
                rect: ruf::Rect::new(0, 0, w, h),
                fill: Some(ruf::Color::rgb(
                    theme.colors.desktop.r,
                    theme.colors.desktop.g,
                    theme.colors.desktop.b,
                )),
                stroke: None,
                radius: 0,
                dashed: false,
            },
        );
        self.canvas.set_geometry(0, 0, w, h);
        self.scroll.set_content_size(layout.width, layout.height);
        self.canvas.set_shapes(shapes);
        std::mem::drop(session);
        self.draw_tray(ide);
    }

    pub fn refresh(&self, ide: &Ide) {
        let session = ide.session();
        let title = match session.form_name() {
            Some(form) => format!("Designer – {form}.rs ({})", session.platform()),
            None => tr!("window.designer"),
        };
        self.window.set_title(&title);
        drop(session);
        self.draw(ide, None);
    }
}

/// A horizontal (or vertical) line where a dropped widget would be inserted.
fn drop_marker(
    session: &crate::Session,
    layout: &tdr_designer::FormLayout,
    parent: &ComponentName,
    index: usize,
) -> Option<Shape> {
    let form = session.form()?;
    let container = layout.item(parent)?.content?;
    let children: Vec<tdr_designer::Rect> = form
        .widget(parent)?
        .children
        .iter()
        .filter_map(|c| layout.rect(&c.component.name))
        .collect();
    let horizontal = matches!(
        form.widget(parent)?.visual.layout.as_ref().map(|l| &l.kind),
        Some(tdr_core::LayoutKind::Horizontal)
    );
    let color = Color::rgb(230, 80, 40);
    let line = |from: Point, to: Point| Shape::Line {
        from: ruf::Point::new(from.x, from.y),
        to: ruf::Point::new(to.x, to.y),
        color,
    };
    Some(if horizontal {
        let x = match children.get(index) {
            Some(r) => r.x - 2,
            None => children.last().map_or(container.x, |r| r.right() + 2),
        };
        line(
            Point::new(x, container.y),
            Point::new(x, container.bottom()),
        )
    } else {
        let y = match children.get(index) {
            Some(r) => r.y - 2,
            None => children.last().map_or(container.y, |r| r.bottom() + 2),
        };
        line(Point::new(container.x, y), Point::new(container.right(), y))
    })
}

pub(super) fn menu_bar() -> MenuBar {
    vec![(
        tr!("menu.title.view"),
        Menu::new().item("designer.code", &tr!("menu.title.code"), Some("F12")),
    )]
}

fn context_menu(kind: Option<ItemKind>) -> Menu {
    let menu = Menu::new();
    match kind {
        Some(ItemKind::Widget) => menu
            .item("designer.delete", "Löschen", Some("Delete"))
            .item("designer.natural_size", "Natürliche Größe", None)
            .separator()
            .item("designer.inspector", "Objektinspektor", None),
        Some(ItemKind::Component) => menu
            .item("designer.delete", "Löschen", Some("Delete"))
            .separator()
            .item("designer.inspector", "Objektinspektor", None),
        _ => menu.item("designer.inspector", "Objektinspektor", None),
    }
}

pub fn connect(ide: &Rc<Ide>) {
    let designer = &ide.designer;
    let weak = Rc::downgrade(ide);
    designer.window.on_resized(move |w, h| {
        if let Some(ide) = weak.upgrade() {
            ide.designer.layout(w, h);
            ide.designer.draw(&ide, None);
        }
    });

    designer.canvas.on_mouse(with(ide, |ide, e: MouseEvent| {
        let at = Point::new(e.x, e.y);
        match (e.action, e.button) {
            (MouseAction::DoubleClicked, 1) => {
                // Wie Delphi: Doppelklick legt den Handler des Standard-Ereignisses
                // an (falls nötig) und springt in den Code.
                let element = ide.session().single_selection().map(|(n, _)| n.clone());
                if let Some(element) = element
                    && !ide.session().is_root(&element)
                {
                    ide.open_handler(&element, None);
                }
            }
            (MouseAction::Pressed, 1) => {
                let add = e.modifiers.ctrl || e.modifiers.shift;
                let result = ide.session.borrow_mut().press(at, add);
                ide.report(result);
            }
            (MouseAction::Pressed, 3) => {
                // Rechtsklick wählt das Element unter der Maus und zeigt das Menü.
                let hit = {
                    let session = ide.session();
                    session
                        .layout()
                        .and_then(|l| tdr_designer::hit_test(&l, at, &[]))
                        .map(|h| h.element)
                };
                if let Some(element) = &hit
                    && !ide.session().selection().contains(element)
                {
                    ide.select(vec![element.clone()]);
                }
                let kind = ide.session().single_selection().map(|(_, k)| k);
                ide.designer
                    .canvas
                    .widget()
                    .popup_menu(&context_menu(kind), e.x, e.y);
            }
            (MouseAction::Moved, _) => {
                let dragging = ide.session.borrow_mut().drag(at);
                if !dragging {
                    // Über einem Anfasser zeigt der Mauszeiger die Richtung.
                    let cursor = {
                        let session = ide.session();
                        session
                            .layout()
                            .and_then(|l| tdr_designer::hit_test(&l, at, session.selection()))
                            .and_then(|h| h.handle)
                            .map_or("", |h| {
                                use tdr_designer::Handle as H;
                                match h {
                                    H::TopLeft | H::BottomRight => "nwse-resize",
                                    H::TopRight | H::BottomLeft => "nesw-resize",
                                    H::Left | H::Right => "ew-resize",
                                    H::Top | H::Bottom => "ns-resize",
                                }
                            })
                    };
                    ide.designer.canvas.widget().set_mouse_cursor(cursor);
                }
                if dragging {
                    let resize = ide.session().resize_preview(at);
                    if resize.is_some() {
                        ide.designer.draw_with(ide, None, resize);
                    } else {
                        let target = ide.session().drop_preview(at);
                        ide.designer.draw(ide, target);
                    }
                }
            }
            (MouseAction::Released, 1) => {
                let result = ide.session.borrow_mut().release(at);
                match result {
                    Ok(changes) if changes.any() => ide.report(Ok(changes)),
                    // Kein Verschieben: Einfügemarke entfernen.
                    Ok(_) => ide.designer.draw(ide, None),
                    Err(e) => ide.report(Err(e)),
                }
            }
            _ => {}
        }
    }));

    // Aus der Palette gezogen: Einfügemarke zeigen, beim Ablegen platzieren.
    designer
        .canvas
        .widget()
        .on_drop(with(ide, |ide, e: ruf::DropEvent| {
            let at = Point::new(e.x, e.y);
            let type_ref = e
                .text
                .as_deref()
                .and_then(|t| t.parse::<tdr_core::TypeRef>().ok());
            match e.action {
                ruf::DropAction::Motion => {
                    let target = type_ref.and_then(|t| ide.session().palette_drop_preview(&t, at));
                    ide.designer.draw(ide, target);
                }
                ruf::DropAction::Leave => ide.designer.draw(ide, None),
                ruf::DropAction::Drop => {
                    let Some(type_ref) = type_ref else {
                        ide.designer.draw(ide, None);
                        return;
                    };
                    ide.arm(None);
                    let result = ide.session.borrow_mut().drop_type(type_ref, at);
                    ide.report(result);
                    ide.designer.canvas.widget().focus();
                }
            }
        }));

    // Komponentenleiste: wählen, Doppelklick zum Handler, Kontextmenü,
    // nicht-visuelle Typen aus der Palette ablegen.
    designer.tray.on_mouse(with(ide, |ide, e: MouseEvent| {
        let at = Point::new(e.x, e.y);
        match (e.action, e.button) {
            (MouseAction::Pressed, 1) => {
                let add = e.modifiers.ctrl || e.modifiers.shift;
                ide.arm(None);
                let changes = ide.session.borrow_mut().press_tray(at, add);
                ide.report(Ok(changes));
            }
            (MouseAction::DoubleClicked, 1) => {
                let element = ide.session().single_selection().map(|(n, _)| n.clone());
                if let Some(element) = element {
                    ide.open_handler(&element, None);
                }
            }
            (MouseAction::Pressed, 3) => {
                let changes = ide.session.borrow_mut().press_tray(at, false);
                ide.report(Ok(changes));
                let kind = ide.session().single_selection().map(|(_, k)| k);
                ide.designer
                    .tray
                    .widget()
                    .popup_menu(&context_menu(kind), e.x, e.y);
            }
            _ => {}
        }
    }));
    designer
        .tray
        .widget()
        .on_drop(with(ide, |ide, e: ruf::DropEvent| {
            if e.action != ruf::DropAction::Drop {
                return;
            }
            let Some(type_ref) = e
                .text
                .as_deref()
                .and_then(|t| t.parse::<tdr_core::TypeRef>().ok())
            else {
                return;
            };
            let non_visual = ide
                .session()
                .document()
                .registry()
                .get(&type_ref)
                .is_some_and(|t| !t.kind.is_widget());
            if non_visual {
                ide.arm(None);
                let result = ide
                    .session
                    .borrow_mut()
                    .drop_type(type_ref, Point::new(0, 0));
                ide.report(result);
            }
        }));
    designer
        .tray
        .on_key(with(ide, |ide, e: KeyEvent| match e.key {
            Key::Delete | Key::Backspace => ide.delete_selection(),
            Key::Escape => ide.select(Vec::new()),
            _ => {}
        }));

    designer
        .canvas
        .on_key(with(ide, |ide, e: KeyEvent| match e.key {
            Key::Delete | Key::Backspace => ide.delete_selection(),
            Key::Escape => {
                ide.arm(None);
                ide.select(Vec::new());
            }
            Key::Char('z') if e.modifiers.ctrl && e.modifiers.shift => ide.redo(),
            Key::Char('z') if e.modifiers.ctrl => ide.undo(),
            _ => {}
        }));

    designer
        .window
        .on_menu(with(ide, |ide, id: String| match id.as_str() {
            "designer.delete" => ide.delete_selection(),
            "designer.natural_size" => ide.natural_size(),
            "designer.inspector" => ide.show_window(crate::desktop::window::INSPECTOR),
            "designer.code" => ide.show_code(),
            _ => {}
        }));
}
