# widgets/

Visuelle Widgets und Backend-Paletten, aufbauend auf `tdr-core` (der gemeinsame Widget-Kern liegt in `core/src/builtin.rs`).
*Visual widgets and backend palettes, built on `tdr-core` (the common widget core lives in `core/src/builtin.rs`).*

## Backend-Pack / backend pack

Ein UI-Backend ist ein Ordner `widgets/<backend>/` mit einer `pack.toml` (`kind = "backend"`):
eigene Widgets (`[[component]]` mit `kind = "widget"`/`"container"`), Code für die Core-Widgets
(`[[extend]]` mit `[extend.codegen]`) und der Descriptor `[backend]`:
*A UI backend is a directory `widgets/<backend>/` with a `pack.toml` (`kind = "backend"`):*

```toml
[backend]
event-loop = "main-thread"      # oder "async"
absolute-positioning = false
cargo = [{ crate = "eframe", version = "…" }]
form = """impl {{ form.struct }} { pub fn ui(&mut self, …) { … self.{{ w.events.on_click }}(); … } }"""
run = """eframe::run_native(…)?; Ok(())"""
```

Kontext der Templates und Einzelheiten: Modul-Doku von `tdr_core::codegen` und `tdr_core::pack`.
*Template context and details: module docs of `tdr_core::codegen` and `tdr_core::pack`.*
