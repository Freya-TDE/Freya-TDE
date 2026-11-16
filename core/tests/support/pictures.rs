//! Pictures in the backend tests: an `ImageBox` with opacity and gamma, a
//! gallery from a list and one from the data source `services::fruits`.

use tdr_core::*;

fn name(n: &str) -> ComponentName {
    ComponentName::new(n).unwrap()
}

/// Adds `photo` (ImageBox, `on_click` → `photo_clicked`), `gallery`
/// (ImageScrollBox with three pictures, `on_select` → `picture_selected`)
/// and `bound_gallery` (filled from `services::fruits`; the names are no
/// pictures, the code must still build and fill the list) to `window`.
pub fn add(form: &mut Form) {
    form.add_widget(
        &name("window"),
        Widget::from_component(
            Component::new(name("photo"), builtin::image_box())
                .with("file", "photo.png")
                .with("opacity", Value::Float(0.8))
                .with("gamma", Value::Float(1.5))
                .on("on_click", HandlerName::new("photo_clicked").unwrap()),
        ),
    )
    .unwrap();
    form.add_widget(
        &name("window"),
        Widget::from_component(
            Component::new(name("gallery"), builtin::image_scroll_box())
                .with(
                    "images",
                    Value::List(
                        ["photo.png", "second.png", "third.png"]
                            .map(|p| Value::Text(p.into()))
                            .to_vec(),
                    ),
                )
                .with(
                    "thumb_size",
                    Value::Size(Size {
                        width: 48,
                        height: 48,
                    }),
                )
                .on("on_select", HandlerName::new("picture_selected").unwrap()),
        ),
    )
    .unwrap();
    form.add_widget(
        &name("window"),
        Widget::from_component(
            Component::new(name("bound_gallery"), builtin::image_scroll_box())
                .with("orientation", Value::Enum("Horizontal".into()))
                .with(
                    data::DATA_SOURCE,
                    Value::reference_on(name("services"), name("fruits")),
                )
                .with(data::DISPLAY_FIELD, "name")
                .with(data::VALUE_FIELD, "id"),
        ),
    )
    .unwrap();
}

/// Writes the three test pictures into the project folder.
pub fn files(dir: &std::path::Path) {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ide/assets/palette");
    for (file, source) in [
        ("photo.png", "imagebox.png"),
        ("second.png", "imagescrollbox.png"),
        ("third.png", "image.png"),
    ] {
        std::fs::copy(assets.join(source), dir.join(file)).unwrap();
    }
}

/// Handlers of the user's form code (inside `impl MainForm`).
pub const HANDLERS: &str = r#"
    pub fn photo_clicked(&mut self) {
        println!("photo_clicked");
    }
    pub fn picture_selected(&mut self, index: usize, path: &str) {
        println!("picture={index} path={path}");
    }
"#;
