//! Secret redaction applied to everything the model sees.

use std::sync::OnceLock;

use regex::Regex;

/// Patterns for well-known key shapes plus a generic long-token fallback.
const PATTERNS: &[&str] = &[
    r"sk-ant-[A-Za-z0-9_-]{20,}",
    r"sk-proj-[A-Za-z0-9_-]{20,}",
    r"sk-[A-Za-z0-9_-]{20,}",
    r"(?:sk|rk|pk)_(?:live|test)_[A-Za-z0-9]{16,}",
    r"whsec_[A-Za-z0-9]{16,}",
    r"sb_(?:secret|publishable)_[A-Za-z0-9_-]{16,}",
    r"eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}",
    r"ghp_[A-Za-z0-9]{30,}",
    r"gho_[A-Za-z0-9]{30,}",
    r"github_pat_[A-Za-z0-9_]{40,}",
    r"AKIA[A-Z0-9]{16}",
    r"AIza[A-Za-z0-9_-]{30,}",
    r"re_[A-Za-z0-9_]{20,}",
    r"xox[abpr]-[A-Za-z0-9-]{20,}",
    r"glpat-[A-Za-z0-9_-]{20,}",
    r"npm_[A-Za-z0-9]{30,}",
    r"pv_sess_[A-Za-z0-9_-]{30,}",
    // generic: 32+ chars of base64/hex without spaces, when preceded by a key-ish word nearby
    r"(?i)(?:key|secret|token|password|credential)[^\n]{0,40}?\b([A-Za-z0-9_\-/+=]{32,})",
];

fn regexes() -> &'static Vec<Regex> {
    static RE: OnceLock<Vec<Regex>> = OnceLock::new();
    RE.get_or_init(|| PATTERNS.iter().map(|p| Regex::new(p).unwrap()).collect())
}

/// Masked preview `abcd…wxyz` for a value.
pub fn mask(value: &str) -> String {
    passvalet_core::model::fingerprint(value)
}

/// Replace every secret-looking token with `[REDACTED abcd…wxyz]`.
pub fn redact(text: &str) -> String {
    let mut out = text.to_string();
    for (i, re) in regexes().iter().enumerate() {
        let is_generic = i == regexes().len() - 1;
        out = re
            .replace_all(&out, |caps: &regex::Captures| {
                if is_generic {
                    // keep the prefix words, mask only the captured token
                    let whole = caps.get(0).unwrap().as_str();
                    let tok = caps.get(1).unwrap().as_str();
                    if looks_like_word(tok) {
                        return whole.to_string();
                    }
                    whole.replacen(tok, &format!("[REDACTED {}]", mask(tok)), 1)
                } else {
                    format!("[REDACTED {}]", mask(caps.get(0).unwrap().as_str()))
                }
            })
            .to_string();
    }
    out
}

/// Also scrub a specific known value wherever it appears (used after capture).
pub fn redact_value(text: &str, value: &str) -> String {
    if value.len() < 6 {
        return text.to_string();
    }
    text.replace(value, &format!("[REDACTED {}]", mask(value)))
}

fn looks_like_word(tok: &str) -> bool {
    // long natural-language words / urls are not secrets
    let letters = tok.chars().filter(|c| c.is_ascii_alphabetic()).count();
    let digits = tok.chars().filter(|c| c.is_ascii_digit()).count();
    tok.contains("://") || (digits == 0 && letters == tok.len() && tok.len() < 40)
}

/// True when the text still contains something that looks like a secret (sanity check).
pub fn contains_secret(text: &str) -> bool {
    regexes().iter().any(|re| re.is_match(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_known_shapes() {
        let t = "Secret key: sk_live_51Habcdefghijklmnopqrstuv and anon eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSJ9.abcdefghijklmnopqrstuvwxyz";
        let r = redact(t);
        assert!(!r.contains("sk_live_51Habcdefghijklmnopqrstuv"));
        assert!(!r.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"));
        assert!(r.contains("[REDACTED sk_l…stuv]"), "{r}");
    }

    #[test]
    fn leaves_normal_text() {
        let t = "Click the Settings button, then API Keys. Project URL https://abc.supabase.co";
        assert_eq!(redact(t), t);
    }

    #[test]
    fn generic_token_after_keyword() {
        let t = "API key   abcdefghijklmnopqrstuvwxyz0123456789ABCDEF";
        let r = redact(t);
        assert!(r.starts_with("API key"));
        assert!(r.contains("[REDACTED"));
    }
}
