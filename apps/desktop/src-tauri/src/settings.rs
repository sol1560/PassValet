//! settings.json — non-secret configuration.

use passvalet_agent::provider::ProviderConfig;
use passvalet_core::paths;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub provider: ProviderConfig,
    /// Lock the vault after this many minutes without a key read (0 = never).
    pub auto_lock_minutes: u32,
    /// Relying-party id for native passkeys (must be an associated domain of the app).
    pub passkey_domain: String,
    pub passkey_username: String,
    /// Chrome extension ids allowed to connect through the native host.
    #[serde(default)]
    pub extension_ids: Vec<String>,
    /// Require Touch ID / passkey on every approval even when the vault is already unlocked.
    #[serde(default = "default_true")]
    pub presence_on_approve: bool,
    #[serde(default)]
    pub onboarding_done: bool,
}

fn default_true() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            provider: ProviderConfig::zenmux_default(),
            auto_lock_minutes: 30,
            passkey_domain: "passvalet.app".into(),
            passkey_username: whoami(),
            extension_ids: vec![],
            presence_on_approve: true,
            onboarding_done: false,
        }
    }
}

fn whoami() -> String {
    std::env::var("USER").unwrap_or_else(|_| "passvalet".into())
}

impl Settings {
    pub fn load() -> Self {
        let p = paths::settings_path();
        match std::fs::read_to_string(&p) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                tracing::warn!("settings.json invalid ({e}); using defaults");
                Settings::default()
            }),
            Err(_) => Settings::default(),
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        paths::ensure_app_dir()?;
        let p = paths::settings_path();
        let mut s = self.clone();
        // never persist the api key in settings.json; it lives in the vault
        s.provider.api_key = None;
        std::fs::write(&p, serde_json::to_string_pretty(&s)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }
}
