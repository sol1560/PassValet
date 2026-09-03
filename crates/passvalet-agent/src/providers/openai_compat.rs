//! OpenAI Chat Completions compatible provider (ZenMux `api/v1`, Ollama, OpenRouter, OpenAI).

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::provider::*;

pub struct OpenAiCompat {
    base_url: String,
    api_key: Option<String>,
    http: reqwest::Client,
}

impl OpenAiCompat {
    pub fn new(base_url: String, api_key: Option<String>) -> Self {
        OpenAiCompat {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            http: super::http_client(),
        }
    }

    fn convert_messages(req: &CompletionRequest) -> Vec<Value> {
        let mut out = vec![json!({ "role": "system", "content": req.system })];
        for m in &req.messages {
            match m.role {
                Role::Assistant => {
                    let text = m
                        .content
                        .iter()
                        .filter_map(|c| match c {
                            ContentPart::Text(t) => Some(t.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    let mut msg = json!({ "role": "assistant", "content": if text.is_empty() { Value::Null } else { Value::String(text) } });
                    if !m.tool_calls.is_empty() {
                        msg["tool_calls"] = Value::Array(
                            m.tool_calls
                                .iter()
                                .map(|tc| {
                                    json!({
                                        "id": tc.id,
                                        "type": "function",
                                        "function": { "name": tc.name, "arguments": tc.arguments.to_string() }
                                    })
                                })
                                .collect(),
                        );
                    }
                    out.push(msg);
                }
                Role::User => {
                    // tool results first, each as a `tool` message; images deferred to a user message
                    let mut deferred_images: Vec<Value> = Vec::new();
                    for r in &m.tool_results {
                        let mut text = String::new();
                        for c in &r.content {
                            match c {
                                ContentPart::Text(t) => {
                                    if !text.is_empty() {
                                        text.push('\n');
                                    }
                                    text.push_str(t);
                                }
                                ContentPart::Image { media_type, data } => {
                                    deferred_images.push(json!({
                                        "type": "image_url",
                                        "image_url": { "url": format!("data:{media_type};base64,{data}") }
                                    }));
                                    text.push_str("\n[screenshot attached in the next message]");
                                }
                            }
                        }
                        if r.is_error && !text.starts_with("ERROR") {
                            text = format!("ERROR: {text}");
                        }
                        out.push(json!({ "role": "tool", "tool_call_id": r.call_id, "content": text }));
                    }
                    let mut parts: Vec<Value> = Vec::new();
                    for c in &m.content {
                        match c {
                            ContentPart::Text(t) => parts.push(json!({ "type": "text", "text": t })),
                            ContentPart::Image { media_type, data } => parts.push(json!({
                                "type": "image_url",
                                "image_url": { "url": format!("data:{media_type};base64,{data}") }
                            })),
                        }
                    }
                    if !deferred_images.is_empty() {
                        parts.push(json!({ "type": "text", "text": "Screenshot(s) from the tool call(s) above:" }));
                        parts.extend(deferred_images);
                    }
                    if !parts.is_empty() {
                        out.push(json!({ "role": "user", "content": parts }));
                    }
                }
            }
        }
        out
    }
}

#[async_trait]
impl LlmProvider for OpenAiCompat {
    fn kind(&self) -> ProviderKind {
        ProviderKind::OpenaiCompat
    }

    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        let body = json!({
            "model": req.model,
            "messages": Self::convert_messages(req),
            "tools": req.tools.iter().map(|t| json!({
                "type": "function",
                "function": { "name": t.name, "description": t.description, "parameters": t.parameters }
            })).collect::<Vec<_>>(),
            "tool_choice": "auto",
            "max_tokens": req.max_tokens,
            "temperature": req.temperature,
        });
        let mut r = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .header("content-type", "application/json");
        if let Some(k) = &self.api_key {
            r = r.bearer_auth(k);
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
                body: text.chars().take(2000).collect(),
            });
        }
        let v: Value = serde_json::from_str(&text)
            .map_err(|e| ProviderError::Malformed(format!("{e}: {}", &text[..text.len().min(500)])))?;
        let choice = v["choices"]
            .get(0)
            .ok_or_else(|| ProviderError::Malformed("no choices".into()))?;
        let msg = &choice["message"];
        let content = msg["content"].as_str().map(|s| s.to_string()).filter(|s| !s.trim().is_empty());
        let mut tool_calls = Vec::new();
        if let Some(arr) = msg["tool_calls"].as_array() {
            for (i, tc) in arr.iter().enumerate() {
                let name = tc["function"]["name"].as_str().unwrap_or("").to_string();
                let args_raw = &tc["function"]["arguments"];
                let arguments = match args_raw {
                    Value::String(s) => serde_json::from_str(s).unwrap_or(json!({})),
                    Value::Object(_) => args_raw.clone(),
                    _ => json!({}),
                };
                tool_calls.push(ToolCall {
                    id: tc["id"]
                        .as_str()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| format!("call_{i}")),
                    name,
                    arguments,
                });
            }
        }
        Ok(CompletionResponse {
            text: content,
            tool_calls,
            usage: Usage {
                input_tokens: v["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
                output_tokens: v["usage"]["completion_tokens"].as_u64().unwrap_or(0),
            },
            stop_reason: choice["finish_reason"].as_str().unwrap_or("").to_string(),
        })
    }
}
