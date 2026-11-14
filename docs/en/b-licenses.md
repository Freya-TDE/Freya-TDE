# Appendix B – Licenses

[Contents](README.md) · back: [Appendix A – Glossary](a-glossary.md) · next: [Appendix C – How it was made](c-history.md)

## Freya TDE, RUF and CT-Editor

All three are licensed under **MIT** or **Apache-2.0** at your option (rights holder: Dragon Software).
Unless explicitly stated otherwise, contributions are licensed under the same two licenses.

## Generated code belongs to you

What Freya TDE generates for your project belongs to you. Each pack states the license of its generated code
in its manifest (`generated-code-license`); all bundled packs generate code under MIT OR Apache-2.0. The
license of the libraries your project then uses applies to those libraries – which is why for **Slint** the
new-project wizard shows the license notice before creating the project.

## Dependencies of the IDE

Free licenses only, no paid dependencies:

| Library | For | License |
|---|---|---|
| GTK 4, GtkSourceView 5 | user interface, code editor | LGPL-2.1+ (dynamically linked, shipped) |
| gtk4-rs, sourceview5 | Rust bindings | MIT |
| serde, serde_json, toml, toml_edit, ron, indexmap, thiserror | data and file formats | MIT OR Apache-2.0 |
| syn, proc-macro2, prettyplease | checking and formatting generated code | MIT OR Apache-2.0 |
| minijinja | code templates | Apache-2.0 |
| portable-pty, vt100 | terminal | MIT |
| trash | recycle bin | MIT |
| keyring | API keys in the keyring | MIT OR Apache-2.0 |
| sqlx, tokio (helper `freya-tde-db`) | database access at design time | MIT OR Apache-2.0 / MIT |
| redis-rs (helper) | Redis/Valkey | BSD-3-Clause |

External programs (not linked, only called): `git`, `gh`, `rust-analyzer`, `lldb-dap`/`gdb`, `rustfmt`,
`cargo`, `flutter`.

## Third-party parts bundled in packs

| Part | Where | License |
|---|---|---|
| Pico CSS 2.1.1 | `widgets/webview/assets/` | MIT |
| cargokit (from flutter_rust_bridge 2.13.0) | `widgets/flutter/cargokit/` | MIT |
| flutter_rust_bridge 2.13 bindings | `widgets/flutter/bridge/` | MIT |
