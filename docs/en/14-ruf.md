# 14 RUF and RNCL

[Contents](README.md) · back: [13 Automation and AI](13-automation.md) · next: [15 CT-Editor](15-ct-editor.md)

**RUF** (Rust native UI Framework) is the UI framework that Freya TDE's user interface and the CT-Editor run
on. It is a project of its own (repository [Freya-TDE/RUF](https://github.com/Freya-TDE/RUF),
a submodule `RUF/` in Freya-TDE) and can be used for any desktop program in Rust.

## What RUF is

- **One backend for all systems:** GTK 4 (through gtk4-rs) on Linux, Windows and macOS – the same code
  everywhere. Releases for Windows and macOS ship the GTK runtime (`tools/bundle-windows.sh`,
  `tools/bundle-macos.sh`). The backend sits behind a trait, so that another toolkit would be possible later.
- **Three layers:** the system layer (backend), a platform-independent core (signals, lifetime, state) and
  the logic layer of the components.
- **RNCL** (Rust native Component Library) provides the components on top: `RnclWindow`, `RnclButton`,
  `RnclTextField`, `RnclComboBox`, `RnclListBox`, `RnclTreeView`, `RnclTabView`, `RnclScrollView`,
  `RnclCodeEditor` (GtkSourceView with folding, marks, breakpoints), `RnclCanvas` (drawing surface) and more.
  Components of your own follow the same pattern and can be registered.

```rust
use rncl::prelude::*;

fn main() {
    let app = Application::new();
    let window = RnclWindow::new("Hello RUF");
    window.set_size(320, 140);
    let button = RnclButton::new(&window, "Click me");
    button.set_position(20, 50);
    let b = button.clone();
    button.on_clicked(move || b.set_text("Thanks!"));
    window.show();
    std::process::exit(app.exec());
}
```

## What RUF learned for Freya TDE

Much of RUF was created because Freya TDE needed it – and is now available to every RUF program:

| Capability | Used in Freya TDE for |
|---|---|
| Positioning windows natively (`SetWindowPos`, `setFrameOrigin`, X11) | restoring the window arrangement – GTK 4 cannot do it itself |
| Anchors (`Widget::set_anchors`) | windows whose content grows along |
| Code editor with folding, error, Git and breakpoint marks, extra context menu | the code window |
| Drag and drop (`set_drag_text`, `on_drop`, also on windows; `on_drag_cancelled`) | palette → designer, combining windows as tabs |
| Right-click on any widget (`on_context_menu`), context menus | snippets, project tree, Git |
| A tooltip per entry of a drop-down (`set_item_tooltips`) | the snippet list |
| Text input through input methods (`on_text_input`) | the terminal |
| Setting the colour scheme (`Application::set_color_scheme`) | appearance light/dark |
| Start folder of file dialogs (`dialog::set_default_folder`) | the working directory |
| Answering dialogs automatically (`dialog::set_automatic`) | tests without a display |
| Program icon (`Application::set_icon`) | title bar, task bar |

## Further reading

RUF's developer documentation (German and English) with getting started, architecture, component reference,
custom components, designer integration, release and troubleshooting:
[RUF/docs/en](https://github.com/Freya-TDE/RUF/blob/main/docs/en/README.md).

**License:** MIT OR Apache-2.0 at your option; programs built with RUF may use any license.
