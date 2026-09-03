//! Unlock providers: native passkey (PRF) and Touch ID + Keychain.

use passvalet_core::crypto::{self, Kek, SymKey};
use passvalet_core::model::UnlockProviderKind;
use passvalet_core::vault::InitParams;
use passvalet_core::{CoreError, Vault};
use tauri::{Manager, Runtime};

use crate::settings::Settings;

const KEYCHAIN_SERVICE: &str = "ai.passvalet.vault";

/// Keychain account: `kek`, or `kek@<PASSVALET_HOME>` so test vaults do not share a key.
fn keychain_account() -> String {
    match std::env::var("PASSVALET_HOME") {
        Ok(h) if !h.is_empty() => format!("kek@{h}"),
        _ => "kek".to_string(),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum UnlockError {
    #[error("{0}")]
    Passkey(String),
    #[error("Touch ID / password verification failed: {0}")]
    Presence(String),
    #[error("keychain: {0}")]
    Keychain(String),
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error("cancelled")]
    Cancelled,
}

impl From<tauri_plugin_macos_passkey::PasskeyError> for UnlockError {
    fn from(e: tauri_plugin_macos_passkey::PasskeyError) -> Self {
        match e {
            tauri_plugin_macos_passkey::PasskeyError::Cancelled => UnlockError::Cancelled,
            other => UnlockError::Passkey(other.to_string()),
        }
    }
}

fn any_window<R: Runtime>(app: &tauri::AppHandle<R>) -> Option<tauri::Window<R>> {
    app.get_webview_window("prompt")
        .filter(|w: &tauri::WebviewWindow<R>| w.is_visible().unwrap_or(false))
        .or_else(|| app.get_webview_window("main"))
        .or_else(|| app.webview_windows().values().next().cloned())
        .map(|w| w.as_ref().window())
}

// ------------------------------------------------------------------ Touch ID + Keychain

/// Debug + PASSVALET_DEV_SKIP_PRESENCE: keep the KEK in a file under PASSVALET_HOME instead of the
/// Keychain, so unsigned dev builds do not trigger Keychain ACL dialogs in automated tests.
fn dev_kek_file() -> Option<std::path::PathBuf> {
    if dev_skip_presence() {
        Some(passvalet_core::paths::app_dir().join("dev-kek.bin"))
    } else {
        None
    }
}

fn keychain_read() -> Result<Option<Kek>, UnlockError> {
    if let Some(f) = dev_kek_file() {
        return match std::fs::read(&f) {
            Ok(b) => Ok(Some(SymKey::from_slice(&b)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(UnlockError::Keychain(e.to_string())),
        };
    }
    match security_framework::passwords::get_generic_password(KEYCHAIN_SERVICE, &keychain_account()) {
        Ok(bytes) => Ok(Some(SymKey::from_slice(&bytes)?)),
        Err(e) if e.code() == -25300 => Ok(None), // errSecItemNotFound
        Err(e) => Err(UnlockError::Keychain(e.to_string())),
    }
}

fn keychain_write(kek: &Kek) -> Result<(), UnlockError> {
    if let Some(f) = dev_kek_file() {
        return std::fs::write(&f, kek.as_bytes()).map_err(|e| UnlockError::Keychain(e.to_string()));
    }
    security_framework::passwords::set_generic_password(
        KEYCHAIN_SERVICE,
        &keychain_account(),
        kek.as_bytes(),
    )
    .map_err(|e| UnlockError::Keychain(e.to_string()))
}

#[allow(dead_code)]
pub fn keychain_delete() {
    let _ = security_framework::passwords::delete_generic_password(KEYCHAIN_SERVICE, &keychain_account());
}

/// Debug builds only: `PASSVALET_DEV_SKIP_PRESENCE=1` bypasses Touch ID for automated tests.
pub fn dev_skip_presence() -> bool {
    cfg!(debug_assertions) && std::env::var("PASSVALET_DEV_SKIP_PRESENCE").map(|v| v == "1").unwrap_or(false)
}

/// Ask for user presence (Touch ID, password fallback).
pub async fn verify_presence(reason: &str) -> Result<(), UnlockError> {
    if dev_skip_presence() {
        tracing::warn!("DEV: skipping presence check ({reason})");
        return Ok(());
    }
    match tauri_plugin_macos_passkey::touch_id(reason, true).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(UnlockError::Presence("denied".into())),
        Err(tauri_plugin_macos_passkey::PasskeyError::Cancelled) => Err(UnlockError::Cancelled),
        Err(e) => Err(UnlockError::Presence(e.to_string())),
    }
}

// ------------------------------------------------------------------ setup

/// Create a new vault with the chosen provider. Returns the recovery key text.
pub async fn setup<R: Runtime>(
    app: &tauri::AppHandle<R>,
    vault: &std::sync::Arc<std::sync::Mutex<Vault>>,
    settings: &Settings,
    provider: UnlockProviderKind,
) -> Result<String, UnlockError> {
    if vault.lock().unwrap().is_initialized()? {
        return Err(CoreError::AlreadyInitialized.into());
    }
    let salt = Vault::new_salt();
    match provider {
        UnlockProviderKind::TouchIdKeychain => {
            verify_presence("设置 PassValet 保险库").await?;
            let kek = SymKey::random();
            keychain_write(&kek)?;
            let text = vault.lock().unwrap().initialize(InitParams {
                provider,
                kek,
                kek_salt: salt,
                credential_id: None,
                user_handle: None,
                with_recovery: true,
            })?;
            Ok(text.unwrap_or_default())
        }
        UnlockProviderKind::PasskeyPrf => {
            let window = any_window(app).ok_or_else(|| UnlockError::Passkey("no window".into()))?;
            let challenge = crypto::random_bytes(32);
            let user_id = crypto::random_bytes(16);
            let reg = tauri_plugin_macos_passkey::register(
                &window,
                &settings.passkey_domain,
                &challenge,
                &settings.passkey_username,
                &user_id,
                &salt,
            )
            .await?;
            if reg.prf_output.is_empty() {
                return Err(UnlockError::Passkey(
                    "该 passkey 提供方不支持 PRF 扩展（需要 macOS 15+ 的 iCloud 钥匙串）".into(),
                ));
            }
            let kek = crypto::derive_kek(&reg.prf_output, &salt)?;
            let text = vault.lock().unwrap().initialize(InitParams {
                provider,
                kek,
                kek_salt: salt,
                credential_id: Some(reg.id),
                user_handle: Some(user_id),
                with_recovery: true,
            })?;
            Ok(text.unwrap_or_default())
        }
    }
}

// ------------------------------------------------------------------ unlock

pub async fn unlock<R: Runtime>(
    app: &tauri::AppHandle<R>,
    vault: &std::sync::Arc<std::sync::Mutex<Vault>>,
    settings: &Settings,
    reason: &str,
) -> Result<(), UnlockError> {
    let (provider, salt) = {
        let v = vault.lock().unwrap();
        let p = v.provider()?.ok_or(CoreError::NotInitialized)?;
        (p, v.kek_salt()?)
    };
    match provider {
        UnlockProviderKind::TouchIdKeychain => {
            verify_presence(reason).await?;
            let kek = keychain_read()?.ok_or_else(|| {
                UnlockError::Keychain("保险库主密钥不在钥匙串中，请使用恢复密钥".into())
            })?;
            vault.lock().unwrap().unlock(kek)?;
            Ok(())
        }
        UnlockProviderKind::PasskeyPrf => {
            let window = any_window(app).ok_or_else(|| UnlockError::Passkey("no window".into()))?;
            let challenge = crypto::random_bytes(32);
            let res = tauri_plugin_macos_passkey::login(
                &window,
                &settings.passkey_domain,
                &challenge,
                &salt,
            )
            .await?;
            if res.prf_output.is_empty() {
                return Err(UnlockError::Passkey("passkey 未返回 PRF 输出".into()));
            }
            let kek = crypto::derive_kek(&res.prf_output, &salt)?;
            vault.lock().unwrap().unlock(kek)?;
            Ok(())
        }
    }
}

/// After a recovery-key unlock: bind a fresh provider (re-register passkey / new Keychain KEK).
pub async fn rebind<R: Runtime>(
    app: &tauri::AppHandle<R>,
    vault: &std::sync::Arc<std::sync::Mutex<Vault>>,
    settings: &Settings,
    provider: UnlockProviderKind,
) -> Result<String, UnlockError> {
    let salt = Vault::new_salt();
    match provider {
        UnlockProviderKind::TouchIdKeychain => {
            verify_presence("重新绑定 Touch ID").await?;
            let kek = SymKey::random();
            keychain_write(&kek)?;
            let text = vault.lock().unwrap().rekey(kek, salt, provider, None, None, true)?;
            Ok(text.unwrap_or_default())
        }
        UnlockProviderKind::PasskeyPrf => {
            let window = any_window(app).ok_or_else(|| UnlockError::Passkey("no window".into()))?;
            let challenge = crypto::random_bytes(32);
            let user_id = crypto::random_bytes(16);
            let reg = tauri_plugin_macos_passkey::register(
                &window,
                &settings.passkey_domain,
                &challenge,
                &settings.passkey_username,
                &user_id,
                &salt,
            )
            .await?;
            if reg.prf_output.is_empty() {
                return Err(UnlockError::Passkey("passkey 未返回 PRF 输出".into()));
            }
            let kek = crypto::derive_kek(&reg.prf_output, &salt)?;
            let text = vault
                .lock()
                .unwrap()
                .rekey(kek, salt, provider, Some(reg.id), Some(user_id), true)?;
            Ok(text.unwrap_or_default())
        }
    }
}
