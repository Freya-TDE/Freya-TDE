//! Messages window: findings of form validation and project analysis.
//! Selecting a message selects the element concerned.

use std::cell::RefCell;
use std::rc::Rc;

use rncl::prelude::*;
use tdr_core::{ComponentName, ComponentPath, Severity};

use super::{Ide, with};

pub struct MessagesWindow {
    pub window: RnclWindow,
    list: RnclListBox,
    /// Element of each line (form, element).
    targets: RefCell<Vec<Option<ComponentPath>>>,
}

fn level(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "Fehler",
        Severity::Warning => "Warnung",
    }
}

impl MessagesWindow {
    pub fn build() -> Self {
        let window = RnclWindow::new("Meldungen");
        let list = RnclListBox::new(&window);
        Self {
            window,
            list,
            targets: RefCell::new(Vec::new()),
        }
    }

    pub fn refresh(&self, ide: &Ide) {
        let findings = ide.session().document().findings();
        let mut lines = Vec::new();
        let mut targets = Vec::new();
        for (form, diagnostics) in &findings.forms {
            for d in diagnostics {
                let element = d
                    .element
                    .clone()
                    .map(|e| ComponentPath::on(form.clone(), e));
                let at = element
                    .as_ref()
                    .map(|p| format!("{p}: "))
                    .unwrap_or_default();
                lines.push(format!("{}: {at}{}", level(d.severity), d.issue));
                targets.push(element);
            }
        }
        for d in &findings.project {
            let at = d
                .element
                .as_ref()
                .map(|p| format!("{p}: "))
                .unwrap_or_default();
            lines.push(format!("{}: {at}{}", level(d.severity), d.issue));
            targets.push(d.element.clone());
        }
        if lines.is_empty() {
            lines.push("Keine Meldungen.".to_owned());
            targets.push(None);
        }
        self.list.set_items(&lines);
        *self.targets.borrow_mut() = targets;
        self.window.set_title(&format!(
            "Meldungen ({})",
            if findings.is_empty() {
                0
            } else {
                self.targets.borrow().len()
            }
        ));
    }
}

pub fn connect(ide: &Rc<Ide>) {
    let messages = &ide.messages;
    let weak = Rc::downgrade(ide);
    messages.window.on_resized(move |w, h| {
        if let Some(ide) = weak.upgrade() {
            ide.messages.list.set_geometry(0, 0, w, h);
        }
    });
    messages
        .list
        .on_current_index_changed(with(ide, |ide, index: i32| {
            if ide.is_refreshing() {
                return;
            }
            let target = usize::try_from(index)
                .ok()
                .and_then(|i| ide.messages.targets.borrow().get(i).cloned())
                .flatten();
            let Some(path) = target else {
                return;
            };
            let Some(form) = path.form.clone() else {
                return;
            };
            let element: ComponentName = path.name;
            if ide.session().form_name() != Some(&form) {
                ide.select_form(&form);
            }
            ide.select(vec![element]);
        }));
}
