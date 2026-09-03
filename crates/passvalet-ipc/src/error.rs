use passvalet_core::CoreError;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const CODE_PARSE: i64 = -32700;
pub const CODE_INVALID_REQUEST: i64 = -32600;
pub const CODE_METHOD_NOT_FOUND: i64 = -32601;
pub const CODE_INVALID_PARAMS: i64 = -32602;
pub const CODE_INTERNAL: i64 = -32603;
/// Application errors carry a stable string code in `data.code`.
pub const CODE_APP: i64 = -32000;
pub const CODE_USER_DENIED: i64 = -32001;
pub const CODE_TIMEOUT: i64 = -32002;
pub const CODE_UNAVAILABLE: i64 = -32003;

/// Wire-level JSON-RPC error object.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
    pub data: Option<serde_json::Value>,
}

impl RpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        RpcError {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn app(code_str: &str, message: impl Into<String>) -> Self {
        RpcError {
            code: CODE_APP,
            message: message.into(),
            data: Some(serde_json::json!({ "code": code_str })),
        }
    }

    pub fn method_not_found(method: &str) -> Self {
        Self::new(CODE_METHOD_NOT_FOUND, format!("method not found: {method}"))
    }

    pub fn invalid_params(msg: impl Into<String>) -> Self {
        Self::new(CODE_INVALID_PARAMS, msg)
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::new(CODE_INTERNAL, msg)
    }

    pub fn user_denied(msg: impl Into<String>) -> Self {
        RpcError {
            code: CODE_USER_DENIED,
            message: msg.into(),
            data: Some(serde_json::json!({ "code": "user_denied" })),
        }
    }

    pub fn timeout(msg: impl Into<String>) -> Self {
        RpcError {
            code: CODE_TIMEOUT,
            message: msg.into(),
            data: Some(serde_json::json!({ "code": "timeout" })),
        }
    }

    pub fn unavailable(msg: impl Into<String>) -> Self {
        RpcError {
            code: CODE_UNAVAILABLE,
            message: msg.into(),
            data: Some(serde_json::json!({ "code": "unavailable" })),
        }
    }

    /// The stable string code if present (`data.code`).
    pub fn app_code(&self) -> Option<&str> {
        self.data
            .as_ref()
            .and_then(|d| d.get("code"))
            .and_then(|c| c.as_str())
    }
}

impl From<CoreError> for RpcError {
    fn from(e: CoreError) -> Self {
        RpcError::app(e.code(), e.to_string())
    }
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.app_code() {
            Some(c) => write!(f, "{} [{}]", self.message, c),
            None => write!(f, "{} ({})", self.message, self.code),
        }
    }
}

impl std::error::Error for RpcError {}

#[derive(Debug, Error)]
pub enum IpcError {
    #[error("PassValet app is not running (socket {0} not found)")]
    NotRunning(String),
    #[error("connection closed")]
    Closed,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("rpc: {0}")]
    Rpc(RpcError),
    #[error("timeout waiting for {0}")]
    Timeout(String),
}

impl From<RpcError> for IpcError {
    fn from(e: RpcError) -> Self {
        IpcError::Rpc(e)
    }
}
