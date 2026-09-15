//! Native-messaging host registration and MCP config snippets.

use anyhow::{bail, Context, Result};
use passvalet_core::paths::NATIVE_HOST_NAME;

pub fn install_host(extension_ids: Vec<String>) -> Result<()> {
    for id in &extension_ids {
        if id.len() != 32 || !id.chars().all(|c| c.is_ascii_lowercase()) {
            bail!("'{id}' is not a Chrome extension id (32 lowercase letters)");
        }
    }
    let exe = std::env::current_exe()?;
    let manifest = serde_json::json!({
        "name": NATIVE_HOST_NAME,
        "description": "PassValet browser bridge",
        "path": exe.display().to_string(),
        "type": "stdio",
        "allowed_origins": extension_ids
            .iter()
            .map(|id| format!("chrome-extension://{id}/"))
            .collect::<Vec<_>>(),
    });
    let text = serde_json::to_string_pretty(&manifest)?;
    let home = directories::BaseDirs::new().context("no home dir")?;
    let base = home.home_dir().join("Library/Application Support");
    let targets = [
        "Google/Chrome/NativeMessagingHosts",
        "Google/Chrome Canary/NativeMessagingHosts",
        "Chromium/NativeMessagingHosts",
        "Microsoft Edge/NativeMessagingHosts",
        "BraveSoftware/Brave-Browser/NativeMessagingHosts",
        "Arc/User Data/NativeMessagingHosts",
        "Vivaldi/NativeMessagingHosts",
    ];
    let mut written = 0;
    for t in targets {
        let dir = base.join(t);
        // Only register for browsers that exist on this machine.
        let browser_root = dir.parent().unwrap();
        if !browser_root.exists() {
            continue;
        }
        std::fs::create_dir_all(&dir)?;
        let file = dir.join(format!("{NATIVE_HOST_NAME}.json"));
        std::fs::write(&file, &text)?;
        println!("wrote {}", file.display());
        written += 1;
    }
    if written == 0 {
        bail!(
            "no Chromium-based browser profile found under {}",
            base.display()
        );
    }
    // Wrapper script is unnecessary: Chrome executes `path` directly with no args, so the
    // binary detects native-messaging mode by the parent origin argument.
    println!("host binary: {}", exe.display());
    Ok(())
}

pub fn print_mcp_config() {
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "passvalet".into());
    println!("# Cursor  (~/.cursor/mcp.json or <project>/.cursor/mcp.json)");
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "mcpServers": { "passvalet": { "command": exe, "args": ["mcp"] } }
        }))
        .unwrap()
    );
    println!();
    println!("# Claude Code");
    println!("claude mcp add passvalet -- {exe} mcp");
    println!();
    println!("# Codex (~/.codex/config.toml)");
    println!("[mcp_servers.passvalet]\ncommand = \"{exe}\"\nargs = [\"mcp\"]");
}
