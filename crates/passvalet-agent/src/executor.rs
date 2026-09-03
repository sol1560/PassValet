//! What the agent needs from the browser side.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Result of one browser tool call, as produced by the extension.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ToolOutput {
    /// Text for the model (will be redacted again on the agent side).
    #[serde(default)]
    pub text: String,
    /// PNG screenshot, base64.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_png_base64: Option<String>,
    /// Set by `capture_secret` only: the raw value, never shown to the model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
    /// Current tab state after the action (url/title), for the model's orientation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser_state: Option<BrowserState>,
    #[serde(default)]
    pub is_error: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct BrowserState {
    pub tab_id: String,
    pub url: String,
    pub title: String,
}

impl ToolOutput {
    pub fn text(t: impl Into<String>) -> Self {
        ToolOutput {
            text: t.into(),
            ..Default::default()
        }
    }
    pub fn error(t: impl Into<String>) -> Self {
        ToolOutput {
            text: t.into(),
            is_error: true,
            ..Default::default()
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExecutorError {
    #[error("browser extension not connected")]
    NotConnected,
    #[error("aborted by user")]
    Aborted,
    #[error("browser call failed: {0}")]
    Failed(String),
}

/// Implemented by the desktop app on top of the extension IPC channel.
#[async_trait]
pub trait BrowserExecutor: Send + Sync {
    /// Open a dedicated tab (in the PassValet tab group) at `start_url`. Returns the tab id.
    async fn session_begin(&self, run_id: &str, start_url: &str) -> Result<String, ExecutorError>;
    /// Run one tool. `params` follows the tool's JSON schema from [`crate::tools`].
    async fn call(&self, run_id: &str, tool: &str, params: Value) -> Result<ToolOutput, ExecutorError>;
    /// Close the run's tabs and wipe all in-memory artifacts on the extension side.
    async fn session_end(&self, run_id: &str, keep_tabs: bool) -> Result<(), ExecutorError>;
}
