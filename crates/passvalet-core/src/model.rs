//! Data model shared by the vault, IPC layer, MCP server and desktop UI.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Access level an agent asks for on a key. v1 does not enforce API-level permissions;
/// the level is shown to the user and recorded in the audit log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Access {
    Read,
    ReadWrite,
}

impl Access {
    pub fn label_zh(&self) -> &'static str {
        match self {
            Access::Read => "只读",
            Access::ReadWrite => "读写",
        }
    }
}

/// Who is asking. `client` is the host tool (cursor, claude-code, windsurf, cli).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AgentInfo {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct KeyRequest {
    /// Service id, e.g. `supabase`, `stripe`, `openai`.
    pub service: String,
    /// Key type id within the service, e.g. `anon_key`, `service_role_key`, `secret_key`.
    pub key_type: String,
    #[serde(default = "default_access")]
    pub access: Access,
    /// Free-text reason shown to the user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

fn default_access() -> Access {
    Access::Read
}

/// What an agent sends before starting a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PermissionManifest {
    pub agent: AgentInfo,
    /// One sentence describing what the agent is about to do.
    pub purpose: String,
    pub requests: Vec<KeyRequest>,
    /// Requested session lifetime. Clamped by the vault to `[60, 7 days]`, default 1h.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_seconds: Option<u64>,
    /// Project path or name, for the audit log.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Grant {
    pub service: String,
    pub key_type: String,
    pub access: Access,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Session {
    pub id: String,
    pub agent: AgentInfo,
    pub purpose: String,
    pub grants: Vec<Grant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revoked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<DateTime<Utc>>,
    pub read_count: i64,
}

impl Session {
    pub fn is_active(&self, now: DateTime<Utc>) -> bool {
        !self.revoked && self.expires_at > now
    }
}

/// Where a secret came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum SecretSource {
    Manual,
    Collected,
    Rotated,
    Imported,
}

/// Everything about a secret except its value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SecretMeta {
    pub id: String,
    pub service: String,
    pub key_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub source: SecretSource,
    /// `sk_live_ab…yz` style preview, never the full value.
    pub fingerprint: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_rotated_at: Option<DateTime<Utc>>,
    /// Arbitrary metadata (project ref, account, environment…).
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(type = "Record<string, unknown>"))]
    pub metadata: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct NewSecret {
    pub service: String,
    pub key_type: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub source: SecretSource,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(type = "Record<string, unknown>"))]
    pub metadata: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum AuditEvent {
    VaultInitialized,
    VaultUnlocked,
    VaultLocked,
    SecretAdded,
    SecretUpdated,
    SecretDeleted,
    SessionRequested,
    SessionApproved,
    SessionDenied,
    SessionRevoked,
    KeyRead,
    KeyDenied,
    KeyReportedInvalid,
    RotationStarted,
    RotationCompleted,
    RotationFailed,
    CollectionStarted,
    CollectionCompleted,
    CollectionAborted,
    RecoveryUsed,
}

impl AuditEvent {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditEvent::VaultInitialized => "vault_initialized",
            AuditEvent::VaultUnlocked => "vault_unlocked",
            AuditEvent::VaultLocked => "vault_locked",
            AuditEvent::SecretAdded => "secret_added",
            AuditEvent::SecretUpdated => "secret_updated",
            AuditEvent::SecretDeleted => "secret_deleted",
            AuditEvent::SessionRequested => "session_requested",
            AuditEvent::SessionApproved => "session_approved",
            AuditEvent::SessionDenied => "session_denied",
            AuditEvent::SessionRevoked => "session_revoked",
            AuditEvent::KeyRead => "key_read",
            AuditEvent::KeyDenied => "key_denied",
            AuditEvent::KeyReportedInvalid => "key_reported_invalid",
            AuditEvent::RotationStarted => "rotation_started",
            AuditEvent::RotationCompleted => "rotation_completed",
            AuditEvent::RotationFailed => "rotation_failed",
            AuditEvent::CollectionStarted => "collection_started",
            AuditEvent::CollectionCompleted => "collection_completed",
            AuditEvent::CollectionAborted => "collection_aborted",
            AuditEvent::RecoveryUsed => "recovery_used",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "vault_initialized" => AuditEvent::VaultInitialized,
            "vault_unlocked" => AuditEvent::VaultUnlocked,
            "vault_locked" => AuditEvent::VaultLocked,
            "secret_added" => AuditEvent::SecretAdded,
            "secret_updated" => AuditEvent::SecretUpdated,
            "secret_deleted" => AuditEvent::SecretDeleted,
            "session_requested" => AuditEvent::SessionRequested,
            "session_approved" => AuditEvent::SessionApproved,
            "session_denied" => AuditEvent::SessionDenied,
            "session_revoked" => AuditEvent::SessionRevoked,
            "key_read" => AuditEvent::KeyRead,
            "key_denied" => AuditEvent::KeyDenied,
            "key_reported_invalid" => AuditEvent::KeyReportedInvalid,
            "rotation_started" => AuditEvent::RotationStarted,
            "rotation_completed" => AuditEvent::RotationCompleted,
            "rotation_failed" => AuditEvent::RotationFailed,
            "collection_started" => AuditEvent::CollectionStarted,
            "collection_completed" => AuditEvent::CollectionCompleted,
            "collection_aborted" => AuditEvent::CollectionAborted,
            "recovery_used" => AuditEvent::RecoveryUsed,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AuditEntry {
    pub id: i64,
    pub ts: DateTime<Utc>,
    pub event: AuditEvent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Which mechanism protects the KEK.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum UnlockProviderKind {
    /// Native macOS passkey with the WebAuthn PRF extension; KEK = HKDF(prf_output).
    PasskeyPrf,
    /// Random KEK stored in the login Keychain, gated by LocalAuthentication (Touch ID).
    TouchIdKeychain,
}

impl UnlockProviderKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            UnlockProviderKind::PasskeyPrf => "passkey_prf",
            UnlockProviderKind::TouchIdKeychain => "touchid_keychain",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "passkey_prf" => Some(UnlockProviderKind::PasskeyPrf),
            "touchid_keychain" => Some(UnlockProviderKind::TouchIdKeychain),
            _ => None,
        }
    }
}

/// Vault-level metadata, safe to show in the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct VaultInfo {
    pub initialized: bool,
    pub locked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<UnlockProviderKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    pub secret_count: i64,
    pub active_session_count: i64,
    pub has_recovery: bool,
    /// Passkey credential id (base64url) when the provider is `passkey_prf`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_id: Option<String>,
}

/// Human-readable summary of a manifest, produced by [`crate::manifest::summarize`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ManifestSummary {
    pub agent_label: String,
    pub purpose: String,
    pub ttl_seconds: u64,
    pub ttl_label: String,
    pub lines: Vec<ManifestLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ManifestLine {
    pub service: String,
    pub service_label: String,
    pub key_type: String,
    pub key_label: String,
    pub access: Access,
    /// Whether the vault currently has this key.
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Build the masked preview stored alongside a secret.
pub fn fingerprint(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    let n = chars.len();
    if n <= 8 {
        return "•".repeat(n.max(4));
    }
    let head: String = chars[..4].iter().collect();
    let tail: String = chars[n - 4..].iter().collect();
    format!("{head}…{tail}")
}
