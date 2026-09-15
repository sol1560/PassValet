//! macOS passkey (WebAuthn PRF) + Touch ID bridge.
//!
//! Two ways to use it:
//! - Rust API ([`login`], [`register`], [`touch_id`]) — used by the PassValet backend so PRF
//!   output never crosses into the webview.
//! - Tauri commands (`plugin:macos-passkey|…`) — thin wrappers for the frontend.

#[cfg(target_os = "macos")]
mod macos;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct PasskeyRegistrationResult {
    pub id: String,
    pub raw_id: String,
    pub client_data_json: String,
    pub attestation_object: String,
    pub prf_output: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PasskeyLoginResult {
    pub id: String,
    pub raw_id: String,
    pub client_data_json: String,
    pub authenticator_data: String,
    pub signature: String,
    pub user_handle: String,
    pub prf_output: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasskeyError {
    Unsupported,
    NoWindow,
    Failed(String),
    Cancelled,
}

impl std::fmt::Display for PasskeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PasskeyError::Unsupported => f.write_str("passkeys are only available on macOS 15+"),
            PasskeyError::NoWindow => f.write_str("no window available to anchor the system sheet"),
            PasskeyError::Failed(s) => f.write_str(s),
            PasskeyError::Cancelled => f.write_str("user cancelled"),
        }
    }
}
impl std::error::Error for PasskeyError {}

/// Register a new passkey for `domain` (the relying party id). `salt` enables PRF.
pub async fn register<R: tauri::Runtime>(
    window: &tauri::Window<R>,
    domain: &str,
    challenge: &[u8],
    username: &str,
    user_id: &[u8],
    salt: &[u8],
) -> Result<PasskeyRegistrationResult, PasskeyError> {
    #[cfg(target_os = "macos")]
    {
        macos::register(window, domain, challenge, username, user_id, salt).await
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, domain, challenge, username, user_id, salt);
        Err(PasskeyError::Unsupported)
    }
}

/// Assert with an existing passkey; returns the PRF output when `salt` is non-empty.
pub async fn login<R: tauri::Runtime>(
    window: &tauri::Window<R>,
    domain: &str,
    challenge: &[u8],
    salt: &[u8],
) -> Result<PasskeyLoginResult, PasskeyError> {
    #[cfg(target_os = "macos")]
    {
        macos::login(window, domain, challenge, salt).await
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, domain, challenge, salt);
        Err(PasskeyError::Unsupported)
    }
}

/// Touch ID prompt (falls back to the account password when `allow_password` is true).
pub async fn touch_id(reason: &str, allow_password: bool) -> Result<bool, PasskeyError> {
    #[cfg(target_os = "macos")]
    {
        macos::touch_id(reason, allow_password).await
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (reason, allow_password);
        Err(PasskeyError::Unsupported)
    }
}

pub fn is_biometrics_available() -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::biometrics_available()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

#[tauri::command]
async fn register_passkey<R: tauri::Runtime>(
    domain: String,
    challenge: Vec<u8>,
    username: String,
    user_id: Vec<u8>,
    salt: Vec<u8>,
    window: tauri::Window<R>,
) -> Result<PasskeyRegistrationResult, String> {
    register(&window, &domain, &challenge, &username, &user_id, &salt)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn login_passkey<R: tauri::Runtime>(
    domain: String,
    challenge: Vec<u8>,
    salt: Vec<u8>,
    window: tauri::Window<R>,
) -> Result<PasskeyLoginResult, String> {
    login(&window, &domain, &challenge, &salt)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn touch_id_authenticate(
    reason: String,
    allow_password: Option<bool>,
) -> Result<bool, String> {
    touch_id(&reason, allow_password.unwrap_or(true))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn biometrics_available() -> bool {
    is_biometrics_available()
}

pub fn init<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("macos-passkey")
        .invoke_handler(tauri::generate_handler![
            register_passkey,
            login_passkey,
            touch_id_authenticate,
            biometrics_available
        ])
        .build()
}
