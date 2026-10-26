//! Messages window: findings of form validation and project analysis, of
//! the last code generation and of the last build. Selecting a message
//! selects the element concerned or opens the source line.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use rncl::prelude::*;
use tdr_core::codegen::Generation;
use tdr_core::preview::BuildReport;
use tdr_core::{ComponentPath, Severity};

use super::{Ide, with};

/// Where a message leads.
#[derive(Clone)]
enum Target {
    Element(ComponentPath),
    Source(PathBuf, usize),
}

/// A message line with its target.
type Line = (String, Option<Target>);

pub struct MessagesWindow {
    pub window: RnclWindow,
    list: RnclListBox,
    targets: RefCell<Vec<Option<Target>>>,
    /// Messages of the last code generation and build (kept until the next).
    codegen: RefCell<Vec<Line>>,
    build: RefCell<Vec<Line>>,
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
            codegen: RefCell::new(Vec::new()),
            build: RefCell::new(Vec::new()),
        }
    }

    /// Messages of a code generation (validation findings are left out –
    /// they are shown from the model already).
    pub fn show_codegen(&self, generation: &Generation) {
        use tdr_core::codegen::CodegenIssue;
        *self.codegen.borrow_mut() = generation
            .diagnostics
            .iter()
            .filter(|d| !matches!(d.issue, CodegenIssue::Form(_) | CodegenIssue::Project(_)))
            .map(|d| {
                let at = d
                    .element
                    .as_ref()
                    .map(|p| format!("{p}: "))
                    .unwrap_or_default();
                (
                    format!("Code – {}: {at}{}", level(d.severity), d.issue),
                    d.element.clone().map(Target::Element),
                )
            })
            .collect();
    }

    /// Compiler messages of a build; `dir` makes file names absolute.
    pub fn show_build(&self, report: &BuildReport, dir: Option<&Path>) {
        let mut lines: Vec<Line> = report
            .messages
            .iter()
            .map(|m| {
                let at = match (&m.file, m.line) {
                    (Some(file), Some(line)) => format!("{file}:{line}: "),
                    _ => String::new(),
                };
                let code = m
                    .code
                    .as_ref()
                    .map(|c| format!("[{c}] "))
                    .unwrap_or_default();
                let target = match (&m.element, &m.file, m.line, dir) {
                    // Fehler im erzeugten Code: zur Komponente im Designer.
                    (Some(element), _, _, _) => Some(Target::Element(element.clone())),
                    (None, Some(file), Some(line), Some(dir)) => {
                        Some(Target::Source(dir.join(file), line))
                    }
                    _ => None,
                };
                (
                    format!("Build – {}: {at}{code}{}", level(m.severity), m.message),
                    target,
                )
            })
            .collect();
        if lines.is_empty() {
            lines.push((
                if report.success {
                    "Build erfolgreich.".to_owned()
                } else {
                    "Build fehlgeschlagen.".to_owned()
                },
                None,
            ));
        }
        *self.build.borrow_mut() = lines;
    }

    pub fn refresh(&self, ide: &Ide) {
        let findings = ide.session().document().findings();
        let mut lines: Vec<Line> = Vec::new();
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
                lines.push((
                    format!("{}: {at}{}", level(d.severity), d.issue),
                    element.map(Target::Element),
                ));
            }
        }
        for d in &findings.project {
            let at = d
                .element
                .as_ref()
                .map(|p| format!("{p}: "))
                .unwrap_or_default();
            lines.push((
                format!("{}: {at}{}", level(d.severity), d.issue),
                d.element.clone().map(Target::Element),
            ));
        }
        lines.extend(self.codegen.borrow().iter().cloned());
        lines.extend(self.build.borrow().iter().cloned());
        let count = lines.len();
        if lines.is_empty() {
            lines.push(("Keine Meldungen.".to_owned(), None));
        }
        let (texts, targets): (Vec<String>, Vec<Option<Target>>) = lines.into_iter().unzip();
        self.list.set_items(&texts);
        *self.targets.borrow_mut() = targets;
        self.window.set_title(&format!("Meldungen ({count})"));
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
            match target {
                Some(Target::Element(path)) => {
                    let Some(form) = path.form.clone() else {
                        return;
                    };
                    if ide.session().form_name() != Some(&form) {
                        ide.select_form(&form);
                    }
                    ide.select(vec![path.name]);
                }
                Some(Target::Source(file, line)) => ide.open_source(&file, line),
                None => {}
            }
        }));
}
