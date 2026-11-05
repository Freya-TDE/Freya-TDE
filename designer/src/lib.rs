//! # tdr-designer – the Freya TDE designer without UI
//!
//! Everything the form designer does, as a library: the UI (and later an
//! AI) only calls these functions and draws the result.
//!
//! | Concept | Type / function |
//! |---|---|
//! | Theme and metric tables per platform (TOML) | [`Theme`] |
//! | Where every widget sits | [`layout()`], [`FormLayout`] |
//! | What to draw | [`render()`], [`Primitive`] |
//! | What is under the mouse | [`hit_test`], [`layout::drop_target`] |
//! | An open project, editing commands, undo/redo, saving | [`Document`], [`Command`] |
//! | Palette and object inspector as data | [`palette`], [`properties`], [`events`] |
//!
//! Commands, results, layouts and display lists are serializable: an AI
//! can drive the designer with JSON commands and "see" the form as a
//! display list, exactly like the UI does.

pub mod geometry;
pub mod layout;
pub mod theme;

pub use geometry::{Point, Rect};
pub use layout::{FormLayout, ItemKind, LayoutItem, layout};
pub use theme::{Colors, Theme, WidgetMetrics};
pub mod hit;
pub mod render;

pub use hit::{Handle, Hit, hit_test};
pub use render::{Primitive, render, render_selection, tab_at};
pub mod command;
pub mod document;

pub use command::{Command, CommandError, Outcome};
pub use document::{Document, DocumentError, Findings, OpenReport};
pub mod inspector;
pub mod placement;

pub use inspector::{
    EventRow, PaletteItem, PropertyRow, events, palette, properties, suggest_handler,
};
pub use placement::{LayoutRowError, is_layout_row, layout_command, layout_rows};
pub mod svg;

pub use svg::to_svg;
