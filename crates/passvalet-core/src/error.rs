use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("vault is locked")]
    Locked,
    #[error("vault is not initialized")]
    NotInitialized,
    #[error("vault is already initialized")]
    AlreadyInitialized,
    #[error("wrong key: verifier did not open")]
    WrongKey,
    #[error("secret not found: {service}/{key_type}")]
    SecretNotFound { service: String, key_type: String },
    #[error("session not found or invalid token")]
    SessionNotFound,
    #[error("session expired")]
    SessionExpired,
    #[error("session revoked")]
    SessionRevoked,
    #[error("key not granted in this session: {service}/{key_type}")]
    NotGranted { service: String, key_type: String },
    #[error("invalid manifest: {0}")]
    InvalidManifest(String),
    #[error("invalid recovery key")]
    InvalidRecoveryKey,
    #[error("crypto failure: {0}")]
    Crypto(String),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type CoreResult<T> = Result<T, CoreError>;

impl CoreError {
    /// Stable machine-readable code, surfaced over IPC / MCP so agents can branch on it.
    pub fn code(&self) -> &'static str {
        match self {
            CoreError::Locked => "vault_locked",
            CoreError::NotInitialized => "vault_not_initialized",
            CoreError::AlreadyInitialized => "vault_already_initialized",
            CoreError::WrongKey => "wrong_key",
            CoreError::SecretNotFound { .. } => "secret_not_found",
            CoreError::SessionNotFound => "session_not_found",
            CoreError::SessionExpired => "session_expired",
            CoreError::SessionRevoked => "session_revoked",
            CoreError::NotGranted { .. } => "not_granted",
            CoreError::InvalidManifest(_) => "invalid_manifest",
            CoreError::InvalidRecoveryKey => "invalid_recovery_key",
            CoreError::Crypto(_) => "crypto_error",
            CoreError::Db(_) => "db_error",
            CoreError::Serde(_) => "serde_error",
            CoreError::Io(_) => "io_error",
        }
    }
}
