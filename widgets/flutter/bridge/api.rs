//! Bridge between the Flutter UI and the Rust side (flutter_rust_bridge).
//!
//! The same for every project: the Dart bindings in `flutter/lib/src/rust/`
//! and `frb_generated.gen.rs` were generated from exactly this file.

#![allow(unexpected_cfgs)]

use flutter_rust_bridge::frb;

/// The state of one widget, as the Flutter side draws it.
pub struct FreyaControl {
    /// Name of the widget in the designer.
    pub name: String,
    /// Caption of buttons, check boxes, frames; title of the window.
    pub caption: String,
    /// Text of labels, edits and links.
    pub text: String,
    /// Check boxes, switches, radio buttons.
    pub checked: bool,
    /// Sliders, progress bars, number inputs.
    pub number: f64,
    /// Selected entry, row or page (-1 = none).
    pub index: i32,
    /// Entries of lists and combo boxes, tab titles, menu entries, column headers.
    pub items: Vec<String>,
    /// Cells of tables, row by row.
    pub rows: Vec<Vec<String>>,
    pub enabled: bool,
    pub visible: bool,
    /// Hint shown on hover (empty = none).
    pub tooltip: String,
}

/// What the user entered; only the field of the widget's kind is set.
pub struct FreyaInput {
    pub text: Option<String>,
    pub checked: Option<bool>,
    pub number: Option<f64>,
    pub index: Option<i32>,
}

#[frb(init)]
pub fn freya_init() {
    flutter_rust_bridge::setup_default_user_utils();
}

/// Creates all components (`App::init`) and calls `started` in `lib.rs`.
#[frb(sync)]
pub fn freya_start() -> Result<(), String> {
    crate::app::freya_host_start()
}

/// The form is shown: loads its data, calls `on_create`, returns its state.
#[frb(sync)]
pub fn freya_open(form: String) -> Vec<FreyaControl> {
    crate::app::freya_host_open(&form)
}

/// The state of all widgets of `form`.
#[frb(sync)]
pub fn freya_snapshot(form: String) -> Vec<FreyaControl> {
    crate::app::freya_host_snapshot(&form)
}

/// The user did something: takes `input` into the widget, calls the
/// handler of `event` (if any) and returns the new state of the form.
#[frb(sync)]
pub fn freya_event(form: String, widget: String, event: String, input: FreyaInput) -> Vec<FreyaControl> {
    crate::app::freya_host_event(&form, &widget, &event, input)
}

/// The new state once data loaded in the background has arrived, else none.
#[frb(sync)]
pub fn freya_poll(form: String) -> Option<Vec<FreyaControl>> {
    crate::app::freya_host_poll(&form)
}

/// The window is about to close; `false` keeps it open (`on_close`).
#[frb(sync)]
pub fn freya_close(form: String) -> bool {
    crate::app::freya_host_close(&form)
}
