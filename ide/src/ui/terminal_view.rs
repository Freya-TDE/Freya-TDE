//! The terminal tab of the panel: draws a [`Terminal`] on a canvas and
//! forwards keys, mouse selection, wheel and clipboard. The shell starts
//! when the tab is shown first, in the project folder.

use crate::tr;
use std::cell::{Cell, RefCell};
use std::path::Path;

use rncl::prelude::*;
use ruf::{Application, Color, ColorScheme, Key, Rect};

use crate::terminal::{Screen, TermColor, TermKey, Terminal, key_bytes};

/// Font size and inner margin in pixels.
const FONT: i32 = 13;
const PAD: i32 = 4;

/// Colors of the terminal (VS Code's default terminal colors).
struct Palette {
    fg: Color,
    bg: Color,
    selection: Color,
    ansi: [Color; 16],
}

fn palette() -> Palette {
    let rgb = Color::rgb;
    if Application::color_scheme() == ColorScheme::Dark {
        Palette {
            fg: rgb(0xcc, 0xcc, 0xcc),
            bg: rgb(0x1e, 0x1e, 0x1e),
            selection: rgb(0x26, 0x4f, 0x78),
            ansi: [
                rgb(0x00, 0x00, 0x00),
                rgb(0xcd, 0x31, 0x31),
                rgb(0x0d, 0xbc, 0x79),
                rgb(0xe5, 0xe5, 0x10),
                rgb(0x24, 0x72, 0xc8),
                rgb(0xbc, 0x3f, 0xbc),
                rgb(0x11, 0xa8, 0xcd),
                rgb(0xe5, 0xe5, 0xe5),
                rgb(0x66, 0x66, 0x66),
                rgb(0xf1, 0x4c, 0x4c),
                rgb(0x23, 0xd1, 0x8b),
                rgb(0xf5, 0xf5, 0x43),
                rgb(0x3b, 0x8e, 0xea),
                rgb(0xd6, 0x70, 0xd6),
                rgb(0x29, 0xb8, 0xdb),
                rgb(0xe5, 0xe5, 0xe5),
            ],
        }
    } else {
        Palette {
            fg: rgb(0x33, 0x33, 0x33),
            bg: rgb(0xff, 0xff, 0xff),
            selection: rgb(0xad, 0xd6, 0xff),
            ansi: [
                rgb(0x00, 0x00, 0x00),
                rgb(0xcd, 0x31, 0x31),
                rgb(0x00, 0xbc, 0x00),
                rgb(0x94, 0x98, 0x00),
                rgb(0x04, 0x51, 0xa5),
                rgb(0xbc, 0x05, 0xbc),
                rgb(0x05, 0x98, 0xbc),
                rgb(0x55, 0x55, 0x55),
                rgb(0x66, 0x66, 0x66),
                rgb(0xcd, 0x31, 0x31),
                rgb(0x14, 0xce, 0x14),
                rgb(0xb5, 0xba, 0x00),
                rgb(0x04, 0x51, 0xa5),
                rgb(0xbc, 0x05, 0xbc),
                rgb(0x05, 0x98, 0xbc),
                rgb(0xa5, 0xa5, 0xa5),
            ],
        }
    }
}

impl Palette {
    fn color(&self, color: TermColor, default: Color) -> Color {
        match color {
            TermColor::Default => default,
            TermColor::Idx(i @ 0..=15) => self.ansi[usize::from(i)],
            // 6×6×6-Farbwürfel und Graustufen.
            TermColor::Idx(i @ 16..=231) => {
                let i = i - 16;
                let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
                Color::rgb(level(i / 36), level((i / 6) % 6), level(i % 6))
            }
            TermColor::Idx(i) => {
                let v = 8 + (i - 232) * 10;
                Color::rgb(v, v, v)
            }
            TermColor::Rgb(r, g, b) => Color::rgb(r, g, b),
        }
    }
}

/// Selection in cells: (row, column) where the mouse was pressed and is now.
type Selection = ((u16, u16), (u16, u16));

pub struct TerminalView {
    pub canvas: RnclCanvas,
    terminal: RefCell<Option<Terminal>>,
    /// Terminal version last drawn.
    drawn: Cell<u64>,
    /// Width and height of a cell.
    cell: Cell<(i32, i32)>,
    selection: Cell<Option<Selection>>,
    selecting: Cell<bool>,
}

impl TerminalView {
    pub fn build(page: &RnclPanel) -> Self {
        let canvas = RnclCanvas::new(page);
        let m = ruf::measure_text("MMMMMMMMMM", FONT, false, true);
        let cell = ((m.width / 10).max(1), m.height.max(1));
        Self {
            canvas,
            terminal: RefCell::new(None),
            drawn: Cell::new(u64::MAX),
            cell: Cell::new(cell),
            selection: Cell::new(None),
            selecting: Cell::new(false),
        }
    }

    /// Rows and columns that fit into the canvas.
    fn grid(&self) -> (u16, u16) {
        let size = self.canvas.size();
        let (cw, ch) = self.cell.get();
        (
            ((size.height - 2 * PAD) / ch).clamp(1, 1000) as u16,
            ((size.width - 2 * PAD) / cw).clamp(1, 1000) as u16,
        )
    }

    /// Starts the shell in `dir` unless one is running.
    pub fn ensure_started(&self, dir: &Path) {
        let running = self
            .terminal
            .borrow()
            .as_ref()
            .is_some_and(Terminal::is_running);
        if running {
            return;
        }
        let (rows, cols) = self.grid();
        let terminal = Terminal::start(dir, rows, cols);
        match terminal {
            Ok(t) => *self.terminal.borrow_mut() = Some(t),
            Err(e) => {
                *self.terminal.borrow_mut() = None;
                self.canvas.set_shapes(vec![Shape::Text {
                    x: PAD,
                    y: PAD,
                    text: tr!("panel.terminal_failed", error = e),
                    color: palette().ansi[1],
                    size: FONT,
                    bold: false,
                    italic: false,
                    family: None,
                }]);
            }
        }
        self.selection.set(None);
        self.drawn.set(u64::MAX);
        self.canvas.widget().focus();
    }

    /// Ends the shell and starts a new one.
    pub fn restart(&self, dir: &Path) {
        if let Some(terminal) = self.terminal.borrow_mut().take() {
            terminal.kill();
        }
        self.ensure_started(dir);
    }

    pub fn terminal(&self) -> std::cell::Ref<'_, Option<Terminal>> {
        self.terminal.borrow()
    }

    pub fn resized(&self) {
        let (rows, cols) = self.grid();
        if let Some(terminal) = self.terminal.borrow().as_ref() {
            terminal.resize(rows, cols);
        }
        self.drawn.set(u64::MAX);
        self.redraw();
    }

    /// Draws when the terminal changed.
    pub fn redraw(&self) {
        let terminal = self.terminal.borrow();
        let Some(terminal) = terminal.as_ref() else {
            return;
        };
        let version = terminal.version();
        if version == self.drawn.get() {
            return;
        }
        self.drawn.set(version);
        let running = terminal.is_running();
        let screen = terminal.screen();
        self.canvas.set_shapes(self.shapes(&screen, running));
    }

    fn shapes(&self, screen: &Screen, running: bool) -> Vec<Shape> {
        let p = palette();
        let (cw, ch) = self.cell.get();
        let size = self.canvas.size();
        let mut shapes = vec![Shape::Rect {
            rect: Rect::new(0, 0, size.width, size.height),
            fill: Some(p.bg),
            stroke: None,
            radius: 0,
            dashed: false,
        }];
        let at = |row: u16, col: u16| (PAD + i32::from(col) * cw, PAD + i32::from(row) * ch);
        // Auswahl unter dem Text.
        if let Some((a, b)) = self.selection.get() {
            let (start, end) = if a <= b { (a, b) } else { (b, a) };
            for row in start.0..=end.0.min(screen.rows.saturating_sub(1)) {
                let from = if row == start.0 { start.1 } else { 0 };
                let to = if row == end.0 {
                    end.1
                } else {
                    screen.cols.saturating_sub(1)
                };
                let (x, y) = at(row, from);
                shapes.push(Shape::Rect {
                    rect: Rect::new(x, y, i32::from(to.saturating_sub(from) + 1) * cw, ch),
                    fill: Some(p.selection),
                    stroke: None,
                    radius: 0,
                    dashed: false,
                });
            }
        }
        for (row, runs) in screen.lines.iter().enumerate() {
            for run in runs {
                let style = run.style;
                let (mut fg, mut bg) = (p.color(style.fg, p.fg), p.color(style.bg, p.bg));
                if style.inverse {
                    std::mem::swap(&mut fg, &mut bg);
                }
                // Fette Standardfarben in der hellen Variante (wie xterm).
                if style.bold
                    && let TermColor::Idx(i @ 0..=7) = style.fg
                {
                    fg = p.ansi[usize::from(i) + 8];
                }
                let (x, y) = at(row as u16, run.column);
                let background = (style.bg != TermColor::Default || style.inverse).then_some(bg);
                if let Some(bg) = background {
                    shapes.push(Shape::Rect {
                        rect: Rect::new(x, y, i32::from(run.cells) * cw, ch),
                        fill: Some(bg),
                        stroke: None,
                        radius: 0,
                        dashed: false,
                    });
                }
                shapes.push(Shape::MonoText {
                    x,
                    y,
                    text: run.text.clone(),
                    color: fg,
                    background: None,
                    size: FONT,
                    bold: style.bold,
                    italic: style.italic,
                    underline: style.underline,
                });
            }
        }
        if let Some((row, col)) = screen.cursor.filter(|_| running) {
            let (x, y) = at(row, col);
            // Kandidatenfenster der Eingabemethode am Cursor.
            self.canvas
                .widget()
                .set_input_cursor(Rect::new(x, y, cw, ch));
            // Block-Cursor; das Zeichen darunter in der Hintergrundfarbe.
            shapes.push(Shape::Rect {
                rect: Rect::new(x, y, cw, ch),
                fill: Some(p.fg),
                stroke: None,
                radius: 0,
                dashed: false,
            });
            let under = screen.lines.get(usize::from(row)).and_then(|runs| {
                runs.iter().find_map(|run| {
                    let offset = col.checked_sub(run.column)?;
                    run.text.chars().nth(usize::from(offset))
                })
            });
            if let Some(c) = under.filter(|c| *c != ' ') {
                shapes.push(Shape::MonoText {
                    x,
                    y,
                    text: c.to_string(),
                    color: p.bg,
                    background: None,
                    size: FONT,
                    bold: false,
                    italic: false,
                    underline: false,
                });
            }
        }
        if !running {
            let last = screen
                .lines
                .iter()
                .rposition(|runs| !runs.is_empty())
                .map_or(0, |r| r + 1) as u16;
            let (x, y) = at(last.min(screen.rows.saturating_sub(1)), 0);
            shapes.push(Shape::MonoText {
                x,
                y,
                text: "[Prozess beendet – Enter startet neu]".to_owned(),
                color: p.ansi[8],
                background: None,
                size: FONT,
                bold: false,
                italic: true,
                underline: false,
            });
        }
        shapes
    }

    /// The cell under a canvas point.
    fn cell_at(&self, x: i32, y: i32) -> (u16, u16) {
        let (cw, ch) = self.cell.get();
        let (rows, cols) = self.grid();
        (
            ((y - PAD).max(0) / ch).min(i32::from(rows) - 1).max(0) as u16,
            ((x - PAD).max(0) / cw).min(i32::from(cols) - 1).max(0) as u16,
        )
    }

    /// Mouse: select with the left button, wheel scrolls back. Returns
    /// `true` for a right click (the panel shows its context menu).
    pub fn mouse(&self, event: MouseEvent) -> bool {
        if self.terminal.borrow().is_none() {
            return false;
        }
        match event.action {
            MouseAction::Pressed if event.button == 1 => {
                let at = self.cell_at(event.x, event.y);
                self.selection.set(Some((at, at)));
                self.selecting.set(true);
                self.canvas.widget().focus();
            }
            MouseAction::Moved if self.selecting.get() => {
                if let Some((start, _)) = self.selection.get() {
                    self.selection
                        .set(Some((start, self.cell_at(event.x, event.y))));
                }
            }
            MouseAction::Released => {
                self.selecting.set(false);
                if let Some((a, b)) = self.selection.get()
                    && a == b
                {
                    self.selection.set(None);
                }
            }
            MouseAction::Pressed if event.button == 3 => return true,
            MouseAction::ScrollUp | MouseAction::ScrollDown => {
                if let Some(t) = self.terminal.borrow().as_ref() {
                    t.scroll(if event.action == MouseAction::ScrollUp {
                        3
                    } else {
                        -3
                    });
                }
            }
            _ => return false,
        }
        self.drawn.set(u64::MAX);
        self.redraw();
        false
    }

    /// The selected text (empty if nothing is selected).
    pub fn selected_text(&self) -> String {
        match (self.selection.get(), self.terminal.borrow().as_ref()) {
            (Some((a, b)), Some(t)) => t.text_between(a, b),
            _ => String::new(),
        }
    }

    pub fn copy(&self) {
        let text = self.selected_text();
        if !text.is_empty() {
            ruf::clipboard::set_text(&text);
        }
    }

    pub fn paste(&self, text: &str) {
        if let Some(t) = self.terminal.borrow().as_ref() {
            t.paste(text);
        }
    }

    /// Clears the screen (Ctrl+L: the shell redraws its prompt).
    pub fn clear(&self) {
        if let Some(t) = self.terminal.borrow().as_ref() {
            t.write(&[0x0c]);
        }
    }

    /// Text from the input method (typed characters, dead keys like
    /// `^` + `e` = `ê`, AltGr characters, IME).
    pub fn text_input(&self, text: &str) {
        let terminal = self.terminal.borrow();
        let Some(terminal) = terminal.as_ref().filter(|t| t.is_running()) else {
            return;
        };
        self.selection.set(None);
        terminal.write_text(text);
    }

    /// A key on the canvas. Returns what the panel should do itself:
    /// `Some(true)` = paste from the clipboard, `Some(false)` = restart.
    pub fn key(&self, event: KeyEvent) -> Option<bool> {
        let m = event.modifiers;
        // Kopieren/Einfügen: Strg+Umschalt+C/V, unter macOS ⌘C/⌘V.
        let clipboard = (m.ctrl && m.shift) || (cfg!(target_os = "macos") && m.meta);
        match event.key {
            Key::Char('c') if clipboard => {
                self.copy();
                return None;
            }
            Key::Char('v') if clipboard => return Some(true),
            Key::PageUp | Key::PageDown if m.shift => {
                if let Some(t) = self.terminal.borrow().as_ref() {
                    let (rows, _) = t.size();
                    let page = i32::from(rows.saturating_sub(1)).max(1);
                    t.scroll(if event.key == Key::PageUp {
                        page
                    } else {
                        -page
                    });
                }
                self.redraw();
                return None;
            }
            _ => {}
        }
        let terminal = self.terminal.borrow();
        let terminal = terminal.as_ref()?;
        if !terminal.is_running() {
            return (event.key == Key::Enter).then_some(false);
        }
        let key = match event.key {
            Key::Char(c) => TermKey::Char(event.text.unwrap_or(c)),
            Key::Space => TermKey::Char(' '),
            Key::Enter => TermKey::Enter,
            Key::Tab => TermKey::Tab,
            Key::Backspace => TermKey::Backspace,
            Key::Escape => TermKey::Escape,
            Key::Up => TermKey::Up,
            Key::Down => TermKey::Down,
            Key::Left => TermKey::Left,
            Key::Right => TermKey::Right,
            Key::Home => TermKey::Home,
            Key::End => TermKey::End,
            Key::PageUp => TermKey::PageUp,
            Key::PageDown => TermKey::PageDown,
            Key::Insert => TermKey::Insert,
            Key::Delete => TermKey::Delete,
            Key::F(n) => TermKey::F(n),
        };
        let application = terminal.application_cursor();
        if let Some(bytes) = key_bytes(key, m.ctrl, m.alt, m.shift, application) {
            self.selection.set(None);
            terminal.write(&bytes);
        }
        None
    }
}
