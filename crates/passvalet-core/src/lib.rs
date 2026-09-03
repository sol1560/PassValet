//! PassValet core: local encrypted vault, permission manifests, session tokens and audit log.
//!
//! Everything in this crate is synchronous and side-effect free apart from SQLite access.
//! Higher layers (Tauri app, CLI, MCP server) wrap a [`Vault`] in an `Arc<Mutex<_>>`.

pub mod crypto;
pub mod db;
pub mod error;
pub mod manifest;
pub mod model;
pub mod paths;
pub mod recovery;
pub mod services;
pub mod token;
pub mod vault;

pub use error::{CoreError, CoreResult};
pub use model::*;
pub use vault::Vault;
