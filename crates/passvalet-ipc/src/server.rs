//! Unix-socket server used by the desktop app.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use tokio::net::UnixListener;

use crate::peer::{spawn_peer, Inbound, Peer};

pub type HandlerFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

/// Implemented by the app. All methods are invoked on the tokio runtime.
pub trait RpcHandler: Send + Sync + 'static {
    /// Handle a request or notification. Implementations must call `inbound.respond(..)`
    /// for requests (possibly after awaiting user interaction).
    fn handle(&self, inbound: Inbound) -> HandlerFuture;
    fn on_connect(&self, _peer: &Peer) {}
    fn on_disconnect(&self, _peer_id: u64) {}
}

pub struct IpcServer {
    path: PathBuf,
    listener: UnixListener,
}

impl IpcServer {
    /// Bind, replacing a stale socket file if the previous owner is gone.
    pub fn bind(path: &Path) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if path.exists() {
            match std::os::unix::net::UnixStream::connect(path) {
                Ok(_) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::AddrInUse,
                        format!("another PassValet instance owns {}", path.display()),
                    ));
                }
                Err(_) => {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
        let listener = UnixListener::bind(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(IpcServer {
            path: path.to_path_buf(),
            listener,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Accept loop. Runs until the future is dropped.
    pub async fn serve<H: RpcHandler>(self, handler: Arc<H>) {
        loop {
            let (stream, _) = match self.listener.accept().await {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("accept failed: {e}");
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    continue;
                }
            };
            let (peer, mut inbound_rx, reader) = spawn_peer(stream);
            let peer_id = peer.id();
            handler.on_connect(&peer);
            let h = handler.clone();
            tokio::spawn(async move {
                while let Some(inbound) = inbound_rx.recv().await {
                    let h2 = h.clone();
                    tokio::spawn(async move {
                        h2.handle(inbound).await;
                    });
                }
                reader.abort();
                h.on_disconnect(peer_id);
            });
        }
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::IpcClient;
    use crate::error::RpcError;
    use std::time::Duration;

    struct Echo;

    impl RpcHandler for Echo {
        fn handle(&self, inbound: Inbound) -> HandlerFuture {
            Box::pin(async move {
                match inbound.method.as_str() {
                    "echo" => inbound.respond(Ok(inbound.params.clone())),
                    "callback" => {
                        // server calls the client back before answering
                        let v: serde_json::Value = inbound
                            .peer
                            .call_value("client.ping", serde_json::json!(1), Duration::from_secs(2))
                            .await
                            .unwrap();
                        inbound.respond(Ok(serde_json::json!({ "client_said": v })));
                    }
                    m => inbound.respond(Err(RpcError::method_not_found(m))),
                }
            })
        }
    }

    #[tokio::test]
    async fn roundtrip_and_reverse_call() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("t.sock");
        let server = IpcServer::bind(&sock).unwrap();
        tokio::spawn(server.serve(Arc::new(Echo)));

        let mut client = IpcClient::connect_to(&sock).await.unwrap();
        let mut inbound = client.take_inbound().unwrap();
        tokio::spawn(async move {
            while let Some(req) = inbound.recv().await {
                if req.method == "client.ping" {
                    req.respond(Ok(serde_json::json!("pong")));
                }
            }
        });

        let v: serde_json::Value = client
            .call("echo", &serde_json::json!({ "a": 1 }))
            .await
            .unwrap();
        assert_eq!(v, serde_json::json!({ "a": 1 }));

        let v: serde_json::Value = client.call("callback", &serde_json::json!({})).await.unwrap();
        assert_eq!(v["client_said"], "pong");

        let err = client
            .call::<_, serde_json::Value>("nope", &serde_json::json!({}))
            .await
            .unwrap_err();
        assert!(matches!(err, crate::error::IpcError::Rpc(_)));
    }
}
