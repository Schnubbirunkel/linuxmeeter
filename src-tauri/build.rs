fn main() {
    // Build identity for the in-app updater: CI passes the commit and the
    // package version (e.g. 0.4.1.r312); local builds ask git.
    println!("cargo:rerun-if-env-changed=LM_BUILD_SHA");
    println!("cargo:rerun-if-env-changed=LM_BUILD_VERSION");
    let sha = std::env::var("LM_BUILD_SHA").ok().filter(|s| !s.is_empty()).or_else(|| {
        std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    });
    println!("cargo:rustc-env=LM_BUILD_SHA={}", sha.unwrap_or_default());
    let version = std::env::var("LM_BUILD_VERSION")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| std::env::var("CARGO_PKG_VERSION").unwrap_or_default());
    println!("cargo:rustc-env=LM_BUILD_VERSION={version}");
    tauri_build::build();
}
