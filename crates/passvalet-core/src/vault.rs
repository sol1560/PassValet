//! The vault: one SQLite file, one in-memory KEK while unlocked.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use zeroize::Zeroizing;

use crate::crypto::{self, Kek, Sealed, SymKey};
use crate::db;
use crate::error::{CoreError, CoreResult};
use crate::manifest;
use crate::model::*;
use crate::recovery;
use crate::token;

const VERIFIER_PLAINTEXT: &[u8] = b"passvalet-vault-verifier-v1";
const VERIFIER_AAD: &[u8] = b"verifier";
const RECOVERY_AAD: &[u8] = b"recovery";

pub struct Vault {
    conn: Connection,
    kek: Option<Kek>,
    path: Option<PathBuf>,
}

/// Parameters for creating a fresh vault.
pub struct InitParams {
    pub provider: UnlockProviderKind,
    pub kek: Kek,
    pub kek_salt: Vec<u8>,
    pub credential_id: Option<String>,
    pub user_handle: Option<Vec<u8>>,
    pub with_recovery: bool,
}

impl Vault {
    pub fn open(path: &Path) -> CoreResult<Self> {
        let conn = db::open(path)?;
        Ok(Vault {
            conn,
            kek: None,
            path: Some(path.to_path_buf()),
        })
    }

    pub fn open_default() -> CoreResult<Self> {
        crate::paths::ensure_app_dir()?;
        Self::open(&crate::paths::db_path())
    }

    pub fn open_in_memory() -> CoreResult<Self> {
        Ok(Vault {
            conn: db::open_in_memory()?,
            kek: None,
            path: None,
        })
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn new_salt() -> Vec<u8> {
        crypto::random_bytes(32)
    }

    // ---------------------------------------------------------------- state

    pub fn is_initialized(&self) -> CoreResult<bool> {
        db::meta_exists(&self.conn)
    }

    pub fn is_locked(&self) -> bool {
        self.kek.is_none()
    }

    fn kek(&self) -> CoreResult<&Kek> {
        self.kek.as_ref().ok_or(CoreError::Locked)
    }

    pub fn provider(&self) -> CoreResult<Option<UnlockProviderKind>> {
        Ok(db::read_meta(&self.conn)?.and_then(|m| UnlockProviderKind::parse(&m.provider)))
    }

    pub fn kek_salt(&self) -> CoreResult<Vec<u8>> {
        db::read_meta(&self.conn)?
            .map(|m| m.kek_salt)
            .ok_or(CoreError::NotInitialized)
    }

    pub fn credential_id(&self) -> CoreResult<Option<String>> {
        Ok(db::read_meta(&self.conn)?.and_then(|m| m.credential_id))
    }

    pub fn user_handle(&self) -> CoreResult<Option<Vec<u8>>> {
        Ok(db::read_meta(&self.conn)?.and_then(|m| m.user_handle))
    }

    pub fn info(&self) -> CoreResult<VaultInfo> {
        let meta = db::read_meta(&self.conn)?;
        let secret_count: i64 =
            self.conn.query_row("SELECT COUNT(*) FROM secrets", [], |r| r.get(0))?;
        let now = Utc::now().to_rfc3339();
        let active_session_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM sessions WHERE revoked = 0 AND expires_at > ?1",
            params![now],
            |r| r.get(0),
        )?;
        Ok(VaultInfo {
            initialized: meta.is_some(),
            locked: self.kek.is_none(),
            provider: meta.as_ref().and_then(|m| UnlockProviderKind::parse(&m.provider)),
            created_at: meta
                .as_ref()
                .and_then(|m| DateTime::parse_from_rfc3339(&m.created_at).ok())
                .map(|d| d.with_timezone(&Utc)),
            secret_count,
            active_session_count,
            has_recovery: meta
                .as_ref()
                .map(|m| m.recovery_wrapped_kek.is_some())
                .unwrap_or(false),
            credential_id: meta.and_then(|m| m.credential_id),
        })
    }

    // ---------------------------------------------------------------- init / unlock

    /// Create the vault. Returns the formatted recovery key when requested; it is shown once.
    pub fn initialize(&mut self, p: InitParams) -> CoreResult<Option<String>> {
        if self.is_initialized()? {
            return Err(CoreError::AlreadyInitialized);
        }
        let verifier = crypto::seal(&p.kek, VERIFIER_PLAINTEXT, VERIFIER_AAD)?.to_blob();
        let (recovery_salt, recovery_blob, recovery_text) = if p.with_recovery {
            let (salt, blob, text) = Self::make_recovery(&p.kek)?;
            (Some(salt), Some(blob), Some(text))
        } else {
            (None, None, None)
        };
        let now = Utc::now().to_rfc3339();
        db::insert_meta(
            &self.conn,
            &db::MetaRow {
                provider: p.provider.as_str().to_string(),
                kek_salt: p.kek_salt,
                verifier,
                recovery_salt,
                recovery_wrapped_kek: recovery_blob,
                credential_id: p.credential_id,
                user_handle: p.user_handle,
                created_at: now,
            },
        )?;
        self.kek = Some(p.kek);
        self.audit(AuditEvent::VaultInitialized, None, None, None, None, Some(p.provider.as_str()))?;
        Ok(recovery_text)
    }

    fn make_recovery(kek: &Kek) -> CoreResult<(Vec<u8>, Vec<u8>, String)> {
        let secret = recovery::generate_recovery_secret();
        let salt = crypto::random_bytes(32);
        let rk = crypto::derive_recovery_key(&secret, &salt)?;
        let blob = crypto::seal(&rk, kek.as_bytes(), RECOVERY_AAD)?.to_blob();
        Ok((salt, blob, recovery::format_recovery_key(&secret)))
    }

    /// Verify the KEK against the stored verifier and unlock.
    pub fn unlock(&mut self, kek: Kek) -> CoreResult<()> {
        let meta = db::read_meta(&self.conn)?.ok_or(CoreError::NotInitialized)?;
        let sealed = Sealed::from_blob(&meta.verifier)?;
        let pt = crypto::open(&kek, &sealed, VERIFIER_AAD)?;
        if &pt[..] != VERIFIER_PLAINTEXT {
            return Err(CoreError::WrongKey);
        }
        self.kek = Some(kek);
        self.audit(AuditEvent::VaultUnlocked, None, None, None, None, Some(&meta.provider))?;
        Ok(())
    }

    /// Unlock with the recovery key. The caller should then call [`Vault::rekey`] to bind a
    /// new passkey / Keychain entry.
    pub fn unlock_with_recovery(&mut self, recovery_key_text: &str) -> CoreResult<()> {
        let meta = db::read_meta(&self.conn)?.ok_or(CoreError::NotInitialized)?;
        let (salt, blob) = match (meta.recovery_salt, meta.recovery_wrapped_kek) {
            (Some(s), Some(b)) => (s, b),
            _ => return Err(CoreError::InvalidRecoveryKey),
        };
        let secret = recovery::parse_recovery_key(recovery_key_text)?;
        let rk = crypto::derive_recovery_key(&secret, &salt)?;
        let kek_bytes = crypto::open(&rk, &Sealed::from_blob(&blob)?, RECOVERY_AAD)
            .map_err(|_| CoreError::InvalidRecoveryKey)?;
        let kek = SymKey::from_slice(&kek_bytes)?;
        self.unlock(kek)?;
        self.audit(AuditEvent::RecoveryUsed, None, None, None, None, None)?;
        Ok(())
    }

    pub fn lock(&mut self) -> CoreResult<()> {
        if self.kek.take().is_some() {
            self.audit(AuditEvent::VaultLocked, None, None, None, None, None)?;
        }
        Ok(())
    }

    /// Replace the KEK: rewrap every DEK, the verifier and (optionally) a fresh recovery copy.
    /// The vault must be unlocked. Returns the new recovery key text when `with_recovery`.
    pub fn rekey(
        &mut self,
        new_kek: Kek,
        new_salt: Vec<u8>,
        provider: UnlockProviderKind,
        credential_id: Option<String>,
        user_handle: Option<Vec<u8>>,
        with_recovery: bool,
    ) -> CoreResult<Option<String>> {
        let old = self.kek()?.clone();
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare("SELECT id, wrapped_dek FROM secrets")?;
            let rows: Vec<(String, Vec<u8>)> = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<_, _>>()?;
            for (id, wrapped) in rows {
                let dek = crypto::open(&old, &Sealed::from_blob(&wrapped)?, id.as_bytes())?;
                let rewrapped = crypto::seal(&new_kek, &dek, id.as_bytes())?.to_blob();
                tx.execute(
                    "UPDATE secrets SET wrapped_dek = ?1 WHERE id = ?2",
                    params![rewrapped, id],
                )?;
            }
        }
        let verifier = crypto::seal(&new_kek, VERIFIER_PLAINTEXT, VERIFIER_AAD)?.to_blob();
        let (rs, rb, rt) = if with_recovery {
            let (s, b, t) = Self::make_recovery(&new_kek)?;
            (Some(s), Some(b), Some(t))
        } else {
            (None, None, None)
        };
        db::update_meta_keys(
            &tx,
            provider.as_str(),
            &new_salt,
            &verifier,
            rs.as_deref(),
            rb.as_deref(),
            credential_id.as_deref(),
            user_handle.as_deref(),
            &Utc::now().to_rfc3339(),
        )?;
        tx.commit()?;
        self.kek = Some(new_kek);
        Ok(rt)
    }

    /// Issue a new recovery key (invalidates the previous one).
    pub fn regenerate_recovery(&mut self) -> CoreResult<String> {
        let kek = self.kek()?.clone();
        let meta = db::read_meta(&self.conn)?.ok_or(CoreError::NotInitialized)?;
        let (salt, blob, text) = Self::make_recovery(&kek)?;
        db::update_meta_keys(
            &self.conn,
            &meta.provider,
            &meta.kek_salt,
            &meta.verifier,
            Some(&salt),
            Some(&blob),
            meta.credential_id.as_deref(),
            meta.user_handle.as_deref(),
            &Utc::now().to_rfc3339(),
        )?;
        Ok(text)
    }

    // ---------------------------------------------------------------- secrets

    pub fn list_secrets(&self) -> CoreResult<Vec<SecretMeta>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, service, key_type, label, source, fingerprint, metadata, created_at, updated_at, last_rotated_at
             FROM secrets ORDER BY service, key_type",
        )?;
        let rows = stmt.query_map([], row_to_secret_meta)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn get_secret_meta(&self, service: &str, key_type: &str) -> CoreResult<Option<SecretMeta>> {
        self.conn
            .query_row(
                "SELECT id, service, key_type, label, source, fingerprint, metadata, created_at, updated_at, last_rotated_at
                 FROM secrets WHERE service = ?1 AND key_type = ?2",
                params![service, key_type],
                row_to_secret_meta,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn has_secret(&self, service: &str, key_type: &str) -> CoreResult<bool> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM secrets WHERE service = ?1 AND key_type = ?2",
            params![service, key_type],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    /// Insert or replace the secret for `(service, key_type)`.
    pub fn put_secret(&mut self, s: NewSecret) -> CoreResult<SecretMeta> {
        let kek = self.kek()?;
        let value = s.value.trim();
        if value.is_empty() {
            return Err(CoreError::InvalidManifest("secret value is empty".into()));
        }
        let now = Utc::now();
        let existing = self.get_secret_meta(&s.service, &s.key_type)?;
        let id = existing
            .as_ref()
            .map(|e| e.id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let dek = SymKey::random();
        let wrapped_dek = crypto::seal(kek, dek.as_bytes(), id.as_bytes())?.to_blob();
        let ciphertext = crypto::seal(&dek, value.as_bytes(), id.as_bytes())?.to_blob();
        let fp = fingerprint(value);
        let metadata = serde_json::Value::Object(s.metadata.clone()).to_string();
        let source = source_str(s.source);
        let rotated_at = match (s.source, existing.as_ref()) {
            (SecretSource::Rotated, _) => Some(now.to_rfc3339()),
            (_, Some(e)) => e.last_rotated_at.map(|d| d.to_rfc3339()),
            _ => None,
        };
        let created_at = existing
            .as_ref()
            .map(|e| e.created_at.to_rfc3339())
            .unwrap_or_else(|| now.to_rfc3339());
        self.conn.execute(
            "INSERT INTO secrets (id, service, key_type, label, source, fingerprint, wrapped_dek, ciphertext, metadata, created_at, updated_at, last_rotated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(service, key_type) DO UPDATE SET
               label = COALESCE(excluded.label, secrets.label),
               source = excluded.source,
               fingerprint = excluded.fingerprint,
               wrapped_dek = excluded.wrapped_dek,
               ciphertext = excluded.ciphertext,
               metadata = excluded.metadata,
               updated_at = excluded.updated_at,
               last_rotated_at = excluded.last_rotated_at",
            params![
                id,
                s.service,
                s.key_type,
                s.label,
                source,
                fp,
                wrapped_dek,
                ciphertext,
                metadata,
                created_at,
                now.to_rfc3339(),
                rotated_at
            ],
        )?;
        let event = if existing.is_some() {
            AuditEvent::SecretUpdated
        } else {
            AuditEvent::SecretAdded
        };
        self.audit(event, None, Some(&s.service), Some(&s.key_type), None, Some(source))?;
        self.get_secret_meta(&s.service, &s.key_type)?
            .ok_or_else(|| CoreError::SecretNotFound {
                service: s.service.clone(),
                key_type: s.key_type.clone(),
            })
    }

    /// Decrypt a value. Does not write an audit entry; callers decide what to record.
    pub fn read_secret_value(&self, service: &str, key_type: &str) -> CoreResult<Zeroizing<String>> {
        let kek = self.kek()?;
        let row: Option<(String, Vec<u8>, Vec<u8>)> = self
            .conn
            .query_row(
                "SELECT id, wrapped_dek, ciphertext FROM secrets WHERE service = ?1 AND key_type = ?2",
                params![service, key_type],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let (id, wrapped, ct) = row.ok_or_else(|| CoreError::SecretNotFound {
            service: service.to_string(),
            key_type: key_type.to_string(),
        })?;
        let dek_bytes = crypto::open(kek, &Sealed::from_blob(&wrapped)?, id.as_bytes())?;
        let dek = SymKey::from_slice(&dek_bytes)?;
        let pt = crypto::open(&dek, &Sealed::from_blob(&ct)?, id.as_bytes())?;
        let s = String::from_utf8(pt.to_vec())
            .map_err(|_| CoreError::Crypto("secret is not utf-8".into()))?;
        Ok(Zeroizing::new(s))
    }

    /// Read for the user's own UI (reveal button). Audited as a read by "user".
    pub fn reveal_secret(&mut self, service: &str, key_type: &str) -> CoreResult<Zeroizing<String>> {
        let v = self.read_secret_value(service, key_type)?;
        self.audit(AuditEvent::KeyRead, Some("user"), Some(service), Some(key_type), None, Some("reveal"))?;
        Ok(v)
    }

    pub fn delete_secret(&mut self, id: &str) -> CoreResult<()> {
        let row: Option<(String, String)> = self
            .conn
            .query_row(
                "SELECT service, key_type FROM secrets WHERE id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((service, key_type)) = row {
            self.conn
                .execute("DELETE FROM secrets WHERE id = ?1", params![id])?;
            self.audit(AuditEvent::SecretDeleted, None, Some(&service), Some(&key_type), None, None)?;
        }
        Ok(())
    }

    pub fn update_secret_meta(
        &mut self,
        id: &str,
        label: Option<String>,
        metadata: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> CoreResult<()> {
        let now = Utc::now().to_rfc3339();
        if let Some(l) = label {
            self.conn.execute(
                "UPDATE secrets SET label = ?1, updated_at = ?2 WHERE id = ?3",
                params![l, now, id],
            )?;
        }
        if let Some(m) = metadata {
            self.conn.execute(
                "UPDATE secrets SET metadata = ?1, updated_at = ?2 WHERE id = ?3",
                params![serde_json::Value::Object(m).to_string(), now, id],
            )?;
        }
        Ok(())
    }

    // ---------------------------------------------------------------- sessions

    /// Record that a manifest arrived (before the user decides).
    pub fn audit_session_requested(&self, m: &PermissionManifest) -> CoreResult<()> {
        let detail = m
            .requests
            .iter()
            .map(|r| format!("{}/{}", r.service, r.key_type))
            .collect::<Vec<_>>()
            .join(",");
        self.audit(AuditEvent::SessionRequested, Some(&m.agent.name), None, None, None, Some(&detail))
    }

    /// Approve a manifest: create the session and return it with the plaintext token.
    pub fn create_session(&mut self, m: &PermissionManifest) -> CoreResult<(Session, String)> {
        manifest::validate(m)?;
        let ttl = manifest::clamp_ttl(m.ttl_seconds);
        let now = Utc::now();
        let expires = now + Duration::seconds(ttl as i64);
        let id = uuid::Uuid::new_v4().to_string();
        let tok = token::generate_session_token();
        let grants = manifest::grants_of(m);
        self.conn.execute(
            "INSERT INTO sessions (id, token_hash, agent_name, agent_id, agent_client, purpose, project, grants, created_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id,
                token::hash_token(&tok),
                m.agent.name,
                m.agent.id,
                m.agent.client,
                m.purpose,
                m.project,
                serde_json::to_string(&grants)?,
                now.to_rfc3339(),
                expires.to_rfc3339()
            ],
        )?;
        self.audit(
            AuditEvent::SessionApproved,
            Some(&m.agent.name),
            None,
            None,
            Some(&id),
            Some(&format!("{} keys, ttl {}s", grants.len(), ttl)),
        )?;
        let session = Session {
            id,
            agent: m.agent.clone(),
            purpose: m.purpose.clone(),
            grants,
            project: m.project.clone(),
            created_at: now,
            expires_at: expires,
            revoked: false,
            last_used_at: None,
            read_count: 0,
        };
        Ok((session, tok))
    }

    pub fn audit_session_denied(&self, m: &PermissionManifest, reason: &str) -> CoreResult<()> {
        self.audit(AuditEvent::SessionDenied, Some(&m.agent.name), None, None, None, Some(reason))
    }

    pub fn list_sessions(&self, include_inactive: bool) -> CoreResult<Vec<Session>> {
        let now = Utc::now();
        let mut stmt = self.conn.prepare(
            "SELECT id, agent_name, agent_id, agent_client, purpose, project, grants, created_at, expires_at, revoked, last_used_at, read_count
             FROM sessions ORDER BY created_at DESC LIMIT 500",
        )?;
        let rows = stmt.query_map([], row_to_session)?;
        let mut out = Vec::new();
        for s in rows {
            let s = s?;
            if include_inactive || s.is_active(now) {
                out.push(s);
            }
        }
        Ok(out)
    }

    pub fn revoke_session(&mut self, id: &str) -> CoreResult<()> {
        let n = self
            .conn
            .execute("UPDATE sessions SET revoked = 1 WHERE id = ?1", params![id])?;
        if n == 0 {
            return Err(CoreError::SessionNotFound);
        }
        self.audit(AuditEvent::SessionRevoked, None, None, None, Some(id), None)?;
        Ok(())
    }

    pub fn revoke_all_sessions(&mut self) -> CoreResult<usize> {
        let now = Utc::now().to_rfc3339();
        let n = self.conn.execute(
            "UPDATE sessions SET revoked = 1 WHERE revoked = 0 AND expires_at > ?1",
            params![now],
        )?;
        if n > 0 {
            self.audit(AuditEvent::SessionRevoked, None, None, None, None, Some(&format!("all ({n})")))?;
        }
        Ok(n)
    }

    /// Look up a session by plaintext token and check it is active.
    pub fn resolve_session(&self, tok: &str) -> CoreResult<Session> {
        if !token::looks_like_token(tok) {
            return Err(CoreError::SessionNotFound);
        }
        let session = self
            .conn
            .query_row(
                "SELECT id, agent_name, agent_id, agent_client, purpose, project, grants, created_at, expires_at, revoked, last_used_at, read_count
                 FROM sessions WHERE token_hash = ?1",
                params![token::hash_token(tok)],
                row_to_session,
            )
            .optional()?
            .ok_or(CoreError::SessionNotFound)?;
        if session.revoked {
            return Err(CoreError::SessionRevoked);
        }
        if session.expires_at <= Utc::now() {
            return Err(CoreError::SessionExpired);
        }
        Ok(session)
    }

    /// The main agent path: token + (service, key_type) → value, fully audited.
    pub fn read_key_for_session(
        &mut self,
        tok: &str,
        service: &str,
        key_type: &str,
    ) -> CoreResult<Zeroizing<String>> {
        let session = self.resolve_session(tok)?;
        let granted = session
            .grants
            .iter()
            .any(|g| g.service == service && g.key_type == key_type);
        if !granted {
            self.audit(
                AuditEvent::KeyDenied,
                Some(&session.agent.name),
                Some(service),
                Some(key_type),
                Some(&session.id),
                Some("not in manifest"),
            )?;
            return Err(CoreError::NotGranted {
                service: service.to_string(),
                key_type: key_type.to_string(),
            });
        }
        let value = match self.read_secret_value(service, key_type) {
            Ok(v) => v,
            Err(e) => {
                self.audit(
                    AuditEvent::KeyDenied,
                    Some(&session.agent.name),
                    Some(service),
                    Some(key_type),
                    Some(&session.id),
                    Some(e.code()),
                )?;
                return Err(e);
            }
        };
        self.conn.execute(
            "UPDATE sessions SET last_used_at = ?1, read_count = read_count + 1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), session.id],
        )?;
        self.audit(
            AuditEvent::KeyRead,
            Some(&session.agent.name),
            Some(service),
            Some(key_type),
            Some(&session.id),
            None,
        )?;
        Ok(value)
    }

    pub fn purge_old_sessions(&mut self, older_than_days: i64) -> CoreResult<usize> {
        let cutoff = (Utc::now() - Duration::days(older_than_days)).to_rfc3339();
        Ok(self.conn.execute(
            "DELETE FROM sessions WHERE expires_at < ?1",
            params![cutoff],
        )?)
    }

    // ---------------------------------------------------------------- audit

    pub fn audit(
        &self,
        event: AuditEvent,
        agent: Option<&str>,
        service: Option<&str>,
        key_type: Option<&str>,
        session_id: Option<&str>,
        detail: Option<&str>,
    ) -> CoreResult<()> {
        self.conn.execute(
            "INSERT INTO audit_log (ts, event, agent, service, key_type, session_id, detail) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                Utc::now().to_rfc3339(),
                event.as_str(),
                agent,
                service,
                key_type,
                session_id,
                detail
            ],
        )?;
        Ok(())
    }

    pub fn audit_log(&self, limit: i64, offset: i64) -> CoreResult<Vec<AuditEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, ts, event, agent, service, key_type, session_id, detail
             FROM audit_log ORDER BY id DESC LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt.query_map(params![limit, offset], |r| {
            let ts: String = r.get(1)?;
            let ev: String = r.get(2)?;
            Ok(AuditEntry {
                id: r.get(0)?,
                ts: parse_ts(&ts),
                event: AuditEvent::parse(&ev).unwrap_or(AuditEvent::KeyDenied),
                agent: r.get(3)?,
                service: r.get(4)?,
                key_type: r.get(5)?,
                session_id: r.get(6)?,
                detail: r.get(7)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

fn source_str(s: SecretSource) -> &'static str {
    match s {
        SecretSource::Manual => "manual",
        SecretSource::Collected => "collected",
        SecretSource::Rotated => "rotated",
        SecretSource::Imported => "imported",
    }
}

fn parse_source(s: &str) -> SecretSource {
    match s {
        "collected" => SecretSource::Collected,
        "rotated" => SecretSource::Rotated,
        "imported" => SecretSource::Imported,
        _ => SecretSource::Manual,
    }
}

fn parse_ts(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

fn row_to_secret_meta(r: &rusqlite::Row<'_>) -> rusqlite::Result<SecretMeta> {
    let source: String = r.get(4)?;
    let metadata: String = r.get(6)?;
    let created: String = r.get(7)?;
    let updated: String = r.get(8)?;
    let rotated: Option<String> = r.get(9)?;
    Ok(SecretMeta {
        id: r.get(0)?,
        service: r.get(1)?,
        key_type: r.get(2)?,
        label: r.get(3)?,
        source: parse_source(&source),
        fingerprint: r.get(5)?,
        created_at: parse_ts(&created),
        updated_at: parse_ts(&updated),
        last_rotated_at: rotated.as_deref().map(parse_ts),
        metadata: serde_json::from_str::<serde_json::Value>(&metadata)
            .ok()
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default(),
    })
}

fn row_to_session(r: &rusqlite::Row<'_>) -> rusqlite::Result<Session> {
    let grants: String = r.get(6)?;
    let created: String = r.get(7)?;
    let expires: String = r.get(8)?;
    let revoked: i64 = r.get(9)?;
    let last_used: Option<String> = r.get(10)?;
    Ok(Session {
        id: r.get(0)?,
        agent: AgentInfo {
            name: r.get(1)?,
            id: r.get(2)?,
            client: r.get(3)?,
        },
        purpose: r.get(4)?,
        project: r.get(5)?,
        grants: serde_json::from_str(&grants).unwrap_or_default(),
        created_at: parse_ts(&created),
        expires_at: parse_ts(&expires),
        revoked: revoked != 0,
        last_used_at: last_used.as_deref().map(parse_ts),
        read_count: r.get(11)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(ttl: Option<u64>) -> PermissionManifest {
        PermissionManifest {
            agent: AgentInfo {
                name: "Cursor".into(),
                id: None,
                client: Some("cursor".into()),
            },
            purpose: "test".into(),
            requests: vec![KeyRequest {
                service: "stripe".into(),
                key_type: "secret_key".into(),
                access: Access::Read,
                reason: None,
            }],
            ttl_seconds: ttl,
            project: None,
        }
    }

    fn unlocked_vault() -> (Vault, Kek) {
        let mut v = Vault::open_in_memory().unwrap();
        let kek = SymKey::random();
        v.initialize(InitParams {
            provider: UnlockProviderKind::TouchIdKeychain,
            kek: kek.clone(),
            kek_salt: Vault::new_salt(),
            credential_id: None,
            user_handle: None,
            with_recovery: false,
        })
        .unwrap();
        (v, kek)
    }

    #[test]
    fn init_lock_unlock() {
        let (mut v, kek) = unlocked_vault();
        assert!(!v.is_locked());
        v.lock().unwrap();
        assert!(v.is_locked());
        assert!(v.unlock(SymKey::random()).is_err());
        v.unlock(kek).unwrap();
        assert!(!v.is_locked());
    }

    #[test]
    fn secret_roundtrip_and_session_flow() {
        let (mut v, _) = unlocked_vault();
        v.put_secret(NewSecret {
            service: "stripe".into(),
            key_type: "secret_key".into(),
            value: "sk_test_abcdefghijklmnopqrstuvwxyz".into(),
            label: None,
            source: SecretSource::Manual,
            metadata: Default::default(),
        })
        .unwrap();
        let metas = v.list_secrets().unwrap();
        assert_eq!(metas.len(), 1);
        assert_eq!(metas[0].fingerprint, "sk_t…wxyz");

        let (session, tok) = v.create_session(&manifest(Some(600))).unwrap();
        assert!(session.is_active(Utc::now()));
        let val = v.read_key_for_session(&tok, "stripe", "secret_key").unwrap();
        assert_eq!(&val[..], "sk_test_abcdefghijklmnopqrstuvwxyz");

        let err = v.read_key_for_session(&tok, "openai", "api_key").unwrap_err();
        assert!(matches!(err, CoreError::NotGranted { .. }));

        v.revoke_session(&session.id).unwrap();
        let err = v.read_key_for_session(&tok, "stripe", "secret_key").unwrap_err();
        assert!(matches!(err, CoreError::SessionRevoked));

        let log = v.audit_log(50, 0).unwrap();
        assert!(log.iter().any(|e| e.event == AuditEvent::KeyRead));
        assert!(log.iter().any(|e| e.event == AuditEvent::KeyDenied));
    }

    #[test]
    fn recovery_and_rekey() {
        let mut v = Vault::open_in_memory().unwrap();
        let kek = SymKey::random();
        let text = v
            .initialize(InitParams {
                provider: UnlockProviderKind::PasskeyPrf,
                kek: kek.clone(),
                kek_salt: Vault::new_salt(),
                credential_id: Some("cred".into()),
                user_handle: None,
                with_recovery: true,
            })
            .unwrap()
            .unwrap();
        v.put_secret(NewSecret {
            service: "openai".into(),
            key_type: "api_key".into(),
            value: "sk-abcdefghijklmnopqrstuvwxyz".into(),
            label: None,
            source: SecretSource::Manual,
            metadata: Default::default(),
        })
        .unwrap();
        v.lock().unwrap();
        v.unlock_with_recovery(&text).unwrap();
        let new_kek = SymKey::random();
        let new_text = v
            .rekey(
                new_kek.clone(),
                Vault::new_salt(),
                UnlockProviderKind::PasskeyPrf,
                Some("cred2".into()),
                None,
                true,
            )
            .unwrap()
            .unwrap();
        assert_ne!(text, new_text);
        v.lock().unwrap();
        assert!(v.unlock(kek).is_err());
        v.unlock(new_kek).unwrap();
        assert_eq!(
            &v.read_secret_value("openai", "api_key").unwrap()[..],
            "sk-abcdefghijklmnopqrstuvwxyz"
        );
        assert!(v.unlock_with_recovery(&text).is_err());
    }

    #[test]
    fn locked_vault_refuses_reads() {
        let (mut v, _) = unlocked_vault();
        v.lock().unwrap();
        assert!(matches!(
            v.read_secret_value("a", "b").unwrap_err(),
            CoreError::Locked
        ));
        assert!(v.list_secrets().is_ok());
    }
}
