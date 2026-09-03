//! Static registry of well-known SaaS services and their key types. Unknown services are
//! still allowed (the vault is schema-less); the registry only improves labels, env var names
//! and validation.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyTypeDef {
    pub id: &'static str,
    pub label: &'static str,
    pub label_zh: &'static str,
    /// Conventional environment variable name for `passvalet inject`.
    pub env_var: &'static str,
    /// False for public identifiers (project URLs, publishable keys) that are safe in browsers.
    pub sensitive: bool,
    /// Regex the value should match, used for validation after collection.
    pub pattern: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceDef {
    pub id: &'static str,
    pub label: &'static str,
    pub dashboard_url: &'static str,
    pub key_types: &'static [KeyTypeDef],
}

/// Owned, serializable view of a [`KeyTypeDef`] for IPC and the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct KeyTypeInfo {
    pub id: String,
    pub label: String,
    pub label_zh: String,
    pub env_var: String,
    pub sensitive: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ServiceInfo {
    pub id: String,
    pub label: String,
    pub dashboard_url: String,
    pub key_types: Vec<KeyTypeInfo>,
}

impl From<&KeyTypeDef> for KeyTypeInfo {
    fn from(k: &KeyTypeDef) -> Self {
        KeyTypeInfo {
            id: k.id.into(),
            label: k.label.into(),
            label_zh: k.label_zh.into(),
            env_var: k.env_var.into(),
            sensitive: k.sensitive,
            pattern: k.pattern.map(Into::into),
        }
    }
}

impl From<&ServiceDef> for ServiceInfo {
    fn from(s: &ServiceDef) -> Self {
        ServiceInfo {
            id: s.id.into(),
            label: s.label.into(),
            dashboard_url: s.dashboard_url.into(),
            key_types: s.key_types.iter().map(Into::into).collect(),
        }
    }
}

pub fn all_services() -> Vec<ServiceInfo> {
    SERVICES.iter().map(Into::into).collect()
}

macro_rules! kt {
    ($id:literal, $label:literal, $zh:literal, $env:literal, $sens:expr, $pat:expr) => {
        KeyTypeDef {
            id: $id,
            label: $label,
            label_zh: $zh,
            env_var: $env,
            sensitive: $sens,
            pattern: $pat,
        }
    };
}

pub static SERVICES: &[ServiceDef] = &[
    ServiceDef {
        id: "supabase",
        label: "Supabase",
        dashboard_url: "https://supabase.com/dashboard/projects",
        key_types: &[
            kt!("url", "Project URL", "项目 URL", "NEXT_PUBLIC_SUPABASE_URL", false, Some(r"^https://[a-z0-9-]+\.supabase\.co$")),
            kt!("anon_key", "anon / publishable key", "anon 公钥", "NEXT_PUBLIC_SUPABASE_ANON_KEY", false, Some(r"^(eyJ[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{20,}|sb_publishable_[A-Za-z0-9_-]{20,})$")),
            kt!("service_role_key", "service_role / secret key", "service_role 私钥", "SUPABASE_SERVICE_ROLE_KEY", true, Some(r"^(eyJ[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{20,}|sb_secret_[A-Za-z0-9_-]{20,})$")),
            kt!("db_password", "Database password", "数据库密码", "SUPABASE_DB_PASSWORD", true, None),
        ],
    },
    ServiceDef {
        id: "stripe",
        label: "Stripe",
        dashboard_url: "https://dashboard.stripe.com/apikeys",
        key_types: &[
            kt!("publishable_key", "Publishable key", "可公开密钥", "NEXT_PUBLIC_STRIPE_PUBLISHABLE_KEY", false, Some(r"^pk_(test|live)_[A-Za-z0-9]{20,}$")),
            kt!("secret_key", "Secret key", "私密密钥", "STRIPE_SECRET_KEY", true, Some(r"^(sk|rk)_(test|live)_[A-Za-z0-9]{20,}$")),
            kt!("webhook_secret", "Webhook signing secret", "Webhook 签名密钥", "STRIPE_WEBHOOK_SECRET", true, Some(r"^whsec_[A-Za-z0-9]{20,}$")),
        ],
    },
    ServiceDef {
        id: "openai",
        label: "OpenAI",
        dashboard_url: "https://platform.openai.com/api-keys",
        key_types: &[
            kt!("api_key", "API key", "API 密钥", "OPENAI_API_KEY", true, Some(r"^sk-[A-Za-z0-9_-]{20,}$")),
        ],
    },
    ServiceDef {
        id: "anthropic",
        label: "Anthropic",
        dashboard_url: "https://console.anthropic.com/settings/keys",
        key_types: &[
            kt!("api_key", "API key", "API 密钥", "ANTHROPIC_API_KEY", true, Some(r"^sk-ant-[A-Za-z0-9_-]{20,}$")),
        ],
    },
    ServiceDef {
        id: "vercel",
        label: "Vercel",
        dashboard_url: "https://vercel.com/account/settings/tokens",
        key_types: &[
            kt!("token", "Access token", "访问令牌", "VERCEL_TOKEN", true, Some(r"^[A-Za-z0-9]{24}$")),
        ],
    },
    ServiceDef {
        id: "cloudflare",
        label: "Cloudflare",
        dashboard_url: "https://dash.cloudflare.com/profile/api-tokens",
        key_types: &[
            kt!("api_token", "API token", "API 令牌", "CLOUDFLARE_API_TOKEN", true, Some(r"^[A-Za-z0-9_-]{40}$")),
            kt!("account_id", "Account ID", "账户 ID", "CLOUDFLARE_ACCOUNT_ID", false, Some(r"^[a-f0-9]{32}$")),
        ],
    },
    ServiceDef {
        id: "github",
        label: "GitHub",
        dashboard_url: "https://github.com/settings/tokens",
        key_types: &[
            kt!("personal_access_token", "Personal access token", "个人访问令牌", "GITHUB_TOKEN", true, Some(r"^(ghp_[A-Za-z0-9]{36}|github_pat_[A-Za-z0-9_]{60,})$")),
        ],
    },
    ServiceDef {
        id: "aws",
        label: "AWS",
        dashboard_url: "https://console.aws.amazon.com/iam/home#/security_credentials",
        key_types: &[
            kt!("access_key_id", "Access key ID", "访问密钥 ID", "AWS_ACCESS_KEY_ID", false, Some(r"^AKIA[A-Z0-9]{16}$")),
            kt!("secret_access_key", "Secret access key", "私密访问密钥", "AWS_SECRET_ACCESS_KEY", true, Some(r"^[A-Za-z0-9/+=]{40}$")),
        ],
    },
    ServiceDef {
        id: "resend",
        label: "Resend",
        dashboard_url: "https://resend.com/api-keys",
        key_types: &[
            kt!("api_key", "API key", "API 密钥", "RESEND_API_KEY", true, Some(r"^re_[A-Za-z0-9_]{20,}$")),
        ],
    },
    ServiceDef {
        id: "clerk",
        label: "Clerk",
        dashboard_url: "https://dashboard.clerk.com",
        key_types: &[
            kt!("publishable_key", "Publishable key", "可公开密钥", "NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY", false, Some(r"^pk_(test|live)_[A-Za-z0-9]{20,}$")),
            kt!("secret_key", "Secret key", "私密密钥", "CLERK_SECRET_KEY", true, Some(r"^sk_(test|live)_[A-Za-z0-9]{20,}$")),
        ],
    },
    ServiceDef {
        id: "zenmux",
        label: "ZenMux",
        dashboard_url: "https://zenmux.ai/settings/keys",
        key_types: &[
            kt!("api_key", "API key", "API 密钥", "ZENMUX_API_KEY", true, Some(r"^sk-[A-Za-z0-9_-]{20,}$")),
        ],
    },
    ServiceDef {
        id: "google_ai",
        label: "Google AI Studio",
        dashboard_url: "https://aistudio.google.com/apikey",
        key_types: &[
            kt!("api_key", "API key", "API 密钥", "GOOGLE_GENERATIVE_AI_API_KEY", true, Some(r"^AIza[A-Za-z0-9_-]{30,}$")),
        ],
    },
];

pub fn find_service(id: &str) -> Option<&'static ServiceDef> {
    SERVICES.iter().find(|s| s.id == id)
}

pub fn find_key_type(service: &str, key_type: &str) -> Option<&'static KeyTypeDef> {
    find_service(service).and_then(|s| s.key_types.iter().find(|k| k.id == key_type))
}

pub fn service_label(id: &str) -> String {
    find_service(id)
        .map(|s| s.label.to_string())
        .unwrap_or_else(|| id.to_string())
}

pub fn key_label(service: &str, key_type: &str) -> String {
    find_key_type(service, key_type)
        .map(|k| k.label.to_string())
        .unwrap_or_else(|| key_type.replace('_', " "))
}

/// Env var name used by `passvalet inject`: registry name or `SERVICE_KEY_TYPE` upper-cased.
pub fn env_var_for(service: &str, key_type: &str) -> String {
    find_key_type(service, key_type)
        .map(|k| k.env_var.to_string())
        .unwrap_or_else(|| format!("{}_{}", service, key_type).to_uppercase())
}

/// Validate a value against the registry pattern when one exists. Unknown types always pass.
pub fn value_matches_pattern(service: &str, key_type: &str, value: &str) -> bool {
    match find_key_type(service, key_type).and_then(|k| k.pattern) {
        Some(p) => regex::Regex::new(p).map(|r| r.is_match(value.trim())).unwrap_or(true),
        None => true,
    }
}

pub fn is_valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_compile() {
        for s in SERVICES {
            for k in s.key_types {
                if let Some(p) = k.pattern {
                    regex::Regex::new(p).unwrap_or_else(|e| panic!("{}/{}: {e}", s.id, k.id));
                }
            }
        }
    }

    #[test]
    fn stripe_pattern() {
        assert!(value_matches_pattern(
            "stripe",
            "secret_key",
            "sk_test_51Habcdefghijklmnopqrstuvwxyz0123"
        ));
        assert!(!value_matches_pattern("stripe", "secret_key", "pk_test_123"));
        assert!(value_matches_pattern("unknown", "thing", "anything"));
    }
}
