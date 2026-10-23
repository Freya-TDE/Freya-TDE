# tdr-designer – Designer-Logik von Freya-TD

Alles, was der Formular-Designer tut, als Bibliothek ohne Oberfläche. Die Oberfläche
(`ide/`, RUF) und später eine KI rufen nur diese Funktionen auf und zeichnen das Ergebnis.
*Everything the form designer does, as a library without UI. The UI (`ide/`, RUF) and
later an AI only call these functions and draw the result.*

| Begriff / Concept | Typ / Type |
|---|---|
| Offenes Projekt, Undo/Redo, Speichern (Probelauf: `files()`) / open project | `Document` |
| Änderungsbefehle (serde/JSON) / editing commands | `Command`, `CommandError` |
| Theme-/Metrik-Tabellen pro Plattform (TOML in `themes/`) | `Theme` |
| Position jedes Widgets, Komponenten-Leiste / layout, component tray | `layout()`, `FormLayout` |
| Zeichenliste / display list | `render()`, `render_selection()`, `Primitive` |
| Was liegt unter der Maus / hit testing | `hit_test()`, `layout::drop_target()` |
| Palette und Objektinspektor als Daten | `palette()`, `properties()`, `events()`, `suggest_handler()` |
| Formular als SVG ansehen / view a form as SVG | `to_svg()` |

```rust
let mut doc = Document::new(ProjectFile::new("demo", "1.99", "gtk")?, TypeRegistry::with_builtins());
doc.apply(serde_json::from_str(r#"{"command": "add_form", "name": "main"}"#)?)?;
doc.apply(serde_json::from_str(
    r#"{"command": "add_widget", "form": "main", "parent": "window", "type": "core/Button"}"#)?)?;
let theme = Theme::for_platform(&Platform::Linux);
let form = doc.form(&ComponentName::new("main")?).unwrap();
let layout = layout(form, doc.registry(), &theme);
let picture = to_svg(&render(form, &layout, doc.registry(), &theme), layout.width, layout.height);
doc.undo();
```

```bash
cargo test -p tdr-designer
cargo run -p tdr-designer --example svg -- /tmp/form.svg macos   # Beispielformular als SVG
```

Optische Anpassungen sind Datenänderungen: `themes/linux.toml`, `themes/windows.toml`,
`themes/macos.toml` (eigene Tabellen per `Theme::parse`).
*Optical adjustments are data changes: edit the tables in `themes/`.*

## Lizenz / License

MIT OR Apache-2.0 – siehe / see `LICENSE-MIT`, `LICENSE-APACHE`.
