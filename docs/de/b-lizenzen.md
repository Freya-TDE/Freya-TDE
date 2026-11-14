# Anhang B – Lizenzen

[Inhalt](README.md) · zurück: [Anhang A – Glossar](a-glossar.md) · weiter: [Anhang C – Entstehung](c-entstehung.md)

## Freya TDE, RUF und CT-Editor

Alle drei stehen nach Wahl unter **MIT** oder **Apache-2.0** (Rechteinhaber: Dragon Software). Beiträge
werden, sofern nicht ausdrücklich anders angegeben, unter denselben beiden Lizenzen eingebracht.

## Erzeugter Code gehört Ihnen

Was Freya TDE für Ihr Projekt erzeugt, gehört Ihnen. Jedes Pack nennt die Lizenz seines erzeugten Codes im
Manifest (`generated-code-license`); alle mitgelieferten Packs erzeugen Code unter MIT OR Apache-2.0. Die
Lizenz der Bibliotheken, die Ihr Projekt dann benutzt, gilt für diese Bibliotheken – bei **Slint** zeigt der
Neu-Assistent deshalb vor dem Anlegen den Lizenzhinweis.

## Abhängigkeiten der IDE

Nur freie Lizenzen, keine kostenpflichtigen Abhängigkeiten:

| Bibliothek | Wofür | Lizenz |
|---|---|---|
| GTK 4, GtkSourceView 5 | Oberfläche, Code-Editor | LGPL-2.1+ (dynamisch gelinkt, wird mitgeliefert) |
| gtk4-rs, sourceview5 | Rust-Bindungen | MIT |
| serde, serde_json, toml, toml_edit, ron, indexmap, thiserror | Daten und Dateiformate | MIT OR Apache-2.0 |
| syn, proc-macro2, prettyplease | erzeugten Code prüfen und formatieren | MIT OR Apache-2.0 |
| minijinja | Code-Vorlagen | Apache-2.0 |
| portable-pty, vt100 | Terminal | MIT |
| trash | Papierkorb | MIT |
| keyring | API-Keys im Schlüsselbund | MIT OR Apache-2.0 |
| sqlx, tokio (Helfer `freya-tde-db`) | Datenbank-Zugriff zur Entwurfszeit | MIT OR Apache-2.0 / MIT |
| redis-rs (Helfer) | Redis/Valkey | BSD-3-Clause |

Externe Programme (nicht eingebunden, nur aufgerufen): `git`, `gh`, `rust-analyzer`, `lldb-dap`/`gdb`,
`rustfmt`, `cargo`, `flutter`.

## Mitgelieferte Dritt-Teile in Packs

| Teil | Wo | Lizenz |
|---|---|---|
| Pico CSS 2.1.1 | `widgets/webview/assets/` | MIT |
| cargokit (aus flutter_rust_bridge 2.13.0) | `widgets/flutter/cargokit/` | MIT |
| Bindungen von flutter_rust_bridge 2.13 | `widgets/flutter/bridge/` | MIT |
