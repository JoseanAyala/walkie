use std::process::Command;

fn main() {
    build_ai_helper();
    tauri_build::build()
}

/// Compiles walkie-ai (`swift/walkie-ai.swift`), the bridge to Apple's
/// on-device model, to where `bundle.externalBin` expects it. tauri-build
/// then copies it next to the walkie binary, and bundling puts it in
/// Walkie.app/Contents/MacOS.
///
/// Deployment target 12: the first macOS with Swift concurrency built in.
/// FoundationModels only exists on 26, so it's weak-linked: the helper
/// launches on older systems and reports "unsupported".
fn build_ai_helper() {
    let src = "swift/walkie-ai.swift";
    println!("cargo:rerun-if-changed={src}");
    let target = std::env::var("TARGET").expect("cargo sets TARGET");
    let arch = match target.split('-').next() {
        Some("aarch64") => "arm64",
        Some(a) => a,
        None => panic!("unexpected target {target}"),
    };
    std::fs::create_dir_all("binaries").expect("creating binaries/");
    let out = format!("binaries/walkie-ai-{target}");
    let status = Command::new("xcrun")
        .args(["swiftc", "-O", "-parse-as-library", "-target"])
        .arg(format!("{arch}-apple-macos12.0"))
        .args([src, "-o", &out])
        .status()
        .expect("running xcrun swiftc (install Xcode's Command Line Tools)");
    assert!(status.success(), "swiftc failed to build walkie-ai");
}
