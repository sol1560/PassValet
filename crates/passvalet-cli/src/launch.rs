//! Connect to the desktop app, launching it when it is not running.

use anyhow::{anyhow, Result};
use passvalet_ipc::{IpcClient, IpcError};

pub async fn connect() -> Result<IpcClient> {
    IpcClient::connect_or_launch(launch_app)
        .await
        .map_err(|e| match e {
            IpcError::NotRunning(p) => anyhow!(
                "PassValet app is not running and could not be launched (socket: {p}).\n\
                 Open PassValet.app, or set PASSVALET_APP to the app path."
            ),
            other => anyhow!(other),
        })
}

/// Try to start the desktop app. Returns true if a launch was attempted.
pub fn launch_app() -> bool {
    if std::env::var("PASSVALET_NO_LAUNCH").is_ok() {
        return false;
    }
    if let Ok(path) = std::env::var("PASSVALET_APP") {
        return spawn_detached(&path);
    }
    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("open")
            .args(["-g", "-a", "PassValet"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        return matches!(status, Ok(s) if s.success());
    }
    #[allow(unreachable_code)]
    false
}

fn spawn_detached(path: &str) -> bool {
    if path.ends_with(".app") {
        return std::process::Command::new("open")
            .args(["-g", path])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
    }
    std::process::Command::new(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok()
}
