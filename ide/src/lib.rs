//! # freya-td – the Freya-TD development environment
//!
//! Free-floating windows (like Delphi 7 / Lazarus) on RUF: main window with
//! menu and component palette, form designer, object inspector, structure
//! and messages. The logic behind them lives in [`session`] (no UI) and in
//! `tdr-designer`; the windows in `ui` only forward input and draw.

pub mod desktop;
pub mod lsp;
pub mod session;
pub mod ui;

pub use ui::Ide;

pub use session::{Changes, Session};
