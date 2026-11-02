//! What to draw: a display list of simple primitives.
//!
//! The UI paints the list on its canvas (RUF) – it needs no knowledge of
//! widgets, themes or layouts. The list is plain data, so tests and an AI
//! can inspect "what the user sees" without a window.

use serde::Serialize;
use tdr_core::{Color, ComponentName, Form, TypeRegistry, Value, Widget, builtin};

use crate::geometry::{Point, Rect};
use crate::hit::handles;
use crate::layout::{FormLayout, ItemKind, Props, display_text};
use crate::theme::Theme;

/// One drawing operation, in painting order.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Primitive {
    Rect {
        rect: Rect,
        fill: Option<Color>,
        stroke: Option<Color>,
        radius: i32,
        dashed: bool,
    },
    /// Single-line text; `x`/`y` is the top-left corner of the text line.
    Text {
        x: i32,
        y: i32,
        text: String,
        color: Color,
        size: i32,
        bold: bool,
    },
    Line {
        from: Point,
        to: Point,
        color: Color,
    },
    /// A component icon (`name` as given by the type, e.g. `"database"`);
    /// the UI draws a symbol for it, or a placeholder.
    Icon { rect: Rect, name: String },
}

fn rect(rect: Rect, fill: Option<Color>, stroke: Option<Color>, radius: i32) -> Primitive {
    Primitive::Rect {
        rect,
        fill,
        stroke,
        radius,
        dashed: false,
    }
}

struct Painter<'a> {
    props: Props<'a>,
    theme: &'a Theme,
    out: Vec<Primitive>,
}

impl Painter<'_> {
    fn text(&mut self, x: i32, y: i32, text: &str, color: Color, bold: bool) {
        if !text.is_empty() {
            self.out.push(Primitive::Text {
                x,
                y,
                text: text.to_owned(),
                color,
                size: self.theme.font_size,
                bold,
            });
        }
    }

    /// Text vertically centred in `r`, left-aligned at `x`.
    fn text_in(&mut self, r: Rect, x: i32, text: &str, color: Color, bold: bool) {
        let y = r.y + (r.height - self.theme.font_size) / 2;
        self.text(x, y, text, color, bold);
    }

    /// Text centred in `r` both ways.
    fn text_centered(&mut self, r: Rect, text: &str, color: Color) {
        let x = r.x + (r.width - self.theme.text_width(text)) / 2;
        self.text_in(r, x, text, color, false);
    }

    fn widget(&mut self, widget: &Widget, r: Rect, kind: ItemKind) {
        let c = &self.theme.colors;
        let enabled = self.props.bool(widget, "enabled", true);
        let text_color = if enabled { c.text } else { c.disabled_text };
        let type_ref = &widget.component.type_ref;
        let caption = display_text(&self.props, widget).unwrap_or_default();

        if kind == ItemKind::Window {
            let title = self.props.text(widget, "title").unwrap_or_default();
            let bar = Rect::new(r.x, r.y, r.width, self.theme.title_bar_height);
            let (window, border, title_bar, title_text) =
                (c.window, c.window_border, c.title_bar, c.title_text);
            self.out
                .push(rect(r, Some(window), Some(border), self.theme.radius));
            self.out.push(rect(bar, Some(title_bar), None, 0));
            self.out.push(Primitive::Line {
                from: Point::new(r.x, bar.bottom()),
                to: Point::new(r.right(), bar.bottom()),
                color: border,
            });
            let x = bar.x + (bar.width - self.theme.text_width(&title)) / 2;
            self.text_in(bar, x, &title, title_text, true);
        } else if *type_ref == builtin::button() {
            let (control, border) = (c.control, c.control_border);
            self.out
                .push(rect(r, Some(control), Some(border), self.theme.radius));
            self.text_centered(r, &caption, text_color);
        } else if *type_ref == builtin::label() {
            self.text_in(r, r.x, &caption, text_color, false);
        } else if *type_ref == builtin::edit() {
            let (input, border, placeholder_color) = (c.input, c.control_border, c.placeholder);
            self.out
                .push(rect(r, Some(input), Some(border), self.theme.radius));
            let padding = self.theme.metrics(type_ref).padding_x;
            let text = self.props.text(widget, "text").unwrap_or_default();
            if text.is_empty() {
                let placeholder = self.props.text(widget, "placeholder").unwrap_or_default();
                self.text_in(r, r.x + padding, &placeholder, placeholder_color, false);
            } else if self.props.bool(widget, "password", false) {
                let dots = "•".repeat(text.chars().count());
                self.text_in(r, r.x + padding, &dots, text_color, false);
            } else {
                self.text_in(r, r.x + padding, &text, text_color, false);
            }
        } else if *type_ref == builtin::check_box() {
            let size = self.theme.check_size;
            let check = Rect::new(r.x, r.y + (r.height - size) / 2, size, size);
            let (input, border, accent) = (c.input, c.control_border, c.accent);
            let checked = matches!(self.props.value(widget, "checked"), Some(Value::Bool(true)));
            self.out.push(rect(
                check,
                Some(if checked { accent } else { input }),
                Some(border),
                self.theme.radius / 2,
            ));
            if checked {
                self.text_centered(check, "✓", Color::rgb(255, 255, 255));
            }
            self.text_in(
                r,
                check.right() + self.theme.spacing,
                &caption,
                text_color,
                false,
            );
        } else if self.props.is_container(widget) {
            self.out.push(Primitive::Rect {
                rect: r,
                fill: None,
                stroke: Some(c.container_border),
                radius: 0,
                dashed: true,
            });
        } else {
            // A backend's own widget: a box with its icon and its type name
            // or caption (the real look comes from the debug run).
            let (control, border) = (c.control, c.control_border);
            self.out.push(rect(r, Some(control), Some(border), 0));
            let label = if caption.is_empty() {
                type_ref.name.clone()
            } else {
                caption
            };
            let icon = self
                .props
                .registry
                .get(type_ref)
                .and_then(|t| t.icon.clone());
            let size = (r.height - 8).min(24);
            match icon {
                Some(name) if size >= 12 && r.width >= size + 8 => {
                    let at = Rect::new(r.x + 6, r.y + (r.height - size) / 2, size, size);
                    self.out.push(Primitive::Icon { rect: at, name });
                    let x = at.right() + 6;
                    if r.right() - x > 8 {
                        self.text_in(r, x, &label, text_color, false);
                    }
                }
                _ => self.text_centered(r, &label, text_color),
            }
        }

        // Hidden widgets stay visible in the designer, dimmed.
        if !self.props.bool(widget, "visible", true) {
            let mut veil = c.window;
            veil.a = 160;
            self.out.push(rect(r, Some(veil), None, 0));
        }
    }
}

/// The display list of `form` laid out as `layout`.
pub fn render(
    form: &Form,
    layout: &FormLayout,
    registry: &TypeRegistry,
    theme: &Theme,
) -> Vec<Primitive> {
    let mut painter = Painter {
        props: Props { form, registry },
        theme,
        out: vec![rect(
            Rect::new(0, 0, layout.width, layout.height),
            Some(theme.colors.desktop),
            None,
            0,
        )],
    };
    let has_tray = layout.items.iter().any(|i| i.kind == ItemKind::Component);
    if has_tray {
        let top = layout
            .items
            .iter()
            .filter(|i| i.kind == ItemKind::Component)
            .map(|i| i.rect.y)
            .min()
            .unwrap_or(0);
        let tray = Rect::new(0, top, layout.width, layout.height - top);
        painter
            .out
            .push(rect(tray, Some(theme.colors.tray), None, 0));
    }
    for item in &layout.items {
        match item.kind {
            ItemKind::Window | ItemKind::Widget => {
                if let Some(widget) = form.widget(&item.name) {
                    painter.widget(widget, item.rect, item.kind);
                }
            }
            ItemKind::Component => {
                let Some(component) = form.component(&item.name) else {
                    continue;
                };
                let icon_name = registry
                    .get(&component.type_ref)
                    .and_then(|t| t.icon.clone())
                    .unwrap_or_else(|| "component".to_owned());
                let size = theme.icon_size;
                let icon = Rect::new(item.rect.center().x - size / 2, item.rect.y + 6, size, size);
                painter.out.push(Primitive::Icon {
                    rect: icon,
                    name: icon_name,
                });
                let name = component.name.as_str();
                let x = item.rect.center().x - theme.text_width(name) / 2;
                let text = theme.colors.text;
                painter.text(x, icon.bottom() + 4, name, text, false);
            }
        }
    }
    painter.out
}

/// The selection frame and resize handles for `selected`.
pub fn render_selection(
    layout: &FormLayout,
    selected: &[ComponentName],
    theme: &Theme,
) -> Vec<Primitive> {
    let mut out = Vec::new();
    for item in layout.items.iter().filter(|i| selected.contains(&i.name)) {
        out.push(Primitive::Rect {
            rect: item.rect,
            fill: None,
            stroke: Some(theme.colors.selection),
            radius: 0,
            dashed: false,
        });
        // Only widgets can be resized (tray icons have no size in the model).
        if item.kind != ItemKind::Component {
            for (_, h) in handles(item.rect) {
                out.push(rect(
                    h,
                    Some(theme.colors.selection),
                    Some(Color::rgb(255, 255, 255)),
                    0,
                ));
            }
        }
    }
    out
}
