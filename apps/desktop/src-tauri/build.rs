fn main() {
    // The vendored passkey plugin links a Swift static library; the Swift runtime is loaded
    // from the system at run time and needs an rpath.
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
        if let Ok(out) = std::process::Command::new("xcode-select").arg("-p").output() {
            let dev = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !dev.is_empty() {
                println!(
                    "cargo:rustc-link-arg=-Wl,-rpath,{dev}/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift/macosx"
                );
            }
        }
    }
    tauri_build::build()
}
