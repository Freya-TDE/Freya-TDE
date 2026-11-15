//! The AI integration against a real provider. Skipped unless
//! `FREYA_TDE_TEST_AI` names one as `provider|endpoint|model`, e.g.
//!
//! ```sh
//! FREYA_TDE_TEST_AI="ollama|http://localhost:11434|qwen3:8b" cargo test -p freya-tde --test ai_live
//! FREYA_TDE_TEST_AI="anthropic||claude-sonnet-5-5" FREYA_TDE_AI_KEY=sk-… cargo test -p freya-tde --test ai_live
//! ```
//!
//! `provider` is the settings name (`anthropic`, `open_ai`, `google`, `xai`,
//! `ollama`, `lm_studio`, `open_ai_compatible` …); an empty endpoint means
//! the provider's default. The key comes from `FREYA_TDE_AI_KEY`.
//!
//! *Die KI-Anbindung gegen einen echten Anbieter (nur mit `FREYA_TDE_TEST_AI`).*

use freya_tde::ai::{Client, Conversation, Decision, SkillTable, run, system_prompt};
use freya_tde::session::Session;
use freya_tde::settings::{AiPermission, AiProfile, AiProvider};
use tdr_core::TypeRegistry;

fn profile() -> Option<AiProfile> {
    let spec = std::env::var("FREYA_TDE_TEST_AI").ok()?;
    let mut parts = spec.splitn(3, '|');
    let provider: AiProvider =
        serde_json::from_value(serde_json::json!(parts.next()?.trim())).expect("provider");
    let mut p = AiProfile::new("live", provider);
    p.endpoint = parts
        .next()
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .map(str::to_owned);
    p.model = parts.next()?.trim().to_owned();
    Some(p)
}

#[test]
fn models_and_tool_round_trip() {
    let Some(profile) = profile() else {
        eprintln!("FREYA_TDE_TEST_AI nicht gesetzt – übersprungen");
        return;
    };
    let key = std::env::var("FREYA_TDE_AI_KEY").ok();
    let client = Client::new(&profile, key).expect("client");
    let models = client.models().expect("models");
    eprintln!("{} Modelle, z. B. {:?}", models.len(), models.first());

    let dir = std::env::temp_dir().join(format!("freya-ai-live-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut ws = Session::create(&dir, "kolibri", "none", TypeRegistry::with_builtins()).unwrap();
    let skills = SkillTable::builtin();
    let mut c = Conversation::new(
        system_prompt(&ws, AiPermission::Preview, &skills),
        AiPermission::Preview,
    );
    c.max_steps = 8;
    let text = run(
        &client,
        &mut c,
        &mut ws,
        &skills,
        "Rufe das Werkzeug project_info auf und lege dann mit write_file die Datei NOTES.md mit dem Projektnamen als Inhalt an. Antworte danach kurz.",
        |proposals| {
            proposals
                .iter()
                .map(|p| Decision {
                    call_id: p.call_id.clone(),
                    approve: true,
                    reason: None,
                })
                .collect()
        },
    )
    .expect("run");
    eprintln!("Antwort: {text}\nVerbrauch: {:?}", c.usage());
    let notes = std::fs::read_to_string(dir.join("NOTES.md")).expect("NOTES.md");
    assert!(notes.contains("kolibri"), "{notes}");
    let _ = std::fs::remove_dir_all(&dir);
}
