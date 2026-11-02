//! Questions of `git` and `ssh` (password, SSH passphrase) for the running
//! program, like VS Code does it.
//!
//! [`Askpass::start`] listens on a local port (127.0.0.1) with a random
//! token. Every git command then runs with `GIT_ASKPASS` and `SSH_ASKPASS`
//! set to a helper program – `freya-tde` itself: when [`ENV`] is set,
//! [`helper_main`] connects, hands over the question and prints the answer
//! for git. The answer is never stored.
//!
//! Protocol (one line each): helper → `token`, `question`; program →
//! `ok <answer>` or `cancel`.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

/// Environment variable with `port:token` for the helper.
pub const ENV: &str = "FREYA_TDE_ASKPASS";

type Answer = dyn Fn(&str) -> Option<String> + Send + Sync;

/// A running question service; cheap to clone.
#[derive(Clone)]
pub struct Askpass {
    program: PathBuf,
    port: u16,
    token: String,
}

impl std::fmt::Debug for Askpass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Askpass")
            .field("program", &self.program)
            .field("port", &self.port)
            .finish_non_exhaustive()
    }
}

fn random_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    (0..4)
        .map(|i| {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u64(i);
            h.write_u128(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos()),
            );
            h.write_u32(std::process::id());
            format!("{:016x}", h.finish())
        })
        .collect()
}

impl Askpass {
    /// Starts the service. `program` is the helper (usually
    /// `std::env::current_exe()`, whose `main` calls [`helper_main`] first);
    /// `answer` gets the question in a background thread and returns the
    /// answer or `None` (cancelled).
    pub fn start(
        program: PathBuf,
        answer: impl Fn(&str) -> Option<String> + Send + Sync + 'static,
    ) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        let token = random_token();
        let answer: Arc<Answer> = Arc::new(answer);
        let expected = token.clone();
        std::thread::Builder::new()
            .name("askpass".into())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    let answer = answer.clone();
                    let expected = expected.clone();
                    std::thread::spawn(move || serve(stream, &expected, &*answer));
                }
            })?;
        Ok(Self {
            program,
            port,
            token,
        })
    }

    /// Sets the environment of a git command.
    pub fn apply(&self, command: &mut Command) {
        command
            .env("GIT_ASKPASS", &self.program)
            .env("SSH_ASKPASS", &self.program)
            .env("SSH_ASKPASS_REQUIRE", "force")
            .env(ENV, format!("{}:{}", self.port, self.token));
    }
}

/// Environment without a question service: a needed login fails at once
/// (no terminal prompt, no hanging `ssh`).
pub fn disable(command: &mut Command) {
    // Ein Programm, das es nicht gibt: ssh und git brechen die Anmeldung ab.
    command
        .env("GIT_ASKPASS", "freya-tde-no-askpass")
        .env("SSH_ASKPASS", "freya-tde-no-askpass")
        .env("SSH_ASKPASS_REQUIRE", "force")
        .env_remove(ENV);
}

fn serve(stream: TcpStream, token: &str, answer: &Answer) {
    let Ok(write) = stream.try_clone() else {
        return;
    };
    let mut lines = BufReader::new(stream).lines();
    let (Some(Ok(got)), Some(Ok(question))) = (lines.next(), lines.next()) else {
        return;
    };
    let mut write = write;
    if got != token {
        let _ = writeln!(write, "cancel");
        return;
    }
    let reply = match answer(&question) {
        Some(text) => format!("ok {}", text.replace(['\r', '\n'], "")),
        None => "cancel".to_owned(),
    };
    let _ = writeln!(write, "{reply}");
}

/// Asks the service at `port` (what the helper does); `None` = cancelled.
pub fn request(port: u16, token: &str, question: &str) -> Option<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    let question = question.replace(['\r', '\n'], " ");
    writeln!(stream, "{token}\n{question}").ok()?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).ok()?;
    let line = line.trim_end_matches(['\r', '\n']);
    line.strip_prefix("ok ")
        .or_else(|| (line == "ok").then_some(""))
        .map(str::to_owned)
}

/// The helper mode: when [`ENV`] is set (git or ssh started this program
/// as `GIT_ASKPASS`/`SSH_ASKPASS`), asks the service and prints the
/// answer. Returns the exit code, or `None` for a normal start. Call it
/// first thing in `main`.
pub fn helper_main() -> Option<i32> {
    let value = std::env::var(ENV).ok()?;
    let (port, token) = value.split_once(':')?;
    let port: u16 = port.parse().ok()?;
    let question = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "Passwort:".to_owned());
    Some(match request(port, token, &question) {
        Some(answer) => {
            println!("{answer}");
            0
        }
        None => 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let askpass = Askpass::start(PathBuf::from("helper"), |question| {
            question
                .contains("Passphrase")
                .then(|| "geheim 123".to_owned())
        })
        .unwrap();
        assert_eq!(
            request(askpass.port, &askpass.token, "Enter Passphrase for key"),
            Some("geheim 123".to_owned())
        );
        assert_eq!(request(askpass.port, &askpass.token, "Username"), None);
        assert_eq!(request(askpass.port, "falsch", "Enter Passphrase"), None);
        assert_ne!(askpass.token, random_token());
        assert_eq!(askpass.token.len(), 64);
    }
}
