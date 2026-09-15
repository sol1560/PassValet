//! Anthropic Messages API (direct or via ZenMux `api/anthropic`).

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::provider::*;

pub struct AnthropicMessages {
    base_url: String,
    api_key: Option<String>,
    http: reqwest::Client,
}

impl AnthropicMessages {
    pub fn new(base_url: String, api_key: Option<String>) -> Self {
        AnthropicMessages {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            http: super::http_client(),
        }
    }

    fn part(c: &ContentPart) -> Value {
        match c {
            ContentPart::Text(t) => json!({ "type": "text", "text": t }),
            ContentPart::Image { media_type, data } => json!({
                "type": "image",
                "source": { "type": "base64", "media_type": media_type, "data": data }
            }),
        }
    }

    fn convert_messages(req: &CompletionRequest) -> Vec<Value> {
        let mut out = Vec::new();
        for m in &req.messages {
            match m.role {
                Role::Assistant => {
                    let mut blocks: Vec<Value> = m.content.iter().map(Self::part).collect();
                    for tc in &m.tool_calls {
                        blocks.push(json!({ "type": "tool_use", "id": tc.id, "name": tc.name, "input": tc.arguments }));
                    }
                    if blocks.is_empty() {
                        blocks.push(json!({ "type": "text", "text": "(continuing)" }));
                    }
                    out.push(json!({ "role": "assistant", "content": blocks }));
                }
                Role::User => {
                    let mut blocks: Vec<Value> = Vec::new();
                    for r in &m.tool_results {
                        let content: Vec<Value> = if r.content.is_empty() {
                            vec![json!({ "type": "text", "text": "(no output)" })]
                        } else {
                            r.content.iter().map(Self::part).collect()
                        };
                        blocks.push(json!({
                            "type": "tool_result",
                            "tool_use_id": r.call_id,
                            "content": content,
                            "is_error": r.is_error
                        }));
                    }
                    blocks.extend(m.content.iter().map(Self::part));
                    if blocks.is_empty() {
                        continue;
                    }
                    out.push(json!({ "role": "user", "content": blocks }));
                }
            }
        }
        out
    }
}

#[async_trait]
impl LlmProvider for AnthropicMessages {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Anthropic
    }

    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        let body = json!({
            "model": req.model,
            "system": req.system,
            "messages": Self::convert_messages(req),
            "tools": req.tools.iter().map(|t| json!({
                "name": t.name, "description": t.description, "input_schema": t.parameters
            })).collect::<Vec<_>>(),
            "max_tokens": req.max_tokens,
            "temperature": req.temperature,
        });
        let mut r = self
            .http
            .post(format!("{}/v1/messages", self.base_url))
            .header("content-type", "application/json")
            .header("anthropic-version", "2023-06-01");
        if let Some(k) = &self.api_key {
            r = r.header("x-api-key", k).bearer_auth(k);
        }
        let resp = r
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Http(e.to_string()))?;
        let status = resp.status().as_u16();
        let text = resp
            .text()
            .await
            .map_err(|e| ProviderError::Http(e.to_string()))?;
        if status == 429 {
            return Err(ProviderError::RateLimited);
        }
        if status >= 400 {
            return Err(ProviderError::Status {
                status,
                body: crate::redact::redact(&text).chars().take(2000).collect(),
            });
        }
        let v: Value = serde_json::from_str(&text)
            .map_err(|e| ProviderError::Malformed(e.to_string()))?;
        let mut texts = Vec::new();
        let mut tool_calls = Vec::new();
        for b in v["content"].as_array().cloned().unwrap_or_default() {
            match b["type"].as_str() {
                Some("text") => {
                    if let Some(t) = b["text"].as_str() {
                        texts.push(t.to_string());
                    }
                }
                Some("tool_use") => tool_calls.push(ToolCall {
                    id: b["id"].as_str().unwrap_or("").to_string(),
                    name: b["name"].as_str().unwrap_or("").to_string(),
                    arguments: b["input"].clone(),
                }),
                _ => {}
            }
        }
        Ok(CompletionResponse {
            text: if texts.is_empty() { None } else { Some(texts.join("\n")) },
            tool_calls,
            usage: Usage {
                input_tokens: v["usage"]["input_tokens"].as_u64().unwrap_or(0),
                output_tokens: v["usage"]["output_tokens"].as_u64().unwrap_or(0),
            },
            stop_reason: v["stop_reason"].as_str().unwrap_or("").to_string(),
        })
    }
}
