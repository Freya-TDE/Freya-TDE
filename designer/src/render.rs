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
        italic: bool,
        /// Font family (`None` = the UI's standard font).
        family: Option<String>,
    },
    Line {
        from: Point,
        to: Point,
        color: Color,
    },
    /// A component icon (`name` as given by the type, e.g. `"database"`);
    /// the UI draws a symbol for it, or a placeholder.
    Icon { rect: Rect, name: String },
    /// A picture file (relative to the project), e.g. a form's background.
    /// `fit`: `Cover`, `Stretch`, `Contain` or `Center`; `opacity` 0…1,
    /// `gamma` 1 = unchanged (above 1 lighter).
    Image {
        rect: Rect,
        path: String,
        fit: String,
        opacity: f64,
        gamma: f64,
    },
}

/// The text style of the element being painted.
#[derive(Debug, Clone, Default)]
struct TextStyle {
    size: i32,
    bold: bool,
    italic: bool,
    family: Option<String>,
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
    /// Colours and text style of the element being painted (theme plus its
    /// own and inherited settings).
    colors: crate::theme::Colors,
    style: TextStyle,
}

impl Painter<'_> {
    fn text(&mut self, x: i32, y: i32, text: &str, color: Color, bold: bool) {
        if !text.is_empty() {
            self.out.push(Primitive::Text {
                x,
                y,
                text: text.to_owned(),
                color,
                size: self.style.size,
                bold: bold || self.style.bold,
                italic: self.style.italic,
                family: self.style.family.clone(),
            });
        }
    }

    /// Text vertically centred in `r`, left-aligned at `x`.
    fn text_in(&mut self, r: Rect, x: i32, text: &str, color: Color, bold: bool) {
        let y = r.y + (r.height - self.style.size) / 2;
        self.text(x, y, text, color, bold);
    }

    /// Text centred in `r` both ways.
    fn text_centered(&mut self, r: Rect, text: &str, color: Color) {
        let width = self.theme.text_width(text) * self.style.size / self.theme.font_size.max(1);
        let x = r.x + (r.width - width) / 2;
        self.text_in(r, x, text, color, false);
    }

    /// Takes the element's own and inherited appearance (colours, font).
    fn take_style(&mut self, widget: &Widget) {
        let own = crate::layout::appearance(&self.props, widget, self.theme);
        let mut colors = self.theme.colors.clone();
        if let Some(fg) = own.foreground {
            colors.text = fg;
        }
        if let Some(bg) = own.background {
            colors.control = bg;
            colors.input = bg;
            colors.window = bg;
        }
        self.colors = colors;
        self.style = TextStyle {
            size: own.font_size,
            bold: own.bold,
            italic: own.italic,
            family: own.family,
        };
    }

    /// The background picture of a form or container in `r`.
    fn background_image(&mut self, widget: &Widget, r: Rect) {
        let path = self
            .props
            .text(widget, "background_image")
            .unwrap_or_default();
        if !path.trim().is_empty() {
            let fit = match self.props.value(widget, "background_fit") {
                Some(Value::Enum(fit)) => fit,
                _ => "Cover".to_owned(),
            };
            self.out.push(Primitive::Image {
                rect: r,
                path: path.trim().to_owned(),
                fit,
                opacity: 1.0,
                gamma: 1.0,
            });
        }
    }

    /// Fit, opacity and gamma of a picture widget.
    fn picture_tone(&self, widget: &Widget, default_fit: &str) -> (String, f64, f64) {
        let fit = match self.props.value(widget, "fit") {
            Some(Value::Enum(fit)) => fit,
            _ => default_fit.to_owned(),
        };
        (
            fit,
            self.props.float(widget, "opacity", 1.0).clamp(0.0, 1.0),
            self.props.float(widget, "gamma", 1.0).clamp(0.1, 5.0),
        )
    }

    /// A dashed frame with an icon where a picture will be.
    fn picture_placeholder(&mut self, r: Rect, icon: &str) {
        self.out.push(Primitive::Rect {
            rect: r,
            fill: None,
            stroke: Some(self.colors.container_border),
            radius: 0,
            dashed: true,
        });
        let size = r.width.min(r.height).min(48) - 8;
        if size > 8 {
            let at = Rect::new(
                r.x + (r.width - size) / 2,
                r.y + (r.height - size) / 2,
                size,
                size,
            );
            self.out.push(Primitive::Icon {
                rect: at,
                name: icon.into(),
            });
        }
    }

    /// A backend widget drawn by its type's `look` – close to the real thing,
    /// from the theme colors (the real look comes from the debug run).
    fn look(&mut self, look: &str, widget: &Widget, r: Rect, caption: &str, text_color: Color) {
        let c = self.colors.clone();
        let radius = self.theme.radius;
        let font = self.style.size;
        let white = Color::rgb(255, 255, 255);
        let value_fraction = |p: &Props<'_>| {
            let (min, max) = (
                p.float(widget, "minimum", 0.0),
                p.float(widget, "maximum", 100.0),
            );
            let value = p.float(widget, "value", 0.0);
            if max > min {
                ((value - min) / (max - min)).clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        match look {
            "switch" => {
                let (w, h) = (r.width.min(48), r.height.min(26));
                let track = Rect::new(r.x, r.y + (r.height - h) / 2, w, h);
                let on = self.props.bool(widget, "active", false);
                let fill = if on { c.accent } else { c.control_border };
                self.out.push(rect(track, Some(fill), None, h / 2));
                let knob = h - 4;
                let x = if on {
                    track.right() - knob - 2
                } else {
                    track.x + 2
                };
                self.out.push(rect(
                    Rect::new(x, track.y + 2, knob, knob),
                    Some(white),
                    None,
                    knob / 2,
                ));
            }
            "slider" => {
                let y = r.y + r.height / 2;
                let (x0, x1) = (r.x + 8, r.right() - 8);
                let at = x0 + ((x1 - x0) as f64 * value_fraction(&self.props)).round() as i32;
                self.out.push(rect(
                    Rect::new(x0, y - 2, x1 - x0, 4),
                    Some(c.control_border),
                    None,
                    2,
                ));
                self.out.push(rect(
                    Rect::new(x0, y - 2, at - x0, 4),
                    Some(c.accent),
                    None,
                    2,
                ));
                self.out.push(rect(
                    Rect::new(at - 8, y - 8, 16, 16),
                    Some(white),
                    Some(c.control_border),
                    8,
                ));
            }
            "spin" => {
                self.out
                    .push(rect(r, Some(c.input), Some(c.control_border), radius));
                let digits = self.props.int(widget, "digits", 0).clamp(0, 20) as usize;
                let text = format!("{:.*}", digits, self.props.float(widget, "value", 0.0));
                let padding = self.theme.metrics(&widget.component.type_ref).padding_x;
                self.text_in(r, r.x + padding, &text, text_color, false);
                let b = r.height;
                for (i, sign) in ["−", "+"].iter().enumerate() {
                    let cell = Rect::new(r.right() - b * (2 - i as i32), r.y, b, r.height);
                    self.out.push(Primitive::Line {
                        from: Point::new(cell.x, r.y),
                        to: Point::new(cell.x, r.bottom()),
                        color: c.control_border,
                    });
                    self.text_centered(cell, sign, text_color);
                }
            }
            "progress" => {
                let text = self.props.text(widget, "text").unwrap_or_default();
                let bar = Rect::new(r.x, r.bottom() - 8, r.width, 6);
                let fraction = self.props.float(widget, "fraction", 0.0).clamp(0.0, 1.0);
                self.out.push(rect(bar, Some(c.control_border), None, 3));
                let done = (bar.width as f64 * fraction).round() as i32;
                self.out.push(rect(
                    Rect::new(bar.x, bar.y, done, bar.height),
                    Some(c.accent),
                    None,
                    3,
                ));
                if !text.is_empty() {
                    let x = r.x + (r.width - self.theme.text_width(&text)) / 2;
                    self.text(x, bar.y - font - 2, &text, c.placeholder, false);
                }
            }
            "dropdown" => {
                self.out
                    .push(rect(r, Some(c.control), Some(c.control_border), radius));
                let items = self.props.list_items(widget);
                let selected = self.props.int(widget, "selected", 0).max(0) as usize;
                let selected = if selected < items.len() { selected } else { 0 };
                let text = items.get(selected).cloned().unwrap_or_default();
                let padding = self.theme.metrics(&widget.component.type_ref).padding_x;
                self.text_in(r, r.x + padding, &text, text_color, false);
                let arrow = Rect::new(r.right() - r.height, r.y, r.height, r.height);
                self.text_centered(arrow, "▾", text_color);
            }
            "memo" | "list" => {
                self.out
                    .push(rect(r, Some(c.input), Some(c.control_border), radius));
                let padding = self.theme.metrics(&widget.component.type_ref).padding_x;
                let row = font + 10;
                let (lines, selected) = if look == "list" {
                    (
                        self.props.list_items(widget),
                        self.props.int(widget, "selected", -1),
                    )
                } else {
                    let text = self.props.text(widget, "text").unwrap_or_default();
                    (text.lines().map(str::to_owned).collect(), -1)
                };
                let mut y = r.y + 4;
                for (i, line) in lines.iter().enumerate() {
                    if y + row > r.bottom() - 2 {
                        break;
                    }
                    let line_rect = Rect::new(r.x + 1, y, r.width - 2, row);
                    let color = if i as i64 == selected {
                        self.out.push(rect(line_rect, Some(c.accent), None, 0));
                        white
                    } else {
                        text_color
                    };
                    self.text_in(line_rect, r.x + padding, line, color, false);
                    y += row;
                }
            }
            "table" => {
                // Kopfzeile aus `headers`/`fields`, sonst die Datenquelle.
                use tdr_core::data::{DATA_SOURCE, FIELDS, HEADERS};
                self.out
                    .push(rect(r, Some(c.input), Some(c.control_border), radius));
                let fields = self.props.texts(widget, FIELDS);
                let headers = self.props.texts(widget, HEADERS);
                let titles: Vec<String> = if !fields.is_empty() {
                    fields
                        .iter()
                        .enumerate()
                        .map(|(i, f)| {
                            headers
                                .get(i)
                                .filter(|h| !h.is_empty())
                                .cloned()
                                .unwrap_or_else(|| f.clone())
                        })
                        .collect()
                } else if !headers.is_empty() {
                    headers
                } else if let Some(Value::Reference(Some(source))) =
                    self.props.value(widget, DATA_SOURCE)
                {
                    vec![format!("⟨{source}⟩")]
                } else {
                    vec![widget.component.type_ref.name.clone()]
                };
                let row = font + 10;
                let head = Rect::new(r.x + 1, r.y + 1, r.width - 2, row);
                self.out.push(rect(head, Some(c.control), None, 0));
                let width = (r.width - 2) / titles.len().max(1) as i32;
                for (i, title) in titles.iter().enumerate() {
                    let cell = Rect::new(r.x + 1 + i as i32 * width, head.y, width, row);
                    self.text_in(cell, cell.x + 6, title, text_color, true);
                    if i > 0 {
                        self.out.push(Primitive::Line {
                            from: Point::new(cell.x, r.y + 1),
                            to: Point::new(cell.x, r.bottom() - 1),
                            color: c.control_border,
                        });
                    }
                }
                let mut y = head.bottom();
                while y + row < r.bottom() {
                    self.out.push(Primitive::Line {
                        from: Point::new(r.x + 1, y),
                        to: Point::new(r.right() - 1, y),
                        color: c.control_border,
                    });
                    y += row;
                }
            }
            "radio" => {
                let size = self.theme.check_size;
                let dot = Rect::new(r.x, r.y + (r.height - size) / 2, size, size);
                let checked = self.props.bool(widget, "checked", false);
                self.out.push(rect(
                    dot,
                    Some(c.input),
                    Some(if checked { c.accent } else { c.control_border }),
                    size / 2,
                ));
                if checked {
                    let inner = Rect::new(dot.x + 4, dot.y + 4, size - 8, size - 8);
                    self.out
                        .push(rect(inner, Some(c.accent), None, (size - 8) / 2));
                }
                self.text_in(
                    r,
                    dot.right() + self.theme.spacing,
                    caption,
                    text_color,
                    false,
                );
            }
            "separator" => {
                let (from, to) = if self.props.bool(widget, "vertical", false) {
                    (
                        Point::new(r.x + r.width / 2, r.y),
                        Point::new(r.x + r.width / 2, r.bottom()),
                    )
                } else {
                    (
                        Point::new(r.x, r.y + r.height / 2),
                        Point::new(r.right(), r.y + r.height / 2),
                    )
                };
                self.out.push(Primitive::Line {
                    from,
                    to,
                    color: c.control_border,
                });
            }
            "image" => {
                self.out.push(Primitive::Rect {
                    rect: r,
                    fill: None,
                    stroke: Some(c.container_border),
                    radius: 0,
                    dashed: true,
                });
                let size = r.width.min(r.height).min(48) - 8;
                if size > 8 {
                    let at = Rect::new(
                        r.x + (r.width - size) / 2,
                        r.y + (r.height - size) / 2,
                        size,
                        size,
                    );
                    self.out.push(Primitive::Icon {
                        rect: at,
                        name: "image".into(),
                    });
                }
            }
            "imagebox" => {
                let file = self.props.text(widget, "file").unwrap_or_default();
                if file.trim().is_empty() {
                    self.picture_placeholder(r, "imagebox");
                } else {
                    let (fit, opacity, gamma) = self.picture_tone(widget, "Contain");
                    self.out.push(Primitive::Image {
                        rect: r,
                        path: file.trim().to_owned(),
                        fit,
                        opacity,
                        gamma,
                    });
                }
            }
            "gallery" => {
                self.out
                    .push(rect(r, Some(c.input), Some(c.control_border), radius));
                let thumb = match self.props.value(widget, "thumb_size") {
                    Some(Value::Size(s)) => (s.width.max(8), s.height.max(8)),
                    _ => (96, 96),
                };
                let gap = self.props.int(widget, "spacing", 8).clamp(0, 200) as i32;
                let orientation = match self.props.value(widget, "orientation") {
                    Some(Value::Enum(o)) => o,
                    _ => "Grid".to_owned(),
                };
                let images: Vec<String> = self
                    .props
                    .texts(widget, "images")
                    .into_iter()
                    .filter(|p| !p.trim().is_empty())
                    .collect();
                let (fit, opacity, gamma) = self.picture_tone(widget, "Cover");
                // Ohne Bilder drei Platzhalter, damit Größe und Raster sichtbar sind.
                let count = if images.is_empty() { 3 } else { images.len() };
                let (mut x, mut y) = (r.x + gap, r.y + gap);
                for i in 0..count {
                    if x + thumb.0 > r.right() - gap {
                        if orientation != "Grid" || i == 0 {
                            break;
                        }
                        x = r.x + gap;
                        y += thumb.1 + gap;
                    }
                    if y + thumb.1 > r.bottom() - gap {
                        break;
                    }
                    let at = Rect::new(x, y, thumb.0, thumb.1);
                    match images.get(i) {
                        Some(path) => self.out.push(Primitive::Image {
                            rect: at,
                            path: path.trim().to_owned(),
                            fit: fit.clone(),
                            opacity,
                            gamma,
                        }),
                        None => self.picture_placeholder(at, "imagescrollbox"),
                    }
                    if orientation == "Vertical" {
                        y += thumb.1 + gap;
                    } else {
                        x += thumb.0 + gap;
                    }
                }
            }
            "frame" => {
                let header = self.theme.metrics(&widget.component.type_ref).header;
                let title = self.props.text(widget, "title").unwrap_or_default();
                let border = Rect::new(r.x, r.y + header / 2, r.width, r.height - header / 2);
                self.out
                    .push(rect(border, None, Some(c.container_border), radius));
                if !title.is_empty() {
                    let w = self.theme.text_width(&title) + 8;
                    self.out.push(rect(
                        Rect::new(r.x + 8, r.y, w, header),
                        Some(c.window),
                        None,
                        0,
                    ));
                    self.text_in(
                        Rect::new(r.x, r.y, r.width, header),
                        r.x + 12,
                        &title,
                        text_color,
                        false,
                    );
                }
            }
            "link" => {
                let text = if caption.is_empty() {
                    self.props.text(widget, "uri").unwrap_or_default()
                } else {
                    caption.to_owned()
                };
                let w = self.theme.text_width(&text);
                let x = r.x + (r.width - w) / 2;
                self.text_in(r, x, &text, c.accent, false);
                let y = r.y + (r.height + font) / 2 + 1;
                self.out.push(Primitive::Line {
                    from: Point::new(x, y),
                    to: Point::new(x + w, y),
                    color: c.accent,
                });
            }
            "tabs" => {
                let header = self.theme.metrics(&widget.component.type_ref).header;
                let body = Rect::new(r.x, r.y + header, r.width, r.height - header);
                self.out
                    .push(rect(body, Some(c.window), Some(c.container_border), 0));
                let page = self.props.int(widget, "page", 0).max(0) as usize;
                for (i, (tab, title)) in tabs(&self.props, self.theme, widget, r)
                    .into_iter()
                    .enumerate()
                {
                    self.text_centered(tab, &title, text_color);
                    if i == page {
                        self.out.push(rect(
                            Rect::new(tab.x + 4, tab.bottom() - 3, tab.width - 8, 3),
                            Some(c.accent),
                            None,
                            0,
                        ));
                    }
                }
            }
            // Aufklappbarer Abschnitt: Kopf mit Titel und Pfeil, darunter der Inhalt.
            "accordion" => {
                let header = self.theme.metrics(&widget.component.type_ref).header;
                let title = self.props.text(widget, "title").unwrap_or_default();
                self.out
                    .push(rect(r, Some(c.window), Some(c.container_border), radius));
                let head = Rect::new(r.x, r.y, r.width, header);
                self.out.push(rect(
                    head,
                    Some(c.control),
                    Some(c.container_border),
                    radius,
                ));
                self.text_in(head, r.x + 10, &title, text_color, true);
                let arrow = Rect::new(head.right() - header, head.y, header, header);
                self.text_centered(arrow, "▾", text_color);
            }
            // Monatsansicht: Kopf mit Monat, darunter 6 × 7 Tage.
            "calendar" => {
                self.out
                    .push(rect(r, Some(c.window), Some(c.container_border), radius));
                let head = Rect::new(r.x, r.y + 2, r.width, font + 10);
                self.text_centered(head, "‹    2026-11    ›", text_color);
                let cell_w = (r.width - 8).max(7) / 7;
                let rows_top = head.bottom() + 2;
                let cell_h = ((r.bottom() - rows_top - 4).max(7)) / 7;
                for (col, day) in ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"]
                    .iter()
                    .enumerate()
                {
                    let cell = Rect::new(r.x + 4 + col as i32 * cell_w, rows_top, cell_w, cell_h);
                    self.text_centered(cell, day, c.disabled_text);
                }
                for i in 0..35 {
                    let (row, col) = (i / 7 + 1, i % 7);
                    let cell = Rect::new(
                        r.x + 4 + col * cell_w,
                        rows_top + row * cell_h,
                        cell_w,
                        cell_h,
                    );
                    let day = i - 5;
                    if (1..=30).contains(&day) {
                        if day == 27 {
                            self.out.push(rect(cell, Some(c.accent), None, 4));
                        }
                        let color = if day == 27 { white } else { text_color };
                        self.text_centered(cell, &day.to_string(), color);
                    }
                }
            }
            // Kleiner Umschaltknopf in Pillenform.
            "chip" => {
                let selected = self.props.bool(widget, "selected", false);
                let pill = r.height / 2;
                if selected {
                    self.out.push(rect(r, Some(c.accent), None, pill));
                    self.text_centered(r, caption, white);
                } else {
                    self.out
                        .push(rect(r, Some(c.control), Some(c.control_border), pill));
                    self.text_centered(r, caption, text_color);
                }
            }
            // Farbfeld mit der gewählten Farbe.
            "colorpicker" => {
                let color = match self.props.value(widget, "color") {
                    Some(Value::Color(v)) if v.a > 0 => Color::rgba(v.r, v.g, v.b, v.a),
                    _ => c.accent,
                };
                let swatch = Rect::new(r.x, r.y, r.height.min(r.width), r.height);
                self.out
                    .push(rect(swatch, Some(color), Some(c.control_border), radius));
            }
            // Kreis als Ladeanzeige.
            "loader" => {
                let size = r.width.min(r.height);
                let circle = Rect::new(
                    r.x + (r.width - size) / 2,
                    r.y + (r.height - size) / 2,
                    size,
                    size,
                );
                self.out
                    .push(rect(circle, None, Some(c.control_border), size / 2));
                self.out.push(Primitive::Line {
                    from: Point::new(circle.x + size / 2, circle.y),
                    to: Point::new(circle.right(), circle.y + size / 2),
                    color: c.accent,
                });
            }
            // Platzhalter beim Laden: graue Fläche mit hellem Streifen.
            "skeleton" => {
                self.out.push(rect(r, Some(c.control), None, radius));
                let band = Rect::new(r.x + r.width / 3, r.y, r.width / 6, r.height);
                self.out.push(rect(band, Some(c.input), None, 0));
            }
            // Knopfreihe, ein Teil gewählt.
            "segmented" => {
                let items = self.props.list_items(widget);
                let selected = self.props.int(widget, "selected", 0).max(0) as usize;
                let pill = r.height / 2;
                self.out
                    .push(rect(r, Some(c.control), Some(c.control_border), pill));
                let count = items.len().max(1) as i32;
                let w = r.width / count;
                for (i, item) in items.iter().enumerate() {
                    let part = Rect::new(r.x + i as i32 * w, r.y, w, r.height);
                    if i == selected {
                        self.out.push(rect(part, Some(c.accent), None, pill));
                        self.text_centered(part, item, white);
                    } else {
                        self.text_centered(part, item, text_color);
                    }
                }
            }
            // Text, linksbündig (mehrzeilig).
            "text" => {
                let text = self.props.text(widget, "text").unwrap_or_default();
                let row = font + 6;
                let mut y = r.y + 2;
                for line in text.lines() {
                    if y + row > r.bottom() + 2 {
                        break;
                    }
                    self.text_in(
                        Rect::new(r.x, y, r.width, row),
                        r.x,
                        line,
                        text_color,
                        false,
                    );
                    y += row;
                }
            }
            // Panels mit Griff: gestrichelter Rahmen, Griff in der Mitte.
            "splitter" => {
                self.out.push(Primitive::Rect {
                    rect: r,
                    fill: None,
                    stroke: Some(c.container_border),
                    radius,
                    dashed: true,
                });
                if self.props.bool(widget, "vertical", false) {
                    let y = r.y + r.height / 2;
                    self.out.push(Primitive::Line {
                        from: Point::new(r.x, y),
                        to: Point::new(r.right(), y),
                        color: c.accent,
                    });
                } else {
                    let x = r.x + r.width / 2;
                    self.out.push(Primitive::Line {
                        from: Point::new(x, r.y),
                        to: Point::new(x, r.bottom()),
                        color: c.accent,
                    });
                }
            }
            // Terminal: dunkle Fläche mit Eingabeaufforderung.
            "terminal" => {
                let dark = Color::rgb(0x12, 0x12, 0x12);
                self.out
                    .push(rect(r, Some(dark), Some(c.control_border), radius));
                let row = Rect::new(r.x + 6, r.y + 4, r.width - 12, font + 6);
                self.text_in(row, row.x, "$ _", Color::rgb(0xe0, 0xe0, 0xe0), false);
            }
            // Diagramm: Achsen und eine Linie (Balken bei `kind = Bar`).
            "chart" => {
                self.out
                    .push(rect(r, Some(c.window), Some(c.container_border), radius));
                let (x0, y0) = (r.x + 24, r.bottom() - 16);
                let (x1, y1) = (r.right() - 8, r.y + 12);
                self.out.push(Primitive::Line {
                    from: Point::new(x0, y1),
                    to: Point::new(x0, y0),
                    color: c.control_border,
                });
                self.out.push(Primitive::Line {
                    from: Point::new(x0, y0),
                    to: Point::new(x1, y0),
                    color: c.control_border,
                });
                let values = match self.props.value(widget, "values") {
                    Some(Value::List(items)) => items
                        .iter()
                        .filter_map(|v| match v {
                            Value::Float(f) => Some(*f),
                            Value::Int(i) => Some(*i as f64),
                            _ => None,
                        })
                        .collect(),
                    _ => Vec::new(),
                };
                let values = if values.is_empty() {
                    vec![1.0, 3.0, 2.0, 4.0]
                } else {
                    values
                };
                let high = values.iter().copied().fold(f64::MIN, f64::max).max(1e-9);
                let n = values.len() as i32;
                let w = ((x1 - x0) / n.max(1)).max(1);
                let y_of = |v: f64| y0 - (((y0 - y1) as f64) * (v / high).clamp(0.0, 1.0)) as i32;
                let bars =
                    matches!(self.props.value(widget, "kind"), Some(Value::Enum(k)) if k == "Bar");
                for (i, v) in values.iter().enumerate() {
                    let x = x0 + i as i32 * w;
                    if bars {
                        let top = y_of(*v);
                        self.out.push(rect(
                            Rect::new(x + w / 6, top, w * 2 / 3, y0 - top),
                            Some(c.accent),
                            None,
                            0,
                        ));
                    } else if i + 1 < values.len() {
                        self.out.push(Primitive::Line {
                            from: Point::new(x + w / 2, y_of(*v)),
                            to: Point::new(x + w + w / 2, y_of(values[i + 1])),
                            color: c.accent,
                        });
                    }
                }
            }
            // Dialog: Karte mit Titel.
            "popup" => {
                let header = self.theme.metrics(&widget.component.type_ref).header;
                let title = self.props.text(widget, "title").unwrap_or_default();
                self.out
                    .push(rect(r, Some(c.window), Some(c.window_border), radius * 2));
                let head = Rect::new(r.x, r.y, r.width, header);
                self.text_in(head, r.x + 12, &title, text_color, true);
            }
            _ => {
                self.out
                    .push(rect(r, Some(c.control), Some(c.control_border), 0));
                self.text_centered(r, &widget.component.type_ref.name, text_color);
            }
        }
    }

    fn widget(&mut self, widget: &Widget, r: Rect, kind: ItemKind) {
        self.take_style(widget);
        let own_background = crate::layout::appearance(&self.props, widget, self.theme).background;
        let c = &self.colors.clone();
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
            // Hintergrundbild unter der Titel- und Menüleiste.
            let bars = self.theme.title_bar_height
                + if self.props.menu_bar().is_some() {
                    self.theme.menu_bar_height
                } else {
                    0
                };
            self.background_image(
                widget,
                Rect::new(r.x + 1, r.y + bars + 1, r.width - 2, r.height - bars - 2),
            );
            let x = bar.x + (bar.width - self.theme.text_width(&title)) / 2;
            self.text_in(bar, x, &title, title_text, true);
            // Hauptmenü wie bei Delphi: die Menüleiste steht im Formular.
            if let Some(titles) = self.props.menu_bar() {
                let menu = Rect::new(
                    r.x + 1,
                    bar.bottom() + 1,
                    r.width - 2,
                    self.theme.menu_bar_height - 1,
                );
                self.out.push(rect(menu, Some(c.control), None, 0));
                self.out.push(Primitive::Line {
                    from: Point::new(r.x, menu.bottom()),
                    to: Point::new(r.right(), menu.bottom()),
                    color: border,
                });
                let mut x = menu.x + 10;
                for title in titles {
                    self.text_in(menu, x, &title, c.text, false);
                    x += self.theme.text_width(&title) + 20;
                }
            }
        } else if let Some(look) = self.props.look(widget).map(str::to_owned) {
            self.look(&look, widget, r, &caption, text_color);
        } else if *type_ref == builtin::button() {
            let (control, border) = (c.control, c.control_border);
            self.out
                .push(rect(r, Some(control), Some(border), self.theme.radius));
            self.text_centered(r, &caption, text_color);
        } else if *type_ref == builtin::label() {
            if let Some(bg) = own_background {
                self.out.push(rect(r, Some(bg), None, 0));
            }
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
            if let Some(bg) = own_background {
                self.out.push(rect(r, Some(bg), None, 0));
            }
            self.background_image(widget, r);
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

/// The tabs of a widget with `look = "tabs"` drawn in `r`: area and title
/// (`pages[i]`, else the child's name).
fn tabs(props: &Props<'_>, theme: &Theme, widget: &Widget, r: Rect) -> Vec<(Rect, String)> {
    let header = theme.metrics(&widget.component.type_ref).header;
    let pages = props.texts(widget, "pages");
    let mut x = r.x;
    widget
        .children
        .iter()
        .enumerate()
        .map(|(i, child)| {
            let title = pages
                .get(i)
                .cloned()
                .unwrap_or_else(|| child.component.name.to_string());
            let w = theme.text_width(&title) + 24;
            let tab = Rect::new(x, r.y, w, header);
            x += w;
            (tab, title)
        })
        .collect()
}

/// The tab under `at`: the widget with `look = "tabs"` and the page index –
/// a click there shows that page in the designer (like Delphi).
pub fn tab_at(
    form: &Form,
    layout: &FormLayout,
    registry: &TypeRegistry,
    theme: &Theme,
    at: Point,
) -> Option<(ComponentName, usize)> {
    let props = Props { form, registry };
    // Innerste Reiter zuerst (tiefere Elemente liegen oben).
    let mut items: Vec<_> = layout
        .items
        .iter()
        .filter(|i| i.kind == ItemKind::Widget)
        .collect();
    items.sort_by_key(|i| std::cmp::Reverse(i.depth));
    items.into_iter().find_map(|item| {
        let widget = form.widget(&item.name)?;
        if props.look(widget) != Some("tabs") {
            return None;
        }
        tabs(&props, theme, widget, item.rect)
            .iter()
            .position(|(tab, _)| tab.contains(at))
            .map(|page| (item.name.clone(), page))
    })
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
        colors: theme.colors.clone(),
        style: TextStyle {
            size: theme.font_size,
            ..TextStyle::default()
        },
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

/// Anchor marks of the selected, absolutely placed widgets: from each
/// anchored edge a line to the same edge of the parent's content, with a pin
/// at its end (like an anchor editor) – so one sees which edges keep their
/// distance when the parent grows.
pub fn render_anchors(
    form: &tdr_core::Form,
    layout: &FormLayout,
    selected: &[ComponentName],
    theme: &Theme,
) -> Vec<Primitive> {
    let mut out = Vec::new();
    let color = theme.colors.selection;
    for item in layout.items.iter().filter(|i| selected.contains(&i.name)) {
        let Some(widget) = form.widget(&item.name) else {
            continue;
        };
        let tdr_core::Placement::Absolute { anchors, .. } = widget.visual.placement else {
            continue;
        };
        let Some(area) = item
            .parent
            .as_ref()
            .and_then(|p| layout.item(p))
            .map(|p| p.content.unwrap_or(p.rect))
        else {
            continue;
        };
        let r = item.rect;
        let (cx, cy) = (r.x + r.width / 2, r.y + r.height / 2);
        let mut mark = |from: Point, to: Point| {
            out.push(Primitive::Line { from, to, color });
            out.push(rect(
                Rect::new(to.x - 3, to.y - 3, 7, 7),
                Some(color),
                Some(color),
                1,
            ));
        };
        if anchors.left {
            mark(Point::new(r.x, cy), Point::new(area.x, cy));
        }
        if anchors.right {
            mark(Point::new(r.right(), cy), Point::new(area.right(), cy));
        }
        if anchors.top {
            mark(Point::new(cx, r.y), Point::new(cx, area.y));
        }
        if anchors.bottom {
            mark(Point::new(cx, r.bottom()), Point::new(cx, area.bottom()));
        }
    }
    out
}
