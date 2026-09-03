//! Pending approval prompts (authorization and rotation) shown in the `prompt` window.

use std::collections::HashMap;
use std::sync::Mutex;

use passvalet_core::model::{ManifestSummary, PermissionManifest};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::oneshot;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PromptKind {
    /// Agent asks for a session.
    Authorize,
    /// Agent reports an invalid key; user decides whether to rotate.
    Rotate {
        service: String,
        key_type: String,
        status_code: Option<u16>,
        message: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct PendingPrompt {
    pub id: String,
    pub kind: PromptKind,
    pub manifest: PermissionManifest,
    pub summary: ManifestSummary,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub vault_locked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Approved,
    Denied,
    TimedOut,
}

struct Entry {
    prompt: PendingPrompt,
    responder: Option<oneshot::Sender<Decision>>,
}

#[derive(Default)]
pub struct PromptQueue {
    entries: Mutex<HashMap<String, Entry>>,
}

impl PromptQueue {
    pub fn list(&self) -> Vec<PendingPrompt> {
        let mut v: Vec<PendingPrompt> = self
            .entries
            .lock()
            .unwrap()
            .values()
            .map(|e| e.prompt.clone())
            .collect();
        v.sort_by_key(|p| p.created_at);
        v
    }

    pub fn get(&self, id: &str) -> Option<PendingPrompt> {
        self.entries.lock().unwrap().get(id).map(|e| e.prompt.clone())
    }

    /// Enqueue and return a receiver that resolves with the user's decision.
    pub fn push(&self, prompt: PendingPrompt) -> oneshot::Receiver<Decision> {
        let (tx, rx) = oneshot::channel();
        self.entries.lock().unwrap().insert(
            prompt.id.clone(),
            Entry {
                prompt,
                responder: Some(tx),
            },
        );
        rx
    }

    /// Resolve a prompt (from the UI). Returns the prompt if it existed.
    pub fn resolve(&self, id: &str, decision: Decision) -> Option<PendingPrompt> {
        let mut map = self.entries.lock().unwrap();
        let entry = map.remove(id)?;
        if let Some(tx) = entry.responder {
            let _ = tx.send(decision);
        }
        Some(entry.prompt)
    }

    pub fn remove(&self, id: &str) {
        self.entries.lock().unwrap().remove(id);
    }

    pub fn is_empty(&self) -> bool {
        self.entries.lock().unwrap().is_empty()
    }
}

/// Show (or focus) the prompt window and notify it.
pub fn show_prompt_window<R: Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(w) = app.get_webview_window("prompt") {
        let _ = w.show();
        let _ = w.set_focus();
        let _ = w.emit("prompt:changed", ());
        return;
    }
    let url = if cfg!(debug_assertions) {
        WebviewUrl::App("index.html#/prompt".into())
    } else {
        WebviewUrl::App("index.html#/prompt".into())
    };
    match WebviewWindowBuilder::new(app, "prompt", url)
        .title("PassValet 授权")
        .inner_size(460.0, 560.0)
        .min_inner_size(400.0, 420.0)
        .resizable(true)
        .always_on_top(true)
        .decorations(true)
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true)
        .center()
        .focused(true)
        .build()
    {
        Ok(w) => {
            let _ = w.set_focus();
        }
        Err(e) => tracing::error!("prompt window: {e}"),
    }
    let _ = app.emit("prompt:changed", ());
}

pub fn close_prompt_window_if_idle<R: Runtime>(app: &tauri::AppHandle<R>, queue: &PromptQueue) {
    if queue.is_empty() {
        if let Some(w) = app.get_webview_window("prompt") {
            let _ = w.hide();
        }
    }
    let _ = app.emit("prompt:changed", ());
}
