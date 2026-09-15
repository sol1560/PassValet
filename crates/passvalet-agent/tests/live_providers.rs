//! Live smoke tests against ZenMux. Ignored by default; run with
//! `ZENMUX_API_KEY=... cargo test -p passvalet-agent --test live_providers -- --ignored`.

use passvalet_agent::provider::{CompletionRequest, Message, ProviderConfig};
use passvalet_agent::providers;
use passvalet_agent::tools;

fn key() -> Option<String> {
    std::env::var("ZENMUX_API_KEY").ok().filter(|k| !k.is_empty())
}

async fn tool_call_roundtrip(cfg: ProviderConfig) {
    let provider = providers::build(&cfg);
    let req = CompletionRequest {
        model: cfg.models[0].clone(),
        system: "You control a browser through tools. Always act with tools.".into(),
        messages: vec![Message::user_text(
            "The tab is open at https://example.com. Read the page's interactive elements now.",
        )],
        tools: tools::all_tools(),
        max_tokens: 512,
        temperature: 0.0,
    };
    let resp = provider.complete(&req).await.expect("completion");
    assert!(
        !resp.tool_calls.is_empty(),
        "{} returned no tool calls: {:?}",
        cfg.models[0],
        resp.text
    );
    let names: Vec<&str> = resp.tool_calls.iter().map(|t| t.name.as_str()).collect();
    println!("{} -> {:?} (in {} / out {})", cfg.models[0], names, resp.usage.input_tokens, resp.usage.output_tokens);
    assert!(names.iter().any(|n| *n == "read_page" || *n == "find" || *n == "screenshot" || *n == "get_page_text"));
}

#[tokio::test]
#[ignore]
async fn openai_compat_gemini_flash() {
    let Some(k) = key() else { return };
    let mut cfg = ProviderConfig::zenmux_default();
    cfg.api_key = Some(k);
    tool_call_roundtrip(cfg).await;
}

#[tokio::test]
#[ignore]
async fn anthropic_messages_haiku() {
    let Some(k) = key() else { return };
    let mut cfg = ProviderConfig::zenmux_anthropic();
    cfg.api_key = Some(k);
    tool_call_roundtrip(cfg).await;
}

#[tokio::test]
#[ignore]
async fn openai_compat_glm_flash() {
    let Some(k) = key() else { return };
    let mut cfg = ProviderConfig::zenmux_default();
    cfg.api_key = Some(k);
    cfg.models = vec!["z-ai/glm-5.3-flash".into()];
    tool_call_roundtrip(cfg).await;
}
