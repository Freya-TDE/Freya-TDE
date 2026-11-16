//! Window "KI": conversation with the AI of the chosen profile (drop-down,
//! switchable at any time, remembered per project), what it may do in the
//! project, its proposals with preview, the active skills and the input.
//! Dockable as a tab like structure and object inspector.
//!
//! The logic is [`crate::ai`]: requests go to a worker thread, tools run
//! here on the [`Session`] (same commands and undo as the designer), cargo
//! runs in the background.
//!
//! *Fenster „KI“: Gespräch mit der KI des gewählten Profils (Drop-down),
//! Rechte, Vorschläge mit Vorschau, Skills und Eingabe.*

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use rncl::prelude::*;
use ruf::dialog;

use super::{Ide, with, with0};
use crate::ai::skills::SkillOrigin;
use crate::ai::{
    self, AiError, ChatResponse, Client, Conversation, Decision, Event, ModelInfo, Proposal,
    SkillTable, Turn, Workspace,
};
use crate::desktop::window;
use crate::session::{Changes, Session};
use crate::settings::{AiPermission, Settings};

/// Results of the background work.
pub enum AiDone {
    /// A piece of the answer's text arrived (request generation, text).
    Text(u64, String),
    /// The model answered (request generation, answer).
    Chat(u64, Result<ChatResponse, AiError>),
    /// A cargo run finished (generation, call id, report).
    Run(u64, String, Result<String, String>),
    /// "Abrufen" in the settings window answered (request number, models).
    Models(u64, Result<Vec<ModelInfo>, AiError>),
}

const M: i32 = 6;
const ROW: i32 = 28;
const GAP: i32 = 6;
/// How often streamed text is shown.
const STREAM_INTERVAL: std::time::Duration = std::time::Duration::from_millis(80);

/// State of the conversation shown in the window.
#[derive(Default)]
pub struct AiState {
    conversation: Option<Conversation>,
    skills: SkillTable,
    /// The project conversation and skills belong to (its `.ptdr`).
    project: Option<PathBuf>,
    loaded: bool,
    busy: bool,
    /// Counts requests; answers of an older (cancelled) one are dropped.
    generation: u64,
    /// Text of the running request already shown (streaming).
    streamed: String,
    shown_events: usize,
    /// Profile and permission chosen while the project is unsaved.
    profile: Option<String>,
    permission: Option<AiPermission>,
    /// The waiting proposals with the developer's decision so far.
    decisions: Vec<(Proposal, Option<bool>)>,
    /// Names of the profiles in the drop-down (same order).
    profile_names: Vec<String>,
}

pub struct AiWindow {
    pub window: RnclWindow,
    pub pane: super::dock::DockPane,
    profile_label: RnclLabel,
    profile: RnclComboBox,
    permission_label: RnclLabel,
    permission: RnclComboBox,
    new_chat: RnclButton,
    skills: RnclLabel,
    approve_skills: RnclButton,
    transcript: RnclTextArea,
    proposals: RnclListBox,
    preview: RnclTextArea,
    accept: RnclButton,
    reject: RnclButton,
    accept_all: RnclButton,
    input: RnclTextArea,
    send: RnclButton,
    cancel: RnclButton,
    status: RnclLabel,
    pub state: RefCell<AiState>,
}

/// The window's menu (shortcuts while it is its own window).
pub(super) fn menu() -> Menu {
    Menu::new()
        .item("ai.send", "Senden", Some("Ctrl+Enter"))
        .item("ai.cancel", "Abbrechen", None)
        .item("ai.new", "Neues Gespräch", None)
        .separator()
        .item("ai.accept", "Vorschlag übernehmen", None)
        .item("ai.reject", "Vorschlag ablehnen", None)
        .item("ai.accept_all", "Alle Vorschläge übernehmen", None)
        .separator()
        .item("ai.skills", "Projekt-Skills freigeben …", None)
}

impl AiWindow {
    pub fn build() -> Self {
        let window = RnclWindow::new("KI");
        window.set_menu_bar(vec![("KI".into(), menu())]);
        let pane = super::dock::DockPane::new(&window, window::AI, "KI");
        let p = &pane.root;
        let profile_label = RnclLabel::new(p, "KI");
        let profile = RnclComboBox::new(p);
        profile
            .widget()
            .set_tool_tip("Die KI für dieses Gespräch; gilt ab der nächsten Anfrage und wird für das Projekt gemerkt");
        let permission_label = RnclLabel::new(p, "Darf");
        let permission = RnclComboBox::new(p);
        permission.set_items(&AiPermission::ALL.map(AiPermission::label));
        permission.widget().set_tool_tip(
            "Lesen ist immer erlaubt. „mit Vorschau“: jede Änderung erst als Vorschlag; „selbstständig“: ohne Rückfrage",
        );
        let new_chat = RnclButton::new(p, "Neu");
        new_chat.widget().set_tool_tip("Neues Gespräch");
        let skills = RnclLabel::new(p, "");
        let approve_skills = RnclButton::new(p, "Projekt-Skills …");
        approve_skills
            .widget()
            .set_tool_tip("Eigene Skills des Projekts (.agents/skills) prüfen und freigeben");
        let transcript = RnclTextArea::new(p);
        transcript.set_read_only(true);
        transcript.set_word_wrap(true);
        transcript.add_highlight_rule(r"^Du:", "#2a7ae2", true);
        transcript.add_highlight_rule(r"^KI:", "#2e9d4f", true);
        transcript.add_highlight_rule(r"^Fehler:.*", "#d03b3b", false);
        transcript.add_highlight_rule(r"^\s+✗.*", "#d03b3b", false);
        transcript.add_highlight_rule(r"^\s+✓.*", "#2e9d4f", false);
        transcript.add_highlight_rule(r"^\s+\?.*", "#c27c0e", true);
        transcript.add_highlight_rule(r"^\s+·.*", "#888888", false);
        transcript.add_highlight_rule(r"^—.*—$", "#888888", false);
        let proposals = RnclListBox::new(p);
        let preview = RnclTextArea::new(p);
        preview.set_read_only(true);
        preview.set_monospace(true);
        preview.set_word_wrap(true);
        preview.add_highlight_rule(r"^\+.*", "#2e9d4f", false);
        preview.add_highlight_rule(r"^-.*", "#d03b3b", false);
        preview.add_highlight_rule(r"^@@.*", "#888888", false);
        let accept = RnclButton::new(p, "Übernehmen");
        let reject = RnclButton::new(p, "Ablehnen");
        let accept_all = RnclButton::new(p, "Alle übernehmen");
        let input = RnclTextArea::new(p);
        input.set_word_wrap(true);
        input.set_placeholder("Nachricht an die KI (Strg+Enter sendet)");
        let send = RnclButton::new(p, "Senden");
        let cancel = RnclButton::new(p, "Abbrechen");
        let status = RnclLabel::new(p, "");
        Self {
            window,
            pane,
            profile_label,
            profile,
            permission_label,
            permission,
            new_chat,
            skills,
            approve_skills,
            transcript,
            proposals,
            preview,
            accept,
            reject,
            accept_all,
            input,
            send,
            cancel,
            status,
            state: RefCell::new(AiState::default()),
        }
    }

    fn deciding(&self) -> bool {
        !self.state.borrow().decisions.is_empty()
    }

    /// Lays out the content of the pane (`w` × `h`).
    fn layout(&self, w: i32, h: i32) {
        let mut y = self.pane.layout_grip(w) + 2;
        let inner = (w - 2 * M).max(100);
        // Profil, Rechte, Neu.
        let new_w = 60;
        self.new_chat.set_geometry(w - M - new_w, y, new_w, ROW);
        let (label_w, permission_label_w) = (26, 42);
        let rest = inner - new_w - label_w - permission_label_w - 3 * GAP;
        let profile_w = (rest * 3 / 5).max(80);
        let permission_w = (rest - profile_w).max(60);
        self.profile_label.set_geometry(M, y, label_w, ROW);
        self.profile.set_geometry(M + label_w, y, profile_w, ROW);
        let x = M + label_w + profile_w + 2 * GAP;
        self.permission_label
            .set_geometry(x, y, permission_label_w, ROW);
        self.permission
            .set_geometry(x + permission_label_w, y, permission_w, ROW);
        y += ROW + GAP;
        // Skills.
        let skill_w = 130;
        self.skills
            .set_geometry(M, y, (inner - skill_w - GAP).max(10), ROW);
        self.approve_skills
            .set_geometry(w - M - skill_w, y, skill_w, ROW);
        y += ROW + GAP;
        // Unten: Status, Eingabe, ggf. Vorschläge.
        let status_y = h - M - 20;
        self.status.set_geometry(M, status_y, inner, 20);
        let input_h = 76;
        let input_y = status_y - GAP - input_h;
        let button_w = 100;
        self.input
            .set_geometry(M, input_y, (inner - button_w - GAP).max(10), input_h);
        self.send
            .set_geometry(w - M - button_w, input_y, button_w, ROW);
        self.cancel
            .set_geometry(w - M - button_w, input_y + ROW + GAP, button_w, ROW);
        let mut bottom = input_y - GAP;
        let deciding = self.deciding();
        for widget in [
            self.proposals.widget(),
            self.preview.widget(),
            self.accept.widget(),
            self.reject.widget(),
            self.accept_all.widget(),
        ] {
            widget.set_visible(deciding);
        }
        if deciding {
            let buttons_y = bottom - ROW;
            let bw = ((inner - 2 * GAP) / 3).max(40);
            self.accept.set_geometry(M, buttons_y, bw, ROW);
            self.reject.set_geometry(M + bw + GAP, buttons_y, bw, ROW);
            self.accept_all
                .set_geometry(M + 2 * (bw + GAP), buttons_y, bw, ROW);
            let area = ((bottom - y) / 2).clamp(120, 320);
            let preview_h = area - 70 - GAP;
            let preview_y = buttons_y - GAP - preview_h;
            self.preview.set_geometry(M, preview_y, inner, preview_h);
            let list_y = preview_y - GAP - 70;
            self.proposals.set_geometry(M, list_y, inner, 70);
            bottom = list_y - GAP;
        }
        self.transcript
            .set_geometry(M, y, inner, (bottom - y).max(40));
    }

    fn relayout(&self) {
        let size = self.pane.root.size();
        if size.width > 0 && size.height > 0 {
            self.layout(size.width, size.height);
        }
    }

    fn log(&self, line: &str) {
        self.transcript.append_line(line);
        self.scroll_down();
    }

    /// Appends `piece` to the last line (streamed answer).
    fn append(&self, piece: &str) {
        let mut text = self.transcript.text();
        text.push_str(piece);
        self.transcript.set_text(&text);
        self.scroll_down();
    }

    fn scroll_down(&self) {
        let lines = self.transcript.text().lines().count() as i32;
        self.transcript.goto_line(lines.max(1));
    }

    /// Fills drop-downs, skills and buttons from the IDE's state.
    pub fn refresh(&self, ide: &Ide) {
        let settings = ide.settings.borrow();
        let names: Vec<String> = settings
            .ai
            .profiles
            .iter()
            .map(|p| p.name.clone())
            .collect();
        let items: Vec<String> = settings
            .ai
            .profiles
            .iter()
            .map(|p| {
                if p.model.is_empty() {
                    format!("{} ({})", p.name, p.provider.label())
                } else {
                    format!("{} – {}", p.name, p.model)
                }
            })
            .collect();
        drop(settings);
        if self.profile.items() != items {
            self.profile.set_items(&items);
        }
        let current = ide.ai_profile();
        let index = current
            .as_deref()
            .and_then(|c| names.iter().position(|n| n == c))
            .map_or(-1, |i| i as i32);
        if self.profile.current_index() != index {
            self.profile.set_current_index(index);
        }
        self.state.borrow_mut().profile_names = names;
        let permission = ide.ai_permission();
        let pi = AiPermission::ALL
            .iter()
            .position(|p| *p == permission)
            .unwrap_or(1) as i32;
        if self.permission.current_index() != pi {
            self.permission.set_current_index(pi);
        }
        let state = self.state.borrow();
        let trusted = state.skills.skills().iter().filter(|s| s.trusted()).count();
        let own = state.skills.skills().len() - trusted;
        let pending = state.skills.pending().len();
        let mut text = format!("Skills: {trusted} geprüft");
        if own > 0 {
            text.push_str(&format!(", {own} eigene (nicht geprüft)"));
        }
        if pending > 0 {
            text.push_str(&format!(", {pending} warten auf Freigabe"));
        }
        self.skills.set_text(&text);
        self.skills.widget().set_tool_tip(
            &state
                .skills
                .skills()
                .iter()
                .map(|s| match &s.origin {
                    SkillOrigin::Pack { version } => {
                        format!("{}: {} (Skill-Paket {version})", s.name, s.description)
                    }
                    _ => format!("{}: {}", s.name, s.description),
                })
                .collect::<Vec<_>>()
                .join("\n"),
        );
        self.approve_skills.widget().set_enabled(pending > 0);
        let busy = state.busy;
        let deciding = !state.decisions.is_empty();
        drop(state);
        self.send
            .widget()
            .set_enabled(!busy && !deciding && index >= 0);
        self.cancel.widget().set_enabled(busy || deciding);
        if index < 0 {
            self.status
                .set_text("Kein KI-Profil: Datei → Einstellungen … → KI");
        }
    }

    fn fill_proposals(&self) {
        let state = self.state.borrow();
        let items: Vec<String> = state
            .decisions
            .iter()
            .map(|(p, d)| {
                let mark = match d {
                    None => "•",
                    Some(true) => "✓",
                    Some(false) => "✗",
                };
                format!("{mark} {}", p.summary)
            })
            .collect();
        let current = self.proposals.current_index();
        drop(state);
        self.proposals.set_items(&items);
        let next = if current >= 0 && (current as usize) < items.len() {
            current
        } else {
            0
        };
        self.proposals.set_current_index(next);
        self.show_preview(next);
        self.relayout();
    }

    fn show_preview(&self, index: i32) {
        let text = usize::try_from(index)
            .ok()
            .and_then(|i| {
                self.state
                    .borrow()
                    .decisions
                    .get(i)
                    .map(|(p, _)| p.preview.clone())
            })
            .unwrap_or_default();
        self.preview.set_text(&text);
    }
}

/// Short form of a tool input for the transcript.
fn brief(tool: &str, input: &serde_json::Value) -> String {
    for key in ["path", "query", "form", "name", "filter", "tool"] {
        if let Some(v) = input[key].as_str() {
            return format!("{tool} {v}");
        }
    }
    tool.to_owned()
}

/// The project as the AI's tools see it in the IDE: the session plus the
/// open editors (unsaved text counts; written files are shown there).
struct IdeWorkspace<'a> {
    ide: &'a Ide,
    session: std::cell::RefMut<'a, Session>,
    written: Vec<(PathBuf, String)>,
    generation: Option<tdr_core::codegen::Generation>,
    model: bool,
}

impl Workspace for IdeWorkspace<'_> {
    fn document(&self) -> &tdr_designer::Document {
        self.session.document()
    }

    fn apply_command(&mut self, command: tdr_designer::Command) -> Result<String, String> {
        let result = Workspace::apply_command(&mut *self.session, command);
        self.model |= result.is_ok();
        result
    }

    fn save_and_generate(&mut self) -> Result<Vec<String>, String> {
        // Wie „Code erzeugen“: offene Editoren zuerst sichern.
        self.ide.code.save_all()?;
        let (generation, written) = self.session.write_code()?;
        self.generation = Some(generation);
        self.model = true;
        Ok(written)
    }

    fn read_text(&self, rel: &str) -> std::io::Result<String> {
        let root = self
            .root()
            .ok_or_else(|| std::io::Error::other("Das Projekt ist noch nicht gespeichert."))?;
        let path = root.join(rel);
        match self.ide.code.text(&path) {
            Some(text) => Ok(text),
            None => std::fs::read_to_string(path),
        }
    }

    fn write_text(&mut self, rel: &str, text: &str) -> std::io::Result<()> {
        let root = self
            .root()
            .ok_or_else(|| std::io::Error::other("Das Projekt ist noch nicht gespeichert."))?;
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, text)?;
        self.written.push((path, text.to_owned()));
        Ok(())
    }
}

impl Ide {
    /// Fenster → KI.
    pub fn show_ai(&self) {
        self.ai_prepare();
        self.ai_window.refresh(self);
        self.show_window(window::AI);
        self.ai_window.input.widget().focus();
    }

    fn ai_project(&self) -> Option<PathBuf> {
        self.path.borrow().clone()
    }

    /// The profile the AI window uses: the project's, else the one chosen
    /// in the window (unsaved project), else the default, else the first.
    pub fn ai_profile(&self) -> Option<String> {
        let settings = self.settings.borrow();
        let project = self.ai_project();
        let exists = |n: &String| settings.ai.profile(n).is_some();
        project
            .as_deref()
            .and_then(|p| settings.ai.projects.get(p))
            .filter(|n| exists(n))
            .cloned()
            .or_else(|| self.ai_window.state.borrow().profile.clone().filter(exists))
            .or_else(|| settings.ai.default_profile.clone().filter(exists))
            .or_else(|| settings.ai.profiles.first().map(|p| p.name.clone()))
    }

    /// What the AI may do in the current project.
    pub fn ai_permission(&self) -> AiPermission {
        match self.ai_project() {
            Some(p) => self.settings.borrow().ai.permission_for(Some(&p)),
            None => self.ai_window.state.borrow().permission.unwrap_or_default(),
        }
    }

    /// Changes the settings and saves them (window choices).
    fn ai_store_settings(&self, change: impl FnOnce(&mut Settings, &Path)) -> bool {
        let Some(project) = self.ai_project() else {
            return false;
        };
        let mut settings = self.settings.borrow_mut();
        change(&mut settings, &project);
        if let Some(file) = &self.settings_file
            && let Err(e) = settings.save_to(file)
        {
            drop(settings);
            self.set_status(&e.detail);
        }
        true
    }

    /// Chooses the AI for the conversation (drop-down); remembered for the
    /// project. The conversation goes on with the new one.
    pub fn set_ai_profile(&self, name: &str) -> Result<(), String> {
        if self.settings.borrow().ai.profile(name).is_none() {
            return Err(format!("unbekanntes KI-Profil „{name}“"));
        }
        if self.ai_profile().as_deref() == Some(name) {
            return Ok(());
        }
        let stored = self.ai_store_settings(|s, p| {
            s.ai.projects.insert(p.to_path_buf(), name.to_owned());
        });
        if !stored {
            self.ai_window.state.borrow_mut().profile = Some(name.to_owned());
        }
        if self.ai_window.state.borrow().conversation.is_some() {
            self.ai_window.log(&format!("— KI: {name} —"));
        }
        self.ai_window.refresh(self);
        Ok(())
    }

    /// Sets what the AI may do in the project (drop-down).
    pub fn set_ai_permission(&self, permission: AiPermission) {
        let stored = self.ai_store_settings(|s, p| {
            if permission == AiPermission::default() {
                s.ai.permissions.remove(p);
            } else {
                s.ai.permissions.insert(p.to_path_buf(), permission);
            }
        });
        if !stored {
            self.ai_window.state.borrow_mut().permission = Some(permission);
        }
        if let Some(c) = self.ai_window.state.borrow_mut().conversation.as_mut() {
            c.permission = permission;
        }
        self.ai_window.refresh(self);
    }

    /// Problems with skills: a refused skill pack (always) and broken or
    /// shadowed project skills (once the AI window loaded them).
    pub(super) fn skill_problems(&self) -> Vec<crate::problems::Problem> {
        let state = self.ai_window.state.borrow();
        crate::ai::skills::installed_pack()
            .as_ref()
            .err()
            .into_iter()
            .chain(state.skills.issues())
            .map(|issue| crate::problems::Problem {
                code: Some(issue.code.to_owned()),
                file: issue.path.clone(),
                ..crate::problems::Problem::new(
                    crate::problems::ProblemSource::Ai,
                    crate::problems::ProblemSeverity::Warning,
                    &issue.detail,
                )
            })
            .collect()
    }

    /// Loads the skills for the current project and starts over when the
    /// project changed.
    pub(super) fn ai_prepare(&self) {
        let project = self.ai_project();
        let changed = {
            let state = self.ai_window.state.borrow();
            !state.loaded || state.project != project
        };
        if !changed {
            return;
        }
        let mut skills = SkillTable::trusted();
        if let (Some(ptdr), Some(dir)) = (&project, self.session().project_dir()) {
            let approved = self.settings.borrow().ai.approved_skills(ptdr);
            skills.load_project(&dir, &approved);
        }
        let had = {
            let mut state = self.ai_window.state.borrow_mut();
            let had = state.conversation.is_some();
            state.generation += 1;
            *state = AiState {
                skills,
                project,
                loaded: true,
                generation: state.generation,
                profile: state.profile.take(),
                permission: state.permission.take(),
                ..AiState::default()
            };
            had
        };
        if had {
            self.ai_window.transcript.set_text("");
            self.ai_window.fill_proposals();
        }
    }

    /// Starts a new conversation (button "Neu").
    pub fn ai_new_conversation(&self) {
        {
            let mut state = self.ai_window.state.borrow_mut();
            state.generation += 1;
            state.conversation = None;
            state.busy = false;
            state.decisions.clear();
            state.shown_events = 0;
        }
        self.ai_window.transcript.set_text("");
        self.ai_window.status.set_text("");
        self.ai_window.fill_proposals();
        self.ai_window.refresh(self);
    }

    /// Sends a message to the AI (in the background).
    pub fn ai_send(&self, text: &str) -> Result<(), String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("Leere Nachricht.".into());
        }
        self.ai_prepare();
        {
            let state = self.ai_window.state.borrow();
            if state.busy {
                return Err("Die KI arbeitet noch.".into());
            }
            if !state.decisions.is_empty() {
                return Err("Erst über die Vorschläge entscheiden.".into());
            }
        }
        let client = self.ai_client()?;
        let permission = self.ai_permission();
        {
            let mut state = self.ai_window.state.borrow_mut();
            let conversation = state.conversation.get_or_insert_with(|| {
                let mut c = Conversation::new(String::new(), permission);
                c.defer_runs = true;
                c
            });
            conversation.permission = permission;
            conversation.say(text);
        }
        if let Some(warning) = client.warning() {
            self.ai_window
                .log(&format!("Fehler: {} ({})", warning.detail, warning.code));
        }
        self.ai_window.log(&format!("Du: {text}"));
        self.ai_window.input.set_text("");
        self.ai_log_events();
        self.ai_request(client);
        Ok(())
    }

    fn ai_client(&self) -> Result<Client, String> {
        let name = self
            .ai_profile()
            .ok_or("Kein KI-Profil: Datei → Einstellungen … → KI")?;
        let settings = self.settings.borrow();
        Client::from_settings(&settings, &name, self.secrets.borrow().as_ref())
            .map_err(|e| format!("{} ({})", e.detail, e.code))
    }

    /// Sends the conversation's next request in a worker thread.
    fn ai_request(&self, client: Client) {
        let Some(sender) = self.ai_results.borrow().clone() else {
            return;
        };
        let permission = self.ai_permission();
        let (request, generation) = {
            let session = self.session();
            let mut state = self.ai_window.state.borrow_mut();
            let system = ai::system_prompt(&*session, permission, &state.skills);
            let skills = state.skills.clone();
            let Some(conversation) = state.conversation.as_mut() else {
                return;
            };
            conversation.set_system(system);
            let request = conversation.request(&skills);
            state.busy = true;
            state.streamed.clear();
            (request, state.generation)
        };
        self.ai_window
            .status
            .set_text(&format!("{} denkt …", client.model()));
        self.ai_window.refresh(self);
        std::thread::spawn(move || {
            // Text höchstens alle 80 ms ans Fenster, damit es flüssig bleibt.
            let mut pending = String::new();
            let mut last = std::time::Instant::now();
            let result = client.chat_stream(&request, &mut |piece| {
                pending.push_str(piece);
                if last.elapsed() >= STREAM_INTERVAL {
                    sender.send(AiDone::Text(generation, std::mem::take(&mut pending)));
                    last = std::time::Instant::now();
                }
            });
            if !pending.is_empty() {
                sender.send(AiDone::Text(generation, pending));
            }
            sender.send(AiDone::Chat(generation, result));
        });
    }

    /// Background work finished.
    pub(super) fn ai_done(&self, done: AiDone) {
        match done {
            AiDone::Text(generation, piece) => {
                let first = {
                    let mut state = self.ai_window.state.borrow_mut();
                    if generation != state.generation {
                        return;
                    }
                    // Leerraum am Anfang erst zeigen, wenn Text folgt.
                    let first = state.streamed.trim().is_empty();
                    state.streamed.push_str(&piece);
                    if state.streamed.trim().is_empty() {
                        return;
                    }
                    first
                };
                if first {
                    let text = self
                        .ai_window
                        .state
                        .borrow()
                        .streamed
                        .trim_start()
                        .to_owned();
                    self.ai_window.log(&format!("KI: {text}"));
                } else {
                    self.ai_window.append(&piece);
                }
            }
            AiDone::Chat(generation, result) => {
                let streamed = {
                    let mut state = self.ai_window.state.borrow_mut();
                    if generation != state.generation {
                        return;
                    }
                    state.busy = false;
                    !std::mem::take(&mut state.streamed).trim().is_empty()
                };
                if streamed {
                    let text = self.ai_window.transcript.text();
                    if text.trim_end().len() != text.len() {
                        self.ai_window.transcript.set_text(text.trim_end());
                    }
                }
                match result {
                    Ok(response) => {
                        let text = response.text();
                        if !streamed && !text.trim().is_empty() {
                            self.ai_window.log(&format!("KI: {}", text.trim()));
                        }
                        let turn = self.ai_with_workspace(|c, ws, skills| {
                            c.receive(response, ws, skills)
                                .map_err(|e| format!("{} ({})", e.detail, e.code))
                        });
                        self.ai_turn(turn);
                    }
                    Err(e) => {
                        self.ai_window
                            .log(&format!("Fehler: {} ({})", e.detail, e.code));
                        self.ai_window.status.set_text("");
                        self.ai_window.refresh(self);
                    }
                }
            }
            AiDone::Run(generation, call_id, result) => {
                if generation != self.ai_window.state.borrow().generation {
                    return;
                }
                self.ai_window.state.borrow_mut().busy = false;
                let turn = self.ai_with_workspace(|c, _, _| Ok(c.finished(&call_id, result)));
                self.ai_turn(turn);
            }
            AiDone::Models(request, result) => self.settings_models_done(request, result),
        }
    }

    /// Runs `f` with the conversation and the IDE's workspace, then shows
    /// what changed (designer, editors, generated code).
    fn ai_with_workspace(
        &self,
        f: impl FnOnce(&mut Conversation, &mut dyn Workspace, &SkillTable) -> Result<Turn, String>,
    ) -> Result<Turn, String> {
        let mut guard = self.ai_window.state.borrow_mut();
        let state = &mut *guard;
        let Some(conversation) = state.conversation.as_mut() else {
            return Err("Kein Gespräch.".into());
        };
        let mut ws = IdeWorkspace {
            ide: self,
            session: self.session.borrow_mut(),
            written: Vec::new(),
            generation: None,
            model: false,
        };
        let turn = f(conversation, &mut ws, &state.skills);
        let IdeWorkspace {
            written,
            generation,
            model,
            session,
            ..
        } = ws;
        drop(session);
        drop(guard);
        for (path, text) in &written {
            self.code.written(self, path, text);
        }
        if let Some(generation) = &generation {
            self.ensure_lsp();
            for (path, text) in self.code.reload_from_disk() {
                self.lsp_changed(&path, &text);
                self.lsp_saved(&[path]);
            }
            self.set_codegen_problems(generation);
        }
        if model || !written.is_empty() {
            self.refresh(Changes {
                model: true,
                selection: model,
            });
        }
        turn
    }

    /// Writes new events of the conversation into the transcript.
    fn ai_log_events(&self) {
        let lines: Vec<String> = {
            let mut state = self.ai_window.state.borrow_mut();
            let Some(conversation) = state.conversation.as_ref() else {
                return;
            };
            let events = conversation.events();
            let start = state.shown_events.min(events.len());
            let lines = events[start..]
                .iter()
                .map(|e| match e {
                    Event::Read { tool, input, ok } => {
                        format!(
                            "  · {}{}",
                            brief(tool, input),
                            if *ok { "" } else { " (Fehler)" }
                        )
                    }
                    Event::Proposed { proposal } => format!("  ? {}", proposal.summary),
                    Event::Applied {
                        proposal, result, ..
                    } => match result {
                        Ok(_) => format!("  ✓ {}", proposal.summary),
                        Err(e) => format!("  ✗ {}: {e}", proposal.summary),
                    },
                    Event::Rejected { proposal } => format!("  ✗ abgelehnt: {}", proposal.summary),
                    Event::Refused { tool, reason } => format!("  ✗ {tool}: {reason}"),
                })
                .collect();
            state.shown_events = events.len();
            lines
        };
        for line in lines {
            self.ai_window.log(&line);
        }
    }

    /// Goes on after a turn of the conversation.
    fn ai_turn(&self, turn: Result<Turn, String>) {
        self.ai_log_events();
        match turn {
            Err(e) => {
                self.ai_window.log(&format!("Fehler: {e}"));
                self.ai_window.status.set_text("");
            }
            Ok(Turn::Continue) => match self.ai_client() {
                Ok(client) => self.ai_request(client),
                Err(e) => self.ai_window.log(&format!("Fehler: {e}")),
            },
            Ok(Turn::Approve { proposals }) => {
                self.ai_window.state.borrow_mut().decisions =
                    proposals.into_iter().map(|p| (p, None)).collect();
                self.ai_window.fill_proposals();
                self.ai_window
                    .status
                    .set_text("Vorschläge prüfen: Übernehmen oder Ablehnen");
                self.show_ai_window_only();
            }
            Ok(Turn::Run { proposals }) => self.ai_run(proposals),
            Ok(Turn::Done { stop, .. }) => {
                let usage = self
                    .ai_window
                    .state
                    .borrow()
                    .conversation
                    .as_ref()
                    .map(Conversation::usage)
                    .unwrap_or_default();
                let mut text = format!(
                    "Fertig · {} Token gelesen, {} geschrieben",
                    usage.input_tokens, usage.output_tokens
                );
                if stop == ai::StopReason::MaxTokens {
                    text.push_str(" · Antwort abgeschnitten (Längengrenze)");
                }
                self.ai_window.status.set_text(&text);
            }
        }
        self.ai_window.refresh(self);
    }

    /// Shows the AI window (without stealing the input focus elsewhere).
    fn show_ai_window_only(&self) {
        self.show_window(window::AI);
    }

    /// Runs cargo calls of the AI in the background.
    fn ai_run(&self, proposals: Vec<Proposal>) {
        let Some(sender) = self.ai_results.borrow().clone() else {
            return;
        };
        let Some(dir) = self.session().project_dir() else {
            return;
        };
        let generation = self.session().generation();
        let request = {
            let mut state = self.ai_window.state.borrow_mut();
            state.busy = true;
            state.generation
        };
        self.ai_window.status.set_text("cargo läuft …");
        std::thread::spawn(move || {
            for p in proposals {
                let result = match &p.action {
                    ai::Action::Cargo { tool } => {
                        crate::tools::run(*tool, &dir, generation.as_ref(), &[])
                            .map(|report| ai::tools::cargo_summary(*tool, &report))
                            .map_err(|e| e.to_string())
                    }
                    _ => Err("kein Programmaufruf".into()),
                };
                sender.send(AiDone::Run(request, p.call_id.clone(), result));
            }
        });
    }

    /// Cancels the running request or the waiting proposals.
    pub fn ai_cancel(&self) {
        let had_decisions = {
            let mut state = self.ai_window.state.borrow_mut();
            state.generation += 1;
            state.busy = false;
            let had = !state.decisions.is_empty();
            state.decisions.clear();
            if let Some(c) = state.conversation.as_mut() {
                c.abort("Vom Entwickler abgebrochen.");
            }
            had
        };
        self.ai_log_events();
        self.ai_window.status.set_text("Abgebrochen.");
        if had_decisions {
            self.ai_window.fill_proposals();
        }
        self.ai_window.refresh(self);
    }

    /// Decides the selected proposal (`index`, default: the selected one).
    pub fn ai_decide(&self, index: Option<usize>, approve: bool) {
        let index = index
            .or_else(|| usize::try_from(self.ai_window.proposals.current_index()).ok())
            .unwrap_or(0);
        {
            let mut state = self.ai_window.state.borrow_mut();
            let Some(entry) = state.decisions.get_mut(index) else {
                return;
            };
            entry.1 = Some(approve);
            // Weiter zum nächsten offenen.
            if let Some(next) = state.decisions.iter().position(|(_, d)| d.is_none()) {
                drop(state);
                self.ai_window.proposals.set_current_index(next as i32);
            }
        }
        self.ai_finish_decisions();
    }

    /// Approves all open proposals.
    pub fn ai_decide_all(&self, approve: bool) {
        for entry in self.ai_window.state.borrow_mut().decisions.iter_mut() {
            entry.1.get_or_insert(approve);
        }
        self.ai_finish_decisions();
    }

    /// Once every proposal is decided, the conversation goes on.
    fn ai_finish_decisions(&self) {
        let decisions: Option<Vec<Decision>> = {
            let state = self.ai_window.state.borrow();
            state
                .decisions
                .iter()
                .map(|(p, d)| {
                    d.map(|approve| Decision {
                        call_id: p.call_id.clone(),
                        approve,
                        reason: None,
                    })
                })
                .collect()
        };
        let Some(decisions) = decisions else {
            self.ai_window.fill_proposals();
            return;
        };
        self.ai_window.state.borrow_mut().decisions.clear();
        self.ai_window.fill_proposals();
        let turn = self.ai_with_workspace(|c, ws, _| Ok(c.decide(&decisions, ws)));
        self.ai_turn(turn);
    }

    /// The proposals waiting for a decision (automation).
    pub fn ai_pending(&self) -> Vec<Proposal> {
        self.ai_window
            .state
            .borrow()
            .decisions
            .iter()
            .map(|(p, _)| p.clone())
            .collect()
    }

    /// Is a request or cargo run going on?
    pub fn ai_busy(&self) -> bool {
        self.ai_window.state.borrow().busy
    }

    /// The transcript of the window (automation).
    pub fn ai_transcript(&self) -> String {
        self.ai_window.transcript.text()
    }

    /// Picks entry `index` of the profile drop-down like the developer
    /// (automation; the change goes through the drop-down's signal).
    pub fn ai_choose_profile(&self, index: usize) {
        self.ai_window.profile.set_current_index(index as i32);
    }

    /// The drop-down's profiles and the chosen one (automation).
    pub fn ai_profile_choices(&self) -> (Vec<String>, Option<String>) {
        (self.ai_window.profile.items(), self.ai_profile())
    }

    /// "Projekt-Skills …": shows the waiting own skills and approves them.
    pub fn ai_approve_skills(&self, ask: bool) {
        let Some(project) = self.ai_project() else {
            return;
        };
        let pending = self.ai_window.state.borrow().skills.pending().to_vec();
        if pending.is_empty() {
            return;
        }
        let list: Vec<String> = pending
            .iter()
            .map(|s| {
                format!(
                    "• {}{}: {}\n  {}",
                    s.name,
                    if s.changed { " (geändert)" } else { "" },
                    s.description,
                    s.path.display()
                )
            })
            .collect();
        if ask
            && !dialog::confirm(
                Some(self.ai_window.window.widget()),
                "Projekt-Skills freigeben",
                &format!(
                    "Diese eigenen Skills des Projekts sind nicht von Freya TDE geprüft. \
Die KI folgt ihren Anweisungen. Nur freigeben, wenn du ihnen vertraust:\n\n{}",
                    list.join("\n")
                ),
            )
        {
            return;
        }
        self.ai_store_settings(|s, p| {
            for skill in &pending {
                s.ai.approve_skill(p, &skill.name, Some(&skill.sha256));
            }
        });
        // Neu laden (die Gesprächsrunde bleibt).
        let dir = self.session().project_dir();
        let approved = self.settings.borrow().ai.approved_skills(&project);
        let mut skills = SkillTable::trusted();
        if let Some(dir) = dir {
            skills.load_project(&dir, &approved);
        }
        self.ai_window.state.borrow_mut().skills = skills;
        self.ai_window.refresh(self);
    }

    fn ai_menu(&self, id: &str) {
        match id {
            "ai.send" => {
                let text = self.ai_window.input.text();
                if let Err(e) = self.ai_send(&text) {
                    self.ai_window.status.set_text(&e);
                }
            }
            "ai.cancel" => self.ai_cancel(),
            "ai.new" => self.ai_new_conversation(),
            "ai.accept" => self.ai_decide(None, true),
            "ai.reject" => self.ai_decide(None, false),
            "ai.accept_all" => self.ai_decide_all(true),
            "ai.skills" => self.ai_approve_skills(true),
            _ => {}
        }
    }
}

pub fn connect(ide: &Rc<Ide>) {
    let w = &ide.ai_window;
    let weak = Rc::downgrade(ide);
    w.pane.root.widget().on_resized(move |width, height| {
        if let Some(ide) = weak.upgrade() {
            ide.ai_window.layout(width, height);
        }
    });
    w.window
        .on_menu(with(ide, |ide, id: String| ide.ai_menu(&id)));
    w.profile
        .on_current_index_changed(with(ide, |ide, index: i32| {
            if ide.is_refreshing() || index < 0 {
                return;
            }
            let name = ide
                .ai_window
                .state
                .borrow()
                .profile_names
                .get(index as usize)
                .cloned();
            if let Some(name) = name
                && let Err(e) = ide.set_ai_profile(&name)
            {
                ide.ai_window.status.set_text(&e);
            }
        }));
    w.permission
        .on_current_index_changed(with(ide, |ide, index: i32| {
            if let Some(p) = usize::try_from(index)
                .ok()
                .and_then(|i| AiPermission::ALL.get(i))
                && *p != ide.ai_permission()
            {
                ide.set_ai_permission(*p);
            }
        }));
    w.new_chat
        .on_clicked(with0(ide, |ide| ide.ai_new_conversation()));
    w.approve_skills
        .on_clicked(with0(ide, |ide| ide.ai_approve_skills(true)));
    w.send.on_clicked(with0(ide, |ide| ide.ai_menu("ai.send")));
    w.cancel.on_clicked(with0(ide, |ide| ide.ai_cancel()));
    w.accept
        .on_clicked(with0(ide, |ide| ide.ai_decide(None, true)));
    w.reject
        .on_clicked(with0(ide, |ide| ide.ai_decide(None, false)));
    w.accept_all
        .on_clicked(with0(ide, |ide| ide.ai_decide_all(true)));
    w.proposals
        .on_current_index_changed(with(ide, |ide, index: i32| {
            ide.ai_window.show_preview(index);
        }));
    // Strg+Enter auch, wenn das Fenster als Reiter angedockt ist (ohne Menü).
    let weak = Rc::downgrade(ide);
    w.input.widget().on_key(move |key| {
        if key.key == ruf::Key::Enter
            && (key.modifiers.ctrl || key.modifiers.meta)
            && let Some(ide) = weak.upgrade()
            && !ide.ai_window.input.text().trim().is_empty()
        {
            ide.ai_menu("ai.send");
        }
    });
}
