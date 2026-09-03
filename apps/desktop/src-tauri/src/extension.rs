//! Bridge to the Chrome extension via the native-host IPC peer.

use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use passvalet_agent::executor::{BrowserExecutor, ExecutorError, ToolOutput};
use passvalet_ipc::methods;
use passvalet_ipc::protocol::ExtHelloParams;
use passvalet_ipc::{IpcError, Peer};
use serde_json::{json, Value};

#[derive(Default)]
pub struct ExtensionBridge {
    peer: Mutex<Option<(Peer, ExtHelloParams)>>,
}

impl ExtensionBridge {
    pub fn attach(&self, peer: Peer, hello: ExtHelloParams) {
        tracing::info!("extension connected: {} {}", hello.browser, hello.extension_version);
        *self.peer.lock().unwrap() = Some((peer, hello));
    }

    pub fn detach_if(&self, peer_id: u64) {
        let mut g = self.peer.lock().unwrap();
        if g.as_ref().map(|(p, _)| p.id() == peer_id).unwrap_or(false) {
            tracing::info!("extension disconnected");
            *g = None;
        }
    }

    pub fn is_connected(&self) -> bool {
        self.peer
            .lock()
            .unwrap()
            .as_ref()
            .map(|(p, _)| !p.is_closed())
            .unwrap_or(false)
    }

    pub fn info(&self) -> Option<ExtHelloParams> {
        self.peer.lock().unwrap().as_ref().map(|(_, h)| h.clone())
    }

    fn peer(&self) -> Result<Peer, ExecutorError> {
        self.peer
            .lock()
            .unwrap()
            .as_ref()
            .filter(|(p, _)| !p.is_closed())
            .map(|(p, _)| p.clone())
            .ok_or(ExecutorError::NotConnected)
    }

    async fn call_raw(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, ExecutorError> {
        let peer = self.peer()?;
        peer.call_value(method, params, timeout).await.map_err(|e| match e {
            IpcError::Closed => ExecutorError::NotConnected,
            IpcError::Rpc(r) if r.app_code() == Some("aborted") => ExecutorError::Aborted,
            IpcError::Rpc(r) => ExecutorError::Failed(r.message),
            IpcError::Timeout(m) => ExecutorError::Failed(format!("timeout waiting for {m}")),
            other => ExecutorError::Failed(other.to_string()),
        })
    }

    #[allow(dead_code)]
    pub async fn ping(&self) -> bool {
        self.call_raw(methods::BROWSER_PING, json!({}), Duration::from_secs(5))
            .await
            .is_ok()
    }
}

#[async_trait]
impl BrowserExecutor for ExtensionBridge {
    async fn session_begin(&self, run_id: &str, start_url: &str) -> Result<String, ExecutorError> {
        let v = self
            .call_raw(
                methods::BROWSER_SESSION_BEGIN,
                json!({ "run_id": run_id, "url": start_url }),
                Duration::from_secs(30),
            )
            .await?;
        v.get("tab_id")
            .and_then(|t| t.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| ExecutorError::Failed("session_begin returned no tab_id".into()))
    }

    async fn call(&self, run_id: &str, tool: &str, mut params: Value) -> Result<ToolOutput, ExecutorError> {
        if let Some(obj) = params.as_object_mut() {
            obj.insert("run_id".into(), Value::String(run_id.to_string()));
        } else {
            params = json!({ "run_id": run_id });
        }
        let timeout = match tool {
            "wait" => Duration::from_secs(20),
            "navigate" => Duration::from_secs(60),
            _ => Duration::from_secs(45),
        };
        let v = self
            .call_raw(&format!("{}{}", methods::BROWSER_PREFIX, tool), params, timeout)
            .await?;
        serde_json::from_value(v).map_err(|e| ExecutorError::Failed(format!("bad tool output: {e}")))
    }

    async fn session_end(&self, run_id: &str, keep_tabs: bool) -> Result<(), ExecutorError> {
        self.call_raw(
            methods::BROWSER_SESSION_END,
            json!({ "run_id": run_id, "keep_tabs": keep_tabs }),
            Duration::from_secs(15),
        )
        .await
        .map(|_| ())
    }
}
