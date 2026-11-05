//! [`Widget`]: a component with a visual extension – placement in its parent,
//! the layout of its own children, and the children themselves.

use serde::{Deserialize, Serialize};

use crate::component::Component;
use crate::name::ComponentName;
use crate::type_ref::TypeRef;
use crate::value::{Point, Size};

/// Horizontal or vertical alignment inside the space a layout assigns.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Align {
    /// Take the whole space.
    #[default]
    Fill,
    Start,
    Center,
    End,
}

/// Inner spacing of a container.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Insets {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Insets {
    pub const fn all(value: i32) -> Self {
        Self {
            left: value,
            top: value,
            right: value,
            bottom: value,
        }
    }
}

/// How a container arranges its children. Container layouts are the basic
/// form; absolute positioning is an option a backend may allow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LayoutKind {
    /// Children top to bottom.
    #[default]
    Vertical,
    /// Children left to right.
    Horizontal,
    /// A grid with a fixed number of columns.
    Grid { columns: u32 },
    /// Children on top of each other (pages, overlays).
    Stack,
    /// Children at absolute positions (only if the backend allows it).
    Absolute,
    /// A backend-specific layout, by name.
    Custom(String),
}

/// The layout of a container's children.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ContainerLayout {
    pub kind: LayoutKind,
    #[serde(default)]
    pub spacing: i32,
    #[serde(default)]
    pub padding: Insets,
}

/// A cell in a grid layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridCell {
    pub row: u32,
    pub column: u32,
    #[serde(default = "one")]
    pub row_span: u32,
    #[serde(default = "one")]
    pub column_span: u32,
}

fn one() -> u32 {
    1
}

fn is_zero(v: &u32) -> bool {
    *v == 0
}

fn is_fill(a: &Align) -> bool {
    *a == Align::Fill
}

fn is_natural(s: &Size) -> bool {
    *s == Size::default()
}

fn is_default_visual(v: &Visual) -> bool {
    *v == Visual::default()
}

/// How a widget sits inside its parent's layout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Placement {
    /// Placed by the parent's layout.
    Layout {
        /// Share of extra space (0 = natural size).
        #[serde(default, skip_serializing_if = "is_zero")]
        stretch: u32,
        #[serde(default, skip_serializing_if = "is_fill")]
        align: Align,
        /// Only for grid layouts.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cell: Option<GridCell>,
        /// Requested size like Delphi's Width/Height; replaces the natural
        /// size per axis (0 = natural). `stretch` and `Align::Fill` may
        /// still make the widget larger.
        #[serde(default, skip_serializing_if = "is_natural")]
        size: Size,
    },
    /// At a fixed position and size (parent with absolute layout).
    Absolute { position: Point, size: Size },
}

impl Default for Placement {
    fn default() -> Self {
        Placement::Layout {
            stretch: 0,
            align: Align::Fill,
            cell: None,
            size: Size::default(),
        }
    }
}

impl Placement {
    pub fn absolute(x: i32, y: i32, width: i32, height: i32) -> Self {
        Placement::Absolute {
            position: Point { x, y },
            size: Size { width, height },
        }
    }

    pub fn is_absolute(&self) -> bool {
        matches!(self, Placement::Absolute { .. })
    }
}

/// The visual extension of a widget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Visual {
    #[serde(default)]
    pub placement: Placement,
    /// Layout of the children – only for containers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<ContainerLayout>,
}

/// A visual component: a [`Component`] plus [`Visual`] data and children.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Widget {
    pub component: Component,
    /// Omitted in files when it only holds defaults.
    #[serde(default, skip_serializing_if = "is_default_visual")]
    pub visual: Visual,
    /// Child widgets (only containers may have any).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Widget>,
}

impl Widget {
    pub fn new(name: ComponentName, type_ref: TypeRef) -> Self {
        Self::from_component(Component::new(name, type_ref))
    }

    pub fn from_component(component: Component) -> Self {
        Self {
            component,
            visual: Visual::default(),
            children: Vec::new(),
        }
    }

    pub fn with_placement(mut self, placement: Placement) -> Self {
        self.visual.placement = placement;
        self
    }

    pub fn with_layout(mut self, layout: ContainerLayout) -> Self {
        self.visual.layout = Some(layout);
        self
    }

    pub fn with_child(mut self, child: Widget) -> Self {
        self.children.push(child);
        self
    }

    /// This widget and all descendants, depth-first.
    pub fn descendants(&self) -> Vec<&Widget> {
        let mut out = vec![self];
        for child in &self.children {
            out.extend(child.descendants());
        }
        out
    }

    pub(crate) fn find_mut(&mut self, name: &ComponentName) -> Option<&mut Widget> {
        if &self.component.name == name {
            return Some(self);
        }
        self.children.iter_mut().find_map(|c| c.find_mut(name))
    }

    pub(crate) fn for_each_mut(&mut self, f: &mut dyn FnMut(&mut Widget)) {
        f(self);
        for child in &mut self.children {
            child.for_each_mut(f);
        }
    }

    /// Removes the descendant `name` (with its subtree) and returns it.
    pub(crate) fn remove_descendant(&mut self, name: &ComponentName) -> Option<Widget> {
        if let Some(i) = self.children.iter().position(|c| &c.component.name == name) {
            return Some(self.children.remove(i));
        }
        self.children
            .iter_mut()
            .find_map(|c| c.remove_descendant(name))
    }
}
