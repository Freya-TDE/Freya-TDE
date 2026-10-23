//! Structure window: forms and their elements as a tree (like Delphi's
//! object tree view). Selecting a node selects the element.

use std::rc::Rc;

use rncl::prelude::*;
use tdr_core::{ComponentName, Form, Widget};

use super::{Ide, with};

/// Node ids: `form` for a form, `form::element` for an element.
fn widget_item(form: &Form, widget: &Widget) -> TreeItem {
    let name = &widget.component.name;
    let mut item = TreeItem::new(
        &format!("{}::{name}", form.name),
        &format!("{name}: {}", widget.component.type_ref.name),
    );
    for child in &widget.children {
        item = item.with_child(widget_item(form, child));
    }
    item
}

fn tree(forms: &[Form]) -> Vec<TreeItem> {
    forms
        .iter()
        .map(|form| {
            let label = if form.is_data_module() {
                format!("{} (Datenmodul)", form.name)
            } else {
                form.name.to_string()
            };
            let mut item = TreeItem::new(form.name.as_str(), &label);
            if let Some(root) = &form.root {
                item = item.with_child(widget_item(form, root));
            }
            for component in &form.components {
                item = item.with_child(TreeItem::new(
                    &format!("{}::{}", form.name, component.name),
                    &format!("{}: {}", component.name, component.type_ref.name),
                ));
            }
            item
        })
        .collect()
}

/// Keeps the expansion of nodes that exist before and after a rebuild.
fn keep_expansion(new: &mut [TreeItem], old: &[TreeItem]) {
    fn find<'a>(items: &'a [TreeItem], id: &str) -> Option<&'a TreeItem> {
        items
            .iter()
            .find_map(|i| (i.id == id).then_some(i).or_else(|| find(&i.children, id)))
    }
    for item in new.iter_mut() {
        if let Some(previous) = find(old, &item.id) {
            item.expanded = previous.expanded;
        }
        keep_expansion(&mut item.children, old);
    }
}

pub struct StructureWindow {
    pub window: RnclWindow,
    tree: RnclTreeView,
}

impl StructureWindow {
    pub fn build() -> Self {
        let window = RnclWindow::new("Struktur");
        let tree = RnclTreeView::new(&window);
        Self { window, tree }
    }

    pub fn refresh(&self, ide: &Ide) {
        let session = ide.session();
        let mut items = tree(session.document().forms());
        keep_expansion(&mut items, &self.tree.items());
        if items != self.tree.items() {
            self.tree.set_items(items);
        }
        let selected = match (session.form_name(), session.selection().first()) {
            (Some(form), Some(element)) => Some(format!("{form}::{element}")),
            (Some(form), None) => Some(form.to_string()),
            _ => None,
        };
        self.tree.select(selected.as_deref());
    }
}

pub fn connect(ide: &Rc<Ide>) {
    let structure = &ide.structure;
    let weak = Rc::downgrade(ide);
    structure.window.on_resized(move |w, h| {
        if let Some(ide) = weak.upgrade() {
            ide.structure.tree.set_geometry(0, 0, w, h);
        }
    });
    structure
        .tree
        .on_selection_changed(with(ide, |ide, id: String| {
            if ide.is_refreshing() || id.is_empty() {
                return;
            }
            let (form, element) = match id.split_once("::") {
                Some((f, e)) => (f, Some(e)),
                None => (id.as_str(), None),
            };
            let Ok(form) = ComponentName::new(form) else {
                return;
            };
            if ide.session().form_name() != Some(&form) {
                ide.select_form(&form);
            }
            let selection = element
                .and_then(|e| ComponentName::new(e).ok())
                .into_iter()
                .collect();
            ide.select(selection);
        }));
    structure
        .tree
        .on_item_activated(with(ide, |ide, _: String| {
            ide.show_window(crate::desktop::window::DESIGNER);
        }));
}
