//! Builds a small form, validates it and prints it as `.ftdr` (RON).
//!
//! ```bash
//! cargo run -p tdr-core --example sample_form
//! ```

use tdr_core::*;

fn main() -> Result<(), ModelError> {
    let name = |n: &str| ComponentName::new(n);

    let mut form = Form::new(
        name("customers")?,
        "egui",
        Widget::from_component(
            Component::new(name("main_form")?, builtin::form()).with("title", "Customers"),
        ),
    );
    form.add_widget(
        &name("main_form")?,
        Widget::from_component(
            Component::new(name("ok_button")?, builtin::button())
                .with(
                    "caption",
                    PropertyValue::new("OK").on(Platform::MacOs, "Okay"),
                )
                .on("on_click", HandlerName::new("ok_clicked")?),
        ),
    )?;
    form.add_widget(
        &name("main_form")?,
        Widget::from_component(
            Component::new(name("name_edit")?, builtin::edit()).with("placeholder", "Name"),
        )
        .with_placement(Placement::Layout {
            stretch: 1,
            align: Align::Fill,
            cell: None,
            size: Size::default(),
            percent: Size::default(),
        }),
    )?;

    let registry = TypeRegistry::with_builtins();
    for diagnostic in form.validate(&ValidationContext::new(&registry)) {
        eprintln!("{diagnostic}");
    }
    println!("{}", form.to_ron()?);
    Ok(())
}
