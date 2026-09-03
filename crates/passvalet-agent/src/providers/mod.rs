pub mod anthropic;
pub mod openai_compat;

use std::sync::Arc;

use crate::provider::{LlmProvider, ProviderConfig, ProviderKind};

pub fn build(cfg: &ProviderConfig) -> Arc<dyn LlmProvider> {
    match cfg.kind {
        ProviderKind::OpenaiCompat => Arc::new(openai_compat::OpenAiCompat::new(
            cfg.base_url.clone(),
            cfg.api_key.clone(),
        )),
        ProviderKind::Anthropic => Arc::new(anthropic::AnthropicMessages::new(
            cfg.base_url.clone(),
            cfg.api_key.clone(),
        )),
    }
}

pub(crate) fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .expect("reqwest client")
}
