//! Putting tool windows together as tabs (structure and object inspector):
//! each has a grip at the top ("⠿ Struktur"); dragging it onto the other
//! window makes both one window with tabs, dragging it out into the open
//! (no drop target) makes it its own window again. Kept in the window
//! arrangement (`desktop.toml`, `docked`).
//!
//! *Struktur und Objektinspektor per Ziehen als Reiter zusammenlegen und
//! wieder trennen.*

use std::rc::Rc;

use rncl::prelude::*;

use super::Ide;

/// Height of the grip.
pub const GRIP: i32 = 20;
/// Prefix of the drag text of a grip.
const DRAG: &str = "freya-view:";

/// The dockable content of a tool window.
pub struct DockPane {
    pub name: &'static str,
    pub title: &'static str,
    /// Everything the window shows (moves into a tab when docked).
    pub root: RnclPanel,
    /// Drag handle at the top of `root`.
    grip: RnclLabel,
    /// Tabs of the window while it hosts other panes (hidden otherwise).
    tabs: RnclTabView,
}

impl DockPane {
    pub fn new(window: &RnclWindow, name: &'static str, title: &'static str) -> Self {
        let tabs = RnclTabView::new(window);
        tabs.widget().set_visible(false);
        let root = RnclPanel::new(window);
        let grip = RnclLabel::new(&root, &format!("⠿ {title}"));
        grip.widget().set_tool_tip(
            "Ziehen: auf ein anderes Fenster ablegen legt beide als Reiter zusammen, ins Freie ziehen trennt wieder",
        );
        grip.widget().set_mouse_cursor("grab");
        grip.widget().set_drag_text(&format!("{DRAG}{name}"));
        Self {
            name,
            title,
            root,
            grip,
            tabs,
        }
    }

    /// Lays out the grip; returns the area below it for the content.
    pub fn layout_grip(&self, width: i32) -> i32 {
        self.grip.set_geometry(6, 0, (width - 12).max(10), GRIP);
        GRIP
    }

    fn tabbed(&self) -> bool {
        self.tabs.widget().is_visible()
    }
}

impl Ide {
    /// The dockable panes with their windows.
    fn panes(&self) -> [(&DockPane, &RnclWindow); 2] {
        [
            (&self.structure.pane, &self.structure.window),
            (&self.inspector.pane, &self.inspector.window),
        ]
    }

    fn pane(&self, name: &str) -> Option<(&DockPane, &RnclWindow)> {
        self.panes().into_iter().find(|(p, _)| p.name == name)
    }

    /// Which pane is docked into which window (`pane → host`).
    pub fn docked(&self) -> std::collections::BTreeMap<String, String> {
        self.docked.borrow().clone()
    }

    /// Docks pane `guest` as a tab into the window of pane `host`.
    pub fn dock_view(&self, guest: &str, host: &str) -> Result<(), String> {
        // Liegt das Ziel selbst in einem anderen Fenster, dorthin.
        let host = self
            .docked
            .borrow()
            .get(host)
            .cloned()
            .unwrap_or_else(|| host.to_owned());
        if guest == host || self.docked.borrow().get(guest) == Some(&host) {
            return Ok(());
        }
        let ((g, g_window), (h, h_window)) = match (self.pane(guest), self.pane(&host)) {
            (Some(g), Some(h)) => (g, h),
            _ => return Err(format!("unbekanntes Fenster: {guest} oder {host}")),
        };
        if self.docked.borrow().values().any(|v| v == guest) {
            return Err(format!("{} enthält schon Reiter", g.title));
        }
        if self.docked.borrow().contains_key(guest) {
            self.undock_view(guest);
        }
        let size = h_window.size();
        if !h.tabbed() {
            let page = h.tabs.add_tab(h.title);
            h.root.widget().set_parent(page.widget());
            h.tabs.set_geometry(0, 0, size.width, size.height);
            h.tabs.widget().set_visible(true);
            follow(&page, &h.root);
        }
        let page = h.tabs.add_tab(g.title);
        g.root.widget().set_parent(page.widget());
        follow(&page, &g.root);
        h.tabs.set_current_index(h.tabs.tab_count() - 1);
        g_window.hide();
        h_window.show();
        self.docked.borrow_mut().insert(guest.to_owned(), host);
        Ok(())
    }

    /// Makes a docked pane its own window again (no-op if it is not docked).
    pub fn undock_view(&self, guest: &str) {
        let Some(host) = self.docked.borrow_mut().remove(guest) else {
            return;
        };
        let (Some((g, g_window)), Some((h, h_window))) = (self.pane(guest), self.pane(&host))
        else {
            return;
        };
        g.root.widget().set_parent(g_window.widget());
        let size = g_window.size();
        g.root.set_geometry(0, 0, size.width, size.height);
        if let Some(index) = h.tabs.tab_titles().iter().position(|t| t == g.title) {
            h.tabs.remove_tab(index as i32);
        }
        g_window.show();
        // Nur noch der eigene Inhalt: wieder ohne Reiter.
        if !self.docked.borrow().values().any(|v| *v == host) {
            h.root.widget().set_parent(h_window.widget());
            let size = h_window.size();
            h.root.set_geometry(0, 0, size.width, size.height);
            while h.tabs.tab_count() > 0 {
                h.tabs.remove_tab(0);
            }
            h.tabs.widget().set_visible(false);
        }
    }

    /// Shows the window a pane is in and its tab (Fenster menu).
    pub(super) fn show_pane(&self, name: &str) -> bool {
        let host = self.docked.borrow().get(name).cloned();
        let Some(host) = host else {
            return false;
        };
        if let (Some((pane, _)), Some((h, h_window))) = (self.pane(name), self.pane(&host)) {
            if let Some(index) = h.tabs.tab_titles().iter().position(|t| t == pane.title) {
                h.tabs.set_current_index(index as i32);
            }
            h_window.show();
        }
        true
    }

    /// The window of a pane was resized: its tabs or its own content follow.
    fn pane_window_resized(&self, name: &str, width: i32, height: i32) {
        let Some((pane, _)) = self.pane(name) else {
            return;
        };
        if pane.tabbed() {
            pane.tabs.set_geometry(0, 0, width, height);
        } else if !self.docked.borrow().contains_key(name) {
            pane.root.set_geometry(0, 0, width, height);
        }
    }

    /// Restores the docked panes of the window arrangement.
    pub(super) fn restore_docking(&self, docked: &std::collections::BTreeMap<String, String>) {
        for guest in self.docked().keys() {
            self.undock_view(guest);
        }
        for (guest, host) in docked {
            let _ = self.dock_view(guest, host);
        }
    }
}

/// `root` fills `page` (now and when the page is resized).
fn follow(page: &RnclPanel, root: &RnclPanel) {
    let size = page.size();
    root.set_geometry(0, 0, size.width.max(1), size.height.max(1));
    let root = root.clone();
    page.widget()
        .on_resized(move |w, h| root.set_geometry(0, 0, w, h));
}

/// Connects grips, drops and window sizes of the dockable windows.
pub fn connect(ide: &Rc<Ide>) {
    for (pane, win) in ide.panes() {
        let name = pane.name;
        let weak = Rc::downgrade(ide);
        win.on_resized(move |w, h| {
            if let Some(ide) = weak.upgrade() {
                ide.pane_window_resized(name, w, h);
            }
        });
        // Ein Griff, auf dieses Fenster gezogen: als Reiter hierher.
        let weak = Rc::downgrade(ide);
        win.widget().on_drop(move |event| {
            if event.action != ruf::DropAction::Drop {
                return;
            }
            let Some(guest) = event.text.as_deref().and_then(|t| t.strip_prefix(DRAG)) else {
                return;
            };
            if let Some(ide) = weak.upgrade()
                && let Err(e) = ide.dock_view(guest, name)
            {
                ide.main.set_status(&e);
            }
        });
        // Ins Freie gezogen: wieder ein eigenes Fenster.
        let weak = Rc::downgrade(ide);
        pane.grip.widget().on_drag_cancelled(move || {
            if let Some(ide) = weak.upgrade() {
                ide.undock_view(name);
            }
        });
    }
}
