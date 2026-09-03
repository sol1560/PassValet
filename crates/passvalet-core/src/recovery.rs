//! Recovery key: 32 random bytes shown once to the user as 8 groups of 5 Crockford-base32
//! characters (200 bits) plus a 4-char checksum group. Used to wrap a copy of the KEK so the
//! vault can be reopened if the passkey is lost while the device survives.

use sha2::{Digest, Sha256};

use crate::crypto::random_bytes;
use crate::error::{CoreError, CoreResult};

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
pub const RECOVERY_SECRET_LEN: usize = 25; // 200 bits -> 40 base32 chars

pub fn generate_recovery_secret() -> Vec<u8> {
    random_bytes(RECOVERY_SECRET_LEN)
}

/// Format as `XXXXX-XXXXX-...-CCCC` (8 groups + checksum).
pub fn format_recovery_key(secret: &[u8]) -> String {
    let encoded = base32_encode(secret);
    let mut groups: Vec<String> = encoded
        .as_bytes()
        .chunks(5)
        .map(|c| String::from_utf8_lossy(c).to_string())
        .collect();
    let check = checksum(secret);
    groups.push(check);
    groups.join("-")
}

/// Parse user input back into the secret, tolerant of case, spaces and the ambiguous chars.
pub fn parse_recovery_key(input: &str) -> CoreResult<Vec<u8>> {
    let cleaned: String = input
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| normalize_char(c.to_ascii_uppercase()))
        .collect();
    if cleaned.len() != 44 {
        return Err(CoreError::InvalidRecoveryKey);
    }
    let (body, check) = cleaned.split_at(40);
    let secret = base32_decode(body).ok_or(CoreError::InvalidRecoveryKey)?;
    if checksum(&secret) != check {
        return Err(CoreError::InvalidRecoveryKey);
    }
    Ok(secret)
}

fn normalize_char(c: char) -> char {
    match c {
        'O' => '0',
        'I' | 'L' => '1',
        'U' => 'V',
        other => other,
    }
}

fn checksum(secret: &[u8]) -> String {
    let digest = Sha256::digest(secret);
    base32_encode(&digest[..3])[..4].to_string()
}

fn base32_encode(data: &[u8]) -> String {
    let mut out = String::new();
    let mut buffer: u32 = 0;
    let mut bits = 0u32;
    for &b in data {
        buffer = (buffer << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let idx = ((buffer >> bits) & 0x1f) as usize;
            out.push(ALPHABET[idx] as char);
        }
    }
    if bits > 0 {
        let idx = ((buffer << (5 - bits)) & 0x1f) as usize;
        out.push(ALPHABET[idx] as char);
    }
    out
}

fn base32_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut buffer: u32 = 0;
    let mut bits = 0u32;
    for c in s.bytes() {
        let val = ALPHABET.iter().position(|&a| a == c)? as u32;
        buffer = (buffer << 5) | val;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let secret = generate_recovery_secret();
        let text = format_recovery_key(&secret);
        assert_eq!(text.split('-').count(), 9);
        let parsed = parse_recovery_key(&text.to_lowercase()).unwrap();
        assert_eq!(parsed, secret);
        let mut bad = text.clone();
        bad.replace_range(0..1, if &text[0..1] == "A" { "B" } else { "A" });
        assert!(parse_recovery_key(&bad).is_err());
    }
}
