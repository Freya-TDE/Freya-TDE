# Anhang C – Entstehung

[Inhalt](README.md) · zurück: [Anhang B – Lizenzen](b-lizenzen.md)

Freya TDE, RUF und der CT-Editor wurden **vollständig mit [Claude Code](https://claude.com/claude-code)**
umgesetzt, dem Programmier-Werkzeug von Anthropic, mit dem Modell **Claude Opus 5.5**. Code, Tests, Packs,
Vorlagen, Symbole und diese Dokumentation sind im Dialog entstanden: Der Rechteinhaber (Dragon Software) hat
Idee, Anforderungen und Entscheidungen vorgegeben, jede Etappe geprüft und getestet (die
fertigen Pakete des CT-Editors auch auf Windows und macOS); Claude Code hat entworfen, geschrieben, getestet und dokumentiert.

## Leitlinien

- **Erst der Kern, dann die Oberfläche:** zuerst Komponentenmodell, Abhängigkeitsgraph, Pack-Format und
  Codegen, dann Code-Editor, Designer und die Werkzeuge.
- **„Alles kann, nichts muss“:** dem Entwickler wird nichts vorgeschrieben – eigene Packs, eigene Backends,
  eigener Code bleiben unangetastet.
- **KI immer mitdenken:** jede Funktion als Bibliotheks-API ohne Oberfläche, mit maschinenlesbaren
  Ergebnissen, Vorschau vor dem Schreiben und stabilen Codes – damit eine KI das Werkzeug später direkt
  bedienen kann.
- **Freie Lizenzen:** keine kostenpflichtigen oder unklaren Abhängigkeiten.

Die Entscheidungen der Entwicklung sind in [`CLAUDE.md`](../../CLAUDE.md) festgehalten.
