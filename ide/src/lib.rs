//! # freya-tde – the Freya TDE development environment
//!
//! Free-floating windows (like Delphi 7 / Lazarus) on RUF: main window with
//! menu and component palette, form designer, object inspector, structure,
//! messages, code editor and project window (file manager). The logic
//! behind them lives in [`session`] and [`files`] (no UI) and in
//! `tdr-designer`; the windows in `ui` only forward input and draw.

pub mod codetools;
pub mod database;
pub mod debug;
pub mod desktop;
pub mod files;
pub mod flutter;
pub mod git;
pub mod lsp;
pub mod output;
pub mod problems;
pub mod projects;
pub mod session;
pub mod settings;
pub mod snippets;
pub mod terminal;
pub mod tools;
pub mod ui;
pub mod wizard;

pub use ui::Ide;

pub use session::{Changes, Session};
