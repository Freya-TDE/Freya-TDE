//! Object inspector: properties and events of the selected element.
//!
//! The editor fits the property type – check box (bool), spin box (int),
//! drop-down (enum, reference) or text field (everything else, applied on
//! Enter or after a short typing pause). Platform overrides are kept when
//! the base value changes; ↺ resets a property to its default.

use std::cell::RefCell;
use std::rc::Rc;

use rncl::prelude::*;
use ruf::Timer;
use tdr_core::{
    Color, ComponentName, ComponentPath, HandlerName, Point, PropertyType, PropertyValue, Size,
    Value,
};
use tdr_designer::{Command, EventRow, PropertyRow, events, properties, suggest_handler};

use super::{Ide, with, with0};

const MARGIN: i32 = 6;
const ROW: i32 = 30;
const LABEL_SHARE: f32 = 0.42;
const RESET: i32 = 28;
/// Typing pause before a text field is applied.
const TYPING_PAUSE: i32 = 700;

/// One line of a page: label, editor, optional reset button.
struct Line {
    label: Widget,
    editor: Option<Widget>,
    reset: Option<Widget>,
}

/// Text → value of a property type; `Err` with a hint for the status line.
fn parse(text: &str, ty: &PropertyType) -> Result<Value, String> {
    let text = text.trim();
    let numbers = |s: &str| -> Option<(i32, i32)> {
        let mut parts = s.split([',', 'x', '×', ' ']).filter(|p| !p.is_empty());
        let a = parts.next()?.trim().parse().ok()?;
        let b = parts.next()?.trim().parse().ok()?;
        parts.next().is_none().then_some((a, b))
    };
    match ty {
        PropertyType::Text => Ok(Value::Text(text.to_owned())),
        PropertyType::Code => Ok(Value::Code(text.to_owned())),
        PropertyType::Float { .. } => text
            .replace(',', ".")
            .parse()
            .map(Value::Float)
            .map_err(|_| "eine Zahl erwartet, z. B. 1.5".to_owned()),
        PropertyType::Point => numbers(text)
            .map(|(x, y)| Value::Point(Point { x, y }))
            .ok_or_else(|| "einen Punkt erwartet, z. B. 10, 20".to_owned()),
        PropertyType::Size => numbers(text)
            .map(|(width, height)| Value::Size(Size { width, height }))
            .ok_or_else(|| "eine Größe erwartet, z. B. 640 × 480".to_owned()),
        PropertyType::Color => {
            let hex = text.strip_prefix('#').unwrap_or(text);
            let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
            match hex.len() {
                6 => byte(0)
                    .zip(byte(2))
                    .zip(byte(4))
                    .map(|((r, g), b)| Color::rgb(r, g, b)),
                8 => (|| Some(Color::rgba(byte(0)?, byte(2)?, byte(4)?, byte(6)?)))(),
                _ => None,
            }
            .map(Value::Color)
            .ok_or_else(|| "eine Farbe erwartet, z. B. #3366ff".to_owned())
        }
        PropertyType::List(inner) => {
            let items: Result<Vec<Value>, String> = text
                .split(';')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| parse(s, inner))
                .collect();
            items.map(Value::List)
        }
        PropertyType::Custom(name) => Ok(Value::Custom {
            type_name: name.clone(),
            data: text.to_owned(),
        }),
        PropertyType::Bool
        | PropertyType::Int { .. }
        | PropertyType::Enum(_)
        | PropertyType::Reference(_) => Err("wird über die Auswahl gesetzt".to_owned()),
    }
}

/// Value → text for a text field.
fn format(value: &Value) -> String {
    match value {
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(x) => x.to_string(),
        Value::Text(s) | Value::Enum(s) | Value::Code(s) => s.clone(),
        Value::Color(c) if c.a == 255 => format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b),
        Value::Color(c) => format!("#{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a),
        Value::Point(p) => format!("{}, {}", p.x, p.y),
        Value::Size(s) => format!("{} × {}", s.width, s.height),
        Value::List(items) => items.iter().map(format).collect::<Vec<_>>().join("; "),
        Value::Reference(Some(path)) => path.to_string(),
        Value::Reference(None) => String::new(),
        Value::Custom { data, .. } => data.clone(),
    }
}

pub struct InspectorWindow {
    pub window: RnclWindow,
    element: RnclComboBox,
    tabs: RnclTabView,
    props: RnclScrollView,
    events: RnclScrollView,
    note: RnclLabel,
    prop_lines: RefCell<Vec<Line>>,
    event_lines: RefCell<Vec<Line>>,
    /// Element the pages were built for.
    shown: RefCell<Option<ComponentPath>>,
    /// Elements listed in the selector, in order.
    listed: RefCell<Vec<ComponentName>>,
    timer: Timer,
    pending: RefCell<Option<Box<dyn Fn()>>>,
}

impl InspectorWindow {
    pub fn build() -> Self {
        let window = RnclWindow::new("Objektinspektor");
        let element = RnclComboBox::new(&window);
        let tabs = RnclTabView::new(&window);
        let props_page = tabs.add_tab("Eigenschaften");
        let events_page = tabs.add_tab("Ereignisse");
        let props = RnclScrollView::new(&props_page);
        let events = RnclScrollView::new(&events_page);
        let note = RnclLabel::new(&window, "");
        let timer = Timer::new();
        timer.set_single_shot(true);
        Self {
            window,
            element,
            tabs,
            props,
            events,
            note,
            prop_lines: RefCell::new(Vec::new()),
            event_lines: RefCell::new(Vec::new()),
            shown: RefCell::new(None),
            listed: RefCell::new(Vec::new()),
            timer,
            pending: RefCell::new(None),
        }
    }

    pub fn shown(&self) -> Option<ComponentPath> {
        self.shown.borrow().clone()
    }

    fn layout(&self, width: i32, height: i32) {
        self.element
            .set_geometry(MARGIN, MARGIN, width - 2 * MARGIN, ROW);
        let top = MARGIN + ROW + MARGIN;
        self.tabs
            .set_geometry(MARGIN, top, width - 2 * MARGIN, height - top - MARGIN);
        self.note
            .set_geometry(MARGIN, top + 40, width - 2 * MARGIN, 60);
        self.layout_pages();
    }

    fn layout_pages(&self) {
        let size = self.tabs.size();
        // Platz für die Reiterleiste.
        let (w, h) = (size.width - 4, size.height - 40);
        for (scroll, lines) in [
            (&self.props, &self.prop_lines),
            (&self.events, &self.event_lines),
        ] {
            scroll.set_geometry(0, 0, w.max(0), h.max(0));
            let inner = (w - 18).max(120);
            let label_w = (inner as f32 * LABEL_SHARE) as i32;
            let mut y = 4;
            for line in lines.borrow().iter() {
                match &line.editor {
                    None => line
                        .label
                        .set_geometry(Rect::new(4, y + 4, inner - 8, ROW - 6)),
                    Some(editor) => {
                        line.label.set_geometry(Rect::new(8, y, label_w - 8, ROW));
                        let reset_w = if line.reset.is_some() { RESET + 2 } else { 0 };
                        editor.set_geometry(Rect::new(
                            label_w,
                            y,
                            inner - label_w - 4 - reset_w,
                            ROW - 2,
                        ));
                        if let Some(reset) = &line.reset {
                            reset.set_geometry(Rect::new(inner - RESET - 2, y, RESET, ROW - 2));
                        }
                    }
                }
                y += ROW + 2;
            }
            scroll.set_content_size(0, y + 4);
        }
    }

    fn clear(&self) {
        for lines in [&self.prop_lines, &self.event_lines] {
            for line in lines.borrow_mut().drain(..) {
                line.label.delete_later();
                if let Some(e) = line.editor {
                    e.delete_later();
                }
                if let Some(r) = line.reset {
                    r.delete_later();
                }
            }
        }
    }

    /// Runs a pending text edit now.
    fn flush(&self) {
        self.timer.stop();
        if let Some(apply) = self.pending.borrow_mut().take() {
            apply();
        }
    }

    pub fn refresh(&self, ide: &Rc<Ide>, model_changed: bool) {
        let session = ide.session();
        // Auswahlliste der Elemente des Formulars.
        let (names, labels): (Vec<ComponentName>, Vec<String>) = session
            .form()
            .map(|f| {
                f.elements()
                    .iter()
                    .map(|e| {
                        (
                            e.name().clone(),
                            format!("{}: {}", e.name(), e.type_ref().name),
                        )
                    })
                    .unzip()
            })
            .unwrap_or_default();
        if self.element.items() != labels {
            self.element.set_items(&labels);
        }
        let target = match (session.form_name(), session.selection()) {
            (Some(form), [element]) => Some(ComponentPath::on(form.clone(), element.clone())),
            _ => None,
        };
        let index = target
            .as_ref()
            .and_then(|t| names.iter().position(|n| n == &t.name))
            .map_or(-1, |i| i as i32);
        self.element.set_current_index(index);
        *self.listed.borrow_mut() = names;
        let note = match session.selection().len() {
            0 => "Kein Element ausgewählt.",
            1 => "",
            _ => "Mehrere Elemente ausgewählt.",
        };
        drop(session);

        if !model_changed && *self.shown.borrow() == target {
            return;
        }
        self.flush();
        self.clear();
        self.note.set_text(note);
        self.note.widget().set_visible(target.is_none());
        self.tabs.widget().set_visible(target.is_some());
        if let Some(target) = &target {
            self.build_pages(ide, target);
        }
        *self.shown.borrow_mut() = target;
        self.layout_pages();
    }

    fn build_pages(&self, ide: &Rc<Ide>, target: &ComponentPath) {
        let session = ide.session();
        let Some(form) = target
            .form
            .as_ref()
            .and_then(|f| session.document().form(f))
        else {
            return;
        };
        let registry = session.document().registry();
        let rows = properties(form, &target.name, registry).unwrap_or_default();
        let event_rows = events(form, &target.name, registry).unwrap_or_default();
        // Kandidaten für Verweise: alle Elemente des Projekts.
        let candidates: Vec<(ComponentPath, Option<tdr_core::TypeRef>)> = session
            .document()
            .forms()
            .iter()
            .flat_map(|f| {
                f.elements()
                    .into_iter()
                    .map(|e| {
                        (
                            ComponentPath::on(f.name.clone(), e.name().clone()),
                            Some(e.type_ref().clone()),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        let providers = |capability: &Option<tdr_core::Capability>| -> Vec<String> {
            candidates
                .iter()
                .filter(|(path, _)| path != target)
                .filter(|(_, ty)| {
                    capability.as_ref().is_none_or(|c| {
                        ty.as_ref()
                            .and_then(|t| registry.get(t))
                            .is_some_and(|t| t.provides(c))
                    })
                })
                .map(|(path, _)| {
                    if path.form == target.form {
                        path.name.to_string()
                    } else {
                        path.to_string()
                    }
                })
                .collect()
        };

        let mut lines = Vec::new();
        let mut category = None;
        let page = self.props.content();
        for row in &rows {
            if category.as_ref() != Some(&row.category) {
                category = Some(row.category.clone());
                let title = if row.category.is_empty() {
                    "Allgemein"
                } else {
                    row.category.as_str()
                };
                let header = RnclLabel::new(page, &format!("<b>{title}</b>"));
                lines.push(Line {
                    label: header.widget().clone(),
                    editor: None,
                    reset: None,
                });
            }
            let refs = match &row.ty {
                PropertyType::Reference(c) => providers(c),
                _ => Vec::new(),
            };
            lines.push(self.property_line(ide, page, target, row, refs));
        }
        drop(session);
        *self.prop_lines.borrow_mut() = lines;

        let page = self.events.content();
        let lines = event_rows
            .iter()
            .map(|row| self.event_line(ide, page, target, row))
            .collect();
        *self.event_lines.borrow_mut() = lines;
    }

    fn property_line(
        &self,
        ide: &Rc<Ide>,
        page: &RnclPanel,
        target: &ComponentPath,
        row: &PropertyRow,
        references: Vec<String>,
    ) -> Line {
        let marker = if row.value.is_some() { " •" } else { "" };
        let label = RnclLabel::new(page, &format!("{}{marker}", row.name));
        let mut tip = row.description.clone();
        if row.backend_specific {
            tip.push_str(" (nur für dieses UI-Backend)");
        }
        if !row.platforms.is_empty() {
            let platforms: Vec<String> = row.platforms.iter().map(ToString::to_string).collect();
            tip.push_str(&format!(" (nur {})", platforms.join(", ")));
        }
        label.widget().set_tool_tip(tip.trim());

        let effective = row.effective().cloned();
        let overrides = row
            .value
            .as_ref()
            .map(|v| v.platforms.clone())
            .unwrap_or_default();
        let (form, element, property) = (
            target.form.clone().expect("qualified"),
            target.name.clone(),
            row.name.clone(),
        );
        let command = move |value: Value| Command::SetProperty {
            form: form.clone(),
            element: element.clone(),
            property: property.clone(),
            value: Some(PropertyValue {
                value,
                platforms: overrides.clone(),
            }),
        };

        let editor: Widget = match &row.ty {
            PropertyType::Bool => {
                let check = RnclCheckBox::new(page, "");
                check.set_checked(matches!(effective, Some(Value::Bool(true))));
                check.on_toggled(with(ide, move |ide, on: bool| {
                    if !ide.is_refreshing() {
                        ide.apply_from_inspector(command(Value::Bool(on)));
                    }
                }));
                check.widget().clone()
            }
            PropertyType::Int { min, max } => {
                let spin = RnclSpinBox::new(page);
                let clamp = |v: i64| v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
                spin.set_range(min.map_or(-1_000_000, clamp), max.map_or(1_000_000, clamp));
                if let Some(Value::Int(v)) = effective {
                    spin.set_value(clamp(v));
                }
                spin.on_value_changed(with(ide, move |ide, v: i32| {
                    if !ide.is_refreshing() {
                        ide.apply_from_inspector(command(Value::Int(i64::from(v))));
                    }
                }));
                spin.widget().clone()
            }
            PropertyType::Enum(variants) => {
                let combo = RnclComboBox::new(page);
                combo.set_items(variants);
                if let Some(Value::Enum(v)) = &effective {
                    combo.set_current_text(v);
                }
                let variants = variants.clone();
                combo.on_current_index_changed(with(ide, move |ide, i: i32| {
                    if ide.is_refreshing() {
                        return;
                    }
                    if let Some(v) = usize::try_from(i).ok().and_then(|i| variants.get(i)) {
                        ide.apply_from_inspector(command(Value::Enum(v.clone())));
                    }
                }));
                combo.widget().clone()
            }
            PropertyType::Reference(_) => {
                let combo = RnclComboBox::new(page);
                let mut items = vec![String::new()];
                items.extend(references);
                combo.set_items(&items);
                combo.set_current_text(&effective.as_ref().map(format).unwrap_or_default());
                let items_c = items.clone();
                combo.on_current_index_changed(with(ide, move |ide, i: i32| {
                    if ide.is_refreshing() {
                        return;
                    }
                    let Some(text) = usize::try_from(i).ok().and_then(|i| items_c.get(i)) else {
                        return;
                    };
                    let target = text.parse::<ComponentPath>().ok();
                    ide.apply_from_inspector(command(Value::Reference(target)));
                }));
                combo.widget().clone()
            }
            ty => {
                let field = RnclTextField::new(page);
                field.set_text(&effective.as_ref().map(format).unwrap_or_default());
                if matches!(ty, PropertyType::List(_)) {
                    field.set_placeholder("Einträge mit ; trennen");
                }
                let ty = ty.clone();
                let f = field.clone();
                let command = Rc::new(command);
                let apply = {
                    let weak = Rc::downgrade(ide);
                    let command = command.clone();
                    move || {
                        let Some(ide) = weak.upgrade() else { return };
                        match parse(&f.text(), &ty) {
                            Ok(value) => ide.apply_from_inspector(command(value)),
                            Err(hint) => ide.main.set_status(&hint),
                        }
                    }
                };
                let apply = Rc::new(apply);
                let a = apply.clone();
                field.on_return_pressed(with0(ide, move |ide| {
                    ide.inspector.pending.borrow_mut().take();
                    ide.inspector.timer.stop();
                    a();
                }));
                field.on_text_changed(with(ide, move |ide, _: String| {
                    if ide.is_refreshing() {
                        return;
                    }
                    let a = apply.clone();
                    *ide.inspector.pending.borrow_mut() = Some(Box::new(move || a()));
                    ide.inspector.timer.start(TYPING_PAUSE);
                }));
                field.widget().clone()
            }
        };
        editor.set_tool_tip(tip.trim());

        let reset = row.value.is_some().then(|| {
            let button = RnclButton::new(page, "↺");
            button
                .widget()
                .set_tool_tip("Auf den Standardwert zurücksetzen");
            let (form, element, property) = (
                target.form.clone().expect("qualified"),
                target.name.clone(),
                row.name.clone(),
            );
            button.on_clicked(with0(ide, move |ide| {
                ide.apply(Command::SetProperty {
                    form: form.clone(),
                    element: element.clone(),
                    property: property.clone(),
                    value: None,
                });
            }));
            button.widget().clone()
        });
        Line {
            label: label.widget().clone(),
            editor: Some(editor),
            reset,
        }
    }

    fn event_line(
        &self,
        ide: &Rc<Ide>,
        page: &RnclPanel,
        target: &ComponentPath,
        row: &EventRow,
    ) -> Line {
        let label = RnclLabel::new(page, &row.name);
        let params: Vec<String> = row
            .params
            .iter()
            .map(|p| format!("{}: {}", p.name, p.rust_type))
            .collect();
        let returns = row
            .returns
            .as_ref()
            .map(|r| format!(" -> {r}"))
            .unwrap_or_default();
        let tip = format!("({}){returns} {}", params.join(", "), row.description);
        label.widget().set_tool_tip(tip.trim());
        let field = RnclTextField::new(page);
        field.set_text(row.handler.as_ref().map(HandlerName::as_str).unwrap_or(""));
        field.set_placeholder("Handler (… schlägt einen vor)");
        let (form, element, event) = (
            target.form.clone().expect("qualified"),
            target.name.clone(),
            row.name.clone(),
        );
        let set = Rc::new(move |ide: &Ide, text: &str| {
            let handler = match text.trim() {
                "" => None,
                t => match HandlerName::new(t) {
                    Ok(h) => Some(h),
                    Err(e) => {
                        ide.main.set_status(&e.to_string());
                        return;
                    }
                },
            };
            ide.apply_from_inspector(Command::SetEvent {
                form: form.clone(),
                element: element.clone(),
                event: event.clone(),
                handler,
            });
        });
        let (s, f) = (set.clone(), field.clone());
        field.on_return_pressed(with0(ide, move |ide| s(ide, &f.text())));
        let suggest = RnclButton::new(page, "…");
        suggest
            .widget()
            .set_tool_tip("Handler-Namen vorschlagen und eintragen");
        let (s, f, element, event) = (set, field.clone(), target.name.clone(), row.name.clone());
        suggest.on_clicked(with0(ide, move |ide| {
            if f.text().trim().is_empty() {
                let name = suggest_handler(&element, &event);
                f.set_text(name.as_str());
                s(ide, name.as_str());
            }
        }));
        Line {
            label: label.widget().clone(),
            editor: Some(field.widget().clone()),
            reset: Some(suggest.widget().clone()),
        }
    }
}

pub fn connect(ide: &Rc<Ide>) {
    let inspector = &ide.inspector;
    let weak = Rc::downgrade(ide);
    inspector.window.on_resized(move |w, h| {
        if let Some(ide) = weak.upgrade() {
            ide.inspector.layout(w, h);
        }
    });
    inspector.timer.on_timeout(with0(ide, |ide| {
        let pending = ide.inspector.pending.borrow_mut().take();
        if let Some(apply) = pending {
            apply();
        }
    }));
    inspector
        .element
        .on_current_index_changed(with(ide, |ide, i: i32| {
            if ide.is_refreshing() {
                return;
            }
            let name = usize::try_from(i)
                .ok()
                .and_then(|i| ide.inspector.listed.borrow().get(i).cloned());
            if let Some(name) = name {
                ide.select(vec![name]);
            }
        }));
}
