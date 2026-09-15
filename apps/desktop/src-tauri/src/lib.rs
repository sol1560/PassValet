mod commands;
mod extension;
mod ipc_handler;
mod prompt;
mod runs;
mod settings;
mod state;
mod unlock;

use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use passvalet_core::Vault;
use passvalet_ipc::IpcServer;
use tauri::{Emitter, Manager};

use crate::state::AppState;

pub fn run() {
    let filter = tracing_subscriber::EnvFilter::try_from_env("PASSVALET_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();

    passvalet_core::paths::ensure_app_dir().expect("app dir");
    let vault = Vault::open_default().expect("open vault");
    let settings = settings::Settings::load();

    let state = Arc::new(AppState {
        vault: Arc::new(Mutex::new(vault)),
        settings: RwLock::new(settings),
        extension: Arc::new(extension::ExtensionBridge::default()),
        prompts: Arc::new(prompt::PromptQueue::default()),
        runs: Arc::new(runs::RunRegistry::default()),
        last_activity: Mutex::new(Instant::now()),
    });

    let builder = tauri::Builder::default();
    #[cfg(all(feature = "e2e", debug_assertions))]
    let builder = builder.plugin(tauri_plugin_wdio_webdriver::init());

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_macos_passkey::init())
        .manage(state.clone())
        .setup(move |app| {
            // IPC server
            let handle = app.handle().clone();
            let st = state.clone();
            tauri::async_runtime::spawn(async move {
                let path = passvalet_core::paths::socket_path();
                match IpcServer::bind(&path) {
                    Ok(server) => {
                        tracing::info!("ipc listening on {}", path.display());
                        server
                            .serve(Arc::new(ipc_handler::IpcHandler {
                                app: handle,
                                state: st,
                            }))
                            .await;
                    }
                    Err(e) => {
                        tracing::error!("ipc bind failed: {e}");
                        let _ = handle.emit("ipc:error", e.to_string());
                    }
                }
            });

            // auto-lock
            let handle = app.handle().clone();
            let st = state.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    let minutes = st.settings().auto_lock_minutes;
                    if minutes == 0 {
                        continue;
                    }
                    let idle = st.last_activity.lock().unwrap().elapsed();
                    let has_active_run = st.runs.active().is_some();
                    if idle > Duration::from_secs(minutes as u64 * 60) && !has_active_run {
                        let locked_now = st.with_vault(|v| {
                            if !v.is_locked() {
                                let _ = v.lock();
                                true
                            } else {
                                false
                            }
                        });
                        if locked_now {
                            tracing::info!("auto-locked after {minutes} min idle");
                            let _ = handle.emit("vault:changed", ());
                        }
                    }
                }
            });

            // keep running in the background when the main window is closed
            if let Some(w) = app.get_webview_window("main") {
                let w2 = w.clone();
                w.on_window_event(move |ev| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = ev {
                        api.prevent_close();
                        let _ = w2.hide();
                    }
                });
            }
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Regular);
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "prompt" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault_info,
            commands::vault_setup,
            commands::vault_unlock,
            commands::vault_lock,
            commands::vault_unlock_recovery,
            commands::vault_rebind,
            commands::vault_regenerate_recovery,
            commands::biometrics_available,
            commands::list_secrets,
            commands::add_secret,
            commands::delete_secret,
            commands::update_secret_label,
            commands::reveal_secret,
            commands::list_services,
            commands::list_sessions,
            commands::revoke_session,
            commands::revoke_all_sessions,
            commands::audit_log,
            commands::settings_get,
            commands::settings_set,
            commands::set_llm_api_key,
            commands::test_provider,
            commands::prompt_list,
            commands::prompt_decide,
            commands::extension_status,
            commands::list_playbooks,
            commands::start_collection,
            commands::start_rotation,
            commands::list_runs,
            commands::abort_run,
            commands::resume_run,
            commands::clear_runs,
            commands::install_extension_host,
            commands::mcp_config_snippet,
            commands::open_main_window,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Reopen { .. } = event {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
        });
}
