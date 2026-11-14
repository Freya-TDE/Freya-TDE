# 9 Generated and own code

[Contents](README.md) · back: [8 Databases and data binding](08-data.md) · next: [10 Building, running, debugging](10-building.md)

Freya TDE strictly separates what it generates from what belongs to you. Nothing you write is overwritten.

## Which file belongs to whom

| File | Belongs to | Behaviour |
|---|---|---|
| `src/main.rs` (program) or `src/lib.rs` (library) | you | created once, never touched afterwards |
| `src/<form>.rs` | you | created once (`include!("<form>.gen.rs")` and handlers); missing handler stubs are **appended** as a new `impl` block |
| `src/app.gen.rs`, `src/<form>.gen.rs` | Freya TDE | rewritten on every run, write-protected, checksum in the header |
| `Cargo.toml` | you | merged (`toml_edit`): missing dependencies and features are added, nothing is removed, your versions stay |
| `src/<form>.ftdr`, `<project>.ptdr` | Freya TDE (through the designer) | RON, readable and versionable |

If a `.gen.rs` is changed from outside, Freya TDE reports it and rewrites it; a file of the same name that was
never generated is not overwritten. The project window can hide the `.gen.rs` files.

"Generate code" (Ctrl+G) works in two steps: first a preview (which files are new, changed, unchanged, which
messages), then writing – never with errors and never over a file changed in the meantime.

## Handlers

Empty handler stubs keep readable parameter names (`fn button1_on_click(&mut self)`,
`fn grid_saving(&mut self, row: &GridRow, inserted: bool) -> bool`); the "unused variable" warning is
accepted for that.

## Code completion like the Lazarus CodeTools

**Ctrl+Shift+C** in the code window creates what is missing at the cursor – nothing is ever deleted:

| At the cursor there is … | Result |
|---|---|
| `self.save(&customer, 3)`, the method is missing | `fn save(&mut self, customer: &Customer, arg: i32) {}` – types from rust-analyzer, inserted after the calling method (or alphabetically / at the end, see settings) |
| `self.counter += 1`, the field is missing | an own field of the form (`state`), type suggested and confirmed in a dialog |
| `impl Trait for Type {}` | the missing trait methods |

## Snippets

Saved pieces of code with name, short description, use line and code (palette tab "Snippets",
[chapter 4](04-windows.md#snippets)). When inserting, the use line is appended to the use block if it is
missing. Stored in `<config>/freya-tde/snippets.toml`.

## Formatting

The tool bar's "Format" formats each of your own files with `rustfmt`. `cargo fmt` is deliberately not
used: it would follow the `app` module into the write-protected `app.gen.rs`.
