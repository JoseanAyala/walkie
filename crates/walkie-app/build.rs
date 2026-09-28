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
/// Universal (arm64 + x86_64), saved under both triples: the bundler looks
/// for the sidecar under the triple tauri-cli itself was built for, and the
/// prebuilt tauri-cli mise installs is x86_64 even on Apple Silicon.
///
/// Deployment target 12: the first macOS with Swift concurrency built in.
/// FoundationModels only exists on 26, so it's weak-linked: the helper
/// launches on older systems and reports "unsupported".
fn build_ai_helper() {
    let src = "swift/walkie-ai.swift";
    println!("cargo:rerun-if-changed={src}");
    let out_dir = std::env::var("OUT_DIR").expect("cargo sets OUT_DIR");
    let slices: Vec<String> = ["arm64", "x86_64"]
        .iter()
        .map(|arch| {
            let out = format!("{out_dir}/walkie-ai-{arch}");
            run(Command::new("xcrun")
                .args(["swiftc", "-O", "-parse-as-library", "-target"])
                .arg(format!("{arch}-apple-macos12.0"))
                .args([src, "-o", &out]));
            out
        })
        .collect();
    std::fs::create_dir_all("binaries").expect("creating binaries/");
    let universal = "binaries/walkie-ai-aarch64-apple-darwin";
    run(Command::new("lipo")
        .arg("-create")
        .args(&slices)
        .args(["-output", universal]));
    std::fs::copy(universal, "binaries/walkie-ai-x86_64-apple-darwin")
        .expect("copying walkie-ai");
}

fn run(cmd: &mut Command) {
    let status = cmd
        .status()
        .unwrap_or_else(|e| panic!("running {cmd:?} (install Xcode's Command Line Tools): {e}"));
    assert!(status.success(), "{cmd:?} failed building walkie-ai");
}
