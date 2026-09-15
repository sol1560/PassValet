//! Serves the Unix socket: CLI / MCP requests and the extension bridge.

use std::sync::Arc;
use std::time::Duration;

use passvalet_agent::run::{RunKind, RunOutcome};
use passvalet_core::model::{AuditEvent, PermissionManifest};
use passvalet_core::{manifest, services, CoreError};
use passvalet_ipc::protocol::*;
use passvalet_ipc::server::HandlerFuture;
use passvalet_ipc::{methods, Inbound, Peer, RpcError, RpcHandler};
use tauri::{Emitter, Runtime};

use crate::prompt::{self, Decision, PendingPrompt, PromptKind};
use crate::state::AppState;

pub struct IpcHandler<R: Runtime> {
    pub app: tauri::AppHandle<R>,
    pub state: Arc<AppState>,
}

impl<R: Runtime> RpcHandler for IpcHandler<R> {
    fn handle(&self, inbound: Inbound) -> HandlerFuture {
        let app = self.app.clone();
        let state = self.state.clone();
        Box::pin(async move {
            let result = dispatch(&app, &state, &inbound).await;
            inbound.respond(result);
        })
    }

    fn on_connect(&self, _peer: &Peer) {}

    fn on_disconnect(&self, peer_id: u64) {
        self.state.extension.detach_if(peer_id);
        let _ = self
            .app
            .emit("extension:changed", self.state.extension.is_connected());
    }
}

async fn dispatch<R: Runtime>(
    app: &tauri::AppHandle<R>,
    state: &Arc<AppState>,
    inbound: &Inbound,
) -> Result<serde_json::Value, RpcError> {
    match inbound.method.as_str() {
        methods::STATUS => {
            let vault = state.with_vault(|v| v.info())?;
            ok(StatusResult {
                version: env!("CARGO_PKG_VERSION").into(),
                vault,
                extension_connected: state.extension.is_connected(),
            })
        }
        methods::REQUEST_PERMISSIONS => {
            let p: RequestPermissionsParams = inbound.params()?;
            request_permissions(app, state, p).await.and_then(ok)
        }
        methods::GET_KEY => {
            let p: GetKeyParams = inbound.params()?;
            state.touch();
            let value = state.with_vault(|v| {
                v.read_key_for_session(&p.session_token, &p.service, &p.key_type)
            })?;
            let _ = app.emit("vault:changed", ());
            ok(GetKeyResult {
                env_var: services::env_var_for(&p.service, &p.key_type),
                service: p.service,
                key_type: p.key_type,
                value: value.to_string(),
            })
        }
        methods::REPORT_KEY_INVALID => {
            let p: ReportKeyInvalidParams = inbound.params()?;
            report_key_invalid(app, state, p).await.and_then(ok)
        }
        methods::LIST_SERVICES => {
            let stored = state.with_vault(|v| v.list_secrets())?;
            let mut out = Vec::new();
            for s in services::SERVICES {
                out.push(ServiceAvailability {
                    service: s.into(),
                    stored_key_types: stored
                        .iter()
                        .filter(|m| m.service == s.id)
                        .map(|m| m.key_type.clone())
                        .collect(),
                });
            }
            let custom = stored
                .into_iter()
                .filter(|m| services::find_service(&m.service).is_none())
                .collect();
            ok(ListServicesResult {
                services: out,
                custom,
            })
        }
        methods::LIST_KEYS => {
            let keys = state.with_vault(|v| v.list_secrets())?;
            ok(ListKeysResult { keys })
        }
        methods::START_COLLECTION => {
            let p: StartCollectionParams = inbound.params()?;
            let run_id =
                crate::commands::start_collection_inner(app, state, p.service, p.key_types, vec![])
                    .map_err(|e| RpcError::app("collection_failed", e))?;
            ok(StartCollectionResult { run_id })
        }
        methods::EXT_HELLO => {
            let hello: ExtHelloParams = inbound.params()?;
            state.extension.attach(inbound.peer.clone(), hello);
            let _ = app.emit("extension:changed", true);
            ok(serde_json::json!({ "ok": true, "app_version": env!("CARGO_PKG_VERSION") }))
        }
        methods::EXT_EVENT => {
            if !state.extension.is_peer(inbound.peer.id()) {
                return Err(RpcError::app(
                    "not_extension",
                    "events require the current extension connection",
                ));
            }
            let ev: ExtEvent = inbound.params()?;
            if let ExtEvent::UserAborted { run_id } = &ev {
                state.runs.abort(run_id);
            }
            let _ = app.emit("extension:event", &ev);
            ok(serde_json::json!({ "ok": true }))
        }
        // Debug builds only, for end-to-end tests: approve/deny the oldest pending prompt.
        "dev.decide" if crate::unlock::dev_skip_presence() => {
            let approve = inbound
                .params
                .get("approve")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let Some(p) = state.prompts.list().into_iter().next() else {
                return Err(RpcError::app("no_prompt", "no pending prompt"));
            };
            let decision = if approve {
                Decision::Approved
            } else {
                Decision::Denied
            };
            state.prompts.resolve(&p.id, decision);
            prompt::close_prompt_window_if_idle(app, &state.prompts);
            ok(serde_json::json!({ "id": p.id, "approved": approve }))
        }
        "dev.setup" if crate::unlock::dev_skip_presence() => {
            let settings = state.settings();
            let text = crate::unlock::setup(
                app,
                &state.vault,
                &settings,
                passvalet_core::model::UnlockProviderKind::TouchIdKeychain,
            )
            .await
            .map_err(|e| RpcError::app("setup_failed", e.to_string()))?;
            ok(serde_json::json!({ "recovery_key": text }))
        }
        "dev.unlock" if crate::unlock::dev_skip_presence() => {
            let settings = state.settings();
            crate::unlock::unlock(app, &state.vault, &settings, "dev")
                .await
                .map_err(|e| RpcError::app("unlock_failed", e.to_string()))?;
            ok(serde_json::json!({ "ok": true }))
        }
        "dev.add_secret" if crate::unlock::dev_skip_presence() => {
            let s: passvalet_core::model::NewSecret = inbound.params()?;
            let meta = state.with_vault(|v| v.put_secret(s))?;
            ok(meta)
        }
        m => Err(RpcError::method_not_found(m)),
    }
}

fn ok<T: serde::Serialize>(v: T) -> Result<serde_json::Value, RpcError> {
    serde_json::to_value(v).map_err(|e| RpcError::internal(e.to_string()))
}

async fn request_permissions<R: Runtime>(
    app: &tauri::AppHandle<R>,
    state: &Arc<AppState>,
    p: RequestPermissionsParams,
) -> Result<RequestPermissionsResult, RpcError> {
    manifest::validate(&p.manifest)?;
    state.with_vault(|v| v.audit_session_requested(&p.manifest))?;
    if !state.with_vault(|v| v.is_initialized())? {
        return Err(RpcError::app(
            "vault_not_initialized",
            "PassValet vault is not set up yet; the user must finish onboarding in the app",
        ));
    }
    let decision = ask_user(
        app,
        state,
        PromptKind::Authorize,
        &p.manifest,
        p.wait_seconds.unwrap_or(120).min(600),
    )
    .await?;
    match decision {
        Decision::Approved => {}
        Decision::Denied => {
            state.with_vault(|v| v.audit_session_denied(&p.manifest, "user denied"))?;
            return Err(RpcError::user_denied("the user denied this request"));
        }
        Decision::TimedOut => {
            state.with_vault(|v| v.audit_session_denied(&p.manifest, "timed out"))?;
            return Err(RpcError::timeout("the user did not respond in time"));
        }
    }
    let (session, token) = state.with_vault(|v| v.create_session(&p.manifest))?;
    let missing = session
        .grants
        .iter()
        .filter(|g| {
            !state
                .with_vault(|v| v.has_secret(&g.service, &g.key_type))
                .unwrap_or(false)
        })
        .map(|g| MissingKey {
            service: g.service.clone(),
            key_type: g.key_type.clone(),
        })
        .collect();
    state.touch();
    let _ = app.emit("vault:changed", ());
    Ok(RequestPermissionsResult {
        session_token: token,
        session,
        missing,
    })
}

/// Enqueue a prompt, show the window, wait for the decision.
async fn ask_user<R: Runtime>(
    app: &tauri::AppHandle<R>,
    state: &Arc<AppState>,
    kind: PromptKind,
    m: &PermissionManifest,
    wait_seconds: u64,
) -> Result<Decision, RpcError> {
    let summary =
        state.with_vault(|v| manifest::summarize(m, |s, k| v.has_secret(s, k).unwrap_or(false)));
    let locked = state.with_vault(|v| v.is_locked());
    let id = uuid::Uuid::new_v4().to_string();
    let rx = state.prompts.push(PendingPrompt {
        id: id.clone(),
        kind,
        manifest: m.clone(),
        summary,
        created_at: chrono::Utc::now(),
        vault_locked: locked,
    });
    prompt::show_prompt_window(app);
    let decision = match tokio::time::timeout(Duration::from_secs(wait_seconds), rx).await {
        Ok(Ok(d)) => d,
        Ok(Err(_)) => Decision::Denied,
        Err(_) => {
            state.prompts.remove(&id);
            Decision::TimedOut
        }
    };
    prompt::close_prompt_window_if_idle(app, &state.prompts);
    Ok(decision)
}

async fn report_key_invalid<R: Runtime>(
    app: &tauri::AppHandle<R>,
    state: &Arc<AppState>,
    p: ReportKeyInvalidParams,
) -> Result<ReportKeyInvalidResult, RpcError> {
    let session = state.with_vault(|v| v.resolve_session(&p.session_token))?;
    if !session
        .grants
        .iter()
        .any(|g| g.service == p.service && g.key_type == p.key_type)
    {
        return Err(CoreError::NotGranted {
            service: p.service.clone(),
            key_type: p.key_type.clone(),
        }
        .into());
    }
    state.with_vault(|v| {
        v.audit(
            AuditEvent::KeyReportedInvalid,
            Some(&session.agent.name),
            Some(&p.service),
            Some(&p.key_type),
            Some(&session.id),
            Some(&format!(
                "{:?} {}",
                p.status_code,
                p.message.clone().unwrap_or_default()
            )),
        )
    })?;

    let playbooks = state.runs.playbooks();
    let supported = playbooks
        .get(&p.service)
        .map(|pb| pb.supports_rotation(&p.key_type))
        .unwrap_or(false);
    if !supported {
        return Ok(ReportKeyInvalidResult {
            outcome: RotationOutcome::Unsupported,
            value: None,
            env_var: None,
            message: Some(format!(
                "no rotation playbook for {}/{}",
                p.service, p.key_type
            )),
        });
    }

    // Ask the user.
    let manifest = PermissionManifest {
        agent: session.agent.clone(),
        purpose: format!(
            "{} 报告 {} 的 {} 已失效（{}），请求自动轮换",
            session.agent.name,
            services::service_label(&p.service),
            services::key_label(&p.service, &p.key_type),
            p.status_code
                .map(|c| c.to_string())
                .unwrap_or_else(|| "未知状态".into())
        ),
        requests: vec![passvalet_core::model::KeyRequest {
            service: p.service.clone(),
            key_type: p.key_type.clone(),
            access: passvalet_core::model::Access::ReadWrite,
            reason: p.message.clone(),
        }],
        ttl_seconds: None,
        project: session.project.clone(),
    };
    let decision = ask_user(
        app,
        state,
        PromptKind::Rotate {
            service: p.service.clone(),
            key_type: p.key_type.clone(),
            status_code: p.status_code,
            message: p.message.clone(),
        },
        &manifest,
        p.wait_seconds.unwrap_or(300).min(900),
    )
    .await?;
    if decision != Decision::Approved {
        return Ok(ReportKeyInvalidResult {
            outcome: RotationOutcome::Denied,
            value: None,
            env_var: None,
            message: Some("user declined".into()),
        });
    }

    // Hints: fingerprint of the old key helps the agent identify it.
    let mut hints = vec![];
    if let Ok(Some(meta)) = state.with_vault(|v| v.get_secret_meta(&p.service, &p.key_type)) {
        hints.push(format!(
            "The OLD key preview is {} (first/last 4 chars).",
            meta.fingerprint
        ));
        if let Some(l) = meta.label {
            hints.push(format!("The OLD key is named \"{l}\" in the dashboard."));
        }
    }
    let run_id = match crate::commands::start_run_inner(
        app,
        state,
        p.service.clone(),
        RunKind::Rotate {
            key_type: p.key_type.clone(),
        },
        hints,
    ) {
        Ok(id) => id,
        Err(e) => {
            return Ok(ReportKeyInvalidResult {
                outcome: RotationOutcome::Failed,
                value: None,
                env_var: None,
                message: Some(e),
            })
        }
    };
    let wait = p.wait_seconds.unwrap_or(300).min(900);
    let outcome = match state.runs.wait(&run_id) {
        Some(rx) => match tokio::time::timeout(Duration::from_secs(wait), rx).await {
            Ok(Ok(o)) => o,
            _ => {
                state.runs.abort(&run_id);
                RunOutcome::Failed {
                    reason: "rotation timed out".into(),
                }
            }
        },
        None => RunOutcome::Failed {
            reason: "run vanished".into(),
        },
    };
    match outcome {
        RunOutcome::Success { .. } => {
            let value = state.with_vault(|v| {
                v.read_key_for_session(&p.session_token, &p.service, &p.key_type)
            })?;
            let _ = app.emit("vault:changed", ());
            Ok(ReportKeyInvalidResult {
                outcome: RotationOutcome::Rotated,
                env_var: Some(services::env_var_for(&p.service, &p.key_type)),
                value: Some(value.to_string()),
                message: None,
            })
        }
        RunOutcome::Failed { reason } | RunOutcome::Partial { reason, .. } => {
            Ok(ReportKeyInvalidResult {
                outcome: RotationOutcome::Failed,
                value: None,
                env_var: None,
                message: Some(reason),
            })
        }
        RunOutcome::Aborted => Ok(ReportKeyInvalidResult {
            outcome: RotationOutcome::Denied,
            value: None,
            env_var: None,
            message: Some("aborted by user".into()),
        }),
    }
}
