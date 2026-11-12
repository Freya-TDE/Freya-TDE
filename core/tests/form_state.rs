//! Own fields of a form (`state` of `core/Form`): in the struct, initialised
//! in `App::init`, wrong entries reported.

use std::collections::BTreeMap;
use std::path::PathBuf;

use tdr_core::codegen::{CodegenIssue, generate};
use tdr_core::pack::load_all;
use tdr_core::state::STATE;
use tdr_core::*;

fn name(n: &str) -> ComponentName {
    ComponentName::new(n).unwrap()
}

fn registry() -> TypeRegistry {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut registry = TypeRegistry::with_builtins();
    for dir in ["components", "widgets"] {
        let (packs, errors) = load_all(&root.join(dir));
        assert!(errors.is_empty(), "{errors:#?}");
        for pack in packs {
            pack.register(&mut registry).unwrap();
        }
    }
    registry
}

fn form(state: &[&str]) -> Form {
    let mut form = Form::new(
        name("main_form"),
        "egui",
        Widget::from_component(Component::new(name("window"), builtin::form()).with(
            STATE,
            Value::List(state.iter().map(|s| Value::Text((*s).into())).collect()),
        )),
    );
    form.add_widget(
        &name("window"),
        Widget::from_component(Component::new(name("ok_button"), builtin::button())),
    )
    .unwrap();
    form
}

fn files(state: &[&str]) -> (String, String, Vec<CodegenIssue>) {
    let project = ProjectFile::new("zustand", "1.95", "egui").unwrap();
    let generation = generate(
        &project,
        &[form(state)],
        &registry(),
        &BTreeMap::<String, String>::new(),
    );
    let file = |path: &str| {
        generation
            .files
            .iter()
            .find(|f| f.path == path)
            .map(|f| f.content.split_whitespace().collect::<String>())
            .unwrap_or_default()
    };
    let issues = generation
        .diagnostics
        .iter()
        .map(|d| d.issue.clone())
        .collect();
    (file("src/main_form.gen.rs"), file("src/app.gen.rs"), issues)
}

#[test]
fn own_fields_are_generated() {
    let (form, app, issues) = files(&["zaehler: i32", "titel: String = \"Neu\".into()"]);
    assert!(
        !issues
            .iter()
            .any(|i| matches!(i, CodegenIssue::FormState { .. })),
        "{issues:#?}"
    );
    assert!(form.contains("pubzaehler:i32,"), "{form}");
    assert!(form.contains("pubtitel:String,"), "{form}");
    assert!(app.contains("zaehler:Default::default(),"), "{app}");
    assert!(app.contains("titel:\"Neu\".into(),"), "{app}");
}

#[test]
fn wrong_fields_are_reported() {
    for (state, needle) in [
        (&["zaehler"][..], "needs a type"),
        (&["ok_button: i32"][..], "already a component"),
        (&["freya_x: i32"][..], "reserved"),
        (&["a: i32", "a: u8"][..], "twice"),
    ] {
        let (form, _, issues) = files(state);
        let messages: Vec<String> = issues
            .iter()
            .filter_map(|i| match i {
                CodegenIssue::FormState { message, .. } => Some(message.clone()),
                _ => None,
            })
            .collect();
        assert!(
            messages.iter().any(|m| m.contains(needle)),
            "{state:?}: {messages:?}"
        );
        assert!(!form.contains("pubok_button:i32"), "{form}");
    }
}
