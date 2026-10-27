//! A small Language Server Protocol client for rust-analyzer – without UI.
//!
//! rust-analyzer runs as its own process (stdio, JSON-RPC with
//! `Content-Length` framing). Requests return an id; results, diagnostics
//! and status arrive as [`LspEvent`]s on a callback (from a reader thread –
//! the UI forwards them to its thread). Documents are sent in full on every
//! change. Positions are 1-based lines and 0-based character columns here;
//! the client converts to the encoding the server chose (UTF-8 or UTF-16).

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::{Value, json};

/// Severity of a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

/// A finding of rust-analyzer (or `cargo check` on save).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    /// 1-based.
    pub line: usize,
    /// 0-based, in characters.
    pub column: usize,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub code: Option<String>,
}

/// A completion proposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompletionItem {
    pub label: String,
    pub detail: Option<String>,
    /// Text to insert in place of the word before the cursor.
    pub insert: String,
}

/// A place in a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Location {
    pub path: PathBuf,
    /// 1-based.
    pub line: usize,
    /// 0-based, in characters.
    pub column: usize,
}

/// What the server reports.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum LspEvent {
    /// The server finished initialising.
    Ready,
    /// Indexing and checks are done (`quiescent`) or not.
    Status {
        quiescent: bool,
        message: Option<String>,
    },
    /// Diagnostics of a file (replace earlier ones).
    Diagnostics {
        path: PathBuf,
        items: Vec<Diagnostic>,
    },
    Completion {
        request: u64,
        items: Vec<CompletionItem>,
    },
    Definition {
        request: u64,
        locations: Vec<Location>,
    },
    /// A request failed.
    Error {
        request: Option<u64>,
        message: String,
    },
    /// A log line of the server (`window/logMessage`, `window/showMessage`
    /// or its standard error output).
    Log { message: String },
    /// The server process ended.
    Exited,
}

/// `file://` URI of an absolute path (percent-encoded).
pub fn path_to_uri(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut out = String::from("file://");
    if !text.starts_with('/') {
        // Windows: file:///C:/…
        out.push('/');
    }
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' | b':' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Path of a `file://` URI.
pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let mut bytes = Vec::with_capacity(rest.len());
    let mut iter = rest.bytes();
    while let Some(b) = iter.next() {
        if b == b'%' {
            let hex = [iter.next()?, iter.next()?];
            bytes.push(u8::from_str_radix(std::str::from_utf8(&hex).ok()?, 16).ok()?);
        } else {
            bytes.push(b);
        }
    }
    let text = String::from_utf8(bytes).ok()?;
    // Windows: /C:/… → C:/…
    let text = match text.as_bytes() {
        [b'/', drive, b':', ..] if drive.is_ascii_alphabetic() => text[1..].to_owned(),
        _ => text,
    };
    Some(PathBuf::from(text))
}

/// How the server counts columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Encoding {
    Utf8,
    Utf16,
}

impl Encoding {
    /// Character column → server units on `line`.
    fn units(self, line: &str, column: usize) -> usize {
        let prefix = line.chars().take(column);
        match self {
            Encoding::Utf8 => prefix.map(char::len_utf8).sum(),
            Encoding::Utf16 => prefix.map(char::len_utf16).sum(),
        }
    }

    /// Server units on `line` → character column.
    fn column(self, line: &str, units: usize) -> usize {
        let mut used = 0;
        for (i, c) in line.chars().enumerate() {
            if used >= units {
                return i;
            }
            used += match self {
                Encoding::Utf8 => c.len_utf8(),
                Encoding::Utf16 => c.len_utf16(),
            };
        }
        line.chars().count()
    }
}

/// State shared with the reader thread.
#[derive(Default)]
struct Shared {
    /// Open documents: text and version.
    documents: HashMap<PathBuf, (String, i64)>,
    /// Pending requests: id → method.
    pending: HashMap<u64, String>,
    encoding: Option<Encoding>,
}

impl Shared {
    fn encoding(&self) -> Encoding {
        self.encoding.unwrap_or(Encoding::Utf16)
    }

    fn line_text(&self, path: &Path, line: usize) -> String {
        self.documents
            .get(path)
            .and_then(|(text, _)| text.lines().nth(line.saturating_sub(1)))
            .unwrap_or("")
            .to_owned()
    }
}

/// A running rust-analyzer.
pub struct LspClient {
    child: Mutex<Child>,
    stdin: Arc<Mutex<ChildStdin>>,
    next_id: AtomicU64,
    shared: Arc<Mutex<Shared>>,
    root: PathBuf,
}

fn send(stdin: &Mutex<ChildStdin>, message: &Value) -> std::io::Result<()> {
    let body = message.to_string();
    let mut stdin = stdin.lock().expect("stdin lock");
    write!(stdin, "Content-Length: {}\r\n\r\n{body}", body.len())?;
    stdin.flush()
}

/// The rust-analyzer program: `$RUST_ANALYZER` or `rust-analyzer` from `PATH`.
pub fn program() -> PathBuf {
    std::env::var_os("RUST_ANALYZER")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("rust-analyzer"))
}

impl LspClient {
    /// Starts rust-analyzer for the Cargo project in `root`.
    pub fn start(
        root: &Path,
        on_event: impl Fn(LspEvent) + Send + Sync + 'static,
    ) -> std::io::Result<Self> {
        let mut child = Command::new(program())
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let on_event = Arc::new(on_event);
        // Fehlerausgabe des Servers als Protokollzeilen.
        if let Some(stderr) = child.stderr.take() {
            let on_event = on_event.clone();
            std::thread::spawn(move || {
                use std::io::BufRead;
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    on_event(LspEvent::Log { message: line });
                }
            });
        }
        let stdin = Arc::new(Mutex::new(child.stdin.take().expect("piped stdin")));
        let stdout = child.stdout.take().expect("piped stdout");
        let shared: Arc<Mutex<Shared>> = Arc::default();
        let client = Self {
            child: Mutex::new(child),
            stdin: stdin.clone(),
            next_id: AtomicU64::new(1),
            shared: shared.clone(),
            root: root.to_path_buf(),
        };

        // Leser-Thread: Nachrichten des Servers in Ereignisse übersetzen.
        let reader_stdin = stdin.clone();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            while let Some(message) = read_message(&mut reader) {
                handle(&message, &shared, &reader_stdin, &*on_event);
            }
            on_event(LspEvent::Exited);
        });

        let id = client.next_id.fetch_add(1, Ordering::Relaxed);
        client
            .shared
            .lock()
            .expect("lock")
            .pending
            .insert(id, "initialize".to_owned());
        send(
            &client.stdin,
            &json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "initialize",
                "params": {
                    "processId": std::process::id(),
                    "rootUri": path_to_uri(root),
                    "workspaceFolders": [{"uri": path_to_uri(root), "name": "project"}],
                    "capabilities": {
                        "general": {"positionEncodings": ["utf-8", "utf-16"]},
                        "textDocument": {
                            "synchronization": {"didSave": true},
                            "publishDiagnostics": {},
                            "completion": {"completionItem": {"snippetSupport": false}},
                            "definition": {},
                        },
                        "experimental": {"serverStatusNotification": true},
                    },
                    "initializationOptions": {
                        "checkOnSave": true,
                        // Dateien, die der Generator schreibt, selbst beobachten.
                        "files": {"watcher": "server"},
                        "cargo": {"buildScripts": {"enable": true}},
                        "procMacro": {"enable": true},
                    },
                },
            }),
        )?;
        Ok(client)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn notify(&self, method: &str, params: Value) {
        let _ = send(
            &self.stdin,
            &json!({"jsonrpc": "2.0", "method": method, "params": params}),
        );
    }

    fn request(&self, method: &str, params: Value) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.shared
            .lock()
            .expect("lock")
            .pending
            .insert(id, method.to_owned());
        let _ = send(
            &self.stdin,
            &json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}),
        );
        id
    }

    /// Opens a document (or replaces its text if already open).
    pub fn open(&self, path: &Path, text: &str) {
        let known = self
            .shared
            .lock()
            .expect("lock")
            .documents
            .contains_key(path);
        if known {
            self.change(path, text);
            return;
        }
        self.shared
            .lock()
            .expect("lock")
            .documents
            .insert(path.to_path_buf(), (text.to_owned(), 1));
        self.notify(
            "textDocument/didOpen",
            json!({"textDocument": {
                "uri": path_to_uri(path),
                "languageId": "rust",
                "version": 1,
                "text": text,
            }}),
        );
    }

    /// Sends the new full text of an open document.
    pub fn change(&self, path: &Path, text: &str) {
        let version = {
            let mut shared = self.shared.lock().expect("lock");
            let Some(entry) = shared.documents.get_mut(path) else {
                drop(shared);
                self.open(path, text);
                return;
            };
            if entry.0 == text {
                return;
            }
            entry.0 = text.to_owned();
            entry.1 += 1;
            entry.1
        };
        self.notify(
            "textDocument/didChange",
            json!({
                "textDocument": {"uri": path_to_uri(path), "version": version},
                "contentChanges": [{"text": text}],
            }),
        );
    }

    /// The file was saved (triggers `cargo check`).
    pub fn save(&self, path: &Path) {
        self.notify(
            "textDocument/didSave",
            json!({"textDocument": {"uri": path_to_uri(path)}}),
        );
    }

    pub fn close(&self, path: &Path) {
        self.shared.lock().expect("lock").documents.remove(path);
        self.notify(
            "textDocument/didClose",
            json!({"textDocument": {"uri": path_to_uri(path)}}),
        );
    }

    fn position(&self, path: &Path, line: usize, column: usize) -> Value {
        let shared = self.shared.lock().expect("lock");
        let units = shared
            .encoding()
            .units(&shared.line_text(path, line), column);
        json!({
            "textDocument": {"uri": path_to_uri(path)},
            "position": {"line": line.saturating_sub(1), "character": units},
        })
    }

    /// Asks for completions at a position; the answer is
    /// [`LspEvent::Completion`] with the returned id.
    pub fn completion(&self, path: &Path, line: usize, column: usize) -> u64 {
        let params = self.position(path, line, column);
        self.request("textDocument/completion", params)
    }

    /// Asks where the symbol at a position is defined;
    /// [`LspEvent::Definition`].
    pub fn definition(&self, path: &Path, line: usize, column: usize) -> u64 {
        let params = self.position(path, line, column);
        self.request("textDocument/definition", params)
    }

    /// Stops the server.
    pub fn shutdown(&self) {
        let _ = self.request("shutdown", Value::Null);
        self.notify("exit", Value::Null);
        if let Ok(mut child) = self.child.lock() {
            // Kurz Zeit geben, dann notfalls beenden.
            for _ in 0..20 {
                if matches!(child.try_wait(), Ok(Some(_))) {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            let _ = child.kill();
        }
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn read_message(reader: &mut impl BufRead) -> Option<Value> {
    let mut length = None;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).ok()? == 0 {
            return None;
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some(value) = header.strip_prefix("Content-Length:") {
            length = value.trim().parse::<usize>().ok();
        }
    }
    let mut body = vec![0; length?];
    reader.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

fn severity(value: &Value) -> DiagnosticSeverity {
    match value.as_u64() {
        Some(1) => DiagnosticSeverity::Error,
        Some(2) => DiagnosticSeverity::Warning,
        Some(3) => DiagnosticSeverity::Information,
        _ => DiagnosticSeverity::Hint,
    }
}

fn location(value: &Value, shared: &Shared) -> Option<Location> {
    // Location oder LocationLink.
    let uri = value["uri"]
        .as_str()
        .or_else(|| value["targetUri"].as_str())?;
    let range = if value["range"].is_object() {
        &value["range"]
    } else {
        &value["targetSelectionRange"]
    };
    let path = uri_to_path(uri)?;
    let line = range["start"]["line"].as_u64()? as usize + 1;
    let units = range["start"]["character"].as_u64()? as usize;
    let text = shared
        .documents
        .get(&path)
        .map(|(t, _)| t.clone())
        .or_else(|| std::fs::read_to_string(&path).ok())
        .unwrap_or_default();
    let line_text = text.lines().nth(line - 1).unwrap_or("");
    Some(Location {
        column: shared.encoding().column(line_text, units),
        path,
        line,
    })
}

fn handle(
    message: &Value,
    shared: &Mutex<Shared>,
    stdin: &Mutex<ChildStdin>,
    on_event: &impl Fn(LspEvent),
) {
    // Anfrage des Servers an uns (z. B. workDoneProgress/create): bestätigen.
    if let (Some(id), Some(_)) = (message.get("id"), message["method"].as_str()) {
        let _ = send(
            stdin,
            &json!({"jsonrpc": "2.0", "id": id, "result": Value::Null}),
        );
        return;
    }
    // Antwort auf eine unserer Anfragen.
    if let Some(id) = message["id"].as_u64() {
        let method = shared.lock().expect("lock").pending.remove(&id);
        if let Some(error) = message.get("error") {
            on_event(LspEvent::Error {
                request: Some(id),
                message: error["message"].as_str().unwrap_or("Fehler").to_owned(),
            });
            return;
        }
        let result = &message["result"];
        match method.as_deref() {
            Some("initialize") => {
                let encoding = match result["capabilities"]["positionEncoding"].as_str() {
                    Some("utf-8") => Encoding::Utf8,
                    _ => Encoding::Utf16,
                };
                shared.lock().expect("lock").encoding = Some(encoding);
                let _ = send(
                    stdin,
                    &json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}),
                );
                on_event(LspEvent::Ready);
            }
            Some("textDocument/completion") => {
                let list = if result.is_array() {
                    result
                } else {
                    &result["items"]
                };
                let items = list
                    .as_array()
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| {
                                let label = item["label"].as_str()?.to_owned();
                                let insert = item["textEdit"]["newText"]
                                    .as_str()
                                    .or_else(|| item["insertText"].as_str())
                                    .unwrap_or(&label)
                                    .to_owned();
                                Some(CompletionItem {
                                    detail: item["detail"].as_str().map(str::to_owned),
                                    label,
                                    insert,
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                on_event(LspEvent::Completion { request: id, items });
            }
            Some("textDocument/definition") => {
                let shared = shared.lock().expect("lock");
                let locations = match result {
                    Value::Array(items) => {
                        items.iter().filter_map(|l| location(l, &shared)).collect()
                    }
                    Value::Object(_) => location(result, &shared).into_iter().collect(),
                    _ => Vec::new(),
                };
                drop(shared);
                on_event(LspEvent::Definition {
                    request: id,
                    locations,
                });
            }
            _ => {}
        }
        return;
    }
    // Benachrichtigungen.
    match message["method"].as_str() {
        Some("textDocument/publishDiagnostics") => {
            let params = &message["params"];
            let Some(path) = params["uri"].as_str().and_then(uri_to_path) else {
                return;
            };
            let shared = shared.lock().expect("lock");
            let text = shared
                .documents
                .get(&path)
                .map(|(t, _)| t.clone())
                .or_else(|| std::fs::read_to_string(&path).ok())
                .unwrap_or_default();
            let encoding = shared.encoding();
            drop(shared);
            let items = params["diagnostics"]
                .as_array()
                .map(|list| {
                    list.iter()
                        .filter_map(|d| {
                            let line = d["range"]["start"]["line"].as_u64()? as usize + 1;
                            let units = d["range"]["start"]["character"].as_u64()? as usize;
                            let line_text = text.lines().nth(line - 1).unwrap_or("");
                            Some(Diagnostic {
                                line,
                                column: encoding.column(line_text, units),
                                severity: severity(&d["severity"]),
                                message: d["message"].as_str().unwrap_or_default().to_owned(),
                                code: d["code"]
                                    .as_str()
                                    .map(str::to_owned)
                                    .or_else(|| d["code"].as_u64().map(|c| c.to_string())),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            on_event(LspEvent::Diagnostics { path, items });
        }
        Some("window/logMessage" | "window/showMessage") => {
            if let Some(text) = message["params"]["message"].as_str() {
                on_event(LspEvent::Log {
                    message: text.to_owned(),
                });
            }
        }
        Some("experimental/serverStatus") => {
            let params = &message["params"];
            on_event(LspEvent::Status {
                quiescent: params["quiescent"].as_bool().unwrap_or(false),
                message: params["message"].as_str().map(str::to_owned),
            });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uris_round_trip() {
        let path = Path::new("/home/user/my project/src/main.rs");
        let uri = path_to_uri(path);
        assert_eq!(uri, "file:///home/user/my%20project/src/main.rs");
        assert_eq!(uri_to_path(&uri).unwrap(), path);
        assert_eq!(
            uri_to_path("file:///C:/proj/main.rs").unwrap(),
            PathBuf::from("C:/proj/main.rs")
        );
    }

    #[test]
    fn encodings() {
        let line = "let ä = \"€\"; x";
        // Spalte 9 (Zeichen) = nach dem Euro-Zeichen.
        assert_eq!(Encoding::Utf16.units(line, 10), 10);
        assert_eq!(Encoding::Utf8.units(line, 10), 13);
        assert_eq!(Encoding::Utf8.column(line, 13), 10);
    }

    #[test]
    fn framing() {
        let body = r#"{"jsonrpc":"2.0","id":1,"result":null}"#;
        let raw = format!("Content-Length: {}\r\n\r\n{body}", body.len());
        let message = read_message(&mut raw.as_bytes()).unwrap();
        assert_eq!(message["id"], 1);
    }
}
