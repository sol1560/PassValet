//! Per-service playbooks: where to start and how to find/rotate keys.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Playbook {
    pub service: String,
    pub label: String,
    pub start_url: String,
    #[serde(default)]
    pub login_url: Option<String>,
    /// Key types this playbook can collect, in the order they usually appear.
    pub key_types: Vec<String>,
    #[serde(default = "default_max_steps")]
    pub max_steps: u32,
    pub collect: Phase,
    #[serde(default)]
    pub rotate: Option<RotatePhase>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Phase {
    pub instructions: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RotatePhase {
    #[serde(default)]
    pub supported: bool,
    #[serde(default = "default_max_steps")]
    pub max_steps: u32,
    pub instructions: String,
    /// Key types that can be rotated (others need manual action).
    #[serde(default)]
    pub key_types: Vec<String>,
}

fn default_max_steps() -> u32 {
    40
}

impl Playbook {
    pub fn parse(toml_text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(toml_text)
    }

    pub fn supports_rotation(&self, key_type: &str) -> bool {
        self.rotate
            .as_ref()
            .map(|r| r.supported && (r.key_types.is_empty() || r.key_types.iter().any(|k| k == key_type)))
            .unwrap_or(false)
    }
}

/// Built-in playbooks compiled into the binary, overridable by files in a user directory.
#[derive(Debug, Clone, Default)]
pub struct PlaybookSet {
    books: BTreeMap<String, Playbook>,
}

const BUILTIN: &[(&str, &str)] = &[
    ("supabase", include_str!("../../../playbooks/supabase.toml")),
    ("stripe", include_str!("../../../playbooks/stripe.toml")),
    ("openai", include_str!("../../../playbooks/openai.toml")),
    ("vercel", include_str!("../../../playbooks/vercel.toml")),
    ("cloudflare", include_str!("../../../playbooks/cloudflare.toml")),
    ("github", include_str!("../../../playbooks/github.toml")),
    ("anthropic", include_str!("../../../playbooks/anthropic.toml")),
];

impl PlaybookSet {
    pub fn builtin() -> Self {
        let mut books = BTreeMap::new();
        for (id, text) in BUILTIN {
            match Playbook::parse(text) {
                Ok(p) => {
                    books.insert(id.to_string(), p);
                }
                Err(e) => tracing::error!("builtin playbook {id} invalid: {e}"),
            }
        }
        PlaybookSet { books }
    }

    /// Load `*.toml` from `dir`, overriding built-ins with the same service id.
    pub fn with_overrides(mut self, dir: &Path) -> Self {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for entry in rd.flatten() {
                let p = entry.path();
                if p.extension().map(|e| e == "toml").unwrap_or(false) {
                    if let Ok(text) = std::fs::read_to_string(&p) {
                        match Playbook::parse(&text) {
                            Ok(pb) => {
                                self.books.insert(pb.service.clone(), pb);
                            }
                            Err(e) => tracing::warn!("playbook {} invalid: {e}", p.display()),
                        }
                    }
                }
            }
        }
        self
    }

    pub fn get(&self, service: &str) -> Option<&Playbook> {
        self.books.get(service)
    }

    pub fn list(&self) -> Vec<&Playbook> {
        self.books.values().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_parse() {
        let set = PlaybookSet::builtin();
        assert_eq!(set.list().len(), BUILTIN.len());
        let sb = set.get("supabase").unwrap();
        assert!(sb.key_types.contains(&"anon_key".to_string()));
        assert!(sb.supports_rotation("service_role_key") || !sb.supports_rotation("url"));
    }
}
