//! Designer window: the form on a canvas. Click selects (Ctrl/Shift adds),
//! a click with an armed palette type places it, dragging reorders or moves
//! into another container, Delete removes, Esc cancels.

use std::rc::Rc;

use rncl::prelude::*;
use ruf::{Color, Key};
use tdr_core::{ComponentName, Platform};
use tdr_designer::{ItemKind, Point};

use super::{Ide, convert, with};

const MARGIN: i32 = 6;
const ROW: i32 = 28;

const PLATFORMS: [(&str, Platform); 3] = [
    ("Linux", Platform::Linux),
    ("Windows", Platform::Windows),
    ("macOS", Platform::MacOs),
];

pub struct DesignerWindow {
    pub window: RnclWindow,
    form_label: RnclLabel,
    form: RnclComboBox,
    look_label: RnclLabel,
    look: RnclComboBox,
    scroll: RnclScrollView,
    canvas: RnclCanvas,
}

impl DesignerWindow {
    pub fn build() -> Self {
        let window = RnclWindow::new("Designer");
        let form_label = RnclLabel::new(&window, "Formular:");
        let form = RnclComboBox::new(&window);
        let look_label = RnclLabel::new(&window, "Aussehen:");
        let look = RnclComboBox::new(&window);
        look.set_items(&PLATFORMS.map(|(n, _)| n));
        let scroll = RnclScrollView::new(&window);
        let canvas = RnclCanvas::new(scroll.content());
        Self {
            window,
            form_label,
            form,
            look_label,
            look,
            scroll,
            canvas,
        }
    }

    fn layout(&self, width: i32, height: i32) {
        self.form_label.set_geometry(MARGIN, MARGIN, 80, ROW);
        self.form.set_geometry(MARGIN + 80, MARGIN, 180, ROW);
        self.look_label.set_geometry(MARGIN + 280, MARGIN, 80, ROW);
        self.look.set_geometry(MARGIN + 360, MARGIN, 140, ROW);
        let top = MARGIN + ROW + MARGIN;
        self.scroll
            .set_geometry(0, top, width, (height - top).max(0));
    }

    pub fn canvas_widget(&self) -> Widget {
        self.canvas.widget().clone()
    }

    /// Redraws the form; `drop` marks where a dragged widget would land.
    fn draw(&self, ide: &Ide, drop: Option<(ComponentName, usize)>) {
        let session = ide.session();
        let Some((layout, primitives)) = session.picture() else {
            self.canvas.set_shapes(Vec::new());
            return;
        };
        let theme = session.theme();
        let mut shapes = convert::shapes(&primitives, theme.colors.window);
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
    }

    pub fn refresh(&self, ide: &Ide) {
        let session = ide.session();
        let names: Vec<String> = session
            .document()
            .forms()
            .iter()
            .map(|f| f.name.to_string())
            .collect();
        if self.form.items() != names {
            self.form.set_items(&names);
        }
        let current = session
            .form_name()
            .map(ToString::to_string)
            .unwrap_or_default();
        self.form.set_current_text(&current);
        let look = PLATFORMS
            .iter()
            .position(|(_, p)| p == session.platform())
            .unwrap_or(0);
        self.look.set_current_index(look as i32);
        self.window.set_title(&format!("Designer – {current}"));
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

fn context_menu(kind: Option<ItemKind>) -> Menu {
    let menu = Menu::new();
    match kind {
        Some(ItemKind::Widget | ItemKind::Component) => menu
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

    designer
        .form
        .on_current_index_changed(with(ide, |ide, _: i32| {
            if ide.is_refreshing() {
                return;
            }
            if let Ok(name) = ComponentName::new(ide.designer.form.current_text()) {
                ide.select_form(&name);
            }
        }));
    designer
        .look
        .on_current_index_changed(with(ide, |ide, index: i32| {
            if ide.is_refreshing() {
                return;
            }
            if let Some((_, platform)) = usize::try_from(index).ok().and_then(|i| PLATFORMS.get(i))
            {
                ide.session.borrow_mut().set_platform(platform.clone());
                ide.designer.draw(ide, None);
            }
        }));

    designer.canvas.on_mouse(with(ide, |ide, e: MouseEvent| {
        let at = Point::new(e.x, e.y);
        match (e.action, e.button) {
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
                if dragging {
                    let target = ide.session().drop_preview(at);
                    ide.designer.draw(ide, target);
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
            "designer.inspector" => ide.show_window(crate::desktop::window::INSPECTOR),
            _ => {}
        }));
}
