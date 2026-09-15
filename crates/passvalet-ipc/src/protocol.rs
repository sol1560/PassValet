//! Typed params/results for the vault methods.

use passvalet_core::services::ServiceInfo;
use passvalet_core::{PermissionManifest, SecretMeta, Session, VaultInfo};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct StatusResult {
    pub version: String,
    pub vault: VaultInfo,
    pub extension_connected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RequestPermissionsParams {
    pub manifest: PermissionManifest,
    /// How long the caller is willing to wait for the user (seconds). Default 120, max 600.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RequestPermissionsResult {
    pub session_token: String,
    pub session: Session,
    /// Keys granted but not present in the vault; `get_key` on them will fail until collected.
    #[serde(default)]
    pub missing: Vec<MissingKey>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct MissingKey {
    pub service: String,
    pub key_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct GetKeyParams {
    pub session_token: String,
    pub service: String,
    pub key_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct GetKeyResult {
    pub service: String,
    pub key_type: String,
    pub value: String,
    /// Suggested environment variable name.
    pub env_var: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ReportKeyInvalidParams {
    pub session_token: String,
    pub service: String,
    pub key_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_code: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Seconds to wait for the rotation to finish. Default 300.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_seconds: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum RotationOutcome {
    /// New key issued and returned in `value`.
    Rotated,
    /// User declined the rotation prompt.
    Denied,
    /// Automation could not complete; user must rotate manually in the dashboard.
    Failed,
    /// No playbook for this service; user must rotate manually.
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ReportKeyInvalidResult {
    pub outcome: RotationOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env_var: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ServiceAvailability {
    #[serde(flatten)]
    pub service: ServiceInfo,
    /// Key types of this service currently stored in the vault.
    pub stored_key_types: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ListServicesResult {
    pub services: Vec<ServiceAvailability>,
    /// Stored keys whose service is not in the registry.
    pub custom: Vec<SecretMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ListKeysResult {
    pub keys: Vec<SecretMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct StartCollectionParams {
    pub service: String,
    #[serde(default)]
    pub key_types: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct StartCollectionResult {
    pub run_id: String,
}

/// Sent by the native host once connected.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ExtHelloParams {
    pub extension_version: String,
    pub browser: String,
}

/// Extension → app events.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExtEvent {
    UserAborted {
        run_id: String,
    },
    TabClosed {
        run_id: String,
        tab_id: String,
    },
    DebuggerDetached {
        run_id: String,
        tab_id: String,
        reason: String,
    },
    Log {
        run_id: Option<String>,
        level: String,
        message: String,
    },
}
