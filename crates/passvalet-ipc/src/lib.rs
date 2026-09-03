//! Newline-delimited JSON-RPC 2.0 over a Unix domain socket.
//!
//! One socket, three kinds of peers:
//! - the desktop app (server, owns the vault),
//! - `passvalet` CLI / MCP processes (clients calling vault methods),
//! - the native-messaging host bridging the Chrome extension (a client that also *serves*
//!   `browser.*` requests issued by the app).
//!
//! Every peer is a [`Peer`]: it can send requests and receives inbound requests on a channel.

pub mod client;
pub mod error;
pub mod methods;
pub mod peer;
pub mod protocol;
pub mod server;

pub use client::IpcClient;
pub use error::{IpcError, RpcError};
pub use peer::{Inbound, Peer};
pub use server::{IpcServer, RpcHandler};
