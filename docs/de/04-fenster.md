# 4 Die Fenster

[Inhalt](README.md) · zurück: [3 Erste Schritte](03-erste-schritte.md) · weiter: [5 Formulare und Komponenten](05-komponenten.md)

Freya TDE besteht wie Delphi 7 und Lazarus aus frei beweglichen Einzelfenstern – jedes lässt sich auf jeden
Bildschirm legen. Die Anordnung wird gespeichert (`<config>/freya-tde/desktop.toml`) und beim Start
wiederhergestellt; **Fenster → Standardanordnung** setzt sie zurück. Die ausführliche Bedienung jedes
Fensters steht in [`ide/README.md`](../../ide/README.md).

| Fenster | Kurz |
|---|---|
| **Hauptfenster** | Menü, Werkzeugleiste (Check, Clippy, Format, Test, Update, Doku, SQLx), Palette mit einem Reiter je Kategorie, Reiter „Snippets“, Statuszeile mit Git-Zweig |
| **Designer** | das Formular: wählen, platzieren, ziehen, Größe per Griff, Anker; Komponentenleiste unten für nicht-visuelle Komponenten; F12 wechselt zum Code |
| **Objektinspektor** | Eigenschaften und Ereignisse des gewählten Elements, Platzierung und Layout |
| **Struktur** | Formulare und Elemente als Baum |
| **Code** | ein Reiter je Datei (GtkSourceView): Hervorhebung, Faltung, Fehlermarken, Git-Änderungsmarken, Haltepunkte |
| **Meldungen** | Reiter Probleme, Ausgabe (Build, Programm, Git, rust-analyzer, Datenbank), Terminal (mehrere), Debug-Konsole |
| **Projekt** | Reiter Dateien (Projektordner als Baum) und Projekt-Manager (Haupt- und Unterprojekte) |
| **Quellcodeverwaltung** | Git wie in VS Code (siehe [Kapitel 11](11-git.md)) |
| **Debugger** | Variablen, Überwacht, Aufrufstapel, Threads, Haltepunkte |
| **Datenbank** | Schema-Browser für SQLx-Pools und Redis |
| **KI** | Gespräch mit der KI: oben die Auswahl der KI (Drop-down mit allen Profilen, jederzeit umschaltbar, je Projekt gemerkt) und was sie darf; Verlauf, Vorschläge mit Vorschau (Übernehmen, Ablehnen, Alle übernehmen), Skills, Eingabe (Strg+Enter) – siehe [Kapitel 13](13-automatisierung.md) |

## Struktur, Objektinspektor und KI zusammenlegen

Die drei Fenster haben oben einen Griff („⠿ Struktur“, „⠿ Objektinspektor“, „⠿ KI“). Zieht man den Griff des einen auf
das andere Fenster, werden beide **ein Fenster mit Reitern**. Zieht man den Griff ins Freie (neben jedes
Fenster), wird der Reiter wieder ein eigenes Fenster. Auch das bleibt in der Fensteranordnung gespeichert.

## Die Werkzeugleiste

| Knopf | Tut |
|---|---|
| Check, Clippy | `cargo check` / `cargo clippy`; Meldungen unter „Probleme“ und als Marken im Code |
| Format | `rustfmt` für jede eigene Datei – erzeugte `.gen.rs` bleiben unberührt |
| Test | `cargo test`, Ausgabe im Kanal „Build“ |
| Update | zeigt, was `cargo update` ändern würde, und fragt nach |
| Doku | `cargo doc --no-deps`, öffnet die Seite im Browser |
| SQLx | `cargo sqlx prepare` (nur bei SQLx-Projekten, URL aus dem ersten Pool) |

## Snippets

Der Reiter **Snippets** ganz rechts in der Palette hält gespeicherte Code-Stücke (Name, Kurzbeschreibung,
use-Zeile, Code):

- Die **Drop-Down** listet alle; beim Darüberfahren erscheint die Kurzbeschreibung, ein Eintrag wählen fügt
  ihn am Cursor im Code ein. Eine nicht leere use-Zeile wird an den use-Block der Datei angehängt.
- **Rechtsklick auf die Drop-Down:** Bearbeiten, Löschen, Auf Reiter ablegen, Neues Snippet.
- **Abgelegte Snippets** sind Knöpfe auf dem Reiter: Klick fügt ein, Rechtsklick löst oder bearbeitet.
- **Im Code-Editor:** Code markieren → Rechtsklick → „Als Snippet speichern“.

## Tastenkürzel

**Hilfe → Tastenkürzel** listet alle Kürzel je Fenster samt Erklärung. Die wichtigsten (wie Delphi/Lazarus):

| Kürzel | Tut |
|---|---|
| Strg+N / Strg+O / Strg+S | Neues Projekt / Öffnen / Speichern |
| Strg+, | Einstellungen |
| F12 | Designer ↔ Code |
| F11 | Objektinspektor |
| Strg+G | Code erzeugen |
| Strg+F9 | Bauen |
| F9 | Starten / Weiter (Debugger) |
| F7 / F8 / Umschalt+F8 | Hinein / Über / Heraus |
| F4 / F5 | Bis Cursor / Haltepunkt |
| Strg+Leertaste | Vervollständigen |
| Strg+Umschalt+C | Code vervollständigen (Methode, Feld, Trait) |
| Strg+F12 / Strg+Klick | Gehe zu Definition |
| Strg+Umschalt+G / B / M / U / T | Quellcodeverwaltung / Datenbank / Probleme / Ausgabe / Terminal |
| Strg+Umschalt+K | KI |

Die Kürzel des Hauptmenüs gelten in jedem Fenster; unter macOS ist Strg die Befehlstaste (⌘).
