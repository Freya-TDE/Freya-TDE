//! Generates a sample project (Tokio runtime, SQLx pool and query, Axum
//! router, a form with widgets) – first as a dry run, printed as JSON the
//! way a tool or an AI would consume it, then written with `--write`.
//!
//! ```bash
//! cargo run -p tdr-core --example generate -- /tmp/crm          # dry run
//! cargo run -p tdr-core --example generate -- /tmp/crm --write  # write files
//! ```

use std::path::PathBuf;

use tdr_core::codegen::{generate, write};
use tdr_core::*;

fn name(n: &str) -> ComponentName {
    ComponentName::new(n).unwrap()
}

fn forms() -> Vec<Form> {
    let mut services = Form::data_module(name("services"), "egui");
    services
        .add_component(Component::new(
            name("runtime"),
            TypeRef::pack("tokio", "Runtime"),
        ))
        .unwrap();
    services
        .add_component(
            Component::new(name("db_pool"), TypeRef::pack("sqlx", "Pool"))
                .with("url", "postgres://localhost/crm"),
        )
        .unwrap();

    let mut main = Form::new(
        name("main_form"),
        "egui",
        Widget::from_component(
            Component::new(name("window"), builtin::form())
                .on("on_close", HandlerName::new("may_close").unwrap()),
        ),
    );
    main.add_widget(
        &name("window"),
        Widget::from_component(
            Component::new(name("name_edit"), builtin::edit())
                .on("on_change", HandlerName::new("name_changed").unwrap()),
        ),
    )
    .unwrap();
    main.add_component(
        Component::new(name("api"), TypeRef::pack("axum", "Router")).with(
            "pool",
            Value::reference_on(name("services"), name("db_pool")),
        ),
    )
    .unwrap();
    main.add_component(
        Component::new(name("load"), TypeRef::pack("sqlx", "Query"))
            .with(
                "pool",
                Value::reference_on(name("services"), name("db_pool")),
            )
            .with("router", Value::reference(name("api"))),
    )
    .unwrap();
    vec![services, main]
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().unwrap_or_else(|| "crm".into()));
    let write_files = args.any(|a| a == "--write");

    let (packs, errors) = tdr_core::pack::load_all(std::path::Path::new("components"));
    for error in &errors {
        eprintln!("error: {error}");
    }
    let mut registry = TypeRegistry::with_builtins();
    for pack in &packs {
        pack.register(&mut registry).unwrap();
    }
    let project = ProjectFile::new("crm", "1.99", "egui").unwrap();

    let generation = generate(&project, &forms(), &registry, &dir);
    let summary: Vec<_> = generation
        .files
        .iter()
        .map(|f| serde_json::json!({ "path": f.path, "kind": f.kind, "change": f.change }))
        .collect();
    let report = serde_json::json!({
        "files": summary,
        "order": generation.order,
        "diagnostics": generation.diagnostics,
    });
    println!("{}", serde_json::to_string_pretty(&report).unwrap());

    if write_files {
        match write(&dir, &generation) {
            Ok(written) => eprintln!("written: {}", written.join(", ")),
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
    }
}
