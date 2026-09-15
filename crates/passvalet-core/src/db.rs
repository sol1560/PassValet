//! SQLite schema and low-level row access.

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::CoreResult;

pub const SCHEMA_VERSION: i64 = 1;

pub fn open(path: &std::path::Path) -> CoreResult<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    configure(&conn)?;
    migrate(&conn)?;
    Ok(conn)
}

pub fn open_in_memory() -> CoreResult<Connection> {
    let conn = Connection::open_in_memory()?;
    configure(&conn)?;
    migrate(&conn)?;
    Ok(conn)
}

fn configure(conn: &Connection) -> CoreResult<()> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = FULL;
         PRAGMA foreign_keys = ON;
         PRAGMA secure_delete = ON;",
    )?;
    Ok(())
}

fn migrate(conn: &Connection) -> CoreResult<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS vault_meta (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            schema_version INTEGER NOT NULL,
            provider TEXT NOT NULL,
            kek_salt BLOB NOT NULL,
            verifier BLOB NOT NULL,
            recovery_salt BLOB,
            recovery_wrapped_kek BLOB,
            credential_id TEXT,
            user_handle BLOB,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS secrets (
            id TEXT PRIMARY KEY,
            service TEXT NOT NULL,
            key_type TEXT NOT NULL,
            label TEXT,
            source TEXT NOT NULL,
            fingerprint TEXT NOT NULL,
            wrapped_dek BLOB NOT NULL,
            ciphertext BLOB NOT NULL,
            metadata TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            last_rotated_at TEXT,
            UNIQUE (service, key_type)
        );

        CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            token_hash TEXT NOT NULL UNIQUE,
            agent_name TEXT NOT NULL,
            agent_id TEXT,
            agent_client TEXT,
            purpose TEXT NOT NULL,
            project TEXT,
            grants TEXT NOT NULL,
            created_at TEXT NOT NULL,
            expires_at TEXT NOT NULL,
            revoked INTEGER NOT NULL DEFAULT 0,
            last_used_at TEXT,
            read_count INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS sessions_expires ON sessions (expires_at);

        CREATE TABLE IF NOT EXISTS audit_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            ts TEXT NOT NULL,
            event TEXT NOT NULL,
            agent TEXT,
            service TEXT,
            key_type TEXT,
            session_id TEXT,
            detail TEXT
        );
        CREATE INDEX IF NOT EXISTS audit_ts ON audit_log (ts);
        "#,
    )?;
    Ok(())
}

pub fn meta_exists(conn: &Connection) -> CoreResult<bool> {
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM vault_meta", [], |r| r.get(0))?;
    Ok(n > 0)
}

pub struct MetaRow {
    pub provider: String,
    pub kek_salt: Vec<u8>,
    pub verifier: Vec<u8>,
    pub recovery_salt: Option<Vec<u8>>,
    pub recovery_wrapped_kek: Option<Vec<u8>>,
    pub credential_id: Option<String>,
    pub user_handle: Option<Vec<u8>>,
    pub created_at: String,
}

pub fn read_meta(conn: &Connection) -> CoreResult<Option<MetaRow>> {
    conn.query_row(
        "SELECT provider, kek_salt, verifier, recovery_salt, recovery_wrapped_kek, credential_id, user_handle, created_at FROM vault_meta WHERE id = 1",
        [],
        |r| {
            Ok(MetaRow {
                provider: r.get(0)?,
                kek_salt: r.get(1)?,
                verifier: r.get(2)?,
                recovery_salt: r.get(3)?,
                recovery_wrapped_kek: r.get(4)?,
                credential_id: r.get(5)?,
                user_handle: r.get(6)?,
                created_at: r.get(7)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

pub fn insert_meta(conn: &Connection, m: &MetaRow) -> CoreResult<()> {
    conn.execute(
        "INSERT INTO vault_meta (id, schema_version, provider, kek_salt, verifier, recovery_salt, recovery_wrapped_kek, credential_id, user_handle, created_at, updated_at)
         VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
        params![
            SCHEMA_VERSION,
            m.provider,
            m.kek_salt,
            m.verifier,
            m.recovery_salt,
            m.recovery_wrapped_kek,
            m.credential_id,
            m.user_handle,
            m.created_at
        ],
    )?;
    Ok(())
}

pub fn update_meta_keys(
    conn: &Connection,
    meta: &MetaRow,
    now: &str,
) -> CoreResult<()> {
    conn.execute(
        "UPDATE vault_meta SET provider = ?1, kek_salt = ?2, verifier = ?3, recovery_salt = ?4, recovery_wrapped_kek = ?5, credential_id = ?6, user_handle = ?7, updated_at = ?8 WHERE id = 1",
        params![meta.provider, meta.kek_salt, meta.verifier, meta.recovery_salt, meta.recovery_wrapped_kek, meta.credential_id, meta.user_handle, now],
    )?;
    Ok(())
}
