//! Permission manifest validation and human-readable summaries.

use std::collections::HashSet;

use crate::error::{CoreError, CoreResult};
use crate::model::{Grant, ManifestLine, ManifestSummary, PermissionManifest};
use crate::services;

pub const MIN_TTL: u64 = 60;
pub const MAX_TTL: u64 = 7 * 24 * 3600;
pub const DEFAULT_TTL: u64 = 3600;

pub fn clamp_ttl(requested: Option<u64>) -> u64 {
    requested.unwrap_or(DEFAULT_TTL).clamp(MIN_TTL, MAX_TTL)
}

pub fn validate(m: &PermissionManifest) -> CoreResult<()> {
    if m.agent.name.trim().is_empty() {
        return Err(CoreError::InvalidManifest("agent.name is required".into()));
    }
    if m.agent.name.len() > 128 {
        return Err(CoreError::InvalidManifest("agent.name too long".into()));
    }
    if m.purpose.trim().is_empty() {
        return Err(CoreError::InvalidManifest("purpose is required".into()));
    }
    if m.purpose.len() > 1000 {
        return Err(CoreError::InvalidManifest(
            "purpose too long (max 1000 chars)".into(),
        ));
    }
    if m.requests.is_empty() {
        return Err(CoreError::InvalidManifest(
            "requests must not be empty".into(),
        ));
    }
    if m.requests.len() > 50 {
        return Err(CoreError::InvalidManifest(
            "too many requests (max 50)".into(),
        ));
    }
    let mut seen = HashSet::new();
    for r in &m.requests {
        if !services::is_valid_id(&r.service) {
            return Err(CoreError::InvalidManifest(format!(
                "invalid service id '{}': use lowercase letters, digits, '_' or '-'",
                r.service
            )));
        }
        if !services::is_valid_id(&r.key_type) {
            return Err(CoreError::InvalidManifest(format!(
                "invalid key_type id '{}': use lowercase letters, digits, '_' or '-'",
                r.key_type
            )));
        }
        if !seen.insert((r.service.clone(), r.key_type.clone())) {
            return Err(CoreError::InvalidManifest(format!(
                "duplicate request for {}/{}",
                r.service, r.key_type
            )));
        }
    }
    Ok(())
}

pub fn grants_of(m: &PermissionManifest) -> Vec<Grant> {
    m.requests
        .iter()
        .map(|r| Grant {
            service: r.service.clone(),
            key_type: r.key_type.clone(),
            access: r.access,
        })
        .collect()
}

pub fn ttl_label(secs: u64) -> String {
    if secs.is_multiple_of(86400) {
        format!("{} 天", secs / 86400)
    } else if secs.is_multiple_of(3600) {
        format!("{} 小时", secs / 3600)
    } else if secs.is_multiple_of(60) {
        format!("{} 分钟", secs / 60)
    } else {
        format!("{secs} 秒")
    }
}

/// Build the summary shown in the approval prompt. `available` tells which keys the vault
/// already holds so the UI can flag missing ones.
pub fn summarize(
    m: &PermissionManifest,
    available: impl Fn(&str, &str) -> bool,
) -> ManifestSummary {
    let ttl = clamp_ttl(m.ttl_seconds);
    let agent_label = match &m.agent.client {
        Some(c) if !c.is_empty() && c != &m.agent.name => format!("{} ({})", m.agent.name, c),
        _ => m.agent.name.clone(),
    };
    ManifestSummary {
        agent_label,
        purpose: m.purpose.clone(),
        ttl_seconds: ttl,
        ttl_label: ttl_label(ttl),
        lines: m
            .requests
            .iter()
            .map(|r| ManifestLine {
                service: r.service.clone(),
                service_label: services::service_label(&r.service),
                key_type: r.key_type.clone(),
                key_label: services::key_label(&r.service, &r.key_type),
                access: r.access,
                available: available(&r.service, &r.key_type),
                reason: r.reason.clone(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Access, AgentInfo, KeyRequest};

    fn manifest() -> PermissionManifest {
        PermissionManifest {
            agent: AgentInfo {
                name: "Cursor".into(),
                id: None,
                client: Some("cursor".into()),
            },
            purpose: "Set up a Next.js project".into(),
            requests: vec![KeyRequest {
                service: "supabase".into(),
                key_type: "anon_key".into(),
                access: Access::ReadWrite,
                reason: None,
            }],
            ttl_seconds: Some(10),
            project: None,
        }
    }

    #[test]
    fn validates_and_clamps() {
        let m = manifest();
        validate(&m).unwrap();
        assert_eq!(clamp_ttl(m.ttl_seconds), MIN_TTL);
        let s = summarize(&m, |_, _| true);
        assert_eq!(s.lines[0].service_label, "Supabase");
        assert_eq!(s.lines[0].key_label, "anon / publishable key");
    }

    #[test]
    fn rejects_bad_ids() {
        let mut m = manifest();
        m.requests[0].service = "Supa Base".into();
        assert!(validate(&m).is_err());
    }
}
