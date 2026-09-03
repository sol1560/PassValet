//! Shared application state.

use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

use passvalet_core::Vault;

use crate::extension::ExtensionBridge;
use crate::prompt::PromptQueue;
use crate::runs::RunRegistry;
use crate::settings::Settings;

pub struct AppState {
    pub vault: Arc<Mutex<Vault>>,
    pub settings: RwLock<Settings>,
    pub extension: Arc<ExtensionBridge>,
    pub prompts: Arc<PromptQueue>,
    pub runs: Arc<RunRegistry>,
    pub last_activity: Mutex<Instant>,
}

impl AppState {
    pub fn touch(&self) {
        *self.last_activity.lock().unwrap() = Instant::now();
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().unwrap().clone()
    }

    pub fn with_vault<T>(&self, f: impl FnOnce(&mut Vault) -> T) -> T {
        let mut v = self.vault.lock().unwrap();
        f(&mut v)
    }
}

/// Service id / key type under which the LLM provider key is stored in the vault.
pub const LLM_KEY_SERVICE: &str = "passvalet";
pub const LLM_KEY_TYPE: &str = "llm_api_key";
