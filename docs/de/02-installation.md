# 2 Installieren und bauen

[Inhalt](README.md) · zurück: [1 Was ist was?](01-ueberblick.md) · weiter: [3 Erste Schritte](03-erste-schritte.md)

Freya TDE gibt es derzeit nur als Quellcode; ein eigener Installer bzw. Launcher ist geplant. Gebaut wird
mit Cargo – auf dem System, für das das Programm gedacht ist (GTK braucht die Bibliotheken des Zielsystems,
ein Cross-Build ist nicht vorgesehen).

## Voraussetzungen

| Was | Wofür | Pflicht? |
|---|---|---|
| Rust ≥ 1.99 (Edition 2024) | alles | ja |
| GTK 4 (≥ 4.12) und GtkSourceView 5 mit Entwicklungsdateien, `pkg-config` | Oberfläche der IDE | ja |
| `git` | Submodule, Quellcodeverwaltung | ja |
| `rust-analyzer` | Vervollständigung, Live-Fehler, „Gehe zu Definition“, Code-Vervollständigung | empfohlen |
| `lldb-dap` (LLVM) oder `gdb` ≥ 14 | Debugger | empfohlen |
| `rustfmt`, `clippy` | Werkzeugleiste „Format“, „Clippy“ | empfohlen (`rustup component add rustfmt clippy`) |
| `gh` (GitHub-CLI) | „Commit & Create Pull Request“ | optional |
| WebKitGTK 4.1 | Zielprojekte mit dem WebView-Backend (Linux) | optional |
| Flutter (aktuelle stabile Version, Dart ≥ 3.9) | Zielprojekte mit dem Flutter-Backend | optional |
| `sqlx-cli` | Werkzeugleiste „SQLx“ (`cargo sqlx prepare`) | optional |

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

In der MSYS2-Shell „UCRT64“ (<https://www.msys2.org>):

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

## Quellen holen und bauen

RUF und der CT-Editor sind Submodule – beim Klonen gleich mitholen:

```bash
git clone --recurse-submodules https://github.com/Freya-TDE/Freya-TDE.git
cd Freya-TDE
cargo build --release -p freya-tde        # die IDE
cargo build --release -p freya-tde-db     # Datenbank-Helfer (für das Fenster „Datenbank“)
(cd CT-Editor && cargo build --release)   # CT-Editor (für Cargo.toml)
cargo run --release -p freya-tde          # starten
```

Wurde ohne Submodule geklont: `git submodule update --init`.

Freya TDE sucht seine Helfer neben dem eigenen Programm, im Repository (`target/…`) und im `PATH`; eigene
Pfade gehen über Umgebungsvariablen:

| Variable | Programm |
|---|---|
| `FREYA_TDE_CT_EDITOR` | CT-Editor |
| `FREYA_TDE_DB` | Datenbank-Helfer |
| `FREYA_TDE_DAP` | Debug-Adapter |
| `FREYA_TDE_GIT`, `FREYA_TDE_GH` | `git`, `gh` |
| `FREYA_TDE_FLUTTER` | `flutter` |
| `RUST_ANALYZER` | rust-analyzer |
| `FREYA_TDE_PACKS` | weitere Pack-Ordner (Pfadliste) |

## Linux: X11 statt Wayland

Freya TDE besteht aus frei beweglichen Einzelfenstern und stellt ihre Anordnung beim Start wieder her.
Wayland verbietet Programmen, Fenster selbst zu positionieren – deshalb startet Freya TDE unter Linux über
X11/XWayland (`GDK_BACKEND=x11`), wenn nichts anderes gesetzt ist. Der CT-Editor läuft nativ unter Wayland.

## Tests

```bash
cargo test --workspace                                                     # mit Bildschirm
env -u WAYLAND_DISPLAY xvfb-run -a -s "-screen 0 1920x1080x24" cargo test   # Linux ohne Bildschirm
```

Der Bildschirm von `xvfb-run` ist ohne Angabe nur 640 × 480 groß – zu klein für die Oberflächentests.
Mehr zu Tests unter Xvfb in [Kapitel 16](16-probleme.md#tests-ohne-bildschirm-xvfb).
