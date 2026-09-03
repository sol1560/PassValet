//! Well-known on-disk locations. Everything lives under the per-user application support dir.

use std::path::PathBuf;

pub const APP_DIR_NAME: &str = "PassValet";
pub const SOCKET_FILE: &str = "passvalet.sock";
pub const DB_FILE: &str = "vault.sqlite";
pub const SETTINGS_FILE: &str = "settings.json";
pub const NATIVE_HOST_NAME: &str = "ai.passvalet.host";

pub fn app_dir() -> PathBuf {
    if let Ok(p) = std::env::var("PASSVALET_HOME") {
        return PathBuf::from(p);
    }
    directories::BaseDirs::new()
        .map(|b| b.data_dir().join(APP_DIR_NAME))
        .unwrap_or_else(|| PathBuf::from(".passvalet"))
}

pub fn socket_path() -> PathBuf {
    if let Ok(p) = std::env::var("PASSVALET_SOCKET") {
        return PathBuf::from(p);
    }
    app_dir().join(SOCKET_FILE)
}

pub fn db_path() -> PathBuf {
    app_dir().join(DB_FILE)
}

pub fn settings_path() -> PathBuf {
    app_dir().join(SETTINGS_FILE)
}

pub fn ensure_app_dir() -> std::io::Result<PathBuf> {
    let dir = app_dir();
    std::fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }
    Ok(dir)
}
