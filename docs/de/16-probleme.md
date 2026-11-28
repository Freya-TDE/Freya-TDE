# 16 Bekannte Probleme und Grenzen

[Inhalt](README.md) · zurück: [15 CT-Editor](15-ct-editor.md) · weiter: [Anhang A – Glossar](a-glossar.md)

Hier steht gesammelt, was wir bei der Entwicklung an Problemen der Oberflächen gefunden haben – mit Ursache
und Lösung bzw. Umgehung. Einiges liegt an den Toolkits selbst, einiges an der Umgebung.

## GTK und die Systeme

| Problem | Ursache | Lösung / Umgehung |
|---|---|---|
| Fenster lassen sich unter Wayland nicht positionieren, die Anordnung geht verloren | Wayland verbietet Programmen, ihre Fenster selbst zu setzen | Freya TDE startet unter Linux über X11/XWayland (`GDK_BACKEND=x11`); der CT-Editor braucht das nicht |
| GTK 4 kann Fenster überhaupt nicht positionieren | so entworfen (Fenstermanager entscheidet) | RUF setzt die Lage nativ: Windows `SetWindowPos`, macOS `setFrameOrigin`, Linux X11 |
| Fenster kommen unter Windows 10/11 bei jedem Start um 39 px niedriger heraus, das Hauptfenster ist abgeschnitten | der Inhalt wird um die Titelleiste kleiner als per `set_default_size` angefordert | RUF lernt den Versatz bei der ersten Zuteilung jedes Fensters und fordert ihn bei jeder Größenänderung zusätzlich an (nur Windows und macOS) |
| `Gdk-CRITICAL: gdk_surface_thaw_updates: assertion …` (macOS) | Fehler in GTKs macOS-Teil beim Öffnen von Auswahllisten (auch in `gtk4-demo`) | harmlos; RUF blendet die Meldung aus |
| `Gtk-WARNING: GtkGizmo … (slider) reported min width -2` | Fehler im Breeze-Theme von KDE (Scrollbalken) | harmlos; RUF blendet die Meldung aus |
| `MESA-EGL: … failed to create dri2 screen`, `Gdk-WARNING: Vulkan: … Failed to detect any valid GPUs` (NVIDIA) | nach einem Treiber-Update ist der neue Treiber noch nicht geladen | Rechner neu starten |
| Ein Widget wird nicht kleiner als sein Inhalt (Linux) | GTK unterschreitet nie die natürliche Größe | Text kürzen, Umbruch einschalten oder mehr Platz vorsehen |
| Rechtsklick auf einen Eintrag einer **aufgeklappten** Auswahlliste geht nicht | GTKs DropDown bietet das nicht an | Kontextmenü der geschlossenen Liste mit Untermenüs je Eintrag (so bei den Snippets) |
| Ein Fenster in ein anderes ziehen gibt es nicht | GTK und die Fenstermanager kennen kein Andocken | Griff im Fenster, der per Ziehen und Ablegen auf das andere Fenster gezogen wird ([Kapitel 4](04-fenster.md#struktur-und-objektinspektor-zusammenlegen)) |
| Leeres Fenster bei WebView-Programmen (Linux, manche Treiber) | DMA-BUF-Renderer von WebKitGTK | das erzeugte Programm setzt `WEBKIT_DISABLE_DMABUF_RENDERER=1`, falls nicht gesetzt |

## Werkzeuge rund um den Code

| Problem | Ursache | Lösung / Umgehung |
|---|---|---|
| `cargo fmt` scheitert oder will erzeugte Dateien ändern | `main.rs` bindet `app.gen.rs` als Modul ein; `cargo fmt` folgt Modulen in die schreibgeschützten `.gen.rs` | „Format“ formatiert jede eigene Datei einzeln über `rustfmt` (per stdin) |
| rust-analyzer schlägt vor, ein fehlendes Feld ins generierte Struct zu schreiben (mit Typ `&mut _`) | er kennt die Trennung von erzeugtem und eigenem Code nicht | die Code-Vervollständigung legt das Feld in der Formular-Eigenschaft `state` an und schlägt den Typ selbst vor |
| rust-analyzer erzeugt Methoden mit `-> _` und `todo!()` | so arbeitet sein Assist | Freya TDE macht daraus `&mut self`, ohne `-> _`, mit leerem Rumpf |
| In einem anderen Editor (VS Code) bleiben alte Fehlermeldungen stehen, obwohl der Code baut | der dortige rust-analyzer hat einen Zwischenstand gesehen und nicht neu geprüft | dort „rust-analyzer: Restart server“ oder die Datei einmal speichern |

## Tests ohne Bildschirm (Xvfb)

| Problem | Ursache | Lösung / Umgehung |
|---|---|---|
| `GTK could not be initialised` | keine grafische Sitzung | `xvfb-run -a …`; in einer Wayland-Sitzung zusätzlich `env -u WAYLAND_DISPLAY` |
| Oberflächentest prüft eine Größe und scheitert („Hinweis wächst mit dem Fenster“) | `xvfb-run` hat ohne Angabe nur einen Bildschirm von 640 × 480; größere Fenster werden begrenzt | `xvfb-run -a -s "-screen 0 1920x1080x24" …` |
| Ein Test hängt ohne CPU-Last | ein modaler Dialog (z. B. „Ungespeicherte Änderungen“) wartet unsichtbar | in Tests `ruf::dialog::set_automatic(Some(true))`: Dialoge werden sofort beantwortet, die Frage steht in stderr |
| `MESA-EGL: warning: DRI3 error` beim Start unter Xvfb | Xvfb hat keine Grafikbeschleunigung | harmlos; die Tests zeichnen mit Software (`GSK_RENDERER=cairo`) |
| Das Fenster eines egui-Programms erscheint unter Xvfb spät | Software-OpenGL braucht nach einem langen Bau länger | der Test wartet bis 40 s und zeigt die Ausgabe des Programms |
| Xvfb startet in einer Sandbox nicht (kein `/tmp/.X11-unix/X99`) | die Sandbox schützt `/tmp/.X11-unix` | Tests außerhalb der Sandbox starten |

## Grenzen der UI-Backends

Jedes Backend setzt um, was sein Toolkit kann. Die wichtigsten Grenzen (vollständig in
[`widgets/README.md`](../../widgets/README.md)):

| Backend | Grenzen |
|---|---|
| GTK 4 | keine Höchstgröße für Fenster (gibt es in GTK 4 nicht); die Fensterlage bestimmt das System |
| egui | gezeigt wird das erste Formular |
| iced | kein Hauptmenü, kein Drehfeld, kein Bild (gibt es in iced nicht); `enabled` an Containern wirkt nicht; im DataGrid ist **Esc** nicht gebunden (iced gibt die Taste aus einem Eingabefeld nicht weiter) – Abbrechen über `<grid>_cancel()` |
| Slint | Standard-Widgets ohne Farbeigenschaften; die Tabelle (`StandardTableView`) kann nicht bearbeiten – darunter erscheint eine Bearbeitungszeile mit einem Feld je Spalte |
| Flutter | kein Debugger; das Fenster erscheint kurz in der Größe des Runners, bevor `window_manager` umstellt; der erste `flutter build` baut die Rust-Bibliothek ein zweites Mal (cargokit); `window_manager` 0.6/0.7 brauchen Flutter ≥ 3.47/3.50 und eine neue API, deshalb 0.5 |
| WebView | gezeigt wird das erste Formular; keine Hintergrundbilder; Menükürzel werden nicht gebunden |
| freya | das erste Formular ist das Hauptfenster (weitere über `crate::app::show_<formular>()`); Hintergrundbilder nur am Formular; `position = Center` wirkt nicht; kein Anker „mittig“; im DataGrid ist **Esc** nicht gebunden; „Bewegungen animieren“ nur mit Wunschgröße in Pixeln (freyas Portal braucht eine feste Größe); Tray nur am Hauptformular; Video lädt ffmpeg nach, wenn es fehlt; der erste Build lädt Skia herunter |

## Ausliefern

| Problem | Ursache | Lösung / Umgehung |
|---|---|---|
| macOS öffnet das Programm nicht („nicht verifiziert“) | nicht von Apple notarisiert | *Systemeinstellungen → Datenschutz & Sicherheit → „Dennoch öffnen“* oder `xattr -dr com.apple.quarantine …` |
| Windows SmartScreen warnt | nicht signiert | *Weitere Informationen → Trotzdem ausführen* |
| Programm startet unter Windows/macOS nicht, DLL/dylib fehlt | GTK-Laufzeit nicht mitgeliefert | mit `RUF/tools/bundle-windows.sh` bzw. `bundle-macos.sh` bündeln |
| Kein Schlüsselbund verfügbar (z. B. Linux ohne Secret Service) | der API-Key kann nicht in den Schlüsselbund | Freya TDE speichert ihn in `settings.toml` (nur für den Nutzer lesbar) |

Weitere Build- und Laufzeitprobleme von RUF:
[RUF – Fehlerbehebung](https://github.com/Freya-TDE/RUF/blob/main/docs/de/fehlerbehebung.md).
