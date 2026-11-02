//! A terminal without UI: the user's shell in a pseudo terminal
//! (`portable-pty`; ConPTY on Windows) and a terminal parser (`vt100`).
//! The window draws [`Terminal::screen`] and forwards keys as bytes
//! ([`key_bytes`]); scripts and an AI write text and read
//! [`Terminal::text`].
//!
//! ```no_run
//! use freya_tde::terminal::Terminal;
//!
//! let terminal = Terminal::start(std::path::Path::new("."), 24, 80).unwrap();
//! terminal.write_text("cargo --version\r");
//! // … later:
//! println!("{}", terminal.text());
//! ```

use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::Serialize;

/// Lines kept above the visible screen.
pub const SCROLLBACK: usize = 5000;

/// A terminal color as the program asked for it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TermColor {
    /// The terminal's foreground or background.
    #[default]
    Default,
    /// One of the 256 indexed colors (0–15 are the classic ones).
    Idx(u8),
    Rgb(u8, u8, u8),
}

impl From<vt100::Color> for TermColor {
    fn from(color: vt100::Color) -> Self {
        match color {
            vt100::Color::Default => Self::Default,
            vt100::Color::Idx(i) => Self::Idx(i),
            vt100::Color::Rgb(r, g, b) => Self::Rgb(r, g, b),
        }
    }
}

/// How a run of characters looks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize)]
pub struct Style {
    pub fg: TermColor,
    pub bg: TermColor,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub bold: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub italic: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub underline: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub inverse: bool,
}

/// Characters of one style in a row, starting at column `column`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Run {
    pub column: u16,
    /// One character per cell (wide characters take two cells; the second
    /// one is left out).
    pub text: String,
    /// Cells the run covers.
    pub cells: u16,
    pub style: Style,
}

/// What the terminal shows right now (scrolled back or not).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Screen {
    pub rows: u16,
    pub cols: u16,
    /// The runs of each visible row.
    pub lines: Vec<Vec<Run>>,
    /// Cursor (row, column); `None` when hidden or scrolled back.
    pub cursor: Option<(u16, u16)>,
    /// Lines scrolled back (0 = the live screen).
    pub scrolled: usize,
    /// The program wants arrow keys in „application“ mode.
    pub application_cursor: bool,
}

struct Shared {
    parser: Mutex<vt100::Parser>,
    version: AtomicU64,
    running: AtomicBool,
}

/// A shell in a pseudo terminal. Dropping it ends the shell.
pub struct Terminal {
    master: Box<dyn MasterPty + Send>,
    writer: Mutex<Box<dyn Write + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    shared: Arc<Shared>,
}

fn size(rows: u16, cols: u16) -> PtySize {
    PtySize {
        rows: rows.max(1),
        cols: cols.max(1),
        pixel_width: 0,
        pixel_height: 0,
    }
}

fn non_zero(n: u16) -> std::num::NonZeroU16 {
    std::num::NonZeroU16::new(n.max(1)).expect("at least 1")
}

impl Terminal {
    /// Starts the user's shell in `dir` (a login shell on Linux and macOS,
    /// `%ComSpec%` on Windows).
    pub fn start(dir: &Path, rows: u16, cols: u16) -> std::io::Result<Self> {
        let mut command = CommandBuilder::new_default_prog();
        command.cwd(dir);
        Self::spawn(command, rows, cols)
    }

    /// Starts a program with arguments instead of the shell (scripts, tests).
    pub fn start_command(
        dir: &Path,
        program: &str,
        args: &[&str],
        rows: u16,
        cols: u16,
    ) -> std::io::Result<Self> {
        let mut command = CommandBuilder::new(program);
        command.args(args);
        command.cwd(dir);
        Self::spawn(command, rows, cols)
    }

    fn spawn(mut command: CommandBuilder, rows: u16, cols: u16) -> std::io::Result<Self> {
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        let pair = native_pty_system()
            .openpty(size(rows, cols))
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        drop(pair.slave);
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        let shared = Arc::new(Shared {
            parser: Mutex::new(vt100::Parser::new(
                non_zero(rows),
                non_zero(cols),
                SCROLLBACK,
            )),
            version: AtomicU64::new(0),
            running: AtomicBool::new(true),
        });
        let reading = shared.clone();
        std::thread::Builder::new()
            .name("terminal".into())
            .spawn(move || {
                let mut buffer = [0u8; 8192];
                loop {
                    match reader.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if let Ok(mut parser) = reading.parser.lock() {
                                parser.process(&buffer[..n]);
                            }
                            reading.version.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
                reading.running.store(false, Ordering::Relaxed);
                reading.version.fetch_add(1, Ordering::Relaxed);
            })?;
        Ok(Self {
            master: pair.master,
            writer: Mutex::new(writer),
            child: Mutex::new(child),
            shared,
        })
    }

    /// Sends bytes to the program (as if typed).
    pub fn write(&self, bytes: &[u8]) {
        if let Ok(mut writer) = self.writer.lock() {
            let _ = writer.write_all(bytes);
            let _ = writer.flush();
        }
        // Wer tippt, will das Aktuelle sehen.
        self.scroll_to(0);
    }

    /// Sends text; `\r` is Enter.
    pub fn write_text(&self, text: &str) {
        self.write(text.as_bytes());
    }

    /// Pastes text: with bracketed paste if the program wants it.
    pub fn paste(&self, text: &str) {
        let bracketed = self
            .shared
            .parser
            .lock()
            .is_ok_and(|p| p.screen().bracketed_paste());
        let text = text.replace("\r\n", "\r").replace('\n', "\r");
        if bracketed {
            self.write_text(&format!("\x1b[200~{text}\x1b[201~"));
        } else {
            self.write_text(&text);
        }
    }

    /// New size in cells (when the window changes).
    pub fn resize(&self, rows: u16, cols: u16) {
        if self.size() == (rows.max(1), cols.max(1)) {
            return;
        }
        let _ = self.master.resize(size(rows, cols));
        if let Ok(mut parser) = self.shared.parser.lock() {
            parser.set_size(non_zero(rows), non_zero(cols));
        }
        self.shared.version.fetch_add(1, Ordering::Relaxed);
    }

    /// (rows, columns).
    pub fn size(&self) -> (u16, u16) {
        self.shared.parser.lock().map_or((1, 1), |p| {
            let (rows, cols) = p.screen().size();
            (rows.get(), cols.get())
        })
    }

    /// Grows with every change of the screen (and when the program ends).
    pub fn version(&self) -> u64 {
        self.shared.version.load(Ordering::Relaxed)
    }

    pub fn is_running(&self) -> bool {
        self.shared.running.load(Ordering::Relaxed)
    }

    /// Scrolls back by `lines` (positive = older lines).
    pub fn scroll(&self, lines: i32) {
        let current = self
            .shared
            .parser
            .lock()
            .map_or(0, |p| p.screen().scrollback());
        self.scroll_to(current.saturating_add_signed(lines as isize));
    }

    fn scroll_to(&self, lines: usize) {
        if let Ok(mut parser) = self.shared.parser.lock()
            && parser.screen().scrollback() != lines
        {
            parser.screen_mut().set_scrollback(lines);
            self.shared.version.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// The visible text, one line per row, without trailing spaces.
    pub fn text(&self) -> String {
        self.shared
            .parser
            .lock()
            .map(|p| p.screen().contents())
            .unwrap_or_default()
    }

    /// Text of the cells from (row, col) to (row, col), both included, as
    /// shown (for copying a selection).
    pub fn text_between(&self, start: (u16, u16), end: (u16, u16)) -> String {
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        self.shared
            .parser
            .lock()
            .map(|p| {
                p.screen()
                    .contents_between(start.0, start.1, end.0, end.1.saturating_add(1))
            })
            .unwrap_or_default()
    }

    /// What is visible, as runs of equally styled characters.
    pub fn screen(&self) -> Screen {
        let Ok(parser) = self.shared.parser.lock() else {
            return Screen {
                rows: 0,
                cols: 0,
                lines: Vec::new(),
                cursor: None,
                scrolled: 0,
                application_cursor: false,
            };
        };
        let screen = parser.screen();
        let (rows, cols) = screen.size();
        let (rows, cols) = (rows.get(), cols.get());
        let mut lines = Vec::with_capacity(rows as usize);
        for row in 0..rows {
            let mut runs: Vec<Run> = Vec::new();
            for column in 0..cols {
                let Some(cell) = screen.cell(row, column) else {
                    continue;
                };
                if cell.is_wide_continuation() {
                    if let Some(run) = runs.last_mut() {
                        run.cells += 1;
                    }
                    continue;
                }
                let style = Style {
                    fg: cell.fgcolor().into(),
                    bg: cell.bgcolor().into(),
                    bold: cell.bold(),
                    italic: cell.italic(),
                    underline: cell.underline(),
                    inverse: cell.inverse(),
                };
                let text = if cell.has_contents() {
                    cell.contents()
                } else {
                    " "
                };
                match runs.last_mut() {
                    Some(run) if run.style == style => {
                        run.text.push_str(text);
                        run.cells += 1;
                    }
                    _ => runs.push(Run {
                        column,
                        text: text.to_owned(),
                        cells: 1,
                        style,
                    }),
                }
            }
            // Leerraum am Zeilenende ohne Hintergrund weglassen.
            if let Some(last) = runs.last_mut()
                && last.style == Style::default()
            {
                let trimmed = last.text.trim_end_matches(' ');
                let removed = last.text.len() - trimmed.len();
                last.cells -= removed as u16;
                last.text.truncate(trimmed.len());
                if last.text.is_empty() {
                    runs.pop();
                }
            }
            lines.push(runs);
        }
        let scrolled = screen.scrollback();
        Screen {
            rows,
            cols,
            lines,
            cursor: (!screen.hide_cursor() && scrolled == 0).then(|| screen.cursor_position()),
            scrolled,
            application_cursor: screen.application_cursor(),
        }
    }

    /// The program wants arrow keys in „application“ mode (`ESC O A`).
    pub fn application_cursor(&self) -> bool {
        self.shared
            .parser
            .lock()
            .is_ok_and(|p| p.screen().application_cursor())
    }

    /// Ends the program.
    pub fn kill(&self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
        }
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        self.kill();
    }
}

/// A key as a terminal program expects it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TermKey {
    /// A typed character.
    Char(char),
    Enter,
    Tab,
    Backspace,
    Escape,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    /// F1 … F12.
    F(u8),
}

/// The bytes for a key (xterm conventions). `application_cursor`: arrows
/// as `ESC O A` instead of `ESC [ A` (see [`Screen::application_cursor`]).
pub fn key_bytes(
    key: TermKey,
    ctrl: bool,
    alt: bool,
    shift: bool,
    application_cursor: bool,
) -> Option<Vec<u8>> {
    let mut bytes = match key {
        TermKey::Char(c) if ctrl => match c.to_ascii_lowercase() {
            c @ 'a'..='z' => vec![c as u8 - b'a' + 1],
            ' ' | '@' | '2' => vec![0],
            '[' | '3' => vec![0x1b],
            '\\' | '4' => vec![0x1c],
            ']' | '5' => vec![0x1d],
            '^' | '6' => vec![0x1e],
            '_' | '-' | '7' => vec![0x1f],
            '8' | '?' => vec![0x7f],
            _ => return None,
        },
        TermKey::Char(c) => c.to_string().into_bytes(),
        TermKey::Enter => vec![b'\r'],
        TermKey::Tab if shift => b"\x1b[Z".to_vec(),
        TermKey::Tab => vec![b'\t'],
        TermKey::Backspace if ctrl => vec![0x08],
        TermKey::Backspace => vec![0x7f],
        TermKey::Escape => vec![0x1b],
        TermKey::Up
        | TermKey::Down
        | TermKey::Right
        | TermKey::Left
        | TermKey::Home
        | TermKey::End => {
            let letter = match key {
                TermKey::Up => 'A',
                TermKey::Down => 'B',
                TermKey::Right => 'C',
                TermKey::Left => 'D',
                TermKey::Home => 'H',
                _ => 'F',
            };
            let modifier = 1 + u8::from(shift) + 2 * u8::from(alt) + 4 * u8::from(ctrl);
            if modifier > 1 {
                format!("\x1b[1;{modifier}{letter}").into_bytes()
            } else if application_cursor {
                format!("\x1bO{letter}").into_bytes()
            } else {
                format!("\x1b[{letter}").into_bytes()
            }
        }
        TermKey::PageUp => b"\x1b[5~".to_vec(),
        TermKey::PageDown => b"\x1b[6~".to_vec(),
        TermKey::Insert => b"\x1b[2~".to_vec(),
        TermKey::Delete => b"\x1b[3~".to_vec(),
        TermKey::F(n @ 1..=4) => format!("\x1bO{}", (b'P' + n - 1) as char).into_bytes(),
        TermKey::F(n @ 5..=12) => {
            let code = [15, 17, 18, 19, 20, 21, 23, 24][usize::from(n - 5)];
            format!("\x1b[{code}~").into_bytes()
        }
        TermKey::F(_) => return None,
    };
    // Alt: ESC davor (wie xterm mit „metaSendsEscape“).
    if alt && matches!(key, TermKey::Char(_) | TermKey::Backspace) {
        bytes.insert(0, 0x1b);
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys() {
        let k = |key, ctrl, alt, shift| key_bytes(key, ctrl, alt, shift, false).unwrap();
        assert_eq!(k(TermKey::Char('ä'), false, false, false), "ä".as_bytes());
        assert_eq!(k(TermKey::Char('c'), true, false, false), [3]);
        assert_eq!(k(TermKey::Char('C'), true, false, true), [3]);
        assert_eq!(k(TermKey::Char('b'), false, true, false), b"\x1bb");
        assert_eq!(k(TermKey::Up, false, false, false), b"\x1b[A");
        assert_eq!(
            key_bytes(TermKey::Up, false, false, false, true).unwrap(),
            b"\x1bOA"
        );
        assert_eq!(k(TermKey::Right, true, false, false), b"\x1b[1;5C");
        assert_eq!(k(TermKey::Tab, false, false, true), b"\x1b[Z");
        assert_eq!(k(TermKey::F(5), false, false, false), b"\x1b[15~");
        assert_eq!(k(TermKey::F(1), false, false, false), b"\x1bOP");
        assert_eq!(k(TermKey::Backspace, false, false, false), [0x7f]);
    }
}
