//! Debugger – a Debug Adapter Protocol (DAP) client without UI.
//!
//! The adapter runs as its own process over stdio: `lldb-dap` (LLVM), or
//! `gdb -i dap` (gdb ≥ 14) when there is no lldb-dap; `FREYA_TDE_DAP` names
//! another one ([`Adapter::find`]). A [`DebugSession`] starts the program
//! under the adapter and keeps everything the UI shows as a serializable
//! [`Snapshot`]: state, threads, call stack, variables, watch expressions,
//! breakpoint status and the debug console. When the program stops, the
//! session loads the call stack, the locals of the selected frame and the
//! watch values by itself. It is driven by [`DebugCommand`]s (JSON, for an
//! AI) or the methods behind them; [`DebugSession::wait_for`] waits for a
//! state (tests, scripts).
//!
//! Debug console input is evaluated as an expression in the selected frame;
//! input starting with `` ` `` goes to the debugger as a command
//! (`` `bt ``, `` `info locals ``).
//!
//! Breakpoints and watches of a project are kept per user by
//! [`DebugStore`].

mod store;

use crate::tools::NoConsole;
use crate::tr;
use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub use store::{Breakpoint, BreakpointKind, DebugSettings, DebugStore, line_shift};

/// Console lines kept at most; older ones are dropped.
pub const CONSOLE_LIMIT: usize = 5000;

/// How many frames of a call stack are loaded.
const STACK_DEPTH: i64 = 100;

/// A failure with a stable code: `adapter_not_found`, `adapter_failed`
/// (could not be started), `not_running` (the session has ended),
/// `not_stopped` (only possible while the program is stopped),
/// `unknown_thread`, `unknown_frame`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DebugError {
    pub code: &'static str,
    pub detail: String,
}

impl DebugError {
    fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for DebugError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.detail, self.code)
    }
}

impl std::error::Error for DebugError {}

// --- Adapter --------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterKind {
    /// `lldb-dap` (formerly `lldb-vscode`).
    Lldb,
    /// `gdb -i dap`.
    Gdb,
}

/// The debug adapter program.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Adapter {
    pub kind: AdapterKind,
    pub program: PathBuf,
}

/// Finds `name` (plus `.exe` on Windows) in `PATH`.
fn which(name: &str) -> Option<PathBuf> {
    let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(&file))
        .find(|p| p.is_file())
}

/// Major version of `gdb --version` („GNU gdb (GDB) 18.1“).
fn gdb_major(program: &Path) -> Option<u32> {
    let out = Command::new(program)
        .no_console()
        .arg("--version")
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let first = text.lines().next()?;
    let version = first.split_whitespace().last()?;
    version.split('.').next()?.parse().ok()
}

impl Adapter {
    /// The adapter for `program`; gdb if its name contains `gdb`.
    pub fn from_program(program: impl Into<PathBuf>) -> Self {
        let program = program.into();
        let name = program
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let kind = if name.contains("gdb") {
            AdapterKind::Gdb
        } else {
            AdapterKind::Lldb
        };
        Self { kind, program }
    }

    /// `$FREYA_TDE_DAP`, else `lldb-dap` (also `lldb-vscode`, Xcode's and
    /// Homebrew's on macOS, LLVM's in `%ProgramFiles%` on Windows), else gdb ≥ 14.
    pub fn find() -> Result<Self, DebugError> {
        if let Some(program) = std::env::var_os("FREYA_TDE_DAP").filter(|p| !p.is_empty()) {
            return Ok(Self::from_program(program));
        }
        if let Some(program) = Self::find_kind(AdapterKind::Lldb) {
            return Ok(Self::from_program(program));
        }
        if let Some(program) = Self::find_kind(AdapterKind::Gdb) {
            return Ok(Self {
                kind: AdapterKind::Gdb,
                program,
            });
        }
        Err(DebugError::new(
            "adapter_not_found",
            "Kein Debugger gefunden: lldb-dap (LLVM) oder gdb ab Version 14 installieren \
             oder FREYA_TDE_DAP setzen.",
        ))
    }

    /// The program of one adapter kind, if installed.
    pub fn find_kind(kind: AdapterKind) -> Option<PathBuf> {
        match kind {
            AdapterKind::Lldb => {
                let mut found = which("lldb-dap").or_else(|| which("lldb-vscode"));
                // Windows: der LLVM-Installer (winget LLVM.LLVM) trägt sich nicht in
                // den PATH ein.
                if found.is_none() && cfg!(windows) {
                    let program_files = std::env::var_os("ProgramFiles")
                        .map(PathBuf::from)
                        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
                    found =
                        Some(program_files.join(r"LLVM\bin\lldb-dap.exe")).filter(|p| p.is_file());
                }
                if found.is_none() && cfg!(target_os = "macos") {
                    found = Command::new("xcrun")
                        .args(["-f", "lldb-dap"])
                        .output()
                        .ok()
                        .filter(|o| o.status.success())
                        .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()))
                        .filter(|p| p.is_file())
                        .or_else(|| {
                            [
                                "/opt/homebrew/opt/llvm/bin/lldb-dap",
                                "/usr/local/opt/llvm/bin/lldb-dap",
                            ]
                            .into_iter()
                            .map(PathBuf::from)
                            .find(|p| p.is_file())
                        });
                }
                found
            }
            AdapterKind::Gdb => which("gdb").filter(|p| gdb_major(p).is_some_and(|v| v >= 14)),
        }
    }

    fn name(&self) -> &'static str {
        match self.kind {
            AdapterKind::Lldb => "lldb-dap",
            AdapterKind::Gdb => "gdb",
        }
    }
}

/// The Rust sysroot of the toolchain in `dir` (pretty printers live in
/// `lib/rustlib/etc`).
fn rust_sysroot(dir: &Path) -> Option<PathBuf> {
    let out = Command::new("rustc")
        .no_console()
        .args(["--print", "sysroot"])
        .current_dir(dir)
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    Some(PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()))
}

// --- Launch configuration and snapshot --------------------------------------------------

/// What to debug.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchConfig {
    pub program: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
    pub cwd: PathBuf,
    /// Extra environment variables.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

impl LaunchConfig {
    pub fn new(program: impl Into<PathBuf>, cwd: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: cwd.into(),
            env: BTreeMap::new(),
        }
    }
}

/// Where the session is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum State {
    /// Adapter and program are starting.
    #[default]
    Starting,
    Running,
    /// `reason`: `breakpoint`, `step`, `pause`, `entry`, `panic`,
    /// `exception`, `goto` …
    Stopped {
        reason: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    /// The program and the adapter are gone.
    Ended {
        #[serde(skip_serializing_if = "Option::is_none")]
        exit_code: Option<i64>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Thread {
    pub id: i64,
    pub name: String,
}

/// A frame of the call stack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Frame {
    pub id: i64,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    /// 1-based; 0 = unknown.
    pub line: usize,
    pub column: usize,
    /// The source file exists here (else: library code without source).
    pub has_source: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Scope {
    pub name: String,
    /// Key into [`Snapshot::variables`].
    pub reference: i64,
    /// Loading is slow (globals, registers); not loaded by itself.
    pub expensive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Variable {
    pub name: String,
    pub value: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
    /// Children (fields, elements) – 0 = none. Load with
    /// [`DebugCommand::LoadVariables`].
    pub reference: i64,
    /// An expression for this value (for watches).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evaluate_name: Option<String>,
}

/// A watch expression and its value in the selected frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Watch {
    pub expression: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
    pub reference: i64,
}

/// What the adapter made of a breakpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BreakpointStatus {
    pub path: PathBuf,
    /// The line it was set on.
    pub line: usize,
    /// Resolved to code.
    pub verified: bool,
    /// The line it actually sits on, if the adapter moved it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip)]
    id: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsoleKind {
    /// Output of the program (stdout).
    Stdout,
    /// Error output of the program.
    Stderr,
    /// Messages of the debugger.
    Debugger,
    /// What the user typed.
    Input,
    /// Result of an evaluation.
    Result,
    /// Failed evaluation.
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConsoleLine {
    pub kind: ConsoleKind,
    pub text: String,
}

/// Everything about the session at one moment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct Snapshot {
    pub adapter: String,
    #[serde(flatten)]
    pub state: State,
    pub threads: Vec<Thread>,
    /// The selected thread.
    pub thread: Option<i64>,
    /// Call stack of the selected thread (innermost first).
    pub frames: Vec<Frame>,
    /// The selected frame.
    pub frame: Option<i64>,
    pub scopes: Vec<Scope>,
    /// Loaded variables by reference (scopes and expanded values).
    pub variables: BTreeMap<i64, Vec<Variable>>,
    pub watches: Vec<Watch>,
    pub breakpoints: Vec<BreakpointStatus>,
    pub console: Vec<ConsoleLine>,
    /// Counts changes.
    pub version: u64,
}

impl Snapshot {
    pub fn is_stopped(&self) -> bool {
        matches!(self.state, State::Stopped { .. })
    }

    pub fn is_ended(&self) -> bool {
        matches!(self.state, State::Ended { .. })
    }

    /// The selected frame.
    pub fn current_frame(&self) -> Option<&Frame> {
        self.frames.iter().find(|f| Some(f.id) == self.frame)
    }

    /// Where the program stands: file and line of the selected frame.
    pub fn location(&self) -> Option<(&Path, usize)> {
        let frame = self.current_frame()?;
        Some((frame.path.as_deref()?, frame.line))
    }

    /// The variables of the first scope that is loaded (the locals).
    pub fn locals(&self) -> &[Variable] {
        self.scopes
            .iter()
            .find_map(|s| self.variables.get(&s.reference))
            .map_or(&[], Vec::as_slice)
    }

    /// The console as text, one entry per line.
    pub fn console_text(&self) -> String {
        let mut text = String::new();
        for line in &self.console {
            text.push_str(&line.text);
            if !line.text.ends_with('\n') {
                text.push('\n');
            }
        }
        text
    }
}

/// A command for the session (JSON with `"command"`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum DebugCommand {
    Continue,
    Pause,
    /// One line, over calls (F8).
    StepOver,
    /// One line, into calls (F7).
    StepInto,
    /// To the caller (Shift+F8).
    StepOut,
    /// Continue to a line (F4).
    RunTo {
        path: PathBuf,
        line: usize,
    },
    /// End program and adapter.
    Stop,
    SelectThread {
        id: i64,
    },
    SelectFrame {
        id: i64,
    },
    LoadVariables {
        reference: i64,
    },
    /// Debug console input (expression, or `` `befehl `` for the debugger).
    Evaluate {
        expression: String,
    },
    SetBreakpoints {
        breakpoints: Vec<Breakpoint>,
    },
    SetWatches {
        watches: Vec<String>,
    },
    SetStopOnPanic {
        enabled: bool,
    },
}

// --- Session ---------------------------------------------------------------------------

/// What an open request was for.
#[derive(Debug, Clone)]
enum Pending {
    Initialize,
    Launch,
    SetBreakpoints(PathBuf),
    SetFunctionBreakpoints,
    ConfigurationDone,
    Threads,
    StackTrace(i64),
    Scopes(i64),
    Variables(i64),
    /// Watch `index` for the frame.
    Watch(usize, i64),
    /// Debug console input.
    Console,
    Other,
}

struct Data {
    snap: Snapshot,
    pending: HashMap<i64, Pending>,
    seq: i64,
    config: LaunchConfig,
    breakpoints: Vec<Breakpoint>,
    stop_on_panic: bool,
    /// Ids of the function breakpoints (panic).
    panic_ids: Vec<i64>,
    /// Temporary breakpoint of „run to“.
    run_to: Option<(PathBuf, usize)>,
    /// The program has started (`process` event); debugger chatter before
    /// that is not shown.
    process_started: bool,
    exit_code: Option<i64>,
    configured: bool,
}

struct Shared {
    adapter: Adapter,
    stdin: Mutex<Option<ChildStdin>>,
    child: Mutex<Child>,
    data: Mutex<Data>,
    changed: Condvar,
    on_change: Box<dyn Fn() + Send + Sync>,
}

/// A running debug session (adapter process plus program).
pub struct DebugSession {
    shared: Arc<Shared>,
}

fn write_message(stdin: &mut impl Write, message: &Value) -> std::io::Result<()> {
    let body = message.to_string();
    write!(stdin, "Content-Length: {}\r\n\r\n{body}", body.len())?;
    stdin.flush()
}

/// Reads one `Content-Length` framed message.
fn read_message(reader: &mut impl BufRead) -> Option<Value> {
    let mut length = None;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).ok()? == 0 {
            return None;
        }
        let header = header.trim_end();
        if header.is_empty() {
            if length.is_some() {
                break;
            }
            continue;
        }
        if let Some(value) = header.strip_prefix("Content-Length:") {
            length = value.trim().parse::<usize>().ok();
        }
    }
    let mut body = vec![0; length?];
    reader.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

fn int(value: &Value) -> Option<i64> {
    value.as_i64()
}

fn text(value: &Value) -> Option<String> {
    value.as_str().map(str::to_owned)
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Data> {
        self.data.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Sends a request (call with the data lock held, so the answer finds
    /// its entry).
    fn request(&self, data: &mut Data, command: &str, arguments: Value, pending: Pending) -> i64 {
        data.seq += 1;
        let seq = data.seq;
        data.pending.insert(seq, pending);
        let message = json!({
            "seq": seq,
            "type": "request",
            "command": command,
            "arguments": arguments,
        });
        if let Some(stdin) = self
            .stdin
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
        {
            let _ = write_message(stdin, &message);
        }
        seq
    }

    fn respond(&self, data: &mut Data, request: &Value, success: bool) {
        data.seq += 1;
        let message = json!({
            "seq": data.seq,
            "type": "response",
            "request_seq": request["seq"],
            "command": request["command"],
            "success": success,
        });
        if let Some(stdin) = self
            .stdin
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
        {
            let _ = write_message(stdin, &message);
        }
    }

    fn changed(&self, data: &mut Data) {
        data.snap.version += 1;
        self.changed.notify_all();
        (self.on_change)();
    }

    fn console(&self, data: &mut Data, kind: ConsoleKind, text: &str) {
        let console = &mut data.snap.console;
        // Programmausgabe in Stücken: an die offene Zeile anhängen.
        if let Some(last) = console.last_mut()
            && last.kind == kind
            && matches!(kind, ConsoleKind::Stdout | ConsoleKind::Stderr)
            && !last.text.ends_with('\n')
        {
            last.text.push_str(text);
        } else {
            console.push(ConsoleLine {
                kind,
                text: text.to_owned(),
            });
        }
        if console.len() > CONSOLE_LIMIT {
            let cut = console.len() - CONSOLE_LIMIT;
            console.drain(..cut);
        }
    }

    // --- Requests ----------------------------------------------------------------

    fn launch(&self, data: &mut Data) {
        let config = data.config.clone();
        let mut args = json!({
            "program": config.program,
            "args": config.args,
            "cwd": config.cwd,
            "env": config.env,
        });
        match self.adapter.kind {
            AdapterKind::Lldb => {
                args["stopOnEntry"] = json!(false);
                // Rust-Darstellung (String, Vec, Option …) wie rust-lldb.
                if let Some(sysroot) = rust_sysroot(&config.cwd) {
                    let etc = sysroot.join("lib/rustlib/etc");
                    if etc.join("lldb_lookup.py").is_file() {
                        args["initCommands"] = json!([
                            format!(
                                "command script import \"{}\"",
                                etc.join("lldb_lookup.py").display()
                            ),
                            format!(
                                "command source -s 0 \"{}\"",
                                etc.join("lldb_commands").display()
                            ),
                        ]);
                    }
                }
            }
            AdapterKind::Gdb => {}
        }
        self.request(data, "launch", args, Pending::Launch);
    }

    fn file_breakpoints(data: &Data, path: &Path) -> Vec<Value> {
        let mut list: Vec<Value> = data
            .breakpoints
            .iter()
            .filter(|b| b.path == path && b.enabled)
            .map(|b| {
                let mut v = json!({"line": b.line});
                if let Some(c) = &b.condition {
                    v["condition"] = json!(c);
                }
                if let Some(c) = &b.hit_condition {
                    v["hitCondition"] = json!(c);
                }
                if let Some(m) = &b.log_message {
                    v["logMessage"] = json!(m);
                }
                v
            })
            .collect();
        if let Some((p, line)) = &data.run_to
            && p == path
            && !list.iter().any(|v| v["line"] == json!(line))
        {
            list.push(json!({"line": line}));
        }
        list
    }

    fn send_breakpoints(&self, data: &mut Data, path: &Path) {
        let list = Self::file_breakpoints(data, path);
        self.request(
            data,
            "setBreakpoints",
            json!({
                "source": {"path": path, "name": path.file_name().map(|n| n.to_string_lossy())},
                "breakpoints": list,
                "lines": list.iter().map(|v| v["line"].clone()).collect::<Vec<_>>(),
            }),
            Pending::SetBreakpoints(path.to_path_buf()),
        );
    }

    fn send_function_breakpoints(&self, data: &mut Data) {
        let mut list = Vec::new();
        if data.stop_on_panic {
            // lldb findet den Namen allein, gdb braucht den Pfad.
            let name = match self.adapter.kind {
                AdapterKind::Lldb => "rust_panic",
                AdapterKind::Gdb => "std::panicking::rust_panic",
            };
            list.push(json!({"name": name}));
        }
        self.request(
            data,
            "setFunctionBreakpoints",
            json!({"breakpoints": list}),
            Pending::SetFunctionBreakpoints,
        );
    }

    /// Statuses for the breakpoints of `path` in sending order.
    fn reset_status(data: &mut Data, path: &Path) {
        data.snap.breakpoints.retain(|b| b.path != path);
        let lines: Vec<usize> = data
            .breakpoints
            .iter()
            .filter(|b| b.path == path && b.enabled)
            .map(|b| b.line)
            .collect();
        for line in lines {
            data.snap.breakpoints.push(BreakpointStatus {
                path: path.to_path_buf(),
                line,
                verified: false,
                actual_line: None,
                message: None,
                id: None,
            });
        }
    }

    fn configure(&self, data: &mut Data) {
        let files: Vec<PathBuf> = data
            .breakpoints
            .iter()
            .map(|b| b.path.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        for path in files {
            Self::reset_status(data, &path);
            self.send_breakpoints(data, &path);
        }
        self.send_function_breakpoints(data);
        self.request(
            data,
            "configurationDone",
            json!({}),
            Pending::ConfigurationDone,
        );
        data.configured = true;
    }

    fn clear_stop(data: &mut Data) {
        data.snap.frames.clear();
        data.snap.frame = None;
        data.snap.scopes.clear();
        data.snap.variables.clear();
        for w in &mut data.snap.watches {
            w.value = None;
            w.error = None;
            w.type_name = None;
            w.reference = 0;
        }
    }

    fn load_frame(&self, data: &mut Data, frame: i64) {
        data.snap.frame = Some(frame);
        data.snap.scopes.clear();
        data.snap.variables.clear();
        self.request(
            data,
            "scopes",
            json!({"frameId": frame}),
            Pending::Scopes(frame),
        );
        self.evaluate_watches(data);
    }

    fn evaluate_watches(&self, data: &mut Data) {
        let Some(frame) = data.snap.frame else {
            return;
        };
        for i in 0..data.snap.watches.len() {
            let expression = data.snap.watches[i].expression.clone();
            self.request(
                data,
                "evaluate",
                json!({"expression": expression, "frameId": frame, "context": "watch"}),
                Pending::Watch(i, frame),
            );
        }
    }

    // --- Messages from the adapter ----------------------------------------------------

    fn handle(&self, message: &Value) {
        let mut data = self.lock();
        match message["type"].as_str() {
            Some("response") => {
                let Some(pending) =
                    int(&message["request_seq"]).and_then(|s| data.pending.remove(&s))
                else {
                    return;
                };
                self.response(&mut data, pending, message);
            }
            Some("event") => self.event(&mut data, message),
            // Rückfragen (runInTerminal …) bieten wir nicht an.
            Some("request") => self.respond(&mut data, message, false),
            _ => return,
        }
        self.changed(&mut data);
    }

    fn response(&self, data: &mut Data, pending: Pending, message: &Value) {
        let success = message["success"].as_bool().unwrap_or(false);
        let body = &message["body"];
        let error = || {
            text(&body["error"]["format"])
                .or_else(|| text(&message["message"]))
                .unwrap_or_else(|| tr!("problems.error"))
        };
        match pending {
            Pending::Initialize => {
                if success {
                    self.launch(data);
                } else {
                    let e = error();
                    self.console(data, ConsoleKind::Error, &format!("Debugger: {e}\n"));
                }
            }
            Pending::Launch => {
                if !success {
                    let e = error();
                    self.console(
                        data,
                        ConsoleKind::Error,
                        &format!("Start fehlgeschlagen: {e}\n"),
                    );
                    self.request(
                        data,
                        "disconnect",
                        json!({"terminateDebuggee": true}),
                        Pending::Other,
                    );
                }
            }
            Pending::SetBreakpoints(path) => {
                let list = body["breakpoints"].as_array().cloned().unwrap_or_default();
                let mut statuses = data.snap.breakpoints.iter_mut().filter(|b| b.path == path);
                for answer in &list {
                    let Some(status) = statuses.next() else {
                        break;
                    };
                    status.id = int(&answer["id"]);
                    status.verified = answer["verified"].as_bool().unwrap_or(false);
                    status.actual_line = answer["line"]
                        .as_u64()
                        .map(|l| l as usize)
                        .filter(|l| *l != status.line);
                    status.message = text(&answer["message"]);
                }
            }
            Pending::SetFunctionBreakpoints => {
                data.panic_ids = body["breakpoints"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .take(usize::from(data.stop_on_panic))
                    .filter_map(|b| int(&b["id"]))
                    .collect();
            }
            Pending::ConfigurationDone => {}
            Pending::Threads => {
                data.snap.threads = body["threads"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|t| {
                        Some(Thread {
                            id: int(&t["id"])?,
                            name: text(&t["name"]).unwrap_or_default(),
                        })
                    })
                    .collect();
                if data.snap.thread.is_none() {
                    data.snap.thread = data.snap.threads.first().map(|t| t.id);
                }
            }
            Pending::StackTrace(thread) => {
                if data.snap.thread != Some(thread) || !data.snap.is_stopped() {
                    return;
                }
                data.snap.frames = body["stackFrames"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|f| {
                        let path = text(&f["source"]["path"]).map(PathBuf::from);
                        let has_source = path.as_deref().is_some_and(Path::is_file)
                            && f["source"]["presentationHint"] != "deemphasize";
                        Some(Frame {
                            id: int(&f["id"])?,
                            name: text(&f["name"]).unwrap_or_default(),
                            path,
                            line: f["line"].as_u64().unwrap_or(0) as usize,
                            column: f["column"].as_u64().unwrap_or(0) as usize,
                            has_source,
                        })
                    })
                    .collect();
                // Der innerste Rahmen im Projekt, sonst mit Quelltext (bei
                // einem Panic steht der Code des Nutzers weiter außen; die
                // Quellen der Standardbibliothek liegen oft auch vor).
                let project = data.config.cwd.clone();
                let frames = &data.snap.frames;
                let frame = frames
                    .iter()
                    .find(|f| {
                        f.has_source && f.path.as_deref().is_some_and(|p| p.starts_with(&project))
                    })
                    .or_else(|| frames.iter().find(|f| f.has_source))
                    .or(frames.first())
                    .map(|f| f.id);
                if let Some(frame) = frame {
                    self.load_frame(data, frame);
                }
            }
            Pending::Scopes(frame) => {
                if data.snap.frame != Some(frame) {
                    return;
                }
                data.snap.scopes = body["scopes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|s| {
                        Some(Scope {
                            name: text(&s["name"])?,
                            reference: int(&s["variablesReference"])?,
                            expensive: s["expensive"].as_bool().unwrap_or(false),
                        })
                    })
                    .collect();
                let cheap: Vec<i64> = data
                    .snap
                    .scopes
                    .iter()
                    .filter(|s| !s.expensive && s.reference > 0)
                    .map(|s| s.reference)
                    .collect();
                for reference in cheap {
                    self.request(
                        data,
                        "variables",
                        json!({"variablesReference": reference}),
                        Pending::Variables(reference),
                    );
                }
            }
            Pending::Variables(reference) => {
                if !data.snap.is_stopped() {
                    return;
                }
                let list = body["variables"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|v| Variable {
                        name: text(&v["name"]).unwrap_or_default(),
                        value: text(&v["value"]).unwrap_or_default(),
                        type_name: text(&v["type"]).filter(|t| !t.is_empty()),
                        reference: int(&v["variablesReference"]).unwrap_or(0),
                        evaluate_name: text(&v["evaluateName"]),
                    })
                    .collect();
                data.snap.variables.insert(reference, list);
            }
            Pending::Watch(index, frame) => {
                if data.snap.frame != Some(frame) {
                    return;
                }
                if let Some(watch) = data.snap.watches.get_mut(index) {
                    let result = text(&body["result"]).unwrap_or_default();
                    // lldb meldet Fehler als Ergebnis mit „error:“.
                    if success && !result.starts_with("error:") {
                        watch.value = Some(result);
                        watch.error = None;
                        watch.type_name = text(&body["type"]).filter(|t| !t.is_empty());
                        watch.reference = int(&body["variablesReference"]).unwrap_or(0);
                    } else {
                        watch.value = None;
                        watch.error = Some(if success {
                            result.trim().to_owned()
                        } else {
                            error()
                        });
                        watch.reference = 0;
                    }
                }
            }
            Pending::Console => {
                let result = text(&body["result"]).unwrap_or_default();
                if success && !result.starts_with("error:") {
                    let reference = int(&body["variablesReference"]).unwrap_or(0);
                    if reference > 0 {
                        self.request(
                            data,
                            "variables",
                            json!({"variablesReference": reference}),
                            Pending::Variables(reference),
                        );
                    }
                    self.console(data, ConsoleKind::Result, &format!("{result}\n"));
                } else {
                    let e = if success {
                        result.trim().to_owned()
                    } else {
                        error()
                    };
                    self.console(data, ConsoleKind::Error, &format!("{e}\n"));
                }
            }
            Pending::Other => {
                if !success {
                    let e = error();
                    self.console(data, ConsoleKind::Error, &format!("{e}\n"));
                }
            }
        }
    }

    fn event(&self, data: &mut Data, message: &Value) {
        let body = &message["body"];
        match message["event"].as_str().unwrap_or_default() {
            "initialized" => self.configure(data),
            "process" => {
                data.process_started = true;
                if !data.snap.is_stopped() {
                    data.snap.state = State::Running;
                }
            }
            "output" => {
                let category = body["category"].as_str().unwrap_or("console");
                let Some(output) = body["output"].as_str() else {
                    return;
                };
                let kind = match category {
                    "stdout" => ConsoleKind::Stdout,
                    "stderr" => ConsoleKind::Stderr,
                    "telemetry" => return,
                    _ => ConsoleKind::Debugger,
                };
                // Begrüßung und Meldungen vor dem Start (z. B. „No symbol
                // table“ bei gdb, das die Haltepunkte später auflöst).
                if !data.process_started {
                    return;
                }
                self.console(data, kind, &output.replace("\r\n", "\n"));
            }
            "stopped" => {
                let reason = body["reason"].as_str().unwrap_or("pause").to_owned();
                let hit: Vec<i64> = body["hitBreakpointIds"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(int)
                    .collect();
                let reason = if hit.iter().any(|id| data.panic_ids.contains(id)) {
                    "panic".to_owned()
                } else {
                    reason
                };
                data.snap.state = State::Stopped {
                    reason,
                    description: text(&body["description"]).or_else(|| text(&body["text"])),
                };
                Self::clear_stop(data);
                if let Some(thread) = int(&body["threadId"]) {
                    data.snap.thread = Some(thread);
                }
                // Ziel von „bis Cursor“ erreicht (oder vorher angehalten).
                if let Some((path, _)) = data.run_to.take() {
                    Self::reset_status(data, &path);
                    self.send_breakpoints(data, &path);
                }
                self.request(data, "threads", json!({}), Pending::Threads);
                if let Some(thread) = data.snap.thread {
                    self.request(
                        data,
                        "stackTrace",
                        json!({"threadId": thread, "startFrame": 0, "levels": STACK_DEPTH}),
                        Pending::StackTrace(thread),
                    );
                }
            }
            "continued" => {
                if !data.snap.is_ended() {
                    data.snap.state = State::Running;
                    Self::clear_stop(data);
                }
            }
            "thread" => {
                if body["reason"] == "started" && data.snap.thread.is_none() {
                    data.snap.thread = int(&body["threadId"]);
                }
            }
            "breakpoint" => {
                let b = &body["breakpoint"];
                if let Some(id) = int(&b["id"])
                    && let Some(status) =
                        data.snap.breakpoints.iter_mut().find(|s| s.id == Some(id))
                {
                    status.verified = b["verified"].as_bool().unwrap_or(status.verified);
                    if let Some(line) = b["line"].as_u64() {
                        status.actual_line = Some(line as usize).filter(|l| *l != status.line);
                    }
                }
            }
            "exited" => {
                data.exit_code = int(&body["exitCode"]);
                let code = data.exit_code.unwrap_or_default();
                self.console(
                    data,
                    ConsoleKind::Debugger,
                    &format!("Programm beendet (Exit-Code {code}).\n"),
                );
            }
            "terminated" => {
                self.request(
                    data,
                    "disconnect",
                    json!({"terminateDebuggee": true}),
                    Pending::Other,
                );
            }
            _ => {}
        }
    }

    fn ended(&self) {
        let mut data = self.lock();
        let exit_code = data.exit_code;
        data.snap.state = State::Ended { exit_code };
        Self::clear_stop(&mut data);
        data.snap.threads.clear();
        data.pending.clear();
        self.changed(&mut data);
    }
}

impl DebugSession {
    /// Starts `adapter` and the program in `config` with these breakpoints
    /// and watches. `on_change` runs (on a reader thread) after every
    /// change of the snapshot.
    pub fn start(
        adapter: &Adapter,
        config: LaunchConfig,
        settings: &DebugSettings,
        on_change: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, DebugError> {
        let mut command = Command::new(&adapter.program);
        command.no_console();
        if adapter.kind == AdapterKind::Gdb {
            command.args(["-q", "-i", "dap"]);
            // Rust-Darstellung wie rust-gdb: Skripte der Toolchain zulassen.
            if let Some(sysroot) = rust_sysroot(&config.cwd) {
                let etc = sysroot.join("lib/rustlib/etc");
                command
                    .arg("-iex")
                    .arg(format!("add-auto-load-safe-path {}", etc.display()));
                command
                    .arg("-iex")
                    .arg(format!("directory {}", etc.display()));
            }
        }
        let mut child = command
            .current_dir(&config.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                DebugError::new(
                    "adapter_failed",
                    tr!(
                        "debug.adapter_failed",
                        program = adapter.program.display(),
                        error = e
                    ),
                )
            })?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take();
        let data = Data {
            snap: Snapshot {
                adapter: adapter.name().into(),
                watches: settings
                    .watches
                    .iter()
                    .map(|e| Watch {
                        expression: e.clone(),
                        value: None,
                        error: None,
                        type_name: None,
                        reference: 0,
                    })
                    .collect(),
                ..Snapshot::default()
            },
            pending: HashMap::new(),
            seq: 0,
            config,
            breakpoints: settings.breakpoints.clone(),
            stop_on_panic: settings.stop_on_panic,
            panic_ids: Vec::new(),
            run_to: None,
            process_started: false,
            exit_code: None,
            configured: false,
        };
        let shared = Arc::new(Shared {
            adapter: adapter.clone(),
            stdin: Mutex::new(stdin),
            child: Mutex::new(child),
            data: Mutex::new(data),
            changed: Condvar::new(),
            on_change: Box::new(on_change),
        });
        // Fehlerausgabe des Adapters in die Konsole.
        if let Some(stderr) = stderr {
            let shared = shared.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    let mut data = shared.lock();
                    shared.console(&mut data, ConsoleKind::Debugger, &format!("{line}\n"));
                    shared.changed(&mut data);
                }
            });
        }
        {
            let shared = shared.clone();
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stdout);
                while let Some(message) = read_message(&mut reader) {
                    shared.handle(&message);
                }
                let _ = shared
                    .child
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .wait();
                shared.ended();
            });
        }
        {
            let mut data = shared.lock();
            shared.request(
                &mut data,
                "initialize",
                json!({
                    "clientID": "freya-tde",
                    "clientName": "Freya TDE",
                    "adapterID": adapter.name(),
                    "pathFormat": "path",
                    "linesStartAt1": true,
                    "columnsStartAt1": true,
                    "supportsVariableType": true,
                    "supportsRunInTerminalRequest": false,
                    "locale": "de",
                }),
                Pending::Initialize,
            );
        }
        Ok(Self { shared })
    }

    pub fn adapter(&self) -> &Adapter {
        &self.shared.adapter
    }

    /// A copy of the current state.
    pub fn snapshot(&self) -> Snapshot {
        self.shared.lock().snap.clone()
    }

    pub fn version(&self) -> u64 {
        self.shared.lock().snap.version
    }

    /// Waits until `done` holds for the snapshot (true) or `timeout` passes.
    pub fn wait_for(&self, timeout: Duration, done: impl Fn(&Snapshot) -> bool) -> bool {
        let end = Instant::now() + timeout;
        let mut data = self.shared.lock();
        loop {
            if done(&data.snap) {
                return true;
            }
            let now = Instant::now();
            if now >= end {
                return false;
            }
            data = self
                .shared
                .changed
                .wait_timeout(data, end - now)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }

    /// Runs a command.
    pub fn execute(&self, command: &DebugCommand) -> Result<(), DebugError> {
        let shared = &self.shared;
        let mut data = shared.lock();
        if data.snap.is_ended() {
            return Err(DebugError::new("not_running", tr!("debug.ended")));
        }
        let stopped = data.snap.is_stopped();
        let thread = data.snap.thread.unwrap_or(0);
        let need_stop = |stopped: bool| {
            if stopped {
                Ok(())
            } else {
                Err(DebugError::new("not_stopped", tr!("debug.not_stopped")))
            }
        };
        let resume = |data: &mut Data| {
            data.snap.state = State::Running;
            Shared::clear_stop(data);
        };
        match command {
            DebugCommand::Continue => {
                need_stop(stopped)?;
                shared.request(
                    &mut data,
                    "continue",
                    json!({"threadId": thread}),
                    Pending::Other,
                );
                resume(&mut data);
            }
            DebugCommand::Pause => {
                if stopped {
                    return Ok(());
                }
                shared.request(
                    &mut data,
                    "pause",
                    json!({"threadId": thread}),
                    Pending::Other,
                );
            }
            DebugCommand::StepOver | DebugCommand::StepInto | DebugCommand::StepOut => {
                need_stop(stopped)?;
                let request = match command {
                    DebugCommand::StepOver => "next",
                    DebugCommand::StepInto => "stepIn",
                    _ => "stepOut",
                };
                shared.request(
                    &mut data,
                    request,
                    json!({"threadId": thread, "granularity": "line"}),
                    Pending::Other,
                );
                resume(&mut data);
            }
            DebugCommand::RunTo { path, line } => {
                need_stop(stopped)?;
                data.run_to = Some((path.clone(), *line));
                Shared::reset_status(&mut data, path);
                shared.send_breakpoints(&mut data, path);
                shared.request(
                    &mut data,
                    "continue",
                    json!({"threadId": thread}),
                    Pending::Other,
                );
                resume(&mut data);
            }
            DebugCommand::Stop => {
                shared.request(
                    &mut data,
                    "disconnect",
                    json!({"terminateDebuggee": true}),
                    Pending::Other,
                );
                // Antwortet der Adapter nicht, nach kurzer Zeit beenden.
                let shared = shared.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_secs(3));
                    let _ = shared
                        .child
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .kill();
                });
            }
            DebugCommand::SelectThread { id } => {
                need_stop(stopped)?;
                if !data.snap.threads.iter().any(|t| t.id == *id) {
                    return Err(DebugError::new("unknown_thread", format!("Thread {id}")));
                }
                data.snap.thread = Some(*id);
                Shared::clear_stop(&mut data);
                shared.request(
                    &mut data,
                    "stackTrace",
                    json!({"threadId": id, "startFrame": 0, "levels": STACK_DEPTH}),
                    Pending::StackTrace(*id),
                );
            }
            DebugCommand::SelectFrame { id } => {
                need_stop(stopped)?;
                if !data.snap.frames.iter().any(|f| f.id == *id) {
                    return Err(DebugError::new("unknown_frame", format!("Rahmen {id}")));
                }
                shared.load_frame(&mut data, *id);
            }
            DebugCommand::LoadVariables { reference } => {
                need_stop(stopped)?;
                if *reference > 0 && !data.snap.variables.contains_key(reference) {
                    shared.request(
                        &mut data,
                        "variables",
                        json!({"variablesReference": reference}),
                        Pending::Variables(*reference),
                    );
                }
            }
            DebugCommand::Evaluate { expression } => {
                let expression = expression.trim();
                if expression.is_empty() {
                    return Ok(());
                }
                shared.console(&mut data, ConsoleKind::Input, &format!("> {expression}\n"));
                let (expression, context) = match expression.strip_prefix('`') {
                    // lldb-dap erkennt Befehle selbst am `; gdb nimmt sie im Kontext repl.
                    Some(cmd) if shared.adapter.kind == AdapterKind::Gdb => {
                        (cmd.to_owned(), "repl")
                    }
                    Some(_) => (expression.to_owned(), "repl"),
                    None => {
                        need_stop(stopped)?;
                        (expression.to_owned(), "watch")
                    }
                };
                let mut args = json!({"expression": expression, "context": context});
                if let Some(frame) = data.snap.frame {
                    args["frameId"] = json!(frame);
                }
                shared.request(&mut data, "evaluate", args, Pending::Console);
            }
            DebugCommand::SetBreakpoints { breakpoints } => {
                let mut files: std::collections::BTreeSet<PathBuf> =
                    data.breakpoints.iter().map(|b| b.path.clone()).collect();
                files.extend(breakpoints.iter().map(|b| b.path.clone()));
                let old = std::mem::replace(&mut data.breakpoints, breakpoints.clone());
                if data.configured {
                    for path in files {
                        let before: Vec<&Breakpoint> =
                            old.iter().filter(|b| b.path == path).collect();
                        let after: Vec<&Breakpoint> =
                            breakpoints.iter().filter(|b| b.path == path).collect();
                        if before != after {
                            Shared::reset_status(&mut data, &path);
                            shared.send_breakpoints(&mut data, &path);
                        }
                    }
                }
            }
            DebugCommand::SetWatches { watches } => {
                let old = std::mem::take(&mut data.snap.watches);
                data.snap.watches = watches
                    .iter()
                    .map(|e| {
                        old.iter()
                            .find(|w| &w.expression == e)
                            .cloned()
                            .unwrap_or(Watch {
                                expression: e.clone(),
                                value: None,
                                error: None,
                                type_name: None,
                                reference: 0,
                            })
                    })
                    .collect();
                if stopped {
                    shared.evaluate_watches(&mut data);
                }
            }
            DebugCommand::SetStopOnPanic { enabled } => {
                data.stop_on_panic = *enabled;
                if data.configured {
                    shared.send_function_breakpoints(&mut data);
                }
            }
        }
        shared.changed(&mut data);
        Ok(())
    }

    pub fn is_ended(&self) -> bool {
        self.shared.lock().snap.is_ended()
    }
}

impl Drop for DebugSession {
    fn drop(&mut self) {
        if !self.is_ended() {
            let _ = self.execute(&DebugCommand::Stop);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framing() {
        let mut buffer = Vec::new();
        write_message(
            &mut buffer,
            &json!({"seq": 1, "type": "event", "event": "x"}),
        )
        .unwrap();
        write_message(&mut buffer, &json!({"seq": 2})).unwrap();
        let mut reader = BufReader::new(buffer.as_slice());
        assert_eq!(read_message(&mut reader).unwrap()["event"], "x");
        assert_eq!(read_message(&mut reader).unwrap()["seq"], 2);
        assert!(read_message(&mut reader).is_none());
    }

    #[test]
    fn adapter_kind_from_name() {
        assert_eq!(Adapter::from_program("/usr/bin/gdb").kind, AdapterKind::Gdb);
        assert_eq!(Adapter::from_program("lldb-dap").kind, AdapterKind::Lldb);
    }

    #[test]
    fn commands_as_json() {
        let command: DebugCommand =
            serde_json::from_str(r#"{"command": "run_to", "path": "src/main.rs", "line": 4}"#)
                .unwrap();
        assert_eq!(
            command,
            DebugCommand::RunTo {
                path: "src/main.rs".into(),
                line: 4
            }
        );
        let snap = Snapshot {
            state: State::Stopped {
                reason: "breakpoint".into(),
                description: None,
            },
            ..Snapshot::default()
        };
        let json = serde_json::to_value(&snap).unwrap();
        assert_eq!(json["state"], "stopped");
        assert_eq!(json["reason"], "breakpoint");
    }
}
