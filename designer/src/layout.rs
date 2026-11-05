//! Layout of a form on the design surface: where every widget sits.
//!
//! The designer draws widgets itself, so it computes their rectangles from
//! the model (container layouts, placements, property values) and the
//! [`Theme`] tables. Text widths are estimated; the result is stable and
//! good enough for designing – the real look comes from the debug run.
//!
//! Non-visual components (pools, timers …) have no position in the model;
//! they appear as icons in a separate component bar ([`tray`]); a data
//! module shows them on its surface.

use serde::Serialize;
use tdr_core::{ComponentName, Form, LayoutKind, Placement, TypeRegistry, Value, Widget, builtin};

use crate::geometry::{Point, Rect};
use crate::theme::Theme;

/// What a laid-out item is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    /// The form's root widget: the window, including its title bar.
    Window,
    Widget,
    /// A non-visual component in the tray.
    Component,
}

/// One element and its rectangle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LayoutItem {
    pub name: ComponentName,
    pub kind: ItemKind,
    pub rect: Rect,
    /// Nesting depth (window = 0); tray components have depth 0.
    pub depth: usize,
    pub parent: Option<ComponentName>,
    /// For containers: the area children are laid out in.
    pub content: Option<Rect>,
}

/// The layout of one form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FormLayout {
    /// Size of the whole design surface.
    pub width: i32,
    pub height: i32,
    /// Widgets depth-first (parents before children), then tray components.
    pub items: Vec<LayoutItem>,
}

impl FormLayout {
    pub fn item(&self, name: &ComponentName) -> Option<&LayoutItem> {
        self.items.iter().find(|i| &i.name == name)
    }

    pub fn rect(&self, name: &ComponentName) -> Option<Rect> {
        self.item(name).map(|i| i.rect)
    }
}

/// Reads property values of a widget, falling back to the type's default.
pub(crate) struct Props<'a> {
    pub form: &'a Form,
    pub registry: &'a TypeRegistry,
}

impl Props<'_> {
    pub fn value(&self, widget: &Widget, property: &str) -> Option<Value> {
        self.component_value(&widget.component, property)
    }

    /// A property of any element (widgets and non-visual components).
    pub fn component_value(
        &self,
        component: &tdr_core::Component,
        property: &str,
    ) -> Option<Value> {
        component
            .get(property)
            .map(|v| v.value.clone())
            .or_else(|| {
                self.registry
                    .property_def(&component.type_ref, &self.form.backend, property)?
                    .default
                    .clone()
            })
    }

    pub fn text(&self, widget: &Widget, property: &str) -> Option<String> {
        match self.value(widget, property)? {
            Value::Text(t) => Some(t),
            _ => None,
        }
    }

    pub fn float(&self, widget: &Widget, property: &str, default: f64) -> f64 {
        match self.value(widget, property) {
            Some(Value::Float(f)) => f,
            Some(Value::Int(i)) => i as f64,
            _ => default,
        }
    }

    pub fn int(&self, widget: &Widget, property: &str, default: i64) -> i64 {
        match self.value(widget, property) {
            Some(Value::Int(i)) => i,
            _ => default,
        }
    }

    /// A list of texts (other items are skipped).
    pub fn texts(&self, widget: &Widget, property: &str) -> Vec<String> {
        texts(self.value(widget, property))
    }

    /// The entries of a list widget: its `items`, or – when it is filled
    /// from a data source – one placeholder `⟨source.column⟩`.
    pub fn list_items(&self, widget: &Widget) -> Vec<String> {
        use tdr_core::data::{DATA_SOURCE, DISPLAY_FIELD};
        if let Some(Value::Reference(Some(source))) = self.value(widget, DATA_SOURCE) {
            let display = self.text(widget, DISPLAY_FIELD).unwrap_or_default();
            let display = if display.is_empty() {
                "?"
            } else {
                display.as_str()
            };
            return vec![format!("⟨{source}.{display}⟩")];
        }
        self.texts(widget, "items")
    }

    /// How the type of `widget` is drawn (`look` of its type).
    pub fn look(&self, widget: &Widget) -> Option<&str> {
        self.registry
            .get(&widget.component.type_ref)?
            .look
            .as_deref()
    }

    /// The top-level entries of the form's main menu (a component with
    /// `look = "menubar"` and an `items` list like `Datei/Öffnen`), `None`
    /// without main menu.
    pub fn menu_bar(&self) -> Option<Vec<String>> {
        let menu = self.form.components.iter().find(|c| {
            self.registry
                .get(&c.type_ref)
                .is_some_and(|t| t.look.as_deref() == Some("menubar"))
        })?;
        let mut titles: Vec<String> = Vec::new();
        for item in texts(self.component_value(menu, "items")) {
            let path = item.split('|').next().unwrap_or("").trim();
            let top = path.split('/').next().unwrap_or("").trim();
            if !top.is_empty() && !titles.iter().any(|t| t == top) {
                titles.push(top.to_owned());
            }
        }
        Some(titles)
    }

    pub fn bool(&self, widget: &Widget, property: &str, default: bool) -> bool {
        match self.value(widget, property) {
            Some(Value::Bool(b)) => b,
            _ => default,
        }
    }

    pub fn is_container(&self, widget: &Widget) -> bool {
        !widget.children.is_empty()
            || self
                .registry
                .get(&widget.component.type_ref)
                .is_some_and(|t| t.kind.is_container())
    }
}

/// The text that decides a widget's natural width.
fn texts(value: Option<Value>) -> Vec<String> {
    match value {
        Some(Value::List(items)) => items
            .into_iter()
            .filter_map(|v| match v {
                Value::Text(t) => Some(t),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// The appearance of an element: its own settings, else those of the form
/// (fonts and text colour are passed on like in CSS), else the theme.
pub(crate) struct Appearance {
    pub background: Option<tdr_core::Color>,
    pub foreground: Option<tdr_core::Color>,
    pub font_size: i32,
    pub bold: bool,
    pub italic: bool,
    pub family: Option<String>,
}

pub(crate) fn appearance(props: &Props<'_>, widget: &Widget, theme: &Theme) -> Appearance {
    let color = |w: &Widget, name: &str| match props.value(w, name) {
        Some(Value::Color(c)) if tdr_core::builtin::is_set(&c) => Some(c),
        _ => None,
    };
    let size = |w: &Widget| match props.value(w, "font_size") {
        Some(Value::Int(s)) if s > 0 => Some(s as i32),
        _ => None,
    };
    let root = props
        .form
        .root
        .as_ref()
        .filter(|r| !std::ptr::eq(*r, widget));
    let family = props
        .form
        .root
        .as_ref()
        .and_then(|r| props.text(r, "font_family"))
        .filter(|f| !f.trim().is_empty());
    Appearance {
        background: color(widget, "background"),
        foreground: color(widget, "foreground")
            .or_else(|| root.and_then(|r| color(r, "foreground"))),
        font_size: size(widget)
            .or_else(|| root.and_then(size))
            .unwrap_or(theme.font_size),
        bold: props.bool(widget, "bold", false)
            || root.is_some_and(|r| props.bool(r, "bold", false)),
        italic: props.bool(widget, "italic", false)
            || root.is_some_and(|r| props.bool(r, "italic", false)),
        family,
    }
}

pub(crate) fn display_text(props: &Props<'_>, widget: &Widget) -> Option<String> {
    let type_ref = &widget.component.type_ref;
    if *type_ref == builtin::label() {
        props.text(widget, "text")
    } else if *type_ref == builtin::button() || *type_ref == builtin::check_box() {
        props.text(widget, "caption")
    } else if *type_ref == builtin::edit() {
        None
    } else {
        props
            .text(widget, "caption")
            .or_else(|| props.text(widget, "text"))
    }
}

struct Layouter<'a> {
    props: Props<'a>,
    theme: &'a Theme,
    items: Vec<LayoutItem>,
}

impl Layouter<'_> {
    /// Layout kind, spacing and padding (left, top, right, bottom).
    fn params(&self, widget: &Widget) -> (LayoutKind, i32, [i32; 4]) {
        // Reiter: die Seiten liegen übereinander und füllen den Inhalt.
        if self.props.look(widget) == Some("tabs") {
            return (LayoutKind::Stack, 0, [0; 4]);
        }
        match &widget.visual.layout {
            Some(l) => (
                l.kind.clone(),
                l.spacing,
                [
                    l.padding.left,
                    l.padding.top,
                    l.padding.right,
                    l.padding.bottom,
                ],
            ),
            None => (
                LayoutKind::Vertical,
                self.theme.spacing,
                [self.theme.padding; 4],
            ),
        }
    }

    /// Natural size, replaced per axis by a requested size (`Placement::Layout::size`).
    fn natural(&self, widget: &Widget) -> (i32, i32) {
        let (w, h) = self.content_size(widget);
        match widget.visual.placement {
            Placement::Layout { size, .. } => (
                if size.width > 0 { size.width } else { w },
                if size.height > 0 { size.height } else { h },
            ),
            Placement::Absolute { .. } => (w, h),
        }
    }

    /// Size from content and theme metrics.
    fn content_size(&self, widget: &Widget) -> (i32, i32) {
        if widget.component.type_ref == builtin::form() {
            let bars = self.theme.title_bar_height + self.menu_bar_height();
            return match self.props.value(widget, "size") {
                Some(Value::Size(s)) => (s.width, s.height + bars),
                _ => (640, 480 + bars),
            };
        }
        let metrics = self.theme.metrics(&widget.component.type_ref);
        if !self.props.is_container(widget) {
            let scale = appearance(&self.props, widget, self.theme).font_size;
            let text = display_text(&self.props, widget)
                .map(|t| self.theme.text_width(&t) * scale / self.theme.font_size.max(1))
                .unwrap_or_else(|| {
                    if self
                        .props
                        .registry
                        .get(&widget.component.type_ref)
                        .is_none()
                    {
                        self.theme.text_width(&widget.component.type_ref.name)
                    } else {
                        0
                    }
                });
            let check = if widget.component.type_ref == builtin::check_box() {
                self.theme.check_size + self.theme.spacing
            } else {
                0
            };
            let width = (text + 2 * metrics.padding_x + check).max(metrics.min_width);
            // Größere Schrift: die Zeile wächst mit.
            let height = metrics.height + (scale - self.theme.font_size).max(0);
            return (width, height);
        }

        let (kind, spacing, [left, top, right, bottom]) = self.params(widget);
        let flow: Vec<(i32, i32)> = widget
            .children
            .iter()
            .filter(|c| !c.visual.placement.is_absolute())
            .map(|c| self.natural(c))
            .collect();
        let gaps = spacing * (flow.len().saturating_sub(1) as i32);
        let (w, h) = match kind {
            LayoutKind::Horizontal => (
                flow.iter().map(|s| s.0).sum::<i32>() + gaps,
                flow.iter().map(|s| s.1).max().unwrap_or(0),
            ),
            LayoutKind::Grid { columns } => {
                let columns = columns.max(1) as usize;
                let rows = flow.len().div_ceil(columns);
                let width = (0..columns)
                    .map(|c| {
                        flow.iter()
                            .skip(c)
                            .step_by(columns)
                            .map(|s| s.0)
                            .max()
                            .unwrap_or(0)
                    })
                    .sum::<i32>()
                    + spacing * (columns.min(flow.len()).saturating_sub(1) as i32);
                let height = (0..rows)
                    .map(|r| {
                        flow.iter()
                            .skip(r * columns)
                            .take(columns)
                            .map(|s| s.1)
                            .max()
                            .unwrap_or(0)
                    })
                    .sum::<i32>()
                    + spacing * (rows.saturating_sub(1) as i32);
                (width, height)
            }
            LayoutKind::Stack => (
                flow.iter().map(|s| s.0).max().unwrap_or(0),
                flow.iter().map(|s| s.1).max().unwrap_or(0),
            ),
            LayoutKind::Absolute => widget
                .children
                .iter()
                .filter_map(|c| match c.visual.placement {
                    Placement::Absolute { position, size, .. } => {
                        Some((position.x + size.width, position.y + size.height))
                    }
                    _ => None,
                })
                .fold((0, 0), |a, b| (a.0.max(b.0), a.1.max(b.1))),
            LayoutKind::Vertical | LayoutKind::Custom(_) => (
                flow.iter().map(|s| s.0).max().unwrap_or(0),
                flow.iter().map(|s| s.1).sum::<i32>() + gaps,
            ),
        };
        (
            (w + left + right).max(metrics.min_width),
            (h + top + bottom + metrics.header).max(metrics.height),
        )
    }

    /// [`Widget::design_area`] as the designer draws it: a header (frame
    /// title) takes its part of a known height.
    fn design_area(&self, widget: &Widget) -> tdr_core::Size {
        let mut area = widget.design_area();
        let known_height = match widget.visual.placement {
            Placement::Absolute { .. } => true,
            Placement::Layout { size, .. } => size.height > 0,
        };
        let is_form = widget.component.type_ref == builtin::form();
        if !is_form && known_height {
            area.height -= self.theme.metrics(&widget.component.type_ref).header;
        }
        // Ohne eigenes Layout zeichnet der Designer den Rand des Themes.
        if widget.visual.layout.is_none() {
            let known_width = is_form
                || match widget.visual.placement {
                    Placement::Absolute { .. } => true,
                    Placement::Layout { size, .. } => size.width > 0,
                };
            let extra = 2 * (tdr_core::ContainerLayout::STANDARD.padding.left - self.theme.padding);
            if known_width {
                area.width += extra;
            }
            if known_height || is_form {
                area.height += extra;
            }
        }
        area
    }

    /// Height of the form's menu bar (0 without main menu).
    fn menu_bar_height(&self) -> i32 {
        if self.props.menu_bar().is_some() {
            self.theme.menu_bar_height
        } else {
            0
        }
    }

    /// Places `natural` inside `cell` by `align` on one axis.
    fn align(align: tdr_core::Align, start: i32, space: i32, natural: i32) -> (i32, i32) {
        let size = natural.min(space);
        match align {
            tdr_core::Align::Fill => (start, space),
            tdr_core::Align::Start => (start, size),
            tdr_core::Align::Center => (start + (space - size) / 2, size),
            tdr_core::Align::End => (start + space - size, size),
        }
    }

    fn arrange(
        &mut self,
        widget: &Widget,
        rect: Rect,
        depth: usize,
        parent: Option<&ComponentName>,
    ) {
        let is_window = depth == 0 && widget.component.type_ref == builtin::form();
        let container = self.props.is_container(widget);
        let (kind, spacing, [left, top, right, bottom]) = self.params(widget);
        let client = if is_window {
            rect.inset(
                0,
                self.theme.title_bar_height + self.menu_bar_height(),
                0,
                0,
            )
        } else {
            rect.inset(
                0,
                self.theme.metrics(&widget.component.type_ref).header,
                0,
                0,
            )
        };
        let content = client.inset(left, top, right, bottom);
        self.items.push(LayoutItem {
            name: widget.component.name.clone(),
            kind: if is_window {
                ItemKind::Window
            } else {
                ItemKind::Widget
            },
            rect,
            depth,
            parent: parent.cloned(),
            content: container.then_some(content),
        });
        if !container {
            return;
        }

        let name = widget.component.name.clone();
        let mut flow: Vec<&Widget> = widget
            .children
            .iter()
            .filter(|c| !c.visual.placement.is_absolute())
            .collect();
        // Reiter: nur die gewählte Seite (`page`) wird angeordnet und gezeichnet.
        let tabs = self.props.look(widget) == Some("tabs");
        if tabs && !flow.is_empty() {
            let page = self
                .props
                .int(widget, "page", 0)
                .clamp(0, flow.len() as i64 - 1) as usize;
            flow = vec![flow[page]];
        }
        // Prozent: Anteil am Inhalt dieses Containers, geht der Wunschgröße vor.
        let percent_of = |w: &Widget| match w.visual.placement {
            Placement::Layout { percent, .. } => (percent.width, percent.height),
            Placement::Absolute { .. } => (0, 0),
        };
        let naturals: Vec<(i32, i32)> = flow
            .iter()
            .map(|c| {
                let (w, h) = self.natural(c);
                let (pw, ph) = percent_of(c);
                (
                    if pw > 0 { content.width * pw / 100 } else { w },
                    if ph > 0 { content.height * ph / 100 } else { h },
                )
            })
            .collect();
        let mut placed: Vec<(&Widget, Rect)> = Vec::new();
        let horizontal_flow = matches!(kind, LayoutKind::Horizontal);
        // Auf einer Achse in Prozent gelten Dehnen und Fill nicht.
        let stretch_of = |w: &Widget| match w.visual.placement {
            Placement::Layout { stretch, .. } => {
                let (pw, ph) = percent_of(w);
                let main = if horizontal_flow { pw } else { ph };
                if main > 0 { 0 } else { stretch as i32 }
            }
            Placement::Absolute { .. } => 0,
        };
        let align_of = |w: &Widget| match w.visual.placement {
            Placement::Layout { align, .. } => {
                let (pw, ph) = percent_of(w);
                if align == tdr_core::Align::Fill && (pw > 0 || ph > 0) {
                    tdr_core::Align::Start
                } else {
                    align
                }
            }
            Placement::Absolute { .. } => tdr_core::Align::Start,
        };
        let gaps = spacing * (flow.len().saturating_sub(1) as i32);

        match kind {
            LayoutKind::Vertical | LayoutKind::Custom(_) | LayoutKind::Horizontal => {
                let vertical = !matches!(kind, LayoutKind::Horizontal);
                let main = |s: (i32, i32)| if vertical { s.1 } else { s.0 };
                let total: i32 = naturals.iter().map(|s| main(*s)).sum::<i32>() + gaps;
                let space = if vertical {
                    content.height
                } else {
                    content.width
                };
                let extra = (space - total).max(0);
                let weights: i32 = flow.iter().map(|w| stretch_of(w)).sum();
                let mut cursor = if vertical { content.y } else { content.x };
                let mut given = 0;
                let stretched = flow.iter().filter(|w| stretch_of(w) > 0).count();
                let mut seen = 0;
                for (child, natural) in flow.iter().zip(&naturals) {
                    let mut size = main(*natural);
                    let weight = stretch_of(child);
                    if weights > 0 && weight > 0 {
                        seen += 1;
                        // The last stretched child takes the rounding rest.
                        let share = if seen == stretched {
                            extra - given
                        } else {
                            extra * weight / weights
                        };
                        given += share;
                        size += share;
                    }
                    let rect = if vertical {
                        let (x, w) =
                            Self::align(align_of(child), content.x, content.width, natural.0);
                        Rect::new(x, cursor, w, size)
                    } else {
                        let (y, h) =
                            Self::align(align_of(child), content.y, content.height, natural.1);
                        Rect::new(cursor, y, size, h)
                    };
                    placed.push((child, rect));
                    cursor += size + spacing;
                }
            }
            LayoutKind::Grid { columns } => {
                let columns = columns.max(1) as usize;
                // Cell of every child: explicit, else in order.
                let cells: Vec<(usize, usize)> = flow
                    .iter()
                    .enumerate()
                    .map(|(i, w)| match &w.visual.placement {
                        Placement::Layout {
                            cell: Some(cell), ..
                        } => (cell.row as usize, (cell.column as usize).min(columns - 1)),
                        _ => (i / columns, i % columns),
                    })
                    .collect();
                let rows = cells.iter().map(|c| c.0 + 1).max().unwrap_or(0);
                let mut widths = vec![0; columns];
                let mut heights = vec![0; rows];
                for ((row, column), natural) in cells.iter().zip(&naturals) {
                    widths[*column] = widths[*column].max(natural.0);
                    heights[*row] = heights[*row].max(natural.1);
                }
                // Columns share the remaining width evenly.
                let used: i32 = widths.iter().sum::<i32>() + spacing * (columns as i32 - 1);
                let extra = (content.width - used).max(0);
                for (i, w) in widths.iter_mut().enumerate() {
                    *w += extra / columns as i32 + i32::from(i < (extra % columns as i32) as usize);
                }
                let xs: Vec<i32> = widths
                    .iter()
                    .scan(content.x, |x, w| {
                        let start = *x;
                        *x += w + spacing;
                        Some(start)
                    })
                    .collect();
                let ys: Vec<i32> = heights
                    .iter()
                    .scan(content.y, |y, h| {
                        let start = *y;
                        *y += h + spacing;
                        Some(start)
                    })
                    .collect();
                for ((child, (row, column)), natural) in flow.iter().zip(&cells).zip(&naturals) {
                    let align = align_of(child);
                    let (x, w) = Self::align(align, xs[*column], widths[*column], natural.0);
                    let (y, h) = Self::align(align, ys[*row], heights[*row], natural.1);
                    placed.push((child, Rect::new(x, y, w, h)));
                }
            }
            LayoutKind::Stack => {
                for (child, natural) in flow.iter().zip(&naturals) {
                    let align = if tabs {
                        tdr_core::Align::Fill
                    } else {
                        align_of(child)
                    };
                    let (x, w) = Self::align(align, content.x, content.width, natural.0);
                    let (y, h) = Self::align(align, content.y, content.height, natural.1);
                    placed.push((child, Rect::new(x, y, w, h)));
                }
            }
            LayoutKind::Absolute => {
                for (child, natural) in flow.iter().zip(&naturals) {
                    placed.push((child, Rect::new(content.x, content.y, natural.0, natural.1)));
                }
            }
        }
        // Absolut: verankert an den Kanten der Fläche, für die sie entworfen wurden.
        let design = self.design_area(widget);
        let actual = tdr_core::Size {
            width: content.width,
            height: content.height,
        };
        for child in widget.children.iter() {
            if let Placement::Absolute {
                position,
                size,
                anchors,
            } = child.visual.placement
            {
                let (position, size) = anchors.apply(position, size, design, actual);
                placed.push((
                    child,
                    Rect::new(
                        content.x + position.x,
                        content.y + position.y,
                        size.width,
                        size.height,
                    ),
                ));
            }
        }
        // Keep the model's child order (depth-first, as in the tree).
        for child in &widget.children {
            if let Some((_, rect)) = placed.iter().find(|(c, _)| std::ptr::eq(*c, child)) {
                self.arrange(child, *rect, depth + 1, Some(&name));
            }
        }
    }
}

/// Lays out `form` on the design surface.
pub fn layout(form: &Form, registry: &TypeRegistry, theme: &Theme) -> FormLayout {
    let mut layouter = Layouter {
        props: Props { form, registry },
        theme,
        items: Vec::new(),
    };
    let margin = theme.margin;
    let mut width = 400;
    let mut bottom = margin;
    if let Some(root) = &form.root {
        let (w, h) = layouter.natural(root);
        layouter.arrange(root, Rect::new(margin, margin, w, h), 0, None);
        width = width.max(w + 2 * margin);
        bottom = margin + h + margin;
    }

    // Ein Datenmodul (ohne Fenster) zeigt seine Komponenten auf der Fläche;
    // bei Formularen stehen sie in einer eigenen Leiste (`tray`).
    if form.root.is_some() {
        return FormLayout {
            width,
            height: bottom,
            items: layouter.items,
        };
    }
    let items = tray_items(form, theme, margin, bottom, Some(width - margin));
    let height = items
        .iter()
        .map(|i| i.rect.bottom() + margin)
        .max()
        .unwrap_or(bottom);
    layouter.items.extend(items);
    FormLayout {
        width,
        height,
        items: layouter.items,
    }
}

/// The non-visual components of `form` as icons in one row from `(x, y)`,
/// wrapping before `wrap` if given.
fn tray_items(form: &Form, theme: &Theme, x0: i32, y0: i32, wrap: Option<i32>) -> Vec<LayoutItem> {
    let tray_item = |name: &str| (theme.text_width(name) + 16).max(theme.icon_size + 32);
    let (mut x, mut y) = (x0, y0);
    let mut items = Vec::new();
    for component in &form.components {
        let w = tray_item(component.name.as_str());
        if wrap.is_some_and(|wrap| x + w > wrap) && x > x0 {
            x = x0;
            y += theme.tray_height;
        }
        items.push(LayoutItem {
            name: component.name.clone(),
            kind: ItemKind::Component,
            rect: Rect::new(x, y, w, theme.tray_height),
            depth: 0,
            parent: None,
            content: None,
        });
        x += w + theme.spacing;
    }
    items
}

/// The component bar of a form (Delphi's non-visual components): its
/// non-visual components as icons in one row, in their own coordinates. A
/// data module has none (its components are on the surface, see [`layout`]).
pub fn tray(form: &Form, theme: &Theme) -> FormLayout {
    if form.root.is_none() {
        return FormLayout {
            width: 0,
            height: 0,
            items: Vec::new(),
        };
    }
    let items = tray_items(form, theme, theme.spacing, 0, None);
    FormLayout {
        width: items.last().map_or(0, |i| i.rect.right() + theme.spacing),
        height: theme.tray_height,
        items,
    }
}

/// Where a new or moved widget dropped at `point` would go: the deepest
/// container under the point and the child index (by position in its
/// layout). `None` outside of every container.
pub fn drop_target(
    form: &Form,
    registry: &TypeRegistry,
    layout: &FormLayout,
    point: Point,
) -> Option<(ComponentName, usize)> {
    let props = Props { form, registry };
    let container = layout.items.iter().rev().find(|i| {
        i.kind != ItemKind::Component
            && i.content.is_some_and(|c| c.contains(point))
            && form.widget(&i.name).is_some_and(|w| props.is_container(w))
    })?;
    let widget = form.widget(&container.name)?;
    let horizontal = matches!(
        widget.visual.layout.as_ref().map(|l| &l.kind),
        Some(LayoutKind::Horizontal)
    );
    let index = widget
        .children
        .iter()
        .filter_map(|c| layout.rect(&c.component.name))
        .filter(|r| {
            let center = r.center();
            if horizontal {
                center.x < point.x
            } else {
                center.y < point.y
            }
        })
        .count();
    Some((container.name.clone(), index))
}
