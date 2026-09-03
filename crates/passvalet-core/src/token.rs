//! Session tokens are opaque random strings. Only their SHA-256 is persisted.

use base64::Engine;

use crate::crypto::{random_bytes, sha256_hex};

pub const TOKEN_PREFIX: &str = "pv_sess_";

pub fn generate_session_token() -> String {
    let raw = random_bytes(32);
    format!(
        "{TOKEN_PREFIX}{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw)
    )
}

pub fn hash_token(token: &str) -> String {
    sha256_hex(token.as_bytes())
}

pub fn looks_like_token(token: &str) -> bool {
    token.starts_with(TOKEN_PREFIX) && token.len() > TOKEN_PREFIX.len() + 20
}
