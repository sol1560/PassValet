//! Tauri commands for the React UI.

use std::sync::Arc;

use passvalet_agent::provider::ProviderConfig;
use passvalet_agent::run::RunKind;
use passvalet_core::model::*;
use passvalet_core::services::{self, ServiceInfo};
use passvalet_ipc::protocol::ExtHelloParams;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use crate::prompt::{self, Decision, PendingPrompt};
use crate::runs::{self, RunInfo};
use crate::settings::Settings;
use crate::state::{AppState, LLM_KEY_SERVICE, LLM_KEY_TYPE};
use crate::unlock;

type Res<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

// ------------------------------------------------------------------ vault lifecycle

#[tauri::command]
pub fn vault_info(state: State<'_, Arc<AppState>>) -> Res<VaultInfo> {
    state.with_vault(|v| v.info()).map_err(err)
}

#[derive(Serialize)]
pub struct SetupResult {
    pub recovery_key: String,
}

#[tauri::command]
pub async fn vault_setup<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    provider: UnlockProviderKind,
) -> Res<SetupResult> {
    let settings = state.settings();
    let text = unlock::setup(&app, &state.vault, &settings, provider)
        .await
        .map_err(err)?;
    {
        let mut s = state.settings.write().unwrap();
        s.onboarding_done = true;
        let _ = s.save();
    }
    state.touch();
    let _ = app.emit("vault:changed", ());
    Ok(SetupResult { recovery_key: text })
}

#[tauri::command]
pub async fn vault_unlock<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Arc<AppState>>,
) -> Res<()> {
    let settings = state.settings();
    unlock::unlock(&app, &state.vault, &settings, "解锁 PassValet 保险库")
        .await
        .map_err(err)?;
    state.touch();
    let _ = app.emit("vault:changed", ());
    Ok(())
}

#[tauri::command]
pub fn vault_lock<R: Runtime>(app: AppHandle<R>, state: State<'_, Arc<AppState>>) -> Res<()> {
    state.with_vault(|v| v.lock()).map_err(err)?;
    let _ = app.emit("vault:changed", ());
    Ok(())
}

#[tauri::command]
pub fn vault_unlock_recovery<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    recovery_key: String,
) -> Res<()> {
    state
        .with_vault(|v| v.unlock_with_recovery(&recovery_key))
        .map_err(err)?;
    state.touch();
    let _ = app.emit("vault:changed", ());
    Ok(())
}

/// After a recovery unlock (or to switch providers): bind a new passkey / Touch ID key.
#[tauri::command]
pub async fn vault_rebind<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    provider: UnlockProviderKind,
) -> Res<SetupResult> {
    let settings = state.settings();
    let text = unlock::rebind(&app, &state.vault, &settings, provider)
        .await
        .map_err(err)?;
    let _ = app.emit("vault:changed", ());
    Ok(SetupResult { recovery_key: text })
}

#[tauri::command]
pub async fn vault_regenerate_recovery(state: State<'_, Arc<AppState>>) -> Res<SetupResult> {
    unlock::verify_presence("重新生成恢复密钥")
        .await
        .map_err(err)?;
    let text = state.with_vault(|v| v.regenerate_recovery()).map_err(err)?;
    Ok(SetupResult { recovery_key: text })
}

#[tauri::command]
pub fn biometrics_available() -> bool {
    tauri_plugin_macos_passkey::is_biometrics_available()
}

// ------------------------------------------------------------------ secrets

#[tauri::command]
pub fn list_secrets(state: State<'_, Arc<AppState>>) -> Res<Vec<SecretMeta>> {
    state.with_vault(|v| v.list_secrets()).map_err(err)
}

#[derive(Deserialize)]
pub struct AddSecretArgs {
    pub service: String,
    pub key_type: String,
    pub value: String,
    pub label: Option<String>,
}

#[tauri::command]
pub fn add_secret<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    args: AddSecretArgs,
) -> Res<SecretMeta> {
    let service = args.service.trim().to_lowercase();
    let key_type = args.key_type.trim().to_lowercase().replace([' ', '-'], "_");
    if !services::is_valid_id(&service) || !services::is_valid_id(&key_type) {
        return Err("服务名和密钥类型只能包含小写字母、数字、_ 或 -".into());
    }
    let meta = state
        .with_vault(|v| {
            v.put_secret(NewSecret {
                service,
                key_type,
                value: args.value,
                label: args.label.filter(|l| !l.trim().is_empty()),
                source: SecretSource::Manual,
                metadata: Default::default(),
            })
        })
        .map_err(err)?;
    let _ = app.emit("vault:changed", ());
    Ok(meta)
}

#[tauri::command]
pub fn delete_secret<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Res<()> {
    state.with_vault(|v| v.delete_secret(&id)).map_err(err)?;
    let _ = app.emit("vault:changed", ());
    Ok(())
}

#[tauri::command]
pub fn update_secret_label(state: State<'_, Arc<AppState>>, id: String, label: String) -> Res<()> {
    state
        .with_vault(|v| v.update_secret_meta(&id, Some(label), None))
        .map_err(err)
}

#[tauri::command]
pub async fn reveal_secret(
    state: State<'_, Arc<AppState>>,
    service: String,
    key_type: String,
) -> Res<String> {
    if state.settings().presence_on_approve {
        unlock::verify_presence(&format!("显示 {} 的 {}", service, key_type))
            .await
            .map_err(err)?;
    }
    let v = state
        .with_vault(|v| v.reveal_secret(&service, &key_type))
        .map_err(err)?;
    Ok(v.to_string())
}

#[tauri::command]
pub fn list_services(state: State<'_, Arc<AppState>>) -> Res<Vec<ServiceInfo>> {
    let _ = state;
    Ok(services::all_services())
}

// ------------------------------------------------------------------ sessions & audit

#[tauri::command]
pub fn list_sessions(
    state: State<'_, Arc<AppState>>,
    include_inactive: Option<bool>,
) -> Res<Vec<Session>> {
    state
        .with_vault(|v| v.list_sessions(include_inactive.unwrap_or(false)))
        .map_err(err)
}

#[tauri::command]
pub fn revoke_session<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Res<()> {
    state.with_vault(|v| v.revoke_session(&id)).map_err(err)?;
    let _ = app.emit("vault:changed", ());
    Ok(())
}

#[tauri::command]
pub fn revoke_all_sessions<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Arc<AppState>>,
) -> Res<usize> {
    let n = state.with_vault(|v| v.revoke_all_sessions()).map_err(err)?;
    let _ = app.emit("vault:changed", ());
    Ok(n)
}

#[tauri::command]
pub fn audit_log(
    state: State<'_, Arc<AppState>>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Res<Vec<AuditEntry>> {
    state
        .with_vault(|v| v.audit_log(limit.unwrap_or(200).clamp(1, 2000), offset.unwrap_or(0)))
        .map_err(err)
}

// ------------------------------------------------------------------ settings

#[derive(Serialize)]
pub struct SettingsView {
    #[serde(flatten)]
    pub settings: Settings,
    pub has_llm_api_key: bool,
    pub llm_api_key_preview: Option<String>,
    pub cli_path: String,
    pub socket_path: String,
    pub app_dir: String,
}

#[tauri::command]
pub fn settings_get(state: State<'_, Arc<AppState>>) -> Res<SettingsView> {
    let settings = state.settings();
    let meta = state
        .with_vault(|v| v.get_secret_meta(LLM_KEY_SERVICE, LLM_KEY_TYPE))
        .unwrap_or(None);
    Ok(SettingsView {
        settings,
        has_llm_api_key: meta.is_some(),
        llm_api_key_preview: meta.map(|m| m.fingerprint),
        cli_path: cli_path().display().to_string(),
        socket_path: passvalet_core::paths::socket_path().display().to_string(),
        app_dir: passvalet_core::paths::app_dir().display().to_string(),
    })
}

#[derive(Deserialize)]
pub struct SettingsPatch {
    pub provider: Option<ProviderConfig>,
    pub auto_lock_minutes: Option<u32>,
    pub passkey_domain: Option<String>,
    pub passkey_username: Option<String>,
    pub extension_ids: Option<Vec<String>>,
    pub presence_on_approve: Option<bool>,
}

#[tauri::command]
pub fn settings_set(state: State<'_, Arc<AppState>>, patch: SettingsPatch) -> Res<()> {
    let mut s = state.settings.write().unwrap();
    if let Some(p) = patch.provider {
        s.provider = ProviderConfig { api_key: None, ..p };
    }
    if let Some(m) = patch.auto_lock_minutes {
        s.auto_lock_minutes = m;
    }
    if let Some(d) = patch.passkey_domain {
        s.passkey_domain = d.trim().to_string();
    }
    if let Some(u) = patch.passkey_username {
        s.passkey_username = u.trim().to_string();
    }
    if let Some(ids) = patch.extension_ids {
        s.extension_ids = ids;
    }
    if let Some(b) = patch.presence_on_approve {
        s.presence_on_approve = b;
    }
    s.save().map_err(err)
}

#[tauri::command]
pub fn set_llm_api_key(state: State<'_, Arc<AppState>>, api_key: String) -> Res<()> {
    let api_key = api_key.trim().to_string();
    if api_key.is_empty() {
        return Err("API key 不能为空".into());
    }
    state
        .with_vault(|v| {
            v.put_secret(NewSecret {
                service: LLM_KEY_SERVICE.into(),
                key_type: LLM_KEY_TYPE.into(),
                value: api_key,
                label: Some("模型网关 API key".into()),
                source: SecretSource::Manual,
                metadata: Default::default(),
            })
        })
        .map(|_| ())
        .map_err(err)
}

/// Provider config with the API key loaded from the vault.
pub fn provider_with_key(state: &AppState) -> Result<ProviderConfig, String> {
    let mut cfg = state.settings().provider;
    if let Ok(k) = state.with_vault(|v| v.read_secret_value(LLM_KEY_SERVICE, LLM_KEY_TYPE)) {
        cfg.api_key = Some(k.to_string());
    }
    Ok(cfg)
}

#[derive(Serialize)]
pub struct ProviderTestResult {
    pub ok: bool,
    pub model: String,
    pub message: String,
}

#[tauri::command]
pub async fn test_provider(state: State<'_, Arc<AppState>>) -> Res<ProviderTestResult> {
    let cfg = provider_with_key(&state)?;
    let model = cfg.models.first().cloned().unwrap_or_default();
    let provider = passvalet_agent::providers::build(&cfg);
    let req = passvalet_agent::provider::CompletionRequest {
        model: model.clone(),
        system: "Reply with the single word OK.".into(),
        messages: vec![passvalet_agent::provider::Message::user_text("ping")],
        tools: vec![],
        max_tokens: 512,
    };
    match provider.complete(&req).await {
        Ok(r) => Ok(ProviderTestResult {
            ok: true,
            model,
            message: r.text.unwrap_or_else(|| "(no text)".into()),
        }),
        Err(e) => Ok(ProviderTestResult {
            ok: false,
            model,
            message: e.to_string(),
        }),
    }
}

// ------------------------------------------------------------------ prompts

#[tauri::command]
pub fn prompt_list(state: State<'_, Arc<AppState>>) -> Vec<PendingPrompt> {
    state.prompts.list()
}

/// Approve or deny. Approval verifies presence (Touch ID / passkey) and unlocks the vault if
/// needed before the IPC handler issues the session.
#[tauri::command]
pub async fn prompt_decide<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    id: String,
    approve: bool,
) -> Res<()> {
    let Some(p) = state.prompts.get(&id) else {
        return Err("该请求已过期".into());
    };
    if !approve {
        state.prompts.resolve(&id, Decision::Denied);
        prompt::close_prompt_window_if_idle(&app, &state.prompts);
        return Ok(());
    }
    let settings = state.settings();
    let locked = state.with_vault(|v| v.is_locked());
    if locked {
        unlock::unlock(
            &app,
            &state.vault,
            &settings,
            &format!("授权 {}", p.summary.agent_label),
        )
        .await
        .map_err(err)?;
    } else if settings.presence_on_approve {
        unlock::verify_presence(&format!("授权 {}", p.summary.agent_label))
            .await
            .map_err(err)?;
    }
    state.prompts.resolve(&id, Decision::Approved);
    state.touch();
    prompt::close_prompt_window_if_idle(&app, &state.prompts);
    let _ = app.emit("vault:changed", ());
    Ok(())
}

// ------------------------------------------------------------------ runs

#[derive(Serialize)]
pub struct ExtensionStatus {
    pub connected: bool,
    pub info: Option<ExtHelloParams>,
}

#[tauri::command]
pub fn extension_status(state: State<'_, Arc<AppState>>) -> ExtensionStatus {
    ExtensionStatus {
        connected: state.extension.is_connected(),
        info: state.extension.info(),
    }
}

#[tauri::command]
pub fn list_playbooks(state: State<'_, Arc<AppState>>) -> Vec<passvalet_agent::Playbook> {
    state.runs.playbooks().list().into_iter().cloned().collect()
}

pub fn start_run_inner<R: Runtime>(
    app: &AppHandle<R>,
    state: &Arc<AppState>,
    service: String,
    kind: RunKind,
    hints: Vec<String>,
) -> Result<String, String> {
    if state.with_vault(|v| v.is_locked()) {
        return Err("保险库已锁定，请先解锁".into());
    }
    if !state.extension.is_connected() {
        return Err("浏览器扩展未连接：请在 Chrome 中安装并启用 PassValet 扩展".into());
    }
    let provider = provider_with_key(state)?;
    runs::start(
        app.clone(),
        state.runs.clone(),
        state.vault.clone(),
        state.extension.clone(),
        runs::StartArgs {
            service,
            kind,
            hints,
            provider,
        },
    )
}

pub fn start_collection_inner<R: Runtime>(
    app: &AppHandle<R>,
    state: &Arc<AppState>,
    service: String,
    key_types: Vec<String>,
    hints: Vec<String>,
) -> Result<String, String> {
    let key_types = if key_types.is_empty() {
        state
            .runs
            .playbooks()
            .get(&service)
            .map(|p| p.key_types.clone())
            .unwrap_or_default()
    } else {
        key_types
    };
    start_run_inner(app, state, service, RunKind::Collect { key_types }, hints)
}

#[tauri::command]
pub fn start_collection<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    service: String,
    key_types: Vec<String>,
    hints: Option<Vec<String>>,
) -> Res<String> {
    start_collection_inner(&app, &state, service, key_types, hints.unwrap_or_default())
}

#[tauri::command]
pub fn start_rotation<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    service: String,
    key_type: String,
) -> Res<String> {
    let mut hints = vec![];
    if let Ok(Some(meta)) = state.with_vault(|v| v.get_secret_meta(&service, &key_type)) {
        hints.push(format!("The OLD key preview is {}.", meta.fingerprint));
    }
    start_run_inner(&app, &state, service, RunKind::Rotate { key_type }, hints)
}

#[tauri::command]
pub fn list_runs(state: State<'_, Arc<AppState>>) -> Vec<RunInfo> {
    state.runs.list()
}

#[tauri::command]
pub fn abort_run(state: State<'_, Arc<AppState>>, run_id: String) -> bool {
    state.runs.abort(&run_id)
}

#[tauri::command]
pub fn resume_run(state: State<'_, Arc<AppState>>, run_id: String) -> bool {
    state.runs.resume(&run_id)
}

#[tauri::command]
pub fn clear_runs(state: State<'_, Arc<AppState>>) {
    state.runs.clear_finished()
}

// ------------------------------------------------------------------ install helpers

/// Path of the `passvalet` CLI: bundled sidecar in release, `target/debug` in dev.
pub fn cli_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("PASSVALET_CLI") {
        return p.into();
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sidecar = dir.join("passvalet");
            if sidecar.exists() {
                return sidecar;
            }
        }
    }
    std::path::PathBuf::from("passvalet")
}

#[tauri::command]
pub fn install_extension_host(
    state: State<'_, Arc<AppState>>,
    extension_id: String,
) -> Res<String> {
    let id = extension_id.trim().to_string();
    if id.len() != 32 || !id.chars().all(|c| c.is_ascii_lowercase()) {
        return Err("扩展 ID 应为 32 个小写字母（见 chrome://extensions）".into());
    }
    let cli = cli_path();
    if !cli.exists() {
        return Err(format!("找不到 passvalet CLI：{}", cli.display()));
    }
    let out = std::process::Command::new(&cli)
        .args(["install-extension-host", "--extension-id", &id])
        .output()
        .map_err(err)?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).to_string());
    }
    {
        let mut s = state.settings.write().unwrap();
        if !s.extension_ids.contains(&id) {
            s.extension_ids.push(id);
        }
        let _ = s.save();
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

#[tauri::command]
pub fn mcp_config_snippet() -> serde_json::Value {
    let cli = cli_path().display().to_string();
    serde_json::json!({
        "cursor": serde_json::json!({ "mcpServers": { "passvalet": { "command": cli, "args": ["mcp"] } } }).to_string(),
        "claude_code": format!("claude mcp add passvalet -- {cli} mcp"),
        "codex": format!("[mcp_servers.passvalet]\ncommand = \"{cli}\"\nargs = [\"mcp\"]"),
        "cli": cli,
    })
}

#[tauri::command]
pub fn open_main_window<R: Runtime>(app: AppHandle<R>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}
