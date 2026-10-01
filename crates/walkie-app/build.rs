use std::os::unix::fs::PermissionsExt;
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
///
/// Apple's model is an optional extra, so it never fails the build: without
/// a working Swift toolchain the sidecar is a stub that says "unsupported".
fn build_ai_helper() {
    let src = "swift/walkie-ai.swift";
    println!("cargo:rerun-if-changed={src}");
    std::fs::create_dir_all("binaries").expect("creating binaries/");

    // Apple's model only exists on macOS; elsewhere there's no Swift
    // toolchain to invoke, so just drop the stub where Tauri's externalBin
    // looks for it (the host triple — there's no universal build off macOS).
    if cfg!(not(target_os = "macos")) {
        let target = std::env::var("TARGET").expect("cargo sets TARGET");
        write_stub(&format!("binaries/walkie-ai-{target}"));
        return;
    }

    let universal = "binaries/walkie-ai-aarch64-apple-darwin";
    if let Err(e) = compile_universal(src, universal) {
        println!("cargo:warning=walkie-ai: {e}; Apple's model will be unavailable");
        write_stub(universal);
    }
    std::fs::copy(universal, "binaries/walkie-ai-x86_64-apple-darwin").expect("copying walkie-ai");
}

fn write_stub(path: &str) {
    std::fs::write(path, STUB).expect("writing the walkie-ai stub");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .expect("making the walkie-ai stub executable");
}

/// walkie-ai's interface, answering "unsupported" to everything.
const STUB: &str = "#!/bin/sh
[ \"$1\" = status ] && { echo unsupported; exit 0; }
echo 'apple model unavailable: unsupported' >&2
exit 1
";

fn compile_universal(src: &str, out: &str) -> Result<(), String> {
    let dir = std::env::var("OUT_DIR").expect("cargo sets OUT_DIR");
    let mut slices = Vec::new();
    for arch in ["arm64", "x86_64"] {
        let slice = format!("{dir}/walkie-ai-{arch}");
        run(Command::new("xcrun")
            .args(["swiftc", "-O", "-parse-as-library", "-target"])
            .arg(format!("{arch}-apple-macos12.0"))
            .args([src, "-o", &slice]))?;
        slices.push(slice);
    }
    run(Command::new("lipo")
        .arg("-create")
        .args(&slices)
        .args(["-output", out]))
}

fn run(cmd: &mut Command) -> Result<(), String> {
    match cmd.status() {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("{:?} failed ({s})", cmd.get_program())),
        Err(e) => Err(format!("couldn't run {:?}: {e}", cmd.get_program())),
    }
}
