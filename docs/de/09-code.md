# 9 Erzeugter und eigener Code

[Inhalt](README.md) · zurück: [8 Datenbanken und Datenbindung](08-daten.md) · weiter: [10 Bauen, Starten, Debuggen](10-bauen.md)

Freya TDE trennt streng, was es erzeugt, und was Ihnen gehört. Nichts, was Sie schreiben, wird überschrieben.

## Wem welche Datei gehört

| Datei | Gehört | Verhalten |
|---|---|---|
| `src/main.rs` (Programm) bzw. `src/lib.rs` (Bibliothek) | Ihnen | einmal angelegt, danach nie angefasst |
| `src/<formular>.rs` | Ihnen | einmal angelegt (`include!("<formular>.gen.rs")` und Handler); fehlende Handler-Stubs werden als neuer `impl`-Block **angehängt** |
| `src/app.gen.rs`, `src/<formular>.gen.rs` | Freya TDE | bei jedem Lauf neu geschrieben, schreibgeschützt, Prüfsumme im Kopf |
| `Cargo.toml` | Ihnen | wird zusammengeführt (`toml_edit`): fehlende Abhängigkeiten und Features kommen dazu, nichts wird entfernt, Ihre Versionen bleiben |
| `src/<formular>.ftdr`, `<projekt>.ptdr` | Freya TDE (über den Designer) | RON, lesbar und versionierbar |

Wird eine `.gen.rs` von außen geändert, meldet Freya TDE das und schreibt sie neu; eine gleichnamige Datei,
die nie erzeugt wurde, wird nicht überschrieben. Im Projektfenster lassen sich die `.gen.rs` ausblenden.

„Code erzeugen“ (Strg+G) ist zweistufig: Erst entsteht eine Vorschau (welche Dateien neu, geändert,
unverändert, welche Meldungen), dann wird geschrieben – nie bei Fehlern und nie über eine zwischenzeitlich
geänderte Datei.

## Handler

Leere Handler-Stubs behalten lesbare Parameternamen (`fn button1_on_click(&mut self)`,
`fn grid_saving(&mut self, row: &GridRow, inserted: bool) -> bool`); die Warnung „unused variable“ wird dafür
in Kauf genommen.

## Code-Vervollständigung wie die Lazarus CodeTools

**Strg+Umschalt+C** im Code-Fenster legt an, was an der Cursorstelle fehlt – gelöscht wird nichts:

| An der Stelle steht … | Ergebnis |
|---|---|
| `self.speichern(&kunde, 3)`, die Methode fehlt | `fn speichern(&mut self, kunde: &Kunde, arg: i32) {}` – Typen von rust-analyzer, eingefügt hinter der aufrufenden Methode (oder alphabetisch / am Ende, siehe Einstellungen) |
| `self.zaehler += 1`, das Feld fehlt | ein eigenes Feld des Formulars (`state`), Typ vorgeschlagen und im Dialog bestätigt |
| `impl Trait for Typ {}` | die fehlenden Trait-Methoden |

## Snippets

Gespeicherte Code-Stücke mit Name, Kurzbeschreibung, use-Zeile und Code (Reiter „Snippets“ der Palette,
[Kapitel 4](04-fenster.md#snippets)). Beim Einfügen wird die use-Zeile an den use-Block angehängt, wenn sie
fehlt. Gespeichert in `<config>/freya-tde/snippets.toml`.

## Formatieren

Die Werkzeugleiste „Format“ formatiert jede eigene Datei mit `rustfmt`. `cargo fmt` wird bewusst nicht
benutzt: Es würde dem Modul `app` in die schreibgeschützte `app.gen.rs` folgen.
