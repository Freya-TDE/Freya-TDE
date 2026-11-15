//! A conversation with the AI: history, tool calls, permissions and
//! approvals, as a state machine without network or UI.
//!
//! The network part ([`Client::chat`]) runs where the caller wants (the
//! IDE: a worker thread); the tools run on the caller's thread with the
//! [`Workspace`]:
//!
//! 1. [`Conversation::say`] adds the developer's message.
//! 2. [`Conversation::request`] builds the [`ChatRequest`]; the caller sends
//!    it with [`Client::chat`].
//! 3. [`Conversation::receive`] takes the answer, runs reading tools and,
//!    depending on the permission, applies or proposes changes. It returns
//!    [`Turn::Continue`] (send the next request), [`Turn::Approve`] (show
//!    the proposals, then [`Conversation::decide`]) or [`Turn::Done`].
//!
//! [`run`] does all of this in one blocking loop (tests, scripts, KI
//! without window).
//!
//! *Ein Gespräch mit der KI als Zustandsautomat: Verlauf, Werkzeuge,
//! Rechte und Freigaben, ohne Netz und Oberfläche.*

use serde::Serialize;
use serde_json::{Value, json};

use super::client::Client;
use super::message::{ChatRequest, ChatResponse, Message, Part, Role, StopReason, ToolSpec, Usage};
use super::skills::SkillTable;
use super::tools::{self, Access, Proposal, Workspace};
use super::{AiError, codes};
use crate::settings::AiPermission;

/// Name of the tool that loads a skill.
pub const LOAD_SKILL: &str = "load_skill";

/// What happened in the conversation besides the messages (for the AI
/// window and logs).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// A reading tool ran.
    Read {
        tool: String,
        input: Value,
        ok: bool,
    },
    /// A change waits for approval.
    Proposed { proposal: Proposal },
    /// A change was applied (`approved`: by the developer, else autonomous).
    Applied {
        proposal: Proposal,
        approved: bool,
        result: Result<String, String>,
    },
    /// The developer rejected a change.
    Rejected { proposal: Proposal },
    /// A tool call was refused (unknown, not allowed, invalid).
    Refused { tool: String, reason: String },
}

/// What the caller does next.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "turn", rename_all = "snake_case")]
pub enum Turn {
    /// Send [`Conversation::request`] again (tool results are in).
    Continue,
    /// Show these proposals and call [`Conversation::decide`].
    Approve { proposals: Vec<Proposal> },
    /// The AI has answered.
    Done { text: String, stop: StopReason },
}

/// The developer's answer to one proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub call_id: String,
    pub approve: bool,
    /// Why not (goes to the AI).
    pub reason: Option<String>,
}

/// A conversation in one project.
#[derive(Debug, Clone)]
pub struct Conversation {
    /// What the AI may do besides reading.
    pub permission: AiPermission,
    /// Upper limit of model calls per developer message.
    pub max_steps: u32,
    /// Answer limit per call.
    pub max_tokens: u32,
    system: String,
    history: Vec<Message>,
    events: Vec<Event>,
    pending: Vec<Proposal>,
    /// Tool results of the current round, sent once all calls are done.
    results: Vec<Part>,
    steps: u32,
    usage: Usage,
}

impl Conversation {
    pub const MAX_STEPS: u32 = 40;

    /// A new conversation; `system` comes from [`system_prompt`].
    pub fn new(system: String, permission: AiPermission) -> Self {
        Self {
            permission,
            max_steps: Self::MAX_STEPS,
            max_tokens: ChatRequest::MAX_TOKENS,
            system,
            history: Vec::new(),
            events: Vec::new(),
            pending: Vec::new(),
            results: Vec::new(),
            steps: 0,
            usage: Usage::default(),
        }
    }

    pub fn history(&self) -> &[Message] {
        &self.history
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    /// Proposals waiting for a decision.
    pub fn pending(&self) -> &[Proposal] {
        &self.pending
    }

    /// Tokens used so far.
    pub fn usage(&self) -> Usage {
        self.usage
    }

    /// Adds the developer's message.
    pub fn say(&mut self, text: &str) {
        self.reject_pending("Der Entwickler hat stattdessen etwas Neues geschrieben.");
        self.steps = 0;
        self.history.push(Message::user(text));
    }

    /// The tools offered now.
    pub fn tools(&self, skills: &SkillTable) -> Vec<ToolSpec> {
        let mut specs = tools::specs(self.permission);
        if !skills.skills().is_empty() {
            specs.push(ToolSpec {
                name: LOAD_SKILL.into(),
                description: "Lädt die Anleitung eines Skills (Liste im Systemprompt). / Loads a skill's instructions.".into(),
                input_schema: json!({ "type": "object", "required": ["name"], "properties": {
                    "name": { "type": "string" }
                }}),
            });
        }
        specs
    }

    /// The next request to the model.
    pub fn request(&self, skills: &SkillTable) -> ChatRequest {
        ChatRequest {
            system: self.system.clone(),
            messages: self.history.clone(),
            tools: self.tools(skills),
            max_tokens: self.max_tokens,
            temperature: None,
        }
    }

    /// Takes the model's answer and runs or proposes its tool calls.
    pub fn receive(
        &mut self,
        response: ChatResponse,
        ws: &mut dyn Workspace,
        skills: &SkillTable,
    ) -> Result<Turn, AiError> {
        self.usage.input_tokens += response.usage.input_tokens;
        self.usage.output_tokens += response.usage.output_tokens;
        self.steps += 1;
        let calls: Vec<(String, String, Value)> = response
            .message
            .tool_calls()
            .map(|(id, name, input)| (id.to_owned(), name.to_owned(), input.clone()))
            .collect();
        let text = response.text();
        self.history.push(response.message);
        if calls.is_empty() {
            return Ok(Turn::Done {
                text,
                stop: response.stop,
            });
        }
        if self.steps > self.max_steps {
            return Err(AiError::new(
                codes::TOO_MANY_STEPS,
                format!("mehr als {} Schritte für eine Nachricht", self.max_steps),
            ));
        }
        for (id, name, input) in calls {
            self.call(ws, skills, id, name, input);
        }
        Ok(self.next_turn())
    }

    fn call(
        &mut self,
        ws: &mut dyn Workspace,
        skills: &SkillTable,
        id: String,
        name: String,
        input: Value,
    ) {
        if name == LOAD_SKILL {
            let wanted = input["name"].as_str().unwrap_or_default();
            let result = skills
                .get(wanted)
                .map(|s| s.body.clone())
                .ok_or_else(|| format!("Skill „{wanted}“ gibt es nicht"));
            self.events.push(Event::Read {
                tool: name.clone(),
                input,
                ok: result.is_ok(),
            });
            self.result(id, name, result);
            return;
        }
        let Some(access) = tools::access(&name) else {
            self.refuse(id, name.clone(), format!("unbekanntes Werkzeug „{name}“"));
            return;
        };
        if access == Access::Read {
            let result = tools::read(ws, &name, &input);
            self.events.push(Event::Read {
                tool: name.clone(),
                input,
                ok: result.is_ok(),
            });
            self.result(id, name, result);
            return;
        }
        if !access.offered(self.permission) {
            self.refuse(
                id,
                name,
                "In diesem Projekt darf die KI nur lesen; Änderungen als Text vorschlagen.".into(),
            );
            return;
        }
        let proposal = match tools::plan(ws, &id, &name, &input) {
            Ok(p) => p,
            Err(e) => {
                self.refuse(id, name, e);
                return;
            }
        };
        if self.permission == AiPermission::Autonomous {
            let result = tools::apply(ws, &proposal);
            self.result(id, name, result.clone());
            self.events.push(Event::Applied {
                proposal,
                approved: false,
                result,
            });
        } else {
            self.events.push(Event::Proposed {
                proposal: proposal.clone(),
            });
            self.pending.push(proposal);
        }
    }

    fn result(&mut self, id: String, name: String, result: Result<String, String>) {
        let (content, is_error) = match result {
            Ok(c) => (c, false),
            Err(e) => (e, true),
        };
        self.results.push(Part::ToolResult {
            id,
            name,
            content,
            is_error,
        });
    }

    fn refuse(&mut self, id: String, tool: String, reason: String) {
        self.events.push(Event::Refused {
            tool: tool.clone(),
            reason: reason.clone(),
        });
        self.result(id, tool, Err(reason));
    }

    /// Once nothing waits, the results go into the history as one message.
    fn next_turn(&mut self) -> Turn {
        if !self.pending.is_empty() {
            return Turn::Approve {
                proposals: self.pending.clone(),
            };
        }
        // Reihenfolge wie die Aufrufe (manche Anbieter verlangen das).
        let order: Vec<String> = self
            .history
            .last()
            .map(|m| m.tool_calls().map(|(id, _, _)| id.to_owned()).collect())
            .unwrap_or_default();
        let mut parts = std::mem::take(&mut self.results);
        parts.sort_by_key(|p| match p {
            Part::ToolResult { id, .. } => order.iter().position(|o| o == id),
            _ => None,
        });
        self.history.push(Message {
            role: Role::User,
            parts,
        });
        Turn::Continue
    }

    /// Applies the approved proposals and rejects the others (proposals
    /// without a decision are rejected).
    pub fn decide(&mut self, decisions: &[Decision], ws: &mut dyn Workspace) -> Turn {
        for proposal in std::mem::take(&mut self.pending) {
            let decision = decisions.iter().find(|d| d.call_id == proposal.call_id);
            if decision.is_some_and(|d| d.approve) {
                let result = tools::apply(ws, &proposal);
                self.result(
                    proposal.call_id.clone(),
                    proposal.tool.clone(),
                    result.clone(),
                );
                self.events.push(Event::Applied {
                    proposal,
                    approved: true,
                    result,
                });
            } else {
                let reason = decision.and_then(|d| d.reason.clone()).map_or_else(
                    || "Vom Entwickler abgelehnt.".to_owned(),
                    |r| format!("Vom Entwickler abgelehnt: {r}"),
                );
                self.result(proposal.call_id.clone(), proposal.tool.clone(), Err(reason));
                self.events.push(Event::Rejected { proposal });
            }
        }
        self.next_turn()
    }

    /// Approves or rejects everything that waits.
    pub fn decide_all(&mut self, approve: bool, ws: &mut dyn Workspace) -> Turn {
        let decisions: Vec<Decision> = self
            .pending
            .iter()
            .map(|p| Decision {
                call_id: p.call_id.clone(),
                approve,
                reason: None,
            })
            .collect();
        self.decide(&decisions, ws)
    }

    /// Drops waiting proposals (e.g. when the developer writes again), so
    /// the history stays valid for the providers.
    fn reject_pending(&mut self, reason: &str) {
        if self.pending.is_empty() && self.results.is_empty() {
            return;
        }
        for proposal in std::mem::take(&mut self.pending) {
            self.result(
                proposal.call_id.clone(),
                proposal.tool.clone(),
                Err(reason.to_owned()),
            );
            self.events.push(Event::Rejected { proposal });
        }
        self.next_turn();
    }
}

/// The system prompt: role, rules of Freya TDE, permission and skills.
pub fn system_prompt(ws: &dyn Workspace, permission: AiPermission, skills: &SkillTable) -> String {
    let doc = ws.document();
    let project = doc.project();
    let rights = match permission {
        AiPermission::ReadOnly => {
            "You may only read. Do not try to change anything; describe proposed changes as text."
        }
        AiPermission::Preview => {
            "Every change and every cargo run is shown to the developer as a preview and only happens after approval; a rejected change comes back as an error with the reason."
        }
        AiPermission::Autonomous => {
            "Changes and cargo runs happen at once; work carefully and in small steps."
        }
    };
    let mut prompt = format!(
        "You are the AI assistant inside Freya TDE, a visual RAD environment for Rust in the style of Delphi/Lazarus. \
Answer in the language the developer writes in.\n\n\
Project: {name} (backend: {backend}, packs: {packs}, Rust {rust}).\n\n\
Rules:\n\
- Read freely with the tools before you change anything.\n\
- Forms (.ftdr) and the project file (.ptdr) change only through the tool `designer`.\n\
- Generated files (*.gen.rs) are never edited; change the form and call `generate_code`.\n\
- User code (src/<form>.rs, src/main.rs, src/lib.rs, Cargo.toml) changes with `edit_file`/`write_file`.\n\
- After changes, check with `cargo` (check).\n\
- {rights}\n",
        name = project.name,
        backend = project.backend,
        packs = if project.packs.is_empty() {
            "none".to_owned()
        } else {
            project.packs.join(", ")
        },
        rust = project.rust_version,
    );
    let catalog = skills.catalog();
    if !catalog.is_empty() {
        prompt.push_str(&format!(
            "\nSkills (load the instructions with `{LOAD_SKILL}` when one fits the task):\n{catalog}"
        ));
    }
    prompt
}

/// Runs one developer message to the end, blocking: requests, tools and
/// approvals (`approve` decides each round of proposals).
pub fn run(
    client: &Client,
    conversation: &mut Conversation,
    ws: &mut dyn Workspace,
    skills: &SkillTable,
    message: &str,
    mut approve: impl FnMut(&[Proposal]) -> Vec<Decision>,
) -> Result<String, AiError> {
    conversation.say(message);
    loop {
        let response = client.chat(&conversation.request(skills))?;
        let mut turn = conversation.receive(response, ws, skills)?;
        loop {
            match turn {
                Turn::Continue => break,
                Turn::Approve { proposals } => {
                    let decisions = approve(&proposals);
                    turn = conversation.decide(&decisions, ws);
                }
                Turn::Done { text, .. } => return Ok(text),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Session;
    use std::path::PathBuf;
    use tdr_core::TypeRegistry;

    fn project(tag: &str) -> (Session, PathBuf) {
        let dir = std::env::temp_dir().join(format!("freya-ai-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/notes.txt"), "eins\nzwei\ndrei\n").unwrap();
        let session = Session::create(&dir, "demo", "none", TypeRegistry::with_builtins()).unwrap();
        (session, dir)
    }

    /// An answer of the model with tool calls `(id, name, input)`.
    fn calls(calls: &[(&str, &str, Value)]) -> ChatResponse {
        ChatResponse {
            message: Message {
                role: Role::Assistant,
                parts: calls
                    .iter()
                    .map(|(id, name, input)| Part::ToolCall {
                        id: (*id).into(),
                        name: (*name).into(),
                        input: input.clone(),
                        signature: None,
                    })
                    .collect(),
            },
            stop: StopReason::ToolUse,
            usage: Usage {
                input_tokens: 10,
                output_tokens: 2,
            },
            model: String::new(),
        }
    }

    fn results(c: &Conversation) -> Vec<(String, String, bool)> {
        match c.history().last() {
            Some(Message {
                role: Role::User,
                parts,
            }) => parts
                .iter()
                .filter_map(|p| match p {
                    Part::ToolResult {
                        id,
                        content,
                        is_error,
                        ..
                    } => Some((id.clone(), content.clone(), *is_error)),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    fn has_button(ws: &Session) -> bool {
        let name = tdr_core::ComponentName::new("ok_button").unwrap();
        ws.document().forms()[0].widget(&name).is_some()
    }

    fn add_button() -> Value {
        json!({ "commands": [{
            "command": "add_widget", "form": "main_form", "parent": "window",
            "type": "core/Button", "name": "ok_button"
        }]})
    }

    #[test]
    fn preview_waits_for_approval() {
        let (mut ws, dir) = project("preview");
        let skills = SkillTable::builtin();
        let mut c = Conversation::new(
            system_prompt(&ws, AiPermission::Preview, &skills),
            AiPermission::Preview,
        );
        assert!(c.request(&skills).system.contains("freya-forms"));
        c.say("Füge einen OK-Knopf hinzu");
        let turn = c
            .receive(
                calls(&[
                    ("r1", "project_info", json!({})),
                    ("w1", "designer", add_button()),
                    (
                        "w2",
                        "edit_file",
                        json!({"path": "src/notes.txt", "old_text": "zwei", "new_text": "2"}),
                    ),
                    ("s1", LOAD_SKILL, json!({"name": "freya-forms"})),
                ]),
                &mut ws,
                &skills,
            )
            .unwrap();
        let Turn::Approve { proposals } = turn else {
            panic!("{turn:?}")
        };
        assert_eq!(proposals.len(), 2);
        assert!(proposals[1].preview.contains("-zwei\n+2"));
        // Noch nichts geändert.
        assert!(!has_button(&ws));
        assert!(
            std::fs::read_to_string(dir.join("src/notes.txt"))
                .unwrap()
                .contains("zwei")
        );
        let turn = c.decide(
            &[
                Decision {
                    call_id: "w1".into(),
                    approve: true,
                    reason: None,
                },
                Decision {
                    call_id: "w2".into(),
                    approve: false,
                    reason: Some("bitte nicht".into()),
                },
            ],
            &mut ws,
        );
        assert_eq!(turn, Turn::Continue);
        assert!(has_button(&ws));
        assert!(
            std::fs::read_to_string(dir.join("src/notes.txt"))
                .unwrap()
                .contains("zwei")
        );
        let r = results(&c);
        let ids: Vec<&str> = r.iter().map(|x| x.0.as_str()).collect();
        assert_eq!(ids, ["r1", "w1", "w2", "s1"], "Reihenfolge wie die Aufrufe");
        assert!(r[0].1.contains("\"name\": \"demo\"") && !r[0].2);
        assert!(r[2].2 && r[2].1.contains("bitte nicht"));
        assert!(r[3].1.contains("Designer-Befehlen"));
        let done = c
            .receive(
                ChatResponse {
                    message: Message::assistant("Fertig."),
                    stop: StopReason::EndTurn,
                    usage: Usage::default(),
                    model: String::new(),
                },
                &mut ws,
                &skills,
            )
            .unwrap();
        assert_eq!(
            done,
            Turn::Done {
                text: "Fertig.".into(),
                stop: StopReason::EndTurn
            }
        );
        assert_eq!(c.usage().input_tokens, 10);
        assert!(matches!(c.events().last(), Some(Event::Rejected { .. })));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_only_and_autonomous() {
        let (mut ws, dir) = project("rights");
        let skills = SkillTable::default();
        let mut c = Conversation::new(String::new(), AiPermission::ReadOnly);
        assert!(
            c.tools(&skills)
                .iter()
                .all(|t| tools::access(&t.name) == Some(Access::Read))
        );
        c.say("x");
        let turn = c
            .receive(calls(&[("w1", "designer", add_button())]), &mut ws, &skills)
            .unwrap();
        assert_eq!(turn, Turn::Continue);
        assert!(results(&c)[0].2);
        assert!(!has_button(&ws));

        let mut c = Conversation::new(String::new(), AiPermission::Autonomous);
        c.say("x");
        let turn = c
            .receive(
                calls(&[
                    ("w1", "designer", add_button()),
                    (
                        "w2",
                        "write_file",
                        json!({"path": "src/neu.txt", "content": "hallo"}),
                    ),
                    (
                        "w3",
                        "write_file",
                        json!({"path": "../raus.txt", "content": "x"}),
                    ),
                    (
                        "w4",
                        "write_file",
                        json!({"path": "src/main_form.gen.rs", "content": "x"}),
                    ),
                    ("w5", "nonsense", json!({})),
                ]),
                &mut ws,
                &skills,
            )
            .unwrap();
        assert_eq!(turn, Turn::Continue);
        let r = results(&c);
        assert!(!r[0].2 && !r[1].2, "{r:?}");
        assert!(r[2].2 && r[2].1.contains("innerhalb"));
        assert!(r[3].2 && r[3].1.contains("schreibgeschützt"));
        assert!(r[4].2 && r[4].1.contains("unbekannt"));
        assert_eq!(
            std::fs::read_to_string(dir.join("src/neu.txt")).unwrap(),
            "hallo"
        );
        assert!(has_button(&ws));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stale_preview_is_refused_and_new_message_rejects() {
        let (mut ws, dir) = project("stale");
        let skills = SkillTable::default();
        let mut c = Conversation::new(String::new(), AiPermission::Preview);
        c.say("x");
        c.receive(
            calls(&[(
                "w1",
                "write_file",
                json!({"path": "src/notes.txt", "content": "neu"}),
            )]),
            &mut ws,
            &skills,
        )
        .unwrap();
        std::fs::write(dir.join("src/notes.txt"), "von Hand geändert").unwrap();
        c.decide_all(true, &mut ws);
        assert!(results(&c)[0].1.contains("seit der Vorschau"));
        assert_eq!(
            std::fs::read_to_string(dir.join("src/notes.txt")).unwrap(),
            "von Hand geändert"
        );
        // Neue Nachricht, während etwas wartet: abgelehnt, Verlauf bleibt gültig.
        c.receive(
            calls(&[(
                "w2",
                "write_file",
                json!({"path": "src/x.txt", "content": "x"}),
            )]),
            &mut ws,
            &skills,
        )
        .unwrap();
        c.say("doch nicht");
        let h = c.history();
        assert!(matches!(
            &h[h.len() - 2].parts[0],
            Part::ToolResult { is_error: true, .. }
        ));
        assert_eq!(h.last().unwrap().text(), "doch nicht");
        assert!(c.pending().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_against_a_server() {
        let (mut ws, dir) = project("run");
        let (addr, rx) = crate::ai::mock::server(vec![
            (
                200,
                json!({"choices": [{"message": {"content": null, "tool_calls": [
                    {"id": "a", "type": "function", "function": {"name": "read_file", "arguments": "{\"path\":\"src/notes.txt\"}"}},
                    {"id": "b", "type": "function", "function": {"name": "write_file", "arguments": "{\"path\":\"src/out.txt\",\"content\":\"ok\"}"}}
                ]}, "finish_reason": "tool_calls"}]}),
            ),
            (
                200,
                json!({"choices": [{"message": {"content": "Erledigt."}, "finish_reason": "stop"}]}),
            ),
        ]);
        let mut profile =
            crate::settings::AiProfile::new("t", crate::settings::AiProvider::LmStudio);
        profile.endpoint = Some(format!("{addr}/v1"));
        profile.model = "local".into();
        let client = Client::new(&profile, None).unwrap();
        let skills = SkillTable::builtin();
        let mut c = Conversation::new(
            system_prompt(&ws, AiPermission::Preview, &skills),
            AiPermission::Preview,
        );
        let mut asked = 0;
        let text = run(&client, &mut c, &mut ws, &skills, "Lies und schreib", |p| {
            asked += 1;
            assert_eq!(p[0].summary, "src/out.txt anlegen");
            vec![Decision {
                call_id: p[0].call_id.clone(),
                approve: true,
                reason: None,
            }]
        })
        .unwrap();
        assert_eq!((text.as_str(), asked), ("Erledigt.", 1));
        assert_eq!(
            std::fs::read_to_string(dir.join("src/out.txt")).unwrap(),
            "ok"
        );
        let first = rx.recv().unwrap();
        assert!(
            first.body["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|t| t["function"]["name"] == LOAD_SKILL)
        );
        let second = rx.recv().unwrap();
        let msgs = second.body["messages"].as_array().unwrap();
        let tool_msgs: Vec<&Value> = msgs.iter().filter(|m| m["role"] == "tool").collect();
        assert_eq!(tool_msgs.len(), 2);
        assert!(tool_msgs[0]["content"].as_str().unwrap().contains("zwei"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
