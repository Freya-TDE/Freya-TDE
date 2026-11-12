//! Editing a data grid in the backend tests: the same steps for every
//! backend – change a cell (a wrong number is refused), save, add a row,
//! discard it, delete a row – through the generated `<grid>_…` methods.

use tdr_core::*;

fn handler(n: &str) -> HandlerName {
    HandlerName::new(n).unwrap()
}

/// `grid` editable, with handlers for `on_post` and `on_delete`.
pub fn editable(grid: Component) -> Component {
    grid.with(tdr_core::data::EDITABLE, true)
        .on(tdr_core::data::ON_POST, handler("grid_saving"))
        .on(tdr_core::data::ON_DELETE, handler("grid_deleting"))
}

/// Statements for a handler of the form (`grid` bound to the fruits): they
/// print with `out(text: &str)`; `select_second` selects row 2 of the grid.
pub fn calls(out: &str, select_second: &str) -> String {
    format!(
        r#"
        {out}(&format!("edit_bad={{}}", self.grid_edit_cell(1, 0, "zwei").is_err()));
        let _ = self.grid_edit_cell(1, 0, "2");
        let _ = self.grid_edit_cell(1, 1, "Birnen");
        {out}(&format!("posted={{}}", self.grid_post()));
        {out}(&format!("edited={{:?}}", self.grid_rows.get(1).map(|r| r.name.clone())));
        self.grid_insert();
        {out}(&format!("inserted_rows={{}}", self.grid_rows.len()));
        self.grid_cancel();
        {out}(&format!("cancelled_rows={{}}", self.grid_rows.len()));
        {select_second}
        {out}(&format!("deleted={{}} rows={{}}", self.grid_delete(), self.grid_rows.len()));
"#
    )
}

/// The handlers of `editable` (methods of the form).
pub fn handlers(out: &str) -> String {
    format!(
        r#"
    pub fn grid_saving(&mut self, row: &GridRow, inserted: bool) -> bool {{
        {out}(&format!("saving={{}} inserted={{inserted}}", row.name));
        true
    }}
    pub fn grid_deleting(&mut self, row: &GridRow) -> bool {{
        {out}(&format!("deleting={{}}", row.name));
        true
    }}
"#
    )
}

/// What the program prints for `calls`.
pub const EXPECTED: [&str; 8] = [
    "edit_bad=true",
    "saving=Birnen inserted=false",
    "posted=true",
    "edited=Some(\"Birnen\")",
    "inserted_rows=4",
    "cancelled_rows=3",
    "deleting=Birnen",
    "deleted=true rows=2",
];
