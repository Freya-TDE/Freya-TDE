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

impl ContainerLayout {
    /// What a container without a layout of its own gets in the backends
    /// (and in the designer's Linux theme): vertical, spacing 8, padding 12.
    pub const STANDARD: Self = Self {
        kind: LayoutKind::Vertical,
        spacing: 8,
        padding: Insets::all(12),
    };
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
        /// Size as a share of the parent's content area in percent, per axis
        /// (0 = not set, else 1–100). Goes before `size`; on that axis
        /// `Align::Fill` acts like `Start` and `stretch` does not apply.
        #[serde(default, skip_serializing_if = "is_natural")]
        percent: Size,
    },
    /// At a fixed position and size (parent with absolute layout).
    Absolute {
        position: Point,
        size: Size,
        /// Edges of the parent the widget keeps its distance to when the
        /// parent's size changes (Delphi's `Anchors`; default left and top).
        #[serde(default, skip_serializing_if = "Anchors::is_top_left")]
        anchors: Anchors,
    },
}

fn yes() -> bool {
    true
}

/// Edges of its parent an absolutely placed widget is anchored to (like
/// Delphi's `Anchors`). When the parent's size changes, the widget keeps its
/// distance to the anchored edges: anchored left and right it grows in
/// width, anchored only right it moves with the right edge, anchored
/// neither left nor right it keeps its centre. Same for top and bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Anchors {
    #[serde(default = "yes")]
    pub left: bool,
    #[serde(default = "yes")]
    pub top: bool,
    #[serde(default)]
    pub right: bool,
    #[serde(default)]
    pub bottom: bool,
}

impl Default for Anchors {
    fn default() -> Self {
        Self::TOP_LEFT
    }
}

impl Anchors {
    pub const TOP_LEFT: Self = Self::new(true, true, false, false);
    pub const ALL: Self = Self::new(true, true, true, true);

    pub const fn new(left: bool, top: bool, right: bool, bottom: bool) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub fn is_top_left(&self) -> bool {
        *self == Self::TOP_LEFT
    }

    /// Position and size in a parent area of size `actual` for a widget
    /// designed at `position`/`size` in an area of size `design`.
    pub fn apply(self, position: Point, size: Size, design: Size, actual: Size) -> (Point, Size) {
        let axis = |start: bool, end: bool, pos: i32, len: i32, delta: i32| match (start, end) {
            (true, true) => (pos, (len + delta).max(0)),
            (false, true) => (pos + delta, len),
            (false, false) => (pos + delta / 2, len),
            (true, false) => (pos, len),
        };
        let (x, width) = axis(
            self.left,
            self.right,
            position.x,
            size.width,
            actual.width - design.width,
        );
        let (y, height) = axis(
            self.top,
            self.bottom,
            position.y,
            size.height,
            actual.height - design.height,
        );
        (Point { x, y }, Size { width, height })
    }
}

impl Default for Placement {
    fn default() -> Self {
        Placement::Layout {
            stretch: 0,
            align: Align::Fill,
            cell: None,
            size: Size::default(),
            percent: Size::default(),
        }
    }
}

impl Placement {
    pub fn absolute(x: i32, y: i32, width: i32, height: i32) -> Self {
        Placement::Absolute {
            position: Point { x, y },
            size: Size { width, height },
            anchors: Anchors::TOP_LEFT,
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

    /// The size of the area this container's absolutely placed children
    /// were designed for (the reference of their [`Anchors`]): a form's
    /// `size`, else the container's own size (absolute placement or
    /// requested size) – both without padding ([`ContainerLayout::STANDARD`]
    /// without a layout) – and per axis without a
    /// known size the extent of the children.
    pub fn design_area(&self) -> Size {
        let padding = self
            .visual
            .layout
            .as_ref()
            .map_or(ContainerLayout::STANDARD.padding, |l| l.padding);
        let extent = self
            .children
            .iter()
            .filter_map(|c| match c.visual.placement {
                Placement::Absolute { position, size, .. } => {
                    Some((position.x + size.width, position.y + size.height))
                }
                _ => None,
            })
            .fold((0, 0), |a, b| (a.0.max(b.0), a.1.max(b.1)));
        let outer = if self.component.type_ref == crate::builtin::form() {
            let size = match self.component.get("size").map(|v| &v.value) {
                Some(crate::Value::Size(s)) => *s,
                _ => Size {
                    width: 640,
                    height: 480,
                },
            };
            (Some(size.width), Some(size.height))
        } else {
            match self.visual.placement {
                Placement::Absolute { size, .. } => (Some(size.width), Some(size.height)),
                Placement::Layout { size, .. } => (
                    (size.width > 0).then_some(size.width),
                    (size.height > 0).then_some(size.height),
                ),
            }
        };
        Size {
            width: outer
                .0
                .map_or(extent.0, |w| (w - padding.left - padding.right).max(0)),
            height: outer
                .1
                .map_or(extent.1, |h| (h - padding.top - padding.bottom).max(0)),
        }
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
