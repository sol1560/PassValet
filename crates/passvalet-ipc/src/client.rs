//! Client side used by the CLI, MCP server and native host.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::error::IpcError;
use crate::methods;
use crate::peer::{spawn_peer, Inbound, Peer};
use crate::protocol::*;

pub struct IpcClient {
    pub peer: Peer,
    /// Requests/notifications sent by the app to this client (progress, browser.* for the
    /// native host). Take it with [`IpcClient::take_inbound`].
    inbound: Option<mpsc::Receiver<Inbound>>,
    _reader: JoinHandle<()>,
    default_timeout: Duration,
}

impl IpcClient {
    pub fn socket_path() -> PathBuf {
        passvalet_core::paths::socket_path()
    }

    pub async fn connect() -> Result<Self, IpcError> {
        Self::connect_to(&Self::socket_path()).await
    }

    pub async fn connect_to(path: &Path) -> Result<Self, IpcError> {
        if !path.exists() {
            return Err(IpcError::NotRunning(path.display().to_string()));
        }
        let stream = UnixStream::connect(path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::ConnectionRefused
                || e.kind() == std::io::ErrorKind::NotFound
            {
                IpcError::NotRunning(path.display().to_string())
            } else {
                IpcError::Io(e)
            }
        })?;
        let (peer, inbound, reader) = spawn_peer(stream);
        Ok(IpcClient {
            peer,
            inbound: Some(inbound),
            _reader: reader,
            default_timeout: Duration::from_secs(30),
        })
    }

    /// Connect, and if the app is not running try to launch it and retry for a while.
    pub async fn connect_or_launch(launch: impl Fn() -> bool) -> Result<Self, IpcError> {
        match Self::connect().await {
            Ok(c) => return Ok(c),
            Err(IpcError::NotRunning(_)) => {}
            Err(e) => return Err(e),
        }
        if !launch() {
            return Err(IpcError::NotRunning(
                Self::socket_path().display().to_string(),
            ));
        }
        for _ in 0..40 {
            tokio::time::sleep(Duration::from_millis(250)).await;
            if let Ok(c) = Self::connect().await {
                return Ok(c);
            }
        }
        Err(IpcError::NotRunning(
            Self::socket_path().display().to_string(),
        ))
    }

    pub fn take_inbound(&mut self) -> Option<mpsc::Receiver<Inbound>> {
        self.inbound.take()
    }

    pub async fn call<P: Serialize, R: DeserializeOwned>(
        &self,
        method: &str,
        params: &P,
    ) -> Result<R, IpcError> {
        self.peer.call(method, params, self.default_timeout).await
    }

    pub async fn call_with_timeout<P: Serialize, R: DeserializeOwned>(
        &self,
        method: &str,
        params: &P,
        timeout: Duration,
    ) -> Result<R, IpcError> {
        self.peer.call(method, params, timeout).await
    }

    // ------------------------------------------------------------ typed helpers

    pub async fn status(&self) -> Result<StatusResult, IpcError> {
        self.call(methods::STATUS, &serde_json::json!({})).await
    }

    pub async fn request_permissions(
        &self,
        params: &RequestPermissionsParams,
    ) -> Result<RequestPermissionsResult, IpcError> {
        let wait = params.wait_seconds.unwrap_or(120).min(600);
        self.call_with_timeout(
            methods::REQUEST_PERMISSIONS,
            params,
            Duration::from_secs(wait + 15),
        )
        .await
    }

    pub async fn get_key(&self, params: &GetKeyParams) -> Result<GetKeyResult, IpcError> {
        self.call(methods::GET_KEY, params).await
    }

    pub async fn report_key_invalid(
        &self,
        params: &ReportKeyInvalidParams,
    ) -> Result<ReportKeyInvalidResult, IpcError> {
        let wait = params.wait_seconds.unwrap_or(300).min(900);
        self.call_with_timeout(
            methods::REPORT_KEY_INVALID,
            params,
            Duration::from_secs(wait + 15),
        )
        .await
    }

    pub async fn list_services(&self) -> Result<ListServicesResult, IpcError> {
        self.call(methods::LIST_SERVICES, &serde_json::json!({}))
            .await
    }

    pub async fn list_keys(&self) -> Result<ListKeysResult, IpcError> {
        self.call(methods::LIST_KEYS, &serde_json::json!({})).await
    }

    pub async fn ext_hello(&self, params: &ExtHelloParams) -> Result<serde_json::Value, IpcError> {
        self.call(methods::EXT_HELLO, params).await
    }
}
