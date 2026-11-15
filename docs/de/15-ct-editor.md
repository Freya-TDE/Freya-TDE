# 15 CT-Editor

[Inhalt](README.md) · zurück: [14 RUF und RNCL](14-ruf.md) · weiter: [16 Bekannte Probleme und Grenzen](16-probleme.md)

Der **CT-Editor** (Cargo TOML Editor) bearbeitet `Cargo.toml`-Dateien visuell. Er ist ein eigenes Projekt
(Repository [Freya-TDE/CT-E](https://github.com/Freya-TDE/CT-E), in Freya-TDE als Submodul
`CT-Editor/`), gebaut mit RUF, und läuft auf Linux, Windows und macOS. Fertige Pakete für alle drei Systeme
gibt es auf der Release-Seite des Repositorys (zuletzt 0.3.1).

## Was er kann

| Registerkarte | Inhalt |
|---|---|
| Quelltext | die `Cargo.toml` mit Hervorhebung und Live-Prüfung |
| Abhängigkeiten | Suche auf crates.io (ab 3 Zeichen), Versionen, Features als Häkchen (vorbelegt mit den Default-Features des Crates), Ziel- und Build-Abhängigkeiten |
| Paket | Name, Version, Edition, Rust-Version, Beschreibung, Lizenz … |
| Features | eigene Features und was sie einschalten |
| Erweitert | jeder weitere Abschnitt: `[lib]`, `[[bin]]`, `[profile.*]`, `[workspace]` … |

Er arbeitet **formatierungserhaltend** (Kommentare und Reihenfolge bleiben), prüft mit `cargo metadata` und
spricht Deutsch und Englisch (weitere Sprachen als TOML-Dateien nachrüstbar).

## Eigenständig oder in Freya TDE

- **Eigenständig** hat er ein Menü **Datei** mit Öffnen (Strg+O), Speichern (Strg+S), Speichern unter
  (Strg+Umschalt+S; eine neue, nie gespeicherte Datei fragt beim Speichern nach dem Ort), Sprache,
  Darstellung (System, Hell, Dunkel) und Beenden. Sprache und Darstellung merkt er sich; die Dateidialoge
  beginnen im Projektordner.
- **In Freya TDE:** Projektfenster → Rechtsklick auf `Cargo.toml` → „Mit dem CT-Editor öffnen“. Freya TDE
  startet ihn mit `--embedded` (ohne Datei-Menü) und reicht Sprache und Darstellung aus den eigenen
  Einstellungen weiter.

```bash
ct-editor --path /pfad/zum/projekt                          # eigenständig
ct-editor --embedded --lang de --theme dark --path …        # so ruft Freya TDE ihn auf
ct-editor --help                                            # alle Optionen
```

## Erster Start der fertigen Pakete

Die Pakete sind nicht von Apple notarisiert bzw. unter Windows nicht signiert (Code-Signing ist für später
geplant):

- **macOS:** Seit macOS 15 genügt Rechtsklick → Öffnen nicht mehr. Nach der Meldung „Fertig“ wählen, dann
  *Systemeinstellungen → Datenschutz & Sicherheit → „Dennoch öffnen“* (einmalig) – oder im Terminal
  `xattr -dr com.apple.quarantine CT-Editor.app`.
- **Windows:** SmartScreen warnt: *Weitere Informationen → Trotzdem ausführen* (oder vor dem Entpacken in den
  Eigenschaften der ZIP „Zulassen“ ankreuzen).

## Weiterlesen

Das Handbuch des CT-Editors (Deutsch und Englisch) mit allen Optionen, Abläufen, Tastenkürzeln, Sprachen und
dem Release-Workflow: [CT-E/docs/de](https://github.com/Freya-TDE/CT-E/blob/main/docs/de/README.md).

**Lizenz:** MIT OR Apache-2.0 nach Wahl.
