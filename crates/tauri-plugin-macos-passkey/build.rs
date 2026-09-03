const COMMANDS: &[&str] = &[
    "register_passkey",
    "login_passkey",
    "touch_id_authenticate",
    "biometrics_available",
];

#[cfg(target_os = "macos")]
fn main() {
    use swift_rs::SwiftLinker;
    SwiftLinker::new("15.0")
        .with_package("PasskeyBridge", "swift-lib")
        .link();
    tauri_plugin::Builder::new(COMMANDS).build();
}

#[cfg(not(target_os = "macos"))]
fn main() {
    tauri_plugin::Builder::new(COMMANDS).build();
}
