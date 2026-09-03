//! A bidirectional JSON-RPC peer over any `AsyncRead + AsyncWrite` pair.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::error::{IpcError, RpcError};

static PEER_COUNTER: AtomicU64 = AtomicU64::new(1);

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
        let v = self.call_value(method, serde_json::to_value(params)?, timeout).await?;
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
        let msg = serde_json::json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
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
        let mut lines = BufReader::new(reader).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    if line.trim().is_empty() {
                        continue;
                    }
                    let msg: Value = match serde_json::from_str(&line) {
                        Ok(v) => v,
                        Err(e) => {
                            tracing::warn!(peer = peer_for_reader.id, "bad json: {e}");
                            continue;
                        }
                    };
                    dispatch(&peer_for_reader, msg, &in_tx).await;
                }
                Ok(None) | Err(_) => break,
            }
        }
        peer_for_reader.fail_all();
        writer_task.abort();
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
