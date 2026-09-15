//! `.passvalet.json`, `passvalet init`, `passvalet inject`, `status`, `list`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use passvalet_core::model::{Access, AgentInfo, KeyRequest, PermissionManifest};
use passvalet_core::services;
use passvalet_ipc::protocol::{GetKeyParams, RequestPermissionsParams};
use serde::{Deserialize, Serialize};

use crate::launch;

pub const CONFIG_FILE: &str = ".passvalet.json";

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub project: String,
    #[serde(default = "default_env_file")]
    pub env_file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    pub requests: Vec<ProjectRequest>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProjectRequest {
    pub service: String,
    pub key_type: String,
    #[serde(default = "default_access")]
    pub access: Access,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env_var: Option<String>,
}

fn default_env_file() -> String {
    ".env.local".into()
}
fn default_access() -> Access {
    Access::Read
}

impl ProjectConfig {
    pub fn load(dir: &Path) -> Result<Self> {
        let p = dir.join(CONFIG_FILE);
        let text = std::fs::read_to_string(&p)
            .with_context(|| format!("{} not found; run `passvalet init` first", p.display()))?;
        Ok(serde_json::from_str(&text)?)
    }

    pub fn manifest(&self, ttl: Option<u64>, project_dir: &Path) -> PermissionManifest {
        PermissionManifest {
            agent: AgentInfo {
                name: "passvalet inject".into(),
                id: None,
                client: Some("cli".into()),
            },
            purpose: self
                .purpose
                .clone()
                .unwrap_or_else(|| format!("把 {} 需要的密钥写入 {}", self.project, self.env_file)),
            requests: self
                .requests
                .iter()
                .map(|r| KeyRequest {
                    service: r.service.clone(),
                    key_type: r.key_type.clone(),
                    access: r.access,
                    reason: None,
                })
                .collect(),
            ttl_seconds: ttl,
            project: Some(project_dir.display().to_string()),
        }
    }
}

/// `passvalet init`
pub fn init(services_arg: Vec<String>, name: Option<String>, force: bool) -> Result<()> {
    let dir = std::env::current_dir()?;
    let path = dir.join(CONFIG_FILE);
    if path.exists() && !force {
        bail!(
            "{} already exists (use --force to overwrite)",
            path.display()
        );
    }
    let project = name.unwrap_or_else(|| {
        dir.file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "project".into())
    });

    let mut requests: Vec<ProjectRequest> = Vec::new();
    // 1. explicit services → all their key types
    for sid in &services_arg {
        let sid = sid.trim();
        match services::find_service(sid) {
            Some(def) => {
                for kt in def.key_types {
                    requests.push(ProjectRequest {
                        service: def.id.into(),
                        key_type: kt.id.into(),
                        access: Access::Read,
                        env_var: None,
                    });
                }
            }
            None => bail!(
                "unknown service '{sid}'. Known: {}",
                services::SERVICES
                    .iter()
                    .map(|s| s.id)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
    // 2. detect from .env.example / .env / .env.local variable names
    let mut detected_env_file: Option<String> = None;
    for candidate in [".env.example", ".env.local", ".env", ".env.development"] {
        let p = dir.join(candidate);
        if let Ok(text) = std::fs::read_to_string(&p) {
            if detected_env_file.is_none() && candidate != ".env.example" {
                detected_env_file = Some(candidate.to_string());
            }
            for var in env_var_names(&text) {
                if let Some((svc, kt)) = lookup_env_var(&var) {
                    if !requests
                        .iter()
                        .any(|r| r.service == svc && r.key_type == kt)
                    {
                        requests.push(ProjectRequest {
                            service: svc,
                            key_type: kt,
                            access: Access::Read,
                            env_var: Some(var),
                        });
                    }
                }
            }
        }
    }
    if requests.is_empty() {
        eprintln!(
            "No services given and nothing detected in .env files; writing a template.\n\
             Edit {} and add the keys your project needs.",
            CONFIG_FILE
        );
        requests.push(ProjectRequest {
            service: "supabase".into(),
            key_type: "anon_key".into(),
            access: Access::Read,
            env_var: None,
        });
    }
    let cfg = ProjectConfig {
        project,
        env_file: detected_env_file.unwrap_or_else(default_env_file),
        purpose: None,
        requests,
    };
    std::fs::write(&path, serde_json::to_string_pretty(&cfg)? + "\n")?;
    println!("wrote {} ({} keys)", path.display(), cfg.requests.len());
    for r in &cfg.requests {
        println!(
            "  {:<12} {:<22} → {}",
            r.service,
            r.key_type,
            r.env_var
                .clone()
                .unwrap_or_else(|| services::env_var_for(&r.service, &r.key_type))
        );
    }
    ensure_gitignore(&dir, &cfg.env_file);
    Ok(())
}

fn env_var_names(text: &str) -> Vec<String> {
    let re = regex::Regex::new(r"(?m)^\s*(?:export\s+)?([A-Z][A-Z0-9_]{2,})\s*=").unwrap();
    re.captures_iter(text).map(|c| c[1].to_string()).collect()
}

fn lookup_env_var(var: &str) -> Option<(String, String)> {
    for s in services::SERVICES {
        for k in s.key_types {
            if k.env_var == var {
                return Some((s.id.to_string(), k.id.to_string()));
            }
        }
    }
    // common aliases
    let alias: &[(&str, &str, &str)] = &[
        ("SUPABASE_URL", "supabase", "url"),
        ("SUPABASE_ANON_KEY", "supabase", "anon_key"),
        ("SUPABASE_SERVICE_KEY", "supabase", "service_role_key"),
        ("STRIPE_PUBLISHABLE_KEY", "stripe", "publishable_key"),
        ("STRIPE_API_KEY", "stripe", "secret_key"),
        ("GH_TOKEN", "github", "personal_access_token"),
        ("GEMINI_API_KEY", "google_ai", "api_key"),
        ("CF_API_TOKEN", "cloudflare", "api_token"),
    ];
    alias
        .iter()
        .find(|(v, _, _)| *v == var)
        .map(|(_, s, k)| (s.to_string(), k.to_string()))
}

fn ensure_gitignore(dir: &Path, env_file: &str) {
    let gi = dir.join(".gitignore");
    let existing = std::fs::read_to_string(&gi).unwrap_or_default();
    let covered = existing.lines().any(|l| {
        let l = l.trim();
        l == env_file || l == ".env*" || l == ".env.*" || (l == ".env" && env_file == ".env")
    });
    if !covered {
        let mut text = existing;
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(env_file);
        text.push('\n');
        if std::fs::write(&gi, text).is_ok() {
            println!("added {env_file} to .gitignore");
        }
    }
}

/// `passvalet inject`
pub async fn inject(out: Option<String>, ttl: Option<u64>, to_stdout: bool) -> Result<()> {
    let dir = std::env::current_dir()?;
    let cfg = ProjectConfig::load(&dir)?;
    if cfg.requests.is_empty() {
        bail!("{} has no requests", CONFIG_FILE);
    }
    let client = launch::connect().await?;
    eprintln!(
        "requesting {} keys for '{}' — confirm in PassValet…",
        cfg.requests.len(),
        cfg.project
    );
    let res = client
        .request_permissions(&RequestPermissionsParams {
            manifest: cfg.manifest(ttl, &dir),
            wait_seconds: Some(300),
        })
        .await
        .map_err(|e| anyhow!("permission request failed: {e}"))?;

    let mut values: BTreeMap<String, String> = BTreeMap::new();
    let mut missing = Vec::new();
    for r in &cfg.requests {
        let env_var = r
            .env_var
            .clone()
            .unwrap_or_else(|| services::env_var_for(&r.service, &r.key_type));
        match client
            .get_key(&GetKeyParams {
                session_token: res.session_token.clone(),
                service: r.service.clone(),
                key_type: r.key_type.clone(),
            })
            .await
        {
            Ok(k) => {
                values.insert(env_var, k.value);
            }
            Err(e) => {
                missing.push(format!("{}/{} ({e})", r.service, r.key_type));
            }
        }
    }

    if to_stdout {
        for (k, v) in &values {
            println!("{k}={}", quote_env(v));
        }
    } else {
        let out_path = dir.join(out.unwrap_or(cfg.env_file.clone()));
        let n = merge_env_file(&out_path, &values)?;
        println!("wrote {} keys to {}", n, out_path.display());
    }
    if !missing.is_empty() {
        eprintln!("not written (missing in vault or not granted):");
        for m in missing {
            eprintln!("  {m}");
        }
        eprintln!(
            "Add them in PassValet (manual entry or 「添加服务」 auto-collection) and re-run."
        );
    }
    Ok(())
}

fn quote_env(v: &str) -> String {
    if v.chars()
        .all(|c| c.is_ascii_alphanumeric() || "-_./:+=".contains(c))
    {
        v.to_string()
    } else {
        format!(
            "\"{}\"",
            v.replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
                .replace('\r', "\\r")
        )
    }
}

/// Update or append `KEY=value` lines, preserving everything else.
pub fn merge_env_file(path: &Path, values: &BTreeMap<String, String>) -> Result<usize> {
    for key in values.keys() {
        let mut chars = key.chars();
        if !chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            bail!("invalid environment variable name");
        }
    }
    let existing = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).context("cannot read existing env file; nothing was written"),
    };
    let mut remaining = values.clone();
    let mut lines: Vec<String> = Vec::new();
    let mut input = existing.lines();
    while let Some(line) = input.next() {
        let trimmed = line.trim_start();
        let assignment = trimmed
            .strip_prefix("export ")
            .unwrap_or(trimmed)
            .split_once('=')
            .filter(|_| !trimmed.starts_with('#'));
        let mut record = line.to_string();
        if let Some((_, value)) = assignment {
            let value = value.trim_start();
            if let Some(quote) = value.chars().next().filter(|c| *c == '\'' || *c == '"') {
                let mut rest = &value[1..];
                loop {
                    let mut escaped = false;
                    let mut closed = false;
                    for c in rest.chars() {
                        if escaped {
                            escaped = false;
                        } else if c == quote {
                            closed = true;
                            break;
                        } else if c == '\\' && quote == '"' {
                            escaped = true;
                        }
                    }
                    if closed {
                        break;
                    }
                    rest = input
                        .next()
                        .context("unterminated quoted env value; nothing was written")?;
                    record.push('\n');
                    record.push_str(rest);
                }
            }
        }
        match assignment.map(|(key, _)| key.trim()) {
            Some(k) if values.contains_key(k) => {
                remaining.remove(k);
                lines.push(format!("{k}={}", quote_env(&values[k])));
            }
            _ => lines.push(record),
        }
    }
    if !remaining.is_empty() {
        if !lines.is_empty() && !lines.last().map(|l| l.is_empty()).unwrap_or(true) {
            lines.push(String::new());
        }
        lines.push("# managed by passvalet inject".into());
        for (k, v) in &remaining {
            lines.push(format!("{k}={}", quote_env(v)));
        }
    }
    let mut text = lines.join("\n");
    text.push('\n');
    write_private(path, &text)?;
    Ok(values.len())
}

fn write_private(path: &Path, text: &str) -> Result<()> {
    use std::io::Write;
    match std::fs::symlink_metadata(path) {
        Ok(meta) if !meta.is_file() => bail!("env destination must be a regular file"),
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    // NamedTempFile从创建起就是0600，同目录rename避免截断旧文件或短暂暴露新值。
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(text.as_bytes())?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    std::fs::File::open(parent)?.sync_all()?;
    Ok(())
}

pub async fn status() -> Result<()> {
    match passvalet_ipc::IpcClient::connect().await {
        Ok(c) => {
            let s = c.status().await?;
            println!("PassValet app        : running (v{})", s.version);
            println!(
                "vault                : {}{}",
                if s.vault.initialized {
                    "initialized"
                } else {
                    "not initialized"
                },
                if s.vault.locked {
                    ", locked"
                } else {
                    ", unlocked"
                }
            );
            if let Some(p) = s.vault.provider {
                println!("unlock provider      : {}", p.as_str());
            }
            println!("stored keys          : {}", s.vault.secret_count);
            println!("active sessions      : {}", s.vault.active_session_count);
            println!(
                "browser extension    : {}",
                if s.extension_connected {
                    "connected"
                } else {
                    "not connected"
                }
            );
        }
        Err(passvalet_ipc::IpcError::NotRunning(p)) => {
            println!("PassValet app        : not running ({p})");
        }
        Err(e) => return Err(e.into()),
    }
    println!(
        "socket               : {}",
        passvalet_ipc::IpcClient::socket_path().display()
    );
    Ok(())
}

pub async fn list() -> Result<()> {
    let client = launch::connect().await?;
    let res = client.list_keys().await?;
    if res.keys.is_empty() {
        println!("vault is empty");
        return Ok(());
    }
    println!(
        "{:<12} {:<22} {:<16} {:<10} updated",
        "service", "key_type", "preview", "source"
    );
    for k in res.keys {
        println!(
            "{:<12} {:<22} {:<16} {:<10} {}",
            k.service,
            k.key_type,
            k.fingerprint,
            format!("{:?}", k.source).to_lowercase(),
            k.updated_at.format("%Y-%m-%d %H:%M")
        );
    }
    Ok(())
}

#[allow(dead_code)]
pub fn config_path(dir: &Path) -> PathBuf {
    dir.join(CONFIG_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_handles_quoted_multiline_values_without_editing_their_contents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".env");
        std::fs::write(
            &path,
            "NOTE='first\nTOKEN=inside-note\nlast'\nTOKEN=\"old\nvalue\"\nTOKEN\nOTHER=keep\n",
        )
        .unwrap();
        merge_env_file(
            &path,
            &BTreeMap::from([("TOKEN".into(), "replacement".into())]),
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "NOTE='first\nTOKEN=inside-note\nlast'\nTOKEN=replacement\nTOKEN\nOTHER=keep\n"
        );
        let incomplete = "TOKEN=\"unfinished\nOTHER=keep\n";
        std::fs::write(&path, incomplete).unwrap();
        assert!(merge_env_file(
            &path,
            &BTreeMap::from([("TOKEN".into(), "replacement".into())])
        )
        .is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), incomplete);
    }

    #[test]
    fn merge_updates_duplicate_definitions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".env");
        std::fs::write(
            &path,
            "# retain\nTOKEN=first\nexport TOKEN=last\nOTHER=keep\n",
        )
        .unwrap();
        let values = BTreeMap::from([("TOKEN".into(), "new-value".into())]);
        merge_env_file(&path, &values).unwrap();
        let text = std::fs::read_to_string(path).unwrap();
        assert_eq!(
            text,
            "# retain\nTOKEN=new-value\nTOKEN=new-value\nOTHER=keep\n"
        );
    }

    #[test]
    fn merge_refuses_invalid_input_without_changing_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".env");
        std::fs::write(&path, b"\xfforiginal").unwrap();
        let values = BTreeMap::from([("TOKEN".into(), "new".into())]);
        assert!(merge_env_file(&path, &values).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"\xfforiginal");
        std::fs::write(&path, "OTHER=keep\n").unwrap();
        let values = BTreeMap::from([("TOKEN\nINJECTED".into(), "new".into())]);
        assert!(merge_env_file(&path, &values).is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"OTHER=keep\n");
    }

    #[test]
    fn private_write_replaces_file_atomically_without_following_links() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".env");
        std::fs::write(&path, "TOKEN=old\n").unwrap();
        let previous = std::fs::File::open(&path).unwrap();
        write_private(&path, "TOKEN=new\n").unwrap();
        let metadata = std::fs::metadata(&path).unwrap();
        assert_ne!(metadata.ino(), previous.metadata().unwrap().ino());
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(write_private(&link, "bad").is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "TOKEN=new\n");
    }

    #[test]
    fn env_newlines_do_not_create_assignments() {
        assert_eq!(
            quote_env("first\nINJECTED=second\r\nlast"),
            "\"first\\nINJECTED=second\\r\\nlast\""
        );
    }

    #[test]
    fn merge_preserves_and_updates() {
        let dir = std::env::temp_dir().join(format!("pv-{}", uuid_like()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(".env");
        std::fs::write(&p, "# hello\nFOO=1\nSTRIPE_SECRET_KEY=old\n").unwrap();
        let mut vals = BTreeMap::new();
        vals.insert("STRIPE_SECRET_KEY".to_string(), "sk_test_new".to_string());
        vals.insert("OPENAI_API_KEY".to_string(), "sk-x y".to_string());
        merge_env_file(&p, &vals).unwrap();
        let t = std::fs::read_to_string(&p).unwrap();
        assert!(t.contains("# hello\nFOO=1\nSTRIPE_SECRET_KEY=sk_test_new\n"));
        assert!(t.contains("OPENAI_API_KEY=\"sk-x y\""));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn uuid_like() -> String {
        format!(
            "{:x}",
            std::time::SystemTime::now()
                .elapsed()
                .unwrap_or_default()
                .as_nanos()
        )
    }

    #[test]
    fn detects_env_vars() {
        let names = env_var_names("export A_B=1\n# C=2\nNEXT_PUBLIC_SUPABASE_URL=x\n");
        assert_eq!(names, vec!["A_B", "NEXT_PUBLIC_SUPABASE_URL"]);
        assert_eq!(
            lookup_env_var("NEXT_PUBLIC_SUPABASE_URL"),
            Some(("supabase".into(), "url".into()))
        );
    }
}
