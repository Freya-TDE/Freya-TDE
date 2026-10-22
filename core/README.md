# tdr-core – Komponentenmodell von Freya-TD

Das neutrale Formularmodell, auf dem alles Weitere aufbaut (Abhängigkeitsgraph,
Komponenten-Packs, Codegen, Vorschau, Designer). Kennt kein UI-Toolkit.
*The neutral form model everything else builds on. Knows no UI toolkit.*

| Begriff / Concept | Typ / Type |
|---|---|
| Nicht-visuelle Komponente (Pool, Timer, Router …) | `Component` |
| Visuelle Komponente: Komponente + Layout + Kinder | `Widget` |
| Einheitlicher Zugriff auf beide / uniform access | Trait `Element` |
| Formular bzw. Datenmodul (`.ftdr`, RON) | `Form` (Datenmodul = ohne Widgets, Service-Container) |
| Verweis, auch formularübergreifend / reference | `ComponentPath` (`db_pool`, `services::db_pool`) |
| Typbeschreibung / type description | `ComponentType`, `TypeRef` (`core/Button`, `backend:egui/Slider`, `pack:sqlx/Pool`) |
| Property-Typsystem | `PropertyType`, `Value`, `PropertyValue` (mit Plattform-Überschreibungen) |
| Events → Handler-Stubs | `EventDef`, `HandlerName` |
| Provides/Requires | `Capability`, `Provide`, `Require`, `Condition` |
| Alle bekannten Typen / all known types | `TypeRegistry` (+ Backend-Erweiterungen) |
| Prüfung eines Formulars / form validation | `validate()`, `Diagnostic` |
| Abhängigkeitsgraph: Requires, Zyklen, Init-Reihenfolge, Cargo / dependency graph | `graph::analyze`, `graph::Graph` |
| Umbenennen/Entfernen über alle Formulare / editing across forms | `project` |
| Packs aus `pack.toml` (TOML) / packs from `pack.toml` | `pack::Pack`, `pack::load_all` |
| Projektdatei (`.ptdr`, RON) / project file | `ProjectFile` |
| Codegen: Probelauf, dann schreiben / dry run, then write | `codegen::generate`, `codegen::write` |
| Gemeinsamer Widget-Kern / common widget core | `builtin`: Form, Container, Label, Button, Edit, CheckBox |

```rust
use tdr_core::*;

let name = |n: &str| ComponentName::new(n).unwrap();
let mut form = Form::new(name("customers"), "egui", Widget::new(name("main_form"), builtin::form()));
// Datenmodul (Service-Container) / data module
let services = Form::data_module(name("services"), "egui");
form.add_widget(
    &name("main_form"),
    Widget::from_component(
        Component::new(name("ok_button"), builtin::button())
            .with("caption", PropertyValue::new("OK").on(Platform::MacOs, "Okay"))
            .on("on_click", HandlerName::new("ok_clicked").unwrap()),
    ),
)?;
let registry = TypeRegistry::with_builtins();
assert!(form.validate(&ValidationContext::new(&registry)).is_empty());
std::fs::write("main_form.ftdr", form.to_ron()?)?;

// Projektweit: Requires, Zyklen, Initialisierungsreihenfolge, Cargo-Abhängigkeiten
let forms = [services, form];
let analysis = graph::analyze(&forms, &registry);
for d in &analysis.diagnostics { eprintln!("{d}"); }
println!("init order: {:?}", analysis.order);

// Codegen: erst Probelauf (nichts wird geschrieben), dann schreiben
let project = ProjectFile::new("crm", "1.99", "egui")?;
let dir = std::path::PathBuf::from("crm");
let generation = codegen::generate(&project, &forms, &registry, &dir);
for file in generation.changed() { println!("{:?} {}", file.change, file.path); }
codegen::write(&dir, &generation)?;   // verweigert bei Fehlern / refuses on errors
```

Erzeugtes Projekt / generated project:

| Datei / file | Gehört / owner | Inhalt / content |
|---|---|---|
| `Cargo.toml` | Nutzer / user | Abhängigkeiten werden ergänzt, nichts entfernt / dependencies merged in |
| `src/main.rs` | Nutzer / user | einmal angelegt: `main()`, `started(app)` |
| `src/app.gen.rs` | Generator | `App`, `App::init` in Abhängigkeitsreihenfolge, `run()` |
| `src/<form>.gen.rs` | Generator | Struct des Formulars, ein Feld je Komponente |
| `src/<form>.rs` | Nutzer / user | einmal angelegt, danach nur fehlende Handler-Stubs angehängt |

Alle Ergebnisse und Diagnosen sind serialisierbar (`serde`), Diagnosen tragen einen
stabilen `code` – für Werkzeuge und KI-Anbindung.
*All results and diagnostics serialize; diagnostics carry a stable `code` – for tools and AI.*

```bash
cargo test -p tdr-core
cargo run -p tdr-core --example sample_form    # Beispiel-.ftdr ausgeben / print a sample .ftdr
cargo run -p tdr-core --example check_packs -- components   # Packs prüfen / check packs
cargo run -p tdr-core --example generate -- /tmp/crm          # Codegen-Probelauf als JSON / dry run as JSON
cargo run -p tdr-core --example generate -- /tmp/crm --write  # Beispielprojekt schreiben / write sample project
```

## Lizenz / License

MIT OR Apache-2.0 – siehe / see `LICENSE-MIT`, `LICENSE-APACHE`.
