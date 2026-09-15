//! A bidirectional JSON-RPC peer over any `AsyncRead + AsyncWrite` pair.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::error::{IpcError, RpcError};

static PEER_COUNTER: AtomicU64 = AtomicU64::new(1);
const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, RpcError>>>>>;

/// Handle to the remote side. Cheap to clone.
#[derive(Clone)]
pub struct Peer {
    id: u64,
    tx: mpsc::UnboundedSender<String>,
    pending: Pending,
    next_id: Arc<AtomicU64>,
}

impl std::fmt::Debug for Peer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Peer#{}", self.id)
    }
}

/// An inbound request or notification.
pub struct Inbound {
    pub method: String,
    pub params: Value,
    /// `None` for notifications.
    pub id: Option<Value>,
    pub peer: Peer,
}

impl Inbound {
    pub fn is_notification(&self) -> bool {
        self.id.is_none()
    }

    pub fn params<T: DeserializeOwned>(&self) -> Result<T, RpcError> {
        serde_json::from_value(self.params.clone())
            .map_err(|e| RpcError::invalid_params(format!("{}: {e}", self.method)))
    }

    /// Send the response (no-op for notifications).
    pub fn respond(&self, result: Result<Value, RpcError>) {
        if let Some(id) = &self.id {
            self.peer.send_response(id.clone(), result);
        }
    }

    pub fn respond_ok<T: Serialize>(&self, value: &T) {
        match serde_json::to_value(value) {
            Ok(v) => self.respond(Ok(v)),
            Err(e) => self.respond(Err(RpcError::internal(e.to_string()))),
        }
    }
}

impl Peer {
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }

    fn send_raw(&self, line: String) -> Result<(), IpcError> {
        self.tx.send(line).map_err(|_| IpcError::Closed)
    }

    fn send_response(&self, id: Value, result: Result<Value, RpcError>) {
        let msg = match result {
            Ok(v) => serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": v }),
            Err(e) => serde_json::json!({ "jsonrpc": "2.0", "id": id, "error": e }),
        };
        let _ = self.send_raw(msg.to_string());
    }

    pub fn notify<P: Serialize>(&self, method: &str, params: &P) -> Result<(), IpcError> {
        let msg = serde_json::json!({ "jsonrpc": "2.0", "method": method, "params": params });
        self.send_raw(msg.to_string())
    }

    /// Send a request and wait for its result (with timeout).
    pub async fn call<P: Serialize, R: DeserializeOwned>(
        &self,
        method: &str,
        params: &P,
        timeout: Duration,
    ) -> Result<R, IpcError> {
        let v = self
            .call_value(method, serde_json::to_value(params)?, timeout)
            .await?;
        Ok(serde_json::from_value(v)?)
    }

    pub async fn call_value(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, IpcError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        let msg =
            serde_json::json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        if let Err(e) = self.send_raw(msg.to_string()) {
            self.pending.lock().unwrap().remove(&id);
            return Err(e);
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(res)) => res.map_err(IpcError::Rpc),
            Ok(Err(_)) => Err(IpcError::Closed),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                Err(IpcError::Timeout(method.to_string()))
            }
        }
    }

    fn resolve(&self, id: u64, result: Result<Value, RpcError>) {
        if let Some(tx) = self.pending.lock().unwrap().remove(&id) {
            let _ = tx.send(result);
        }
    }

    fn fail_all(&self) {
        let mut map = self.pending.lock().unwrap();
        for (_, tx) in map.drain() {
            let _ = tx.send(Err(RpcError::unavailable("connection closed")));
        }
    }
}

/// Wrap a stream into a [`Peer`] plus the receiver of inbound requests. Both reader and writer
/// tasks end when the stream closes or the returned receiver is dropped.
pub fn spawn_peer<S>(stream: S) -> (Peer, mpsc::Receiver<Inbound>, JoinHandle<()>)
where
    S: AsyncRead + AsyncWrite + Send + 'static,
{
    let (reader, mut writer) = tokio::io::split(stream);
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<String>();
    let (in_tx, in_rx) = mpsc::channel::<Inbound>(256);
    let peer = Peer {
        id: PEER_COUNTER.fetch_add(1, Ordering::Relaxed),
        tx: out_tx,
        pending: Arc::new(Mutex::new(HashMap::new())),
        next_id: Arc::new(AtomicU64::new(1)),
    };

    let writer_task = tokio::spawn(async move {
        while let Some(line) = out_rx.recv().await {
            if writer.write_all(line.as_bytes()).await.is_err() {
                break;
            }
            if writer.write_all(b"\n").await.is_err() {
                break;
            }
            let _ = writer.flush().await;
        }
    });

    let peer_for_reader = peer.clone();
    let handle = tokio::spawn(async move {
        let mut reader = BufReader::new(reader);
        loop {
            let mut line = String::new();
            let mut limited = (&mut reader).take((MAX_MESSAGE_BYTES + 1) as u64);
            let read = tokio::select! {
                _ = in_tx.closed() => break,
                result = limited.read_line(&mut line) => result,
            };
            match read {
                Ok(0) | Err(_) => break,
                Ok(n) if n > MAX_MESSAGE_BYTES => {
                    tracing::warn!(peer = peer_for_reader.id, "IPC message too large");
                    break;
                }
                Ok(_) => {
                    if line.trim().is_empty() {
                        continue;
                    }
                    let msg: Value = match serde_json::from_str(&line) {
                        Ok(v) => v,
                        Err(_) => {
                            tracing::warn!(peer = peer_for_reader.id, "invalid IPC JSON");
                            continue;
                        }
                    };
                    dispatch(&peer_for_reader, msg, &in_tx).await;
                }
            }
        }
        peer_for_reader.fail_all();
        writer_task.abort();
        let _ = writer_task.await;
    });

    (peer, in_rx, handle)
}

async fn dispatch(peer: &Peer, msg: Value, in_tx: &mpsc::Sender<Inbound>) {
    if let Some(method) = msg.get("method").and_then(|m| m.as_str()) {
        let inbound = Inbound {
            method: method.to_string(),
            params: msg.get("params").cloned().unwrap_or(Value::Null),
            id: msg.get("id").cloned().filter(|v| !v.is_null()),
            peer: peer.clone(),
        };
        if in_tx.send(inbound).await.is_err() {
            tracing::debug!("inbound receiver dropped");
        }
        return;
    }
    let Some(id) = msg.get("id").and_then(|i| i.as_u64()) else {
        return;
    };
    if let Some(err) = msg.get("error") {
        let e: RpcError = serde_json::from_value(err.clone())
            .unwrap_or_else(|_| RpcError::internal("malformed error"));
        peer.resolve(id, Err(e));
    } else {
        peer.resolve(id, Ok(msg.get("result").cloned().unwrap_or(Value::Null)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn accepts_message_at_size_limit() {
        let (stream, mut remote) = tokio::io::duplex(1024);
        let (_peer, mut receiver, task) = spawn_peer(stream);
        let prefix = "{\"method\":\"echo\",\"params\":\"";
        let suffix = "\"}\n";
        let line = format!(
            "{prefix}{}{suffix}",
            "x".repeat(1024 * 1024 - prefix.len() - suffix.len())
        );
        let send = tokio::spawn(async move {
            remote.write_all(line.as_bytes()).await.unwrap();
        });
        let inbound = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(inbound.method, "echo");
        assert_eq!(
            inbound.params.as_str().unwrap().len(),
            1024 * 1024 - prefix.len() - suffix.len()
        );
        send.await.unwrap();
        task.await.unwrap();
    }

    #[tokio::test]
    async fn dropping_receiver_closes_idle_peer() {
        let (stream, _remote) = tokio::io::duplex(1024);
        let (peer, receiver, task) = spawn_peer(stream);
        drop(receiver);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();
        tokio::task::yield_now().await;
        assert!(peer.is_closed());
    }

    #[tokio::test]
    async fn oversized_unterminated_message_closes_connection() {
        let (stream, mut remote) = tokio::io::duplex(1024);
        let (_peer, mut receiver, task) = spawn_peer(stream);
        let send = tokio::spawn(async move {
            let _ = remote.write_all(&vec![b'x'; 1024 * 1024 + 1]).await;
            // 保持连接，不发送换行，服务端必须自己拒绝。
            tokio::time::sleep(Duration::from_secs(3)).await;
        });
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();
        assert!(receiver.recv().await.is_none());
        send.abort();
    }
}
