# The `walkie` Tauri app, packaged with `cargo-tauri.hook` (nixpkgs'
# blessed way to build Tauri 2 apps: it drives `cargo tauri build` itself,
# which both compiles the Rust side and bundles the webview assets,
# instead of us reimplementing that). See nix/frontend-deps.nix for how
# the Svelte UI's bun dependencies get into the sandbox, and AGENTS.md for
# the crate layout this mirrors (walkie-core/walkie-app/e2e workspace,
# cargo-tauri.hook bundling the externalBin walkie-ai stub, etc).
{
  lib,
  stdenv,
  pkgs,
  self,
  rustPlatform,
  cargo-tauri,
  pkg-config,
  cmake,
  jq,
  wrapGAppsHook3,
  webkitgtk_4_1,
  gtk3,
  librsvg,
  libsoup_3,
  glib,
  libayatana-appindicator,
  gtk-layer-shell,
  alsa-lib,
  libxkbcommon,
  wayland,
  vulkan-loader,
  vulkan-headers,
  shaderc,
  wireplumber,
  nodejs,
}:
let
  bun = import ./bun.nix { inherit pkgs; };
  frontendDeps = pkgs.callPackage ./frontend-deps.nix { };

  # Compile-time *and* runtime libs: cargo needs their headers/.pc files to
  # link (whisper-rs's Vulkan backend, cpal/alsa, enigo/gtk-layer-shell's
  # Wayland bits, tauri's webview+tray), and the wrapped binary needs the
  # .so's findable at runtime. wrapGAppsHook3 only auto-discovers GTK/GIO
  # concerns (schemas, modules, icon themes) from these; the rest get an
  # explicit --prefix LD_LIBRARY_PATH below.
  runtimeLibs = [
    webkitgtk_4_1
    gtk3
    librsvg
    libsoup_3
    glib
    libayatana-appindicator
    gtk-layer-shell
    alsa-lib
    libxkbcommon
    wayland
    vulkan-loader
    # CMake's FindVulkan.cmake resolves Vulkan_INCLUDE_DIR via
    # CMAKE_PREFIX_PATH, which nix's cmake setup hook only populates from
    # buildInputs (host-platform deps), not nativeBuildInputs — so this
    # needs to live here, not next to shaderc below.
    vulkan-headers
  ];
in
rustPlatform.buildRustPackage (finalAttrs: {
  pname = "walkie";
  version = "0.7.0";

  # The flake's own (git-filtered, so no target/, node_modules/, dist/,
  # crates/walkie-app/{binaries,gen}/ — see .gitignore) source tree.
  src = self;

  # Cargo.lock lives at the workspace root (not inside crates/walkie-app),
  # same file the sandboxed build sees under `src`; no git deps in it today
  # (checked via `grep git+ Cargo.lock`), so no outputHashes are needed.
  cargoLock.lockFile = ../Cargo.lock;
  # `cargo tauri build` is run from crates/walkie-app (where tauri.conf.json
  # lives); this also tells rustPlatform's build/check hooks the same.
  buildAndTestSubdir = "crates/walkie-app";

  # Unit tests assume real hardware (an audio device, a Wayland/X11
  # session, /dev/input) that the build sandbox doesn't have; `make check`
  # (CI) is where those run. `nix build` is about producing the bundle.
  doCheck = false;

  nativeBuildInputs = [
    pkg-config
    cmake
    jq
    cargo-tauri.hook
    rustPlatform.bindgenHook # whisper-rs-sys's bindgen needs libclang
    shaderc # whisper.cpp's Vulkan backend compiles its shaders with glslc
    wrapGAppsHook3
    # Only so `patchShebangs` has a `node` to repoint node_modules/.bin's
    # `#!/usr/bin/env node` scripts (vite, etc.) at; the UI itself only
    # ever runs under bun (bun run build), never this interpreter.
    nodejs
  ];

  buildInputs = runtimeLibs;

  # .cargo/config.toml (checked in) already sets GGML_NATIVE=OFF for
  # whisper-rs's cmake build; nothing extra needed here.

  postPatch = ''
    # tauri.conf.json's beforeBuildCommand shells out to `git
    # rev-parse --show-toplevel` and `bun run build` — no network and no
    # .git in the sandbox. We build the UI ourselves in preBuild instead
    # (frontendDeps' prefetched node_modules), so just drop it.
    jq 'del(.build.beforeBuildCommand, .build.beforeDevCommand)' \
      crates/walkie-app/tauri.conf.json > crates/walkie-app/tauri.conf.json.tmp
    mv crates/walkie-app/tauri.conf.json.tmp crates/walkie-app/tauri.conf.json
  '';

  preBuild = ''
    pushd crates/walkie-app/ui
    cp -R ${frontendDeps}/node_modules .
    chmod -R u+w node_modules
    patchShebangs node_modules
    HOME="$TMPDIR" ${bun}/bin/bun run build
    popd
  '';

  preFixup = ''
    gappsWrapperArgs+=(
      --prefix LD_LIBRARY_PATH : "${lib.makeLibraryPath runtimeLibs}"
      # Volume ducking shells out to `wpctl` (see
      # crates/walkie-core/src/audio/duck/linux.rs); it's looked up on PATH.
      --prefix PATH : "${wireplumber}/bin"
    )
  '';

  meta = {
    description = "Local-only dictation for any agent";
    homepage = "https://github.com/JoseanAyala/walkie";
    license = lib.licenses.mit;
    mainProgram = "walkie";
    platforms = lib.platforms.linux;
  };
})
