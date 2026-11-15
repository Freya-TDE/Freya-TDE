# 16 Known problems and limits

[Contents](README.md) · back: [15 CT-Editor](15-ct-editor.md) · next: [Appendix A – Glossary](a-glossary.md)

This chapter collects the UI problems we found during development – with cause and solution or
workaround. Some come from the toolkits themselves, some from the environment.

## GTK and the systems

| Problem | Cause | Solution / workaround |
|---|---|---|
| Windows cannot be positioned on Wayland, the arrangement is lost | Wayland does not let programs place their own windows | on Linux Freya TDE starts through X11/XWayland (`GDK_BACKEND=x11`); the CT-Editor does not need that |
| GTK 4 cannot position windows at all | by design (the window manager decides) | RUF sets the position natively: Windows `SetWindowPos`, macOS `setFrameOrigin`, Linux X11 |
| `Gdk-CRITICAL: gdk_surface_thaw_updates: assertion …` (macOS) | a bug in GTK's macOS part when opening drop-downs (also in `gtk4-demo`) | harmless; RUF hides the message |
| `Gtk-WARNING: GtkGizmo … (slider) reported min width -2` | a bug in KDE's Breeze theme (scroll bars) | harmless; RUF hides the message |
| `MESA-EGL: … failed to create dri2 screen`, `Gdk-WARNING: Vulkan: … Failed to detect any valid GPUs` (NVIDIA) | after a driver update the new driver is not loaded yet | restart the computer |
| A widget does not get smaller than its content (Linux) | GTK never goes below the natural size | shorten the text, turn on wrapping or plan more space |
| Right-click on an entry of an **open** drop-down does not work | GTK's DropDown does not offer it | the closed list's context menu with a submenu per entry (as for the snippets) |
| Dragging one window into another does not exist | GTK and the window managers have no docking | a grip in the window, dragged and dropped onto the other window ([chapter 4](04-windows.md#combining-structure-and-object-inspector)) |
| Empty window in WebView programs (Linux, some drivers) | WebKitGTK's DMA-BUF renderer | the generated program sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` unless set |

## Tools around the code

| Problem | Cause | Solution / workaround |
|---|---|---|
| `cargo fmt` fails or wants to change generated files | `main.rs` includes `app.gen.rs` as a module; `cargo fmt` follows modules into the write-protected `.gen.rs` | "Format" formats each of your own files separately through `rustfmt` (via stdin) |
| rust-analyzer suggests writing a missing field into the generated struct (with type `&mut _`) | it does not know the split between generated and own code | code completion creates the field in the form property `state` and suggests the type itself |
| rust-analyzer generates methods with `-> _` and `todo!()` | that is how its assist works | Freya TDE turns it into `&mut self`, no `-> _`, an empty body |
| In another editor (VS Code) old errors remain although the code builds | the rust-analyzer there saw an intermediate state and did not check again | "rust-analyzer: Restart server" there, or save the file once |

## Tests without a display (Xvfb)

| Problem | Cause | Solution / workaround |
|---|---|---|
| `GTK konnte nicht initialisiert werden` (GTK could not be initialised) | no graphical session | `xvfb-run -a …`; in a Wayland session also `env -u WAYLAND_DISPLAY` |
| A UI test checks a size and fails ("Hinweis wächst mit dem Fenster") | without options `xvfb-run` has a screen of only 640 × 480; bigger windows are clamped | `xvfb-run -a -s "-screen 0 1920x1080x24" …` |
| A test hangs without CPU load | a modal dialog (e.g. "unsaved changes") waits invisibly | in tests `ruf::dialog::set_automatic(Some(true))`: dialogs are answered at once, the question goes to stderr |
| `MESA-EGL: warning: DRI3 error` when starting under Xvfb | Xvfb has no graphics acceleration | harmless; the tests draw in software (`GSK_RENDERER=cairo`) |
| The window of an egui program appears late under Xvfb | software OpenGL takes longer after a long build | the test waits up to 40 s and shows the program's output |
| Xvfb does not start inside a sandbox (no `/tmp/.X11-unix/X99`) | the sandbox protects `/tmp/.X11-unix` | run the tests outside the sandbox |

## Limits of the UI backends

Each backend does what its toolkit can. The most important limits (complete in
[`widgets/README.md`](../../widgets/README.md)):

| Backend | Limits |
|---|---|
| GTK 4 | no maximum window size (GTK 4 does not have one); the system decides the window position |
| egui | the first form is shown |
| iced | no main menu, spin box or image (iced does not have them); `enabled` on containers has no effect; in the DataGrid **Esc** is not bound (iced does not pass the key on from a text input) – cancel through `<grid>_cancel()` |
| Slint | standard widgets without colour properties; the table (`StandardTableView`) cannot edit – an edit row with one field per column appears below it |
| Flutter | no debugger; the window first appears at the runner's size before `window_manager` changes it; the first `flutter build` compiles the Rust library a second time (cargokit); `window_manager` 0.6/0.7 need Flutter ≥ 3.47/3.50 and a new API, hence 0.5 |
| WebView | the first form is shown; no background pictures; menu shortcuts are not bound |

## Shipping

| Problem | Cause | Solution / workaround |
|---|---|---|
| macOS does not open the program ("not verified") | not notarized by Apple | *System Settings → Privacy & Security → "Open Anyway"* or `xattr -dr com.apple.quarantine …` |
| Windows SmartScreen warns | not signed | *More info → Run anyway* |
| The program does not start on Windows/macOS, a DLL/dylib is missing | the GTK runtime was not shipped | bundle it with `RUF/tools/bundle-windows.sh` or `bundle-macos.sh` |
| No keyring available (e.g. Linux without Secret Service) | the API key cannot go into the keyring | Freya TDE stores it in `settings.toml` (readable only by the user) |

More build and runtime problems of RUF:
[RUF – troubleshooting](https://github.com/Freya-TDE/RUF/blob/main/docs/en/troubleshooting.md).
