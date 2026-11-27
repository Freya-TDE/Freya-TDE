# Freya TDE – das Handbuch

🇬🇧 [English version](../en/README.md)

Freya TDE ist eine visuelle Entwicklungsumgebung für Rust im Stil von Delphi und Lazarus:
Formulare im Designer zusammenstellen, Komponenten verdrahten, Ereignisse in Rust schreiben – und das
Ergebnis für das UI-Toolkit erzeugen, das man möchte. Dieses Handbuch ist wie ein Buch aufgebaut: erst der
Überblick, dann Freya TDE selbst, dann die beiden Bausteine RUF und CT-Editor, zum Schluss alles über
bekannte Probleme und Grenzen.

## Inhalt

**Teil I – Überblick**

1. [Was ist was?](01-ueberblick.md) – Freya TDE, RUF und RNCL, CT-Editor, der Datenbank-Helfer
2. [Installieren und bauen](02-installation.md) – Voraussetzungen je System, aus den Quellen bauen

**Teil II – Freya TDE**

3. [Erste Schritte](03-erste-schritte.md) – vom Neu-Assistenten zum laufenden Programm
4. [Die Fenster](04-fenster.md) – Hauptfenster, Designer, Objektinspektor, Code, Meldungen …
5. [Formulare und Komponenten](05-komponenten.md) – das Modell hinter dem Designer
6. [UI-Backends](06-backends.md) – GTK 4, egui, iced, Slint, Flutter, WebView, freya
7. [Komponenten-Packs](07-packs.md) – Tokio, SQLx, Redis, Axum, WebSocket, HTTP
8. [Datenbanken und Datenbindung](08-daten.md) – Listen, Tabellen, Bearbeiten, Schema-Browser
9. [Erzeugter und eigener Code](09-code.md) – welche Datei wem gehört, Code-Vervollständigung, Snippets
10. [Bauen, Starten, Debuggen](10-bauen.md) – Werkzeugleiste, Debugger, Terminal
11. [Git](11-git.md) – Quellcodeverwaltung wie in VS Code
12. [Einstellungen](12-einstellungen.md) – Arbeitsverzeichnis, Sprache, Darstellung, KI-Profile
13. [Automatisierung und KI](13-automatisierung.md) – alles auch ohne Oberfläche

**Teil III – RUF**

14. [RUF und RNCL](14-ruf.md) – das UI-Framework, auf dem Freya TDE und der CT-Editor laufen

**Teil IV – CT-Editor**

15. [CT-Editor](15-ct-editor.md) – der visuelle Editor für `Cargo.toml`

**Teil V – Bekannte Probleme**

16. [Bekannte Probleme und Grenzen](16-probleme.md) – was wir gefunden haben, Ursache und Lösung

**Anhang**

- [A – Glossar](a-glossar.md)
- [B – Lizenzen](b-lizenzen.md)
- [C – Entstehung](c-entstehung.md)

## Weiterführende Referenzen

Das Handbuch erklärt und verweist; die ausführlichen Nachschlagewerke liegen bei den Quellen:

| Thema | Datei |
|---|---|
| Fenster und Bedienung im Detail | [`ide/README.md`](../../ide/README.md) |
| UI-Backends, Pack-Format, Grenzen je Backend | [`widgets/README.md`](../../widgets/README.md) |
| Komponenten-Packs | [`components/README.md`](../../components/README.md) |
| Komponentenmodell, Codegen | [`core/README.md`](../../core/README.md) |
| Designer-Logik | [`designer/README.md`](../../designer/README.md) |
| Datenbank-Helfer | [`db/README.md`](../../db/README.md) |
| RUF-Entwicklerdoku | [RUF/docs/de](https://github.com/Freya-TDE/RUF/blob/main/docs/de/README.md) |
| CT-Editor-Handbuch | [CT-E/docs/de](https://github.com/Freya-TDE/CT-E/blob/main/docs/de/README.md) |
