# Anhang A – Glossar

[Inhalt](README.md) · zurück: [16 Bekannte Probleme und Grenzen](16-probleme.md) · weiter: [Anhang B – Lizenzen](b-lizenzen.md)

| Begriff | Bedeutung |
|---|---|
| **Backend (UI-Backend)** | Pack, das Formulare für ein UI-Toolkit übersetzt (GTK 4, egui, iced, Slint, Flutter, WebView) |
| **Komponente** | Baustein mit Name, Typ, Eigenschaften und Ereignissen |
| **Widget** | Komponente mit visueller Erweiterung (Platzierung, Kinder) |
| **Formular** | ein Fenster des Zielprogramms; Datei `.ftdr` neben der `.rs` |
| **Datenmodul** | Formular ohne Widgets, Service-Container (z. B. `services`) |
| **Pack** | Ordner mit `pack.toml`: Typen, Vorlagen, Abhängigkeiten |
| **Provides / Requires** | was eine Komponente anbietet bzw. braucht; Grundlage der Prüfung und der Reihenfolge |
| **Datenquelle** | Typ, der Zeilen liefert (`[component.data]`), z. B. SQLx-`Query` |
| **Handler-Stub** | leere Methode im eigenen Code, die ein Ereignis behandelt |
| **`.gen.rs`** | von Freya TDE erzeugte, schreibgeschützte Datei |
| **`.ptdr` / `.ftdr`** | Projekt- bzw. Formulardatei (RON) |
| **`state`** | eigene Felder eines Formulars |
| **Anker** | Ränder, zu denen ein absolut platziertes Widget seinen Abstand hält |
| **Snippet** | gespeichertes Code-Stück mit use-Zeile |
| **RUF** | Rust native UI Framework – das UI-Framework der IDE |
| **RNCL** | Rust native Component Library – die Komponenten von RUF |
| **CT-E / CT-Editor** | Cargo TOML Editor |
| **freya-tde-db** | Datenbank-Helfer (eigener Prozess) |
| **DAP / LSP** | Debug Adapter Protocol / Language Server Protocol |
