//! Provider-neutral chat types and the [`LlmProvider`] trait.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tools::ToolDef;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContentPart {
    Text(String),
    /// PNG/JPEG, base64.
    Image { media_type: String, data: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub call_id: String,
    pub name: String,
    pub content: Vec<ContentPart>,
    pub is_error: bool,
}

/// One turn. Assistant turns carry text + tool calls; user turns carry text/images and/or
/// tool results (results always come first in the turn).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    #[serde(default)]
    pub content: Vec<ContentPart>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default)]
    pub tool_results: Vec<ToolResult>,
}

impl Message {
    pub fn user_text(t: impl Into<String>) -> Self {
        Message {
            role: Role::User,
            content: vec![ContentPart::Text(t.into())],
            tool_calls: vec![],
            tool_results: vec![],
        }
    }
    pub fn assistant(text: Option<String>, tool_calls: Vec<ToolCall>) -> Self {
        Message {
            role: Role::Assistant,
            content: text.map(|t| vec![ContentPart::Text(t)]).unwrap_or_default(),
            tool_calls,
            tool_results: vec![],
        }
    }
    pub fn tool_results(results: Vec<ToolResult>) -> Self {
        Message {
            role: Role::User,
            content: vec![],
            tool_calls: vec![],
            tool_results: results,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub model: String,
    pub system: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDef>,
    pub max_tokens: u32,
    pub temperature: f32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone, Default)]
pub struct CompletionResponse {
    pub text: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Usage,
    pub stop_reason: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("http: {0}")]
    Http(String),
    #[error("provider returned {status}: {body}")]
    Status { status: u16, body: String },
    #[error("bad response: {0}")]
    Malformed(String),
    #[error("rate limited")]
    RateLimited,
    #[error("model {0} does not support tool calling or images")]
    Unsupported(String),
}

impl ProviderError {
    /// Errors worth retrying on another model.
    pub fn is_retryable(&self) -> bool {
        match self {
            ProviderError::RateLimited => true,
            ProviderError::Status { status, .. } => *status >= 500 || *status == 429,
            ProviderError::Http(_) => true,
            _ => false,
        }
    }
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, ProviderError>;
    fn kind(&self) -> ProviderKind;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// OpenAI Chat Completions compatible (ZenMux `api/v1`, OpenAI, Ollama, OpenRouter…).
    OpenaiCompat,
    /// Anthropic Messages API (ZenMux `api/anthropic` or api.anthropic.com).
    Anthropic,
}

/// Persisted in settings.json. API keys are stored in the vault under service `passvalet`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ProviderConfig {
    pub kind: ProviderKind,
    pub base_url: String,
    /// Header value; for OpenAI-compat sent as `Authorization: Bearer`, for Anthropic as `x-api-key`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Ordered list: first is the default, later ones are escalation targets.
    pub models: Vec<String>,
}

impl ProviderConfig {
    pub fn zenmux_default() -> Self {
        ProviderConfig {
            kind: ProviderKind::OpenaiCompat,
            base_url: "https://zenmux.ai/api/v1".into(),
            api_key: None,
            models: vec![
                "google/gemini-3.7-flash".into(),
                "anthropic/claude-sonnet-5".into(),
            ],
        }
    }

    pub fn zenmux_anthropic() -> Self {
        ProviderConfig {
            kind: ProviderKind::Anthropic,
            base_url: "https://zenmux.ai/api/anthropic".into(),
            api_key: None,
            models: vec![
                "anthropic/claude-haiku-4.5".into(),
                "anthropic/claude-sonnet-5".into(),
            ],
        }
    }

    pub fn ollama_local() -> Self {
        ProviderConfig {
            kind: ProviderKind::OpenaiCompat,
            base_url: "http://127.0.0.1:11434/v1".into(),
            api_key: None,
            models: vec!["qwen3.5-vl:9b".into()],
        }
    }
}
