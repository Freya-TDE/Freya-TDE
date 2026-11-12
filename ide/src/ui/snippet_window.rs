//! Window "Snippet": name, short description, `use` line and code of a new
//! or existing snippet (see [`crate::snippets`]).

use std::cell::RefCell;
use std::rc::Rc;

use rncl::prelude::*;
use ruf::dialog;

use super::{Ide, with0};
use crate::snippets::Snippet;

const W: i32 = 560;
const H: i32 = 440;
const M: i32 = 16;
const ROW: i32 = 32;
const GAP: i32 = 8;
const LABEL_W: i32 = 130;

pub struct SnippetWindow {
    pub window: RnclWindow,
    name: RnclTextField,
    description: RnclTextField,
    use_line: RnclTextField,
    code: RnclTextArea,
    cancel: RnclButton,
    ok: RnclButton,
    labels: Vec<RnclLabel>,
    /// The snippet being edited (`None` = a new one).
    editing: RefCell<Option<String>>,
}

impl SnippetWindow {
    pub fn build() -> Self {
        let window = RnclWindow::new("Snippet");
        window.set_size(W, H);
        let labels = ["Name", "Kurzbeschreibung", "use-Zeile", "Code"]
            .map(|t| RnclLabel::new(&window, t))
            .to_vec();
        let name = RnclTextField::new(&window);
        let description = RnclTextField::new(&window);
        description.set_placeholder("erscheint beim Darüberfahren in der Liste");
        let use_line = RnclTextField::new(&window);
        use_line.set_placeholder("z. B. use std::fs;  (leer = keine)");
        let code = RnclTextArea::new(&window);
        code.set_monospace(true);
        let cancel = RnclButton::new(&window, "Abbrechen");
        let ok = RnclButton::new(&window, "OK");
        let w = Self {
            window,
            name,
            description,
            use_line,
            code,
            cancel,
            ok,
            labels,
            editing: RefCell::new(None),
        };
        w.place();
        w
    }

    fn place(&self) {
        use ruf::Anchors as A;
        let put = |w: &dyn Component, x, y, width, height, anchors| {
            w.set_geometry(x, y, width, height);
            w.set_anchors(anchors);
        };
        let fx = M + LABEL_W + GAP;
        let fw = W - fx - M;
        let line = |i: i32| M + i * (ROW + GAP);
        for (i, l) in self.labels.iter().enumerate() {
            put(l, M, line(i as i32), LABEL_W, ROW, A::TOP_LEFT);
        }
        put(&self.name, fx, line(0), fw, ROW, A::TOP_STRETCH);
        put(&self.description, fx, line(1), fw, ROW, A::TOP_STRETCH);
        put(&self.use_line, fx, line(2), fw, ROW, A::TOP_STRETCH);
        let bar = H - M - ROW;
        put(&self.code, fx, line(3), fw, bar - GAP - line(3), A::ALL);
        put(
            &self.cancel,
            W - M - 2 * 110 - GAP,
            bar,
            110,
            ROW,
            A::BOTTOM_RIGHT,
        );
        put(&self.ok, W - M - 110, bar, 110, ROW, A::BOTTOM_RIGHT);
    }

    /// The snippet as entered.
    pub fn snippet(&self) -> Snippet {
        Snippet::new(
            self.name.text().trim(),
            self.description.text().trim(),
            self.use_line.text().trim(),
            &self.code.text(),
        )
    }

    fn fill(&self, snippet: &Snippet) {
        self.name.set_text(&snippet.name);
        self.description.set_text(&snippet.description);
        self.use_line.set_text(&snippet.use_line);
        self.code.set_text(&snippet.code);
    }
}

impl Ide {
    /// Opens the snippet window for a new snippet (`None`) or to edit one.
    pub fn show_snippet_editor(&self, name: Option<&str>) {
        let w = &self.snippet_window;
        let snippet = name
            .and_then(|n| self.snippets.borrow().get(n).cloned())
            .unwrap_or_else(|| Snippet::new("", "", "", ""));
        *w.editing.borrow_mut() = name.map(str::to_owned);
        w.fill(&snippet);
        w.window.set_title(if name.is_some() {
            "Snippet bearbeiten"
        } else {
            "Neues Snippet"
        });
        w.window.show();
    }

    /// The snippet window (screenshots, tests).
    pub fn snippet_window(&self) -> &RnclWindow {
        &self.snippet_window.window
    }

    /// "OK" in the snippet window.
    fn snippet_ok(&self) {
        let w = &self.snippet_window;
        let old = w.editing.borrow().clone();
        match self.put_snippet(old.as_deref(), w.snippet()) {
            Ok(()) => w.window.hide(),
            Err(e) => dialog::error(
                Some(w.window.widget()),
                "Snippet",
                &format!("{} ({})", e.detail, e.code),
            ),
        }
    }
}

pub fn connect(ide: &Rc<Ide>) {
    let w = &ide.snippet_window;
    w.cancel
        .on_clicked(with0(ide, |ide| ide.snippet_window.window.hide()));
    w.ok.on_clicked(with0(ide, |ide| ide.snippet_ok()));
}
