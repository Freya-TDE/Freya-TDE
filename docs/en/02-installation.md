# 2 Installing and building

[Contents](README.md) · back: [1 What is what?](01-overview.md) · next: [3 First steps](03-first-steps.md)

Freya TDE is currently available as source code only; an installer/launcher of its own is planned. It is
built with Cargo – on the system the program is meant for (GTK needs the target system's libraries; cross
builds are not intended).

## Requirements

| What | For | Required? |
|---|---|---|
| Rust ≥ 1.99 (edition 2024) | everything | yes |
| GTK 4 (≥ 4.12) and GtkSourceView 5 with development files, `pkg-config` | the IDE's UI | yes |
| `git` | submodules, source control | yes |
| `rust-analyzer` | completion, live errors, go to definition, code completion | recommended |
| `lldb-dap` (LLVM) or `gdb` ≥ 14 | debugger | recommended |
| `rustfmt`, `clippy` | tool bar "Format", "Clippy" | recommended (`rustup component add rustfmt clippy`) |
| `gh` (GitHub CLI) | "Commit & Create Pull Request" | optional |
| WebKitGTK 4.1 | target projects with the WebView backend (Linux) | optional |
| Flutter (current stable, Dart ≥ 3.9) | target projects with the Flutter backend | optional |
| `sqlx-cli` | tool bar "SQLx" (`cargo sqlx prepare`) | optional |

### Linux

```bash
# Arch / CachyOS
sudo pacman -S gtk4 gtksourceview5 rust-analyzer lldb git
# Debian / Ubuntu (≥ 24.04)
sudo apt install libgtk-4-dev libgtksourceview-5-dev pkg-config git lldb
# Fedora
sudo dnf install gtk4-devel gtksourceview5-devel git lldb
```

### Windows

In the MSYS2 shell "UCRT64" (<https://www.msys2.org>):

```bash
pacman -S --needed mingw-w64-ucrt-x86_64-gtk4 mingw-w64-ucrt-x86_64-gtksourceview5 \
                   mingw-w64-ucrt-x86_64-pkgconf mingw-w64-ucrt-x86_64-gcc git
rustup target add x86_64-pc-windows-gnu
```

### macOS

```bash
xcode-select --install
brew install gtk4 gtksourceview5
```

## Getting the sources and building

RUF and the CT-Editor are submodules – fetch them while cloning:

```bash
git clone --recurse-submodules https://github.com/Freya-TDE/Freya-TDE.git
cd Freya-TDE
cargo build --release -p freya-tde        # the IDE
cargo build --release -p freya-tde-db     # database helper (for the "Database" window)
(cd CT-Editor && cargo build --release)   # CT-Editor (for Cargo.toml)
cargo run --release -p freya-tde          # start
```

Cloned without submodules: `git submodule update --init`.

Freya TDE looks for its helpers next to its own program, in the repository (`target/…`) and in `PATH`;
other locations are set with environment variables:

| Variable | Program |
|---|---|
| `FREYA_TDE_CT_EDITOR` | CT-Editor |
| `FREYA_TDE_DB` | database helper |
| `FREYA_TDE_DAP` | debug adapter |
| `FREYA_TDE_GIT`, `FREYA_TDE_GH` | `git`, `gh` |
| `FREYA_TDE_FLUTTER` | `flutter` |
| `RUST_ANALYZER` | rust-analyzer |
| `FREYA_TDE_PACKS` | further pack folders (path list) |

## Linux: X11 instead of Wayland

Freya TDE consists of free-floating windows and restores their arrangement on start. Wayland does not let
programs position their windows – so on Linux Freya TDE starts through X11/XWayland (`GDK_BACKEND=x11`)
unless something else is set. The CT-Editor runs natively on Wayland.

## Tests

```bash
cargo test --workspace                                                     # with a display
env -u WAYLAND_DISPLAY xvfb-run -a -s "-screen 0 1920x1080x24" cargo test   # Linux without a display
```

Without options, the screen of `xvfb-run` is only 640 × 480 – too small for the UI tests. More on tests
under Xvfb in [chapter 16](16-problems.md#tests-without-a-display-xvfb).
