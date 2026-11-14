# 14 RUF und RNCL

[Inhalt](README.md) · zurück: [13 Automatisierung und KI](13-automatisierung.md) · weiter: [15 CT-Editor](15-ct-editor.md)

**RUF** (Rust native UI Framework) ist das UI-Framework, auf dem die Oberfläche von Freya TDE und der
CT-Editor laufen. Es ist ein eigenes Projekt (Repository
[Dragon-InterActive/RUF](https://github.com/Dragon-InterActive/RUF), in Freya-TDE als Submodul `RUF/`) und
lässt sich für jedes Desktop-Programm in Rust nutzen.

## Was RUF ist

- **Ein Backend für alle Systeme:** GTK 4 (über gtk4-rs) auf Linux, Windows und macOS – derselbe Code überall.
  Releases für Windows und macOS liefern die GTK-Laufzeit mit (`tools/bundle-windows.sh`,
  `tools/bundle-macos.sh`). Das Backend steht hinter einem Trait, damit später ein anderes Toolkit möglich
  wäre.
- **Drei Ebenen:** System-Ebene (Backend), ein plattformunabhängiger Kern (Signale, Lebensdauer, Zustand) und
  die Logik-Ebene der Komponenten.
- **RNCL** (Rust native Component Library) liefert darauf die Komponenten: `RnclWindow`, `RnclButton`,
  `RnclTextField`, `RnclComboBox`, `RnclListBox`, `RnclTreeView`, `RnclTabView`, `RnclScrollView`,
  `RnclCodeEditor` (GtkSourceView mit Faltung, Marken, Haltepunkten), `RnclCanvas` (Zeichenfläche) und mehr.
  Eigene Komponenten lassen sich nach demselben Muster schreiben und registrieren.

```rust
use rncl::prelude::*;

fn main() {
    let app = Application::new();
    let window = RnclWindow::new("Hallo RUF");
    window.set_size(320, 140);
    let button = RnclButton::new(&window, "Klick mich");
    button.set_position(20, 50);
    let b = button.clone();
    button.on_clicked(move || b.set_text("Danke!"));
    window.show();
    std::process::exit(app.exec());
}
```

## Was RUF für Freya TDE dazugelernt hat

Vieles in RUF ist entstanden, weil Freya TDE es brauchte – und steht jetzt jedem RUF-Programm zur Verfügung:

| Fähigkeit | Wofür in Freya TDE |
|---|---|
| Fenster nativ positionieren (`SetWindowPos`, `setFrameOrigin`, X11) | Fensteranordnung wiederherstellen – GTK 4 kann das selbst nicht |
| Anker (`Widget::set_anchors`) | Fenster, deren Inhalt mitwächst |
| Code-Editor mit Faltung, Fehler-, Git- und Haltepunkt-Marken, Zusatz-Kontextmenü | Code-Fenster |
| Ziehen und Ablegen (`set_drag_text`, `on_drop`, auch auf Fenstern; `on_drag_cancelled`) | Palette → Designer, Fenster als Reiter zusammenlegen |
| Rechtsklick auf jedes Widget (`on_context_menu`), Kontextmenüs | Snippets, Projektbaum, Git |
| Tooltip je Eintrag einer Auswahlliste (`set_item_tooltips`) | Snippet-Liste |
| Texteingabe über Eingabemethoden (`on_text_input`) | Terminal |
| Farbschema setzen (`Application::set_color_scheme`) | Darstellung Hell/Dunkel |
| Startordner der Dateidialoge (`dialog::set_default_folder`) | Arbeitsverzeichnis |
| Dialoge automatisch beantworten (`dialog::set_automatic`) | Tests ohne Bildschirm |
| Programmsymbol (`Application::set_icon`) | Titelleiste, Taskleiste |

## Weiterlesen

Die Entwicklerdoku von RUF (Deutsch und Englisch) mit Erste Schritte, Architektur, Komponenten-Referenz,
eigenen Komponenten, Designer-Anbindung, Release und Fehlerbehebung:
[RUF/docs/de](https://github.com/Dragon-InterActive/RUF/blob/main/docs/de/README.md).

**Lizenz:** MIT OR Apache-2.0 nach Wahl; Programme, die mit RUF gebaut werden, dürfen unter beliebiger Lizenz
stehen.
