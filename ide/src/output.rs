//! Output channels (like VS Code's „Output“): a text log per source –
//! build, Git, rust-analyzer, the started program. Shared between threads;
//! readers notice changes by [`Output::version`].
//!
//! ```
//! use freya_tde::output::{Output, channel};
//!
//! let output = Output::new();
//! output.line(channel::BUILD, "> cargo build");
//! assert_eq!(output.text(channel::BUILD), "> cargo build\n");
//! assert_eq!(output.channels()[0], channel::BUILD);
//! ```

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// The fixed channels (always listed, in this order).
pub mod channel {
    pub const BUILD: &str = "Build";
    pub const PROGRAM: &str = "Programm";
    pub const GIT: &str = "Git";
    pub const RUST_ANALYZER: &str = "rust-analyzer";
    pub const DATABASE: &str = "Datenbank";

    pub const ALL: [&str; 5] = [BUILD, PROGRAM, GIT, RUST_ANALYZER, DATABASE];
}

/// Text logs by channel name.
#[derive(Debug, Default)]
pub struct Output {
    channels: Mutex<BTreeMap<String, String>>,
    version: AtomicU64,
}

impl Output {
    /// A channel keeps at most this many bytes; older lines are dropped.
    pub const LIMIT: usize = 1 << 20;

    pub fn new() -> Arc<Self> {
        Arc::default()
    }

    /// Appends text to a channel.
    pub fn append(&self, channel: &str, text: &str) {
        if text.is_empty() {
            return;
        }
        let mut channels = self.channels.lock().unwrap_or_else(|e| e.into_inner());
        let log = channels.entry(channel.to_owned()).or_default();
        log.push_str(text);
        if log.len() > Self::LIMIT {
            // Vorne ganze Zeilen abschneiden.
            let cut = log.len() - Self::LIMIT;
            let cut = (cut..=log.len())
                .find(|&i| log.is_char_boundary(i))
                .unwrap_or(log.len());
            let cut = log[cut..].find('\n').map_or(cut, |i| cut + i + 1);
            log.drain(..cut);
        }
        self.version.fetch_add(1, Ordering::Relaxed);
    }

    /// Appends a line (adds the line break).
    pub fn line(&self, channel: &str, line: &str) {
        let mut text = line.to_owned();
        if !text.ends_with('\n') {
            text.push('\n');
        }
        self.append(channel, &text);
    }

    pub fn clear(&self, channel: &str) {
        let mut channels = self.channels.lock().unwrap_or_else(|e| e.into_inner());
        channels.remove(channel);
        self.version.fetch_add(1, Ordering::Relaxed);
    }

    pub fn text(&self, channel: &str) -> String {
        let channels = self.channels.lock().unwrap_or_else(|e| e.into_inner());
        channels.get(channel).cloned().unwrap_or_default()
    }

    /// The fixed channels, then others by name.
    pub fn channels(&self) -> Vec<String> {
        let channels = self.channels.lock().unwrap_or_else(|e| e.into_inner());
        let mut names: Vec<String> = channel::ALL.iter().map(|c| (*c).to_owned()).collect();
        names.extend(
            channels
                .keys()
                .filter(|k| !channel::ALL.contains(&k.as_str()))
                .cloned(),
        );
        names
    }

    /// Grows with every change.
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_and_limit() {
        let output = Output::new();
        let before = output.version();
        output.append("Eigener", "a");
        output.line(channel::GIT, "> git status");
        assert!(output.version() > before);
        assert_eq!(
            output.channels(),
            [
                "Build",
                "Programm",
                "Git",
                "rust-analyzer",
                "Datenbank",
                "Eigener"
            ]
        );
        let line = "ä".repeat(100) + "\n";
        for _ in 0..(Output::LIMIT / line.len() + 10) {
            output.append(channel::PROGRAM, &line);
        }
        let text = output.text(channel::PROGRAM);
        assert!(text.len() <= Output::LIMIT);
        assert!(text.starts_with('ä'));
        output.clear(channel::PROGRAM);
        assert!(output.text(channel::PROGRAM).is_empty());
    }
}
