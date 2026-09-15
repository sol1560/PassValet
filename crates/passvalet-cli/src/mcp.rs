//! MCP server (stdio). Thin client of the desktop app over the Unix socket.

use std::sync::Arc;

use anyhow::Result;
use passvalet_core::model::{Access, AgentInfo, KeyRequest, PermissionManifest};
use passvalet_ipc::protocol::{
    GetKeyParams, ReportKeyInvalidParams, RequestPermissionsParams, RotationOutcome,
};
use passvalet_ipc::{IpcClient, IpcError};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, ContentBlock, ErrorData, Implementation, ServerCapabilities, ServerInfo,
};
use rmcp::service::{NotificationContext, RoleServer};
use rmcp::{tool, tool_handler, tool_router, ServerHandler, ServiceExt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::launch;

const INSTRUCTIONS: &str = r#"PassValet holds the user's API keys in an encrypted local vault.

Workflow:
1. Call `request_permissions` ONCE at the start of a task with every key you will need
   (service + key_type, e.g. supabase/anon_key, stripe/secret_key). The user sees a prompt
   and approves with a passkey/Touch ID. You get a session token (kept for you automatically).
2. Call `get_key` for each key. Keys not in the approved manifest are refused with
   code `not_granted` — call `request_permissions` again with the full list.
3. If an API rejects a key (401/403), call `report_key_invalid`; PassValet may rotate it in the
   user's browser and return the new value.
Use `list_services` to see known service ids, key types and which ones the vault already holds.
Never print secret values into chat or commit them; write them to .env files or use them directly."#;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RequestItem {
    /// Service id, e.g. "supabase", "stripe", "openai", "vercel", "cloudflare", "github".
    pub service: String,
    /// Key type id, e.g. "anon_key", "service_role_key", "secret_key", "api_key", "token".
    pub key_type: String,
    /// "read" (default) or "read_write" — how you intend to use the key.
    #[serde(default)]
    pub access: Option<Access>,
    /// Short reason shown to the user.
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RequestPermissionsArgs {
    /// One sentence: what you are about to do with these keys.
    pub purpose: String,
    /// All keys needed for this task.
    pub requests: Vec<RequestItem>,
    /// Session lifetime in seconds (60..604800, default 3600).
    #[serde(default)]
    pub ttl_seconds: Option<u64>,
    /// Project name or path, for the user's audit log.
    #[serde(default)]
    pub project: Option<String>,
    /// Override the agent display name (defaults to the MCP client name).
    #[serde(default)]
    pub agent_name: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetKeyArgs {
    pub service: String,
    pub key_type: String,
    /// Optional; defaults to the token from the last successful `request_permissions`.
    #[serde(default)]
    pub session_token: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReportKeyInvalidArgs {
    pub service: String,
    pub key_type: String,
    /// HTTP status you received (401, 403…).
    #[serde(default)]
    pub status_code: Option<u16>,
    /// Error message from the API, if any.
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub session_token: Option<String>,
}

#[derive(Debug, Serialize)]
struct ToolError<'a> {
    error: &'a str,
    code: &'a str,
    hint: &'a str,
}

#[derive(Clone)]
pub struct PassValetMcp {
    tool_router: ToolRouter<Self>,
    state: Arc<Mutex<State>>,
}

#[derive(Default)]
struct State {
    client_name: Option<String>,
    session_token: Option<String>,
}

#[tool_router(router = tool_router)]
impl PassValetMcp {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
            state: Arc::new(Mutex::new(State::default())),
        }
    }

    async fn connect(&self) -> Result<IpcClient, CallToolResult> {
        launch::connect().await.map_err(|e| {
            err_result(
                "app_unavailable",
                &e.to_string(),
                "Ask the user to open the PassValet app, then retry.",
            )
        })
    }

    #[tool(
        name = "request_permissions",
        description = "Declare every API key this task needs. The user approves once (passkey); you receive a session token that later get_key calls use automatically. Call this before any get_key."
    )]
    async fn request_permissions(
        &self,
        Parameters(args): Parameters<RequestPermissionsArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let client = match self.connect().await {
            Ok(c) => c,
            Err(r) => return Ok(r),
        };
        let client_name = self.state.lock().await.client_name.clone();
        let name = args
            .agent_name
            .clone()
            .or(client_name.clone())
            .unwrap_or_else(|| "AI agent".into());
        let manifest = PermissionManifest {
            agent: AgentInfo {
                name,
                id: None,
                client: client_name,
            },
            purpose: args.purpose,
            requests: args
                .requests
                .into_iter()
                .map(|r| KeyRequest {
                    service: r.service.trim().to_lowercase(),
                    key_type: r.key_type.trim().to_lowercase(),
                    access: r.access.unwrap_or(Access::Read),
                    reason: r.reason,
                })
                .collect(),
            ttl_seconds: args.ttl_seconds,
            project: args.project,
        };
        match client
            .request_permissions(&RequestPermissionsParams {
                manifest,
                wait_seconds: Some(180),
            })
            .await
        {
            Ok(res) => {
                self.state.lock().await.session_token = Some(res.session_token.clone());
                Ok(CallToolResult::structured(serde_json::json!({
                    "status": "approved",
                    "session_id": res.session.id,
                    "expires_at": res.session.expires_at,
                    "granted": res.session.grants,
                    "missing_in_vault": res.missing,
                    "note": if res.missing.is_empty() {
                        "All keys are available. Call get_key for each."
                    } else {
                        "Some keys are not in the vault yet; the user must add them in PassValet before get_key succeeds."
                    }
                })))
            }
            Err(e) => Ok(ipc_err(e, "The user denied or did not answer. Explain what you need and try again, or ask the user to add the keys manually.")),
        }
    }

    #[tool(
        name = "get_key",
        description = "Fetch one API key value that was approved via request_permissions. Returns the value plus a suggested env var name. Write it to .env / use it directly; do not echo it in chat."
    )]
    async fn get_key(
        &self,
        Parameters(args): Parameters<GetKeyArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let token = match args
            .session_token
            .or(self.state.lock().await.session_token.clone())
        {
            Some(t) => t,
            None => {
                return Ok(err_result(
                    "no_session",
                    "no session token",
                    "Call request_permissions first.",
                ))
            }
        };
        let client = match self.connect().await {
            Ok(c) => c,
            Err(r) => return Ok(r),
        };
        match client
            .get_key(&GetKeyParams {
                session_token: token,
                service: args.service.trim().to_lowercase(),
                key_type: args.key_type.trim().to_lowercase(),
            })
            .await
        {
            Ok(k) => Ok(CallToolResult::structured(serde_json::json!({
                "service": k.service,
                "key_type": k.key_type,
                "env_var": k.env_var,
                "value": k.value,
            }))),
            Err(e) => Ok(ipc_err(
                e,
                "If code is not_granted/session_expired/session_revoked: call request_permissions again with the full key list. If secret_not_found: ask the user to add the key in PassValet.",
            )),
        }
    }

    #[tool(
        name = "report_key_invalid",
        description = "Tell PassValet an API rejected a key (401/403). The user is asked to confirm; PassValet then rotates the key in their browser dashboard and returns the new value when it can."
    )]
    async fn report_key_invalid(
        &self,
        Parameters(args): Parameters<ReportKeyInvalidArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let token = match args
            .session_token
            .or(self.state.lock().await.session_token.clone())
        {
            Some(t) => t,
            None => {
                return Ok(err_result(
                    "no_session",
                    "no session token",
                    "Call request_permissions first.",
                ))
            }
        };
        let client = match self.connect().await {
            Ok(c) => c,
            Err(r) => return Ok(r),
        };
        match client
            .report_key_invalid(&ReportKeyInvalidParams {
                session_token: token,
                service: args.service.trim().to_lowercase(),
                key_type: args.key_type.trim().to_lowercase(),
                status_code: args.status_code,
                message: args.message,
                wait_seconds: Some(420),
            })
            .await
        {
            Ok(r) => {
                let hint = match r.outcome {
                    RotationOutcome::Rotated => "Use the new value; the vault has been updated.",
                    RotationOutcome::Denied => "The user declined rotation. Ask them how to proceed.",
                    RotationOutcome::Failed => "Automatic rotation failed. Ask the user to rotate the key in the dashboard, then add it to PassValet.",
                    RotationOutcome::Unsupported => "No rotation playbook for this service. Ask the user to rotate manually and add the new key to PassValet.",
                };
                Ok(CallToolResult::structured(serde_json::json!({
                    "outcome": r.outcome,
                    "value": r.value,
                    "env_var": r.env_var,
                    "message": r.message,
                    "hint": hint,
                })))
            }
            Err(e) => Ok(ipc_err(e, "Rotation could not be started.")),
        }
    }

    #[tool(
        name = "list_services",
        description = "List known services, their key types, conventional env var names, and which keys the vault already holds."
    )]
    async fn list_services(&self) -> Result<CallToolResult, ErrorData> {
        let client = match self.connect().await {
            Ok(c) => c,
            Err(r) => return Ok(r),
        };
        match client.list_services().await {
            Ok(r) => Ok(CallToolResult::structured(
                serde_json::to_value(r).unwrap_or_default(),
            )),
            Err(e) => Ok(ipc_err(e, "")),
        }
    }

    #[tool(
        name = "vault_status",
        description = "Whether the PassValet app is running, the vault is unlocked, and the browser extension is connected."
    )]
    async fn vault_status(&self) -> Result<CallToolResult, ErrorData> {
        match IpcClient::connect().await {
            Ok(c) => match c.status().await {
                Ok(s) => Ok(CallToolResult::structured(
                    serde_json::to_value(s).unwrap_or_default(),
                )),
                Err(e) => Ok(ipc_err(e, "")),
            },
            Err(IpcError::NotRunning(p)) => Ok(CallToolResult::structured(serde_json::json!({
                "running": false,
                "socket": p,
                "hint": "Ask the user to open the PassValet app."
            }))),
            Err(e) => Ok(ipc_err(e, "")),
        }
    }
}

fn err_result(code: &str, error: &str, hint: &str) -> CallToolResult {
    let body = serde_json::to_string(&ToolError { error, code, hint }).unwrap_or_default();
    CallToolResult::error(vec![ContentBlock::text(body)])
}

fn ipc_err(e: IpcError, hint: &str) -> CallToolResult {
    match e {
        IpcError::Rpc(r) => err_result(r.app_code().unwrap_or("rpc_error"), &r.message, hint),
        IpcError::NotRunning(_) => err_result(
            "app_unavailable",
            &e.to_string(),
            "Ask the user to open the PassValet app.",
        ),
        IpcError::Timeout(_) => err_result("timeout", &e.to_string(), hint),
        other => err_result("ipc_error", &other.to_string(), hint),
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for PassValetMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(INSTRUCTIONS)
            .with_server_info(Implementation::new("passvalet", env!("CARGO_PKG_VERSION")))
    }

    fn on_initialized(
        &self,
        context: NotificationContext<RoleServer>,
    ) -> impl std::future::Future<Output = ()> + rmcp::service::MaybeSendFuture + '_ {
        let name = context.peer.peer_info().map(|i| i.client_info.name.clone());
        let state = self.state.clone();
        async move {
            if let Some(n) = name {
                state.lock().await.client_name = Some(n);
            }
        }
    }
}

pub async fn run() -> Result<()> {
    let service = PassValetMcp::new().serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
