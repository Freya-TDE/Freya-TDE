//! WebSocket, HTTP and Axum packs end to end (`components/ws`, `http`,
//! `axum`): a GTK 4 program whose data module `services` serves an Axum
//! router with a WebSocket endpoint, and whose form connects with a
//! WebSocket client, sends a request and fills a list from a request.
//! Handlers run through the event queues – in the data module's Tokio task
//! and on the form's (GTK) thread.
//!
//! Needs GTK 4 and a display (`xvfb-run`) like the GTK backend test; the
//! first build fetches the crates. Without them the test only generates
//! (set `FREYA_TDE_REQUIRE_GTK=1` to make that an error).

use std::path::PathBuf;

use tdr_core::codegen::{generate, write};
use tdr_core::pack::load_all;
use tdr_core::*;

fn name(n: &str) -> ComponentName {
    ComponentName::new(n).unwrap()
}

fn handler(n: &str) -> HandlerName {
    HandlerName::new(n).unwrap()
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

fn forms(port: u16) -> Vec<Form> {
    let pack = |p: &str, t: &str| TypeRef::pack(p, t);
    let mut services = Form::data_module(name("services"), "gtk4");
    let add = |form: &mut Form, c: Component| form.add_component(c).unwrap();
    add(
        &mut services,
        Component::new(name("runtime"), pack("tokio", "Runtime")),
    );
    add(
        &mut services,
        Component::new(name("api"), pack("axum", "Router"))
            .with("bind", format!("127.0.0.1:{port}"))
            .on("on_start", handler("server_started")),
    );
    add(
        &mut services,
        Component::new(name("echo"), pack("ws", "Endpoint"))
            .with("router", Value::reference(name("api")))
            .on("on_message", handler("server_message")),
    );
    add(
        &mut services,
        Component::new(name("web"), pack("http", "Client"))
            .with("base_url", format!("http://127.0.0.1:{port}/"))
            .with("user_agent", "freya-test"),
    );
    add(
        &mut services,
        Component::new(name("items"), pack("http", "Request"))
            .with("client", Value::reference(name("web")))
            .with("url", "/items")
            .with("rows", "data.items")
            .with(
                "columns",
                Value::List(vec![
                    Value::Text("id: i64".into()),
                    Value::Text("name: String".into()),
                    Value::Text("note: Option<String>".into()),
                ]),
            ),
    );

    let mut main = Form::new(
        name("main_form"),
        "gtk4",
        Widget::from_component(
            Component::new(name("window"), builtin::form()).with("title", "web-Test"),
        ),
    );
    main.add_widget(
        &name("window"),
        Widget::from_component(
            Component::new(name("fruits"), TypeRef::backend("gtk4", "DropDown"))
                .with(
                    data::DATA_SOURCE,
                    Value::reference_on(name("services"), name("items")),
                )
                .with(data::DISPLAY_FIELD, "name")
                .with(data::VALUE_FIELD, "id")
                .with(data::LOAD, Value::Enum("Manual".into())),
        ),
    )
    .unwrap();
    add(
        &mut main,
        Component::new(name("socket"), pack("ws", "Client"))
            .with("url", format!("ws://127.0.0.1:{port}/ws"))
            .on("on_open", handler("socket_open"))
            .on("on_message", handler("socket_message"))
            .on("on_error", handler("socket_failed")),
    );
    add(
        &mut main,
        Component::new(name("ping"), pack("http", "Request"))
            .with("client", Value::reference_on(name("services"), name("web")))
            .with("method", Value::Enum("POST".into()))
            .with("url", "/items")
            .with("body", "{}")
            .on("on_response", handler("ping_answered")),
    );
    vec![services, main]
}

const MAIN_RS: &str = r##"
#[path = "app.gen.rs"]
mod app;

fn main() {
    if let Err(error) = app::run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn started(app: &mut app::App) -> Result<(), app::Error> {
    // Eine eigene Route neben dem WebSocket-Endpunkt.
    let rows = r#"{"data": {"items": [{"id": 1, "name": "Apfel"}, {"id": 2, "name": "Birne", "note": "reif"}]}}"#;
    app.services.api = app.services.api.clone().route(
        "/items",
        axum::routing::get(move || async move {
            ([(axum::http::header::CONTENT_TYPE, "application/json")], rows)
        })
        .post(|| async { (axum::http::StatusCode::CREATED, "neu") }),
    );
    Ok(())
}
"##;

const SERVICES_RS: &str = r#"
include!("services.gen.rs");

impl Services {
    pub fn server_started(&mut self, address: std::net::SocketAddr) {
        println!("started={address}");
    }
    pub fn server_message(&mut self, client: u64, text: &str) {
        println!("server_got={text} clients={}", self.echo_clients().len());
        self.echo_send(client, &text.to_uppercase());
    }
}
"#;

const FORM_RS: &str = r#"
include!("main_form.gen.rs");

impl MainForm {
    pub fn socket_open(&mut self) {
        println!("open={}", self.socket_is_open());
        self.socket_send("hallo");
    }
    pub fn socket_failed(&mut self, message: &str) {
        // Der Server lauscht vielleicht noch nicht: kurz warten, neu verbinden.
        eprintln!("retry: {message}");
        std::thread::sleep(std::time::Duration::from_millis(200));
        self.socket_connect();
    }
    pub fn socket_message(&mut self, text: &str) {
        println!("ws={text}");
        self.fruits_reload();
        self.ping_send();
        let fruits = self.fruits.clone();
        gtk4::glib::timeout_add_local_once(std::time::Duration::from_millis(1500), move || {
            use gtk4::prelude::*;
            let items = fruits.model().map_or(0, |m| m.n_items());
            println!("items={items}");
            std::process::exit(0);
        });
    }
    pub fn ping_answered(&mut self, status: u16, body: &str) {
        println!("response={status} {body}");
        println!("selected={:?}", self.fruits_selected_value());
    }
}
"#;

fn gtk_available() -> bool {
    let found = std::process::Command::new("pkg-config")
        .args(["--exists", "gtk4"])
        .status()
        .is_ok_and(|s| s.success());
    let display = std::env::var_os("DISPLAY").is_some()
        && (std::env::var("XAUTHORITY").is_ok_and(|a| a.contains("xvfb-run"))
            || std::env::var_os("FREYA_TDE_REAL_DISPLAY").is_some());
    if !(found && display) && std::env::var_os("FREYA_TDE_REQUIRE_GTK").is_some() {
        panic!("GTK 4 or a display is missing");
    }
    found && display
}

#[test]
fn websocket_http_and_axum_end_to_end() {
    let registry = registry();
    let port = 39000 + (std::process::id() % 500) as u16;
    let forms = forms(port);
    for form in &forms {
        let issues = validate(form, &ValidationContext::for_backend(&registry, "gtk4"));
        assert!(
            issues.iter().all(|i| i.severity != Severity::Error),
            "{issues:#?}"
        );
    }
    let dir = std::env::temp_dir().join(format!("freya-web-demo-{}", std::process::id()));
    let target = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("web-demo-target");
    // SAFETY: einziger Test dieser Datei, gesetzt bevor Threads starten.
    unsafe { std::env::set_var("CARGO_TARGET_DIR", &target) };
    let _ = std::fs::remove_dir_all(&dir);
    let project = ProjectFile::new("web_demo", "1.85", "gtk4").unwrap();
    let generation = generate(&project, &forms, &registry, &dir);
    assert!(!generation.has_errors(), "{:#?}", generation.diagnostics);
    let text = |path: &str| {
        let file = generation.files.iter().find(|f| f.path == path).unwrap();
        file.content.clone()
    };
    // Erzeugt: Anhängen in App::init, Warteschlangen, Start des Datenmoduls.
    let app = text("src/app.gen.rs");
    assert!(
        app.contains(".route(\"/ws\", services__echo.route())"),
        "{app}"
    );
    assert!(app.contains("app.services.freya_run();"), "{app}");
    assert!(app.contains("pub struct WsHub"), "{app}");
    assert!(app.contains("pub struct WsClient"), "{app}");
    let main_gen = text("src/main_form.gen.rs");
    assert!(main_gen.contains("pub fn socket_send(&self, text: &str) -> bool"));
    assert!(main_gen.contains("pub freya_queue: crate::app::Queue<Self>"));
    // Basis-URL und Pfad.
    assert!(text("src/app.gen.rs").contains(&format!("\"http://127.0.0.1:{port}/items\"")));
    let manifest = text("Cargo.toml");
    for krate in [
        "tokio-tungstenite",
        "reqwest",
        "rustls",
        "serde_json",
        "futures-util",
    ] {
        assert!(manifest.contains(krate), "{krate} missing:\n{manifest}");
    }
    assert!(manifest.contains("\"ws\""), "axum feature ws:\n{manifest}");

    write(&dir, &generation).unwrap();
    std::fs::write(dir.join("src/main.rs"), MAIN_RS).unwrap();
    std::fs::write(dir.join("src/services.rs"), SERVICES_RS).unwrap();
    std::fs::write(dir.join("src/main_form.rs"), FORM_RS).unwrap();

    if !gtk_available() {
        eprintln!("GTK 4 oder Bildschirm fehlt – Bauen und Starten übersprungen");
        return;
    }
    let report = preview::build(&dir, Some(&generation)).unwrap();
    let errors: Vec<_> = report.errors().collect();
    assert!(report.success, "{errors:#?}");
    let warnings: Vec<_> = report
        .messages
        .iter()
        .filter(|m| m.file.as_deref().is_some_and(|f| f.ends_with(".gen.rs")))
        .collect();
    assert!(
        warnings.is_empty(),
        "generated code has warnings: {warnings:#?}"
    );
    // TLS nur mit ring, ohne aws-lc (C-Bibliothek, eigene Lizenzen).
    let lock = std::fs::read_to_string(dir.join("Cargo.lock")).unwrap();
    assert!(!lock.contains("name = \"aws-lc-sys\""), "aws-lc pulled in");

    let output = std::process::Command::new(report.executable.expect("binary"))
        .current_dir(&dir)
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains(&format!("started=127.0.0.1:{port}")),
        "{stdout}"
    );
    assert!(stdout.contains("open=true"), "{stdout}");
    assert!(stdout.contains("server_got=hallo clients=1"), "{stdout}");
    assert!(stdout.contains("ws=HALLO"), "{stdout}");
    assert!(stdout.contains("response=201 neu"), "{stdout}");
    assert!(stdout.contains("items=2"), "{stdout}");
    if std::env::var_os("FREYA_TDE_KEEP_DEMO").is_none() {
        let _ = std::fs::remove_dir_all(&dir);
    } else {
        println!("demo project: {}", dir.display());
    }
}
