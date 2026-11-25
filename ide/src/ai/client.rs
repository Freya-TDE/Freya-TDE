//! The HTTP client for one AI profile (synchronous, rustls).

use crate::tr;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::message::{ChatRequest, ChatResponse};
use super::{AiError, AiIssue, anthropic, codes, gemini, openai, security, stream};
use crate::settings::{AiProfile, AiProtocol, AiProvider, SecretStore, Settings};

/// A model offered by the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelInfo {
    /// The name to put into the profile (`claude-opus-5-5`, `llama3.2` …).
    pub id: String,
    /// A readable name, if the server gives one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Talks to the service of one profile.
#[derive(Debug, Clone)]
pub struct Client {
    provider: AiProvider,
    endpoint: String,
    model: String,
    key: Option<String>,
    agent: ureq::Agent,
}

/// Time to connect.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Time for a whole answer: local models on small machines are slow.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(600);

impl Client {
    /// A client for `profile` with the API key `key` (see
    /// [`Client::from_settings`] to take it from the keyring).
    pub fn new(profile: &AiProfile, key: Option<String>) -> Result<Self, AiError> {
        let endpoint = profile
            .effective_endpoint()
            .ok_or_else(|| {
                AiError::new(
                    codes::ENDPOINT_MISSING,
                    tr!("settings.msg.no_endpoint", name = profile.name),
                )
            })?
            .trim()
            .trim_end_matches('/')
            .to_owned();
        let key = key.filter(|k| !k.trim().is_empty());
        if profile.provider.needs_key() && key.is_none() {
            return Err(AiError::new(
                codes::KEY_MISSING,
                tr!("settings.msg.no_key", name = profile.name),
            ));
        }
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .timeout_global(Some(ANSWER_TIMEOUT))
            .user_agent(concat!("freya-tde/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        Ok(Self {
            provider: profile.provider,
            endpoint,
            model: profile.model.trim().to_owned(),
            key,
            agent,
        })
    }

    /// The client for profile `name`, its key from `store` (or the file,
    /// or `FREYA_TDE_AI_KEY`).
    pub fn from_settings(
        settings: &Settings,
        name: &str,
        store: &dyn SecretStore,
    ) -> Result<Self, AiError> {
        let profile = settings.ai.profile(name).ok_or_else(|| {
            AiError::new(
                codes::PROFILE_UNKNOWN,
                tr!("settings.msg.profile_unknown", name = name),
            )
        })?;
        Self::new(profile, settings.key(name, store))
    }

    pub fn provider(&self) -> AiProvider {
        self.provider
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    /// Uses another model of the same service.
    pub fn with_model(mut self, model: &str) -> Self {
        model.trim().clone_into(&mut self.model);
        self
    }

    /// A warning if the key would go unencrypted to another machine.
    pub fn warning(&self) -> Option<AiIssue> {
        security::key_transport_risk(&self.endpoint, self.key.is_some())
    }

    /// Base address for the OpenAI protocol: Ollama's endpoint has no `/v1`.
    fn openai_base(&self) -> String {
        if self.provider == AiProvider::Ollama && !self.endpoint.ends_with("/v1") {
            format!("{}/v1", self.endpoint)
        } else {
            self.endpoint.clone()
        }
    }

    /// Base address for Anthropic and Gemini (the paths bring the version).
    fn root(&self) -> &str {
        self.endpoint
            .strip_suffix("/v1beta")
            .or_else(|| self.endpoint.strip_suffix("/v1"))
            .unwrap_or(&self.endpoint)
    }

    /// The models the service offers ("Modelle abrufen").
    pub fn models(&self) -> Result<Vec<ModelInfo>, AiError> {
        let (url, parse): (String, fn(&Value) -> Vec<ModelInfo>) = match self.provider.protocol() {
            AiProtocol::Anthropic => (
                format!("{}/v1/models?limit=1000", self.root()),
                anthropic::models,
            ),
            AiProtocol::Gemini => (
                format!("{}/v1beta/models?pageSize=1000", self.root()),
                gemini::models,
            ),
            AiProtocol::OpenAi if self.provider == AiProvider::Ollama => (
                format!("{}/api/tags", self.endpoint.trim_end_matches("/v1")),
                openai::ollama_models,
            ),
            AiProtocol::OpenAi => (format!("{}/models", self.openai_base()), openai::models),
        };
        let v = self.send(self.headers(self.agent.get(&url)).call())?;
        let mut models = parse(&v);
        models.sort_by(|a, b| a.id.cmp(&b.id));
        models.dedup_by(|a, b| a.id == b.id);
        Ok(models)
    }

    /// Sends `req` and waits for the whole answer.
    pub fn chat(&self, req: &ChatRequest) -> Result<ChatResponse, AiError> {
        let (url, body) = self.chat_request(req, false)?;
        let v = self.send(self.headers(self.agent.post(&url)).send_json(&body))?;
        self.parse(&v)
    }

    /// Sends `req` with streaming: `on_text` gets the answer's text piece
    /// by piece as it arrives; the result is the same as from
    /// [`Client::chat`]. A server that answers without streaming is read
    /// the usual way (`on_text` then gets the whole text at once).
    pub fn chat_stream(
        &self,
        req: &ChatRequest,
        on_text: &mut dyn FnMut(&str),
    ) -> Result<ChatResponse, AiError> {
        let (url, body) = self.chat_request(req, true)?;
        let mut resp = self
            .headers(self.agent.post(&url))
            .header("Accept", "text/event-stream")
            .send_json(&body)
            .map_err(transport_error)?;
        let status = resp.status().as_u16();
        let events = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|t| t.starts_with("text/event-stream"));
        if !(200..300).contains(&status) || !events {
            let v = self.send(Ok(resp))?;
            let response = self.parse(&v)?;
            let text = response.text();
            if !text.is_empty() {
                on_text(&text);
            }
            return Ok(response);
        }
        let mut collector = stream::Collector::new(self.provider.protocol());
        let reader = std::io::BufReader::new(
            resp.body_mut()
                .with_config()
                .limit(64 * 1024 * 1024)
                .reader(),
        );
        stream::read(reader, &mut collector, on_text)?;
        self.parse(&collector.finish())
    }

    /// Address and body of a chat request.
    fn chat_request(&self, req: &ChatRequest, stream: bool) -> Result<(String, Value), AiError> {
        if self.model.is_empty() {
            return Err(AiError::new(codes::MODEL_MISSING, tr!("ai.msg.no_model")));
        }
        Ok(match self.provider.protocol() {
            AiProtocol::Anthropic => {
                let mut body = anthropic::request(&self.model, req);
                if stream {
                    body["stream"] = Value::Bool(true);
                }
                (format!("{}/v1/messages", self.root()), body)
            }
            AiProtocol::Gemini => (
                format!("{}{}", self.root(), gemini::path(&self.model, stream)),
                gemini::request(req),
            ),
            AiProtocol::OpenAi => {
                let own = self.provider == AiProvider::OpenAi;
                let mut body = openai::request(&self.model, req, own);
                if stream {
                    body["stream"] = Value::Bool(true);
                    // Verbrauch am Ende des Stroms; nicht jeder kompatible
                    // Server kennt das Feld, deshalb nur bei OpenAI selbst.
                    if own {
                        body["stream_options"] = serde_json::json!({ "include_usage": true });
                    }
                }
                (format!("{}/chat/completions", self.openai_base()), body)
            }
        })
    }

    fn parse(&self, v: &Value) -> Result<ChatResponse, AiError> {
        match self.provider.protocol() {
            AiProtocol::Anthropic => anthropic::response(v),
            AiProtocol::Gemini => gemini::response(v),
            AiProtocol::OpenAi => openai::response(v),
        }
    }

    fn headers<B>(&self, req: ureq::RequestBuilder<B>) -> ureq::RequestBuilder<B> {
        let req = match self.provider.protocol() {
            AiProtocol::Anthropic => req.header("anthropic-version", anthropic::VERSION),
            _ => req,
        };
        let Some(key) = &self.key else { return req };
        match self.provider.protocol() {
            AiProtocol::Anthropic => req.header("x-api-key", key),
            AiProtocol::Gemini => req.header("x-goog-api-key", key),
            AiProtocol::OpenAi => req.header("Authorization", format!("Bearer {key}")),
        }
    }

    /// Reads the answer as JSON and turns HTTP errors into codes.
    fn send(
        &self,
        result: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    ) -> Result<Value, AiError> {
        let mut resp = result.map_err(transport_error)?;
        let status = resp.status().as_u16();
        let text = resp
            .body_mut()
            .with_config()
            .limit(64 * 1024 * 1024)
            .read_to_string()
            .map_err(transport_error)?;
        if (200..300).contains(&status) {
            return serde_json::from_str(&text)
                .map_err(|e| AiError::new(codes::BAD_RESPONSE, tr!("ai.msg.not_json", error = e)));
        }
        let code = match status {
            401 | 403 => codes::AUTH_FAILED,
            404 => codes::NOT_FOUND,
            429 => codes::RATE_LIMITED,
            _ => codes::HTTP_ERROR,
        };
        Err(AiError {
            code,
            detail: format!("HTTP {status}: {}", error_message(&text)),
            status: Some(status),
        })
    }
}

/// The server's error text: `error.message` (Anthropic, OpenAI, Gemini),
/// `error` as text (Ollama) or the beginning of the body.
fn error_message(body: &str) -> String {
    if let Ok(v) = serde_json::from_str::<Value>(body) {
        if let Some(m) = v["error"]["message"].as_str().or(v["error"].as_str()) {
            return m.to_owned();
        }
        if let Some(m) = v["message"].as_str() {
            return m.to_owned();
        }
    }
    body.chars().take(300).collect()
}

fn transport_error(e: ureq::Error) -> AiError {
    use ureq::Error as E;
    let code = match &e {
        E::Timeout(_) => codes::TIMEOUT,
        E::HostNotFound | E::ConnectionFailed | E::Io(_) => codes::UNREACHABLE,
        E::Tls(_) | E::Rustls(_) | E::Pem(_) | E::TlsRequired => codes::TLS_FAILED,
        E::Json(_) => codes::BAD_RESPONSE,
        _ => codes::UNREACHABLE,
    };
    AiError::new(code, e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::message::{Message, Part, StopReason, ToolSpec};
    use crate::ai::mock::server;
    use serde_json::json;
    use std::net::TcpListener;

    fn profile(provider: AiProvider, endpoint: &str) -> AiProfile {
        let mut p = AiProfile::new("Test", provider);
        p.endpoint = Some(endpoint.to_owned());
        p.model = "m1".into();
        p
    }

    fn tool_request() -> ChatRequest {
        let mut req = ChatRequest::new(vec![
            Message::user("Wie heißt das Projekt?"),
            Message {
                role: crate::ai::Role::Assistant,
                parts: vec![Part::ToolCall {
                    id: "c1".into(),
                    name: "project_info".into(),
                    input: json!({}),
                    signature: Some("sig".into()),
                }],
            },
            Message {
                role: crate::ai::Role::User,
                parts: vec![Part::ToolResult {
                    id: "c1".into(),
                    name: "project_info".into(),
                    content: "demo".into(),
                    is_error: false,
                }],
            },
        ]);
        req.system = "Du bist Freya.".into();
        req.tools.push(ToolSpec {
            name: "project_info".into(),
            description: "Projekt".into(),
            input_schema: json!({"type": "object", "properties": {}, "additionalProperties": false}),
        });
        req
    }

    #[test]
    fn anthropic_round_trip() {
        let (addr, rx) = server(vec![(
            200,
            json!({
                "model": "m1",
                "content": [
                    {"type": "text", "text": "Ich sehe nach."},
                    {"type": "tool_use", "id": "t9", "name": "read_file", "input": {"path": "src/main.rs"}}
                ],
                "stop_reason": "tool_use",
                "usage": {"input_tokens": 12, "output_tokens": 5}
            }),
        )]);
        let c = Client::new(&profile(AiProvider::Anthropic, &addr), Some("sk-a".into())).unwrap();
        let r = c.chat(&tool_request()).unwrap();
        let seen = rx.recv().unwrap();
        assert_eq!(seen.line, "POST /v1/messages HTTP/1.1");
        assert_eq!(seen.header("x-api-key"), Some("sk-a"));
        assert_eq!(seen.header("anthropic-version"), Some(anthropic::VERSION));
        assert_eq!(seen.body["system"], "Du bist Freya.");
        assert_eq!(seen.body["messages"][1]["content"][0]["type"], "tool_use");
        assert_eq!(seen.body["messages"][2]["content"][0]["tool_use_id"], "c1");
        assert_eq!(seen.body["tools"][0]["input_schema"]["type"], "object");
        assert_eq!(r.stop, StopReason::ToolUse);
        assert_eq!(r.text(), "Ich sehe nach.");
        let calls: Vec<_> = r.message.tool_calls().collect();
        assert_eq!(calls[0].1, "read_file");
        assert_eq!(calls[0].2["path"], "src/main.rs");
        assert_eq!(r.usage.input_tokens, 12);
    }

    #[test]
    fn openai_round_trip_and_token_field() {
        let answer = json!({
            "model": "m1",
            "choices": [{
                "message": {"role": "assistant", "content": null, "tool_calls": [
                    {"id": "x1", "type": "function", "function": {"name": "read_file", "arguments": "{\"path\":\"a.rs\"}"}}
                ]},
                "finish_reason": "tool_calls"
            }],
            "usage": {"prompt_tokens": 3, "completion_tokens": 4}
        });
        let (addr, rx) = server(vec![(200, answer.clone()), (200, answer)]);
        // OpenAI selbst: max_completion_tokens, Bearer.
        let c = Client::new(
            &profile(AiProvider::OpenAi, &format!("{addr}/v1")),
            Some("sk-o".into()),
        )
        .unwrap();
        let r = c.chat(&tool_request()).unwrap();
        let seen = rx.recv().unwrap();
        assert_eq!(seen.line, "POST /v1/chat/completions HTTP/1.1");
        assert_eq!(seen.header("authorization"), Some("Bearer sk-o"));
        assert!(seen.body.get("max_completion_tokens").is_some());
        let msgs = seen.body["messages"].as_array().unwrap();
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[2]["tool_calls"][0]["function"]["arguments"], "{}");
        assert_eq!(msgs[3]["role"], "tool");
        assert_eq!(msgs[3]["tool_call_id"], "c1");
        assert_eq!(r.stop, StopReason::ToolUse);
        assert_eq!(r.message.tool_calls().next().unwrap().2["path"], "a.rs");
        // Ollama: /v1 wird ergänzt, kein Key, max_tokens.
        let c = Client::new(&profile(AiProvider::Ollama, &addr), None).unwrap();
        c.chat(&tool_request()).unwrap();
        let seen = rx.recv().unwrap();
        assert_eq!(seen.line, "POST /v1/chat/completions HTTP/1.1");
        assert_eq!(seen.header("authorization"), None);
        assert!(seen.body.get("max_tokens").is_some());
    }

    #[test]
    fn gemini_round_trip() {
        let (addr, rx) = server(vec![(
            200,
            json!({
                "modelVersion": "m1",
                "candidates": [{
                    "content": {"role": "model", "parts": [
                        {"text": "denke", "thought": true},
                        {"functionCall": {"name": "read_file", "args": {"path": "b.rs"}}, "thoughtSignature": "s2"}
                    ]},
                    "finishReason": "STOP"
                }],
                "usageMetadata": {"promptTokenCount": 7, "candidatesTokenCount": 2}
            }),
        )]);
        let c = Client::new(&profile(AiProvider::Google, &addr), Some("g-k".into())).unwrap();
        let r = c.chat(&tool_request()).unwrap();
        let seen = rx.recv().unwrap();
        assert_eq!(seen.line, "POST /v1beta/models/m1:generateContent HTTP/1.1");
        assert_eq!(seen.header("x-goog-api-key"), Some("g-k"));
        let b = &seen.body;
        assert_eq!(b["systemInstruction"]["parts"][0]["text"], "Du bist Freya.");
        assert_eq!(b["contents"][1]["role"], "model");
        assert_eq!(b["contents"][1]["parts"][0]["thoughtSignature"], "sig");
        assert_eq!(
            b["contents"][2]["parts"][0]["functionResponse"]["response"]["result"],
            "demo"
        );
        let params = &b["tools"][0]["functionDeclarations"][0]["parameters"];
        assert!(params.get("additionalProperties").is_none());
        assert_eq!(r.stop, StopReason::ToolUse);
        assert_eq!(r.text(), "");
        match &r.message.parts[0] {
            Part::ToolCall { id, signature, .. } => {
                assert!(id.starts_with(gemini::LOCAL_ID));
                assert_eq!(signature.as_deref(), Some("s2"));
            }
            p => panic!("{p:?}"),
        }
    }

    #[test]
    fn streamed_answers() {
        use crate::ai::mock::raw_server;
        let sse = "event: message_start\n\
data: {\"type\":\"message_start\",\"message\":{\"model\":\"m1\",\"usage\":{\"input_tokens\":2}}}\n\n\
event: content_block_start\n\
data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hallo \"}}\n\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Welt\"}}\n\n\
event: message_delta\n\
data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":3}}\n\n\
event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
        let (addr, rx) = raw_server(vec![
            (200, "text/event-stream; charset=utf-8", sse.to_owned()),
            // Ein Server, der das Streaming überhört, antwortet mit JSON.
            (
                200,
                "application/json",
                json!({"choices": [{"message": {"content": "am Stück"}, "finish_reason": "stop"}]})
                    .to_string(),
            ),
            (
                200,
                "text/event-stream",
                "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"G\"}]},\"finishReason\":\"STOP\"}]}\n\n"
                    .to_owned(),
            ),
            (401, "application/json", json!({"error": {"message": "falscher Key"}}).to_string()),
        ]);
        let c = Client::new(&profile(AiProvider::Anthropic, &addr), Some("k".into())).unwrap();
        let mut pieces = Vec::new();
        let r = c
            .chat_stream(&tool_request(), &mut |t| pieces.push(t.to_owned()))
            .unwrap();
        let seen = rx.recv().unwrap();
        assert_eq!(seen.body["stream"], true);
        assert_eq!(seen.header("accept"), Some("text/event-stream"));
        assert_eq!(pieces, ["Hallo ", "Welt"]);
        assert_eq!(r.text(), "Hallo Welt");
        assert_eq!(r.stop, StopReason::EndTurn);
        assert_eq!(r.usage.output_tokens, 3);

        let c = Client::new(
            &profile(AiProvider::OpenAi, &format!("{addr}/v1")),
            Some("k".into()),
        )
        .unwrap();
        let mut pieces = Vec::new();
        let r = c
            .chat_stream(&tool_request(), &mut |t| pieces.push(t.to_owned()))
            .unwrap();
        let seen = rx.recv().unwrap();
        assert_eq!(seen.body["stream"], true);
        assert_eq!(seen.body["stream_options"]["include_usage"], true);
        assert_eq!(
            (pieces, r.text()),
            (vec!["am Stück".to_owned()], "am Stück".to_owned())
        );

        let c = Client::new(&profile(AiProvider::Google, &addr), Some("k".into())).unwrap();
        let r = c.chat_stream(&tool_request(), &mut |_| {}).unwrap();
        assert_eq!(
            rx.recv().unwrap().line,
            "POST /v1beta/models/m1:streamGenerateContent?alt=sse HTTP/1.1"
        );
        assert_eq!(r.text(), "G");

        let c = Client::new(&profile(AiProvider::Mistral, &addr), Some("k".into())).unwrap();
        let e = c.chat_stream(&tool_request(), &mut |_| {}).unwrap_err();
        assert_eq!((e.code, e.status), (codes::AUTH_FAILED, Some(401)));
        assert!(rx.recv().unwrap().body.get("stream_options").is_none());
    }

    #[test]
    fn model_lists() {
        let (addr, rx) = server(vec![
            (
                200,
                json!({"models": [{"name": "qwen3:8b"}, {"name": "llama3.2:latest"}]}),
            ),
            (
                200,
                json!({"data": [{"id": "b"}, {"id": "a"}, {"id": "a"}]}),
            ),
            (
                200,
                json!({"models": [
                    {"name": "models/gemini-x", "displayName": "Gemini X", "supportedGenerationMethods": ["generateContent"]},
                    {"name": "models/embed", "supportedGenerationMethods": ["embedContent"]}
                ]}),
            ),
            (
                200,
                json!({"data": [{"id": "claude-x", "display_name": "Claude X"}]}),
            ),
        ]);
        let ids = |m: Vec<ModelInfo>| m.into_iter().map(|m| m.id).collect::<Vec<_>>();
        let ollama = Client::new(&profile(AiProvider::Ollama, &addr), None).unwrap();
        assert_eq!(
            ids(ollama.models().unwrap()),
            ["llama3.2:latest", "qwen3:8b"]
        );
        assert_eq!(rx.recv().unwrap().line, "GET /api/tags HTTP/1.1");
        let lm = Client::new(&profile(AiProvider::LmStudio, &format!("{addr}/v1")), None).unwrap();
        assert_eq!(ids(lm.models().unwrap()), ["a", "b"]);
        assert_eq!(rx.recv().unwrap().line, "GET /v1/models HTTP/1.1");
        let g = Client::new(&profile(AiProvider::Google, &addr), Some("k".into())).unwrap();
        let gm = g.models().unwrap();
        assert_eq!(gm.len(), 1);
        assert_eq!(gm[0].id, "gemini-x");
        assert!(rx.recv().unwrap().line.starts_with("GET /v1beta/models?"));
        let a = Client::new(
            &profile(AiProvider::Anthropic, &format!("{addr}/v1")),
            Some("k".into()),
        )
        .unwrap();
        assert_eq!(a.models().unwrap()[0].name.as_deref(), Some("Claude X"));
        assert!(rx.recv().unwrap().line.starts_with("GET /v1/models?"));
    }

    #[test]
    fn errors_have_codes() {
        let (addr, _rx) = server(vec![
            (401, json!({"error": {"message": "invalid x-api-key"}})),
            (429, json!({"error": {"message": "slow down"}})),
            (404, json!({"error": "model 'm1' not found"})),
            (200, json!({"unexpected": true})),
        ]);
        let c = Client::new(&profile(AiProvider::OpenAiCompatible, &addr), None).unwrap();
        let req = ChatRequest::new(vec![Message::user("hi")]);
        let e = c.chat(&req).unwrap_err();
        assert_eq!((e.code, e.status), (codes::AUTH_FAILED, Some(401)));
        assert!(e.detail.contains("invalid x-api-key"));
        assert_eq!(c.chat(&req).unwrap_err().code, codes::RATE_LIMITED);
        let e = c.chat(&req).unwrap_err();
        assert_eq!(e.code, codes::NOT_FOUND);
        assert!(e.detail.contains("not found"));
        assert_eq!(c.chat(&req).unwrap_err().code, codes::BAD_RESPONSE);
        // Niemand lauscht.
        let dead = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = dead.local_addr().unwrap().port();
        drop(dead);
        let c = Client::new(
            &profile(AiProvider::LmStudio, &format!("http://127.0.0.1:{port}/v1")),
            None,
        )
        .unwrap();
        assert_eq!(c.chat(&req).unwrap_err().code, codes::UNREACHABLE);
    }

    #[test]
    fn profile_checks() {
        let mut p = profile(AiProvider::Anthropic, "https://api.anthropic.com");
        assert_eq!(Client::new(&p, None).unwrap_err().code, codes::KEY_MISSING);
        p.provider = AiProvider::OpenAiCompatible;
        p.endpoint = None;
        assert_eq!(
            Client::new(&p, None).unwrap_err().code,
            codes::ENDPOINT_MISSING
        );
        p.endpoint = Some("http://192.168.1.20:8000/v1".into());
        p.model.clear();
        let c = Client::new(&p, Some("k".into())).unwrap();
        assert_eq!(c.warning().unwrap().code, codes::INSECURE_KEY_TRANSPORT);
        let e = c
            .chat(&ChatRequest::new(vec![Message::user("x")]))
            .unwrap_err();
        assert_eq!(e.code, codes::MODEL_MISSING);
        // Jeder Anbieter hat ein Protokoll und, außer dem eigenen Server, eine Adresse.
        for provider in AiProvider::ALL {
            let _ = provider.protocol();
            assert_eq!(
                provider.default_endpoint().is_none(),
                provider == AiProvider::OpenAiCompatible
            );
        }
    }
}
