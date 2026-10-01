{
  description = "walkie — local-only dictation for any agent";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  # V1: dev shell only (Linux package + home-manager module land in v1.x,
  # per the spec's platform tiering). walkie itself only *runs* on macOS
  # today; this shell exists so the workspace builds and its unit tests pass
  # on the NixOS machine too (no hotkey/ducking/etc. there yet — see
  # AGENTS.md). On macOS prefer rustup + system clang instead of this shell
  # (and the Linux-only packages/hooks below are skipped there, so the flake
  # still evaluates).
  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        isLinux = pkgs.stdenv.hostPlatform.isLinux;
        # Tauri's runtime libs (webview + tray) on Linux.
        tauriLibs = pkgs.lib.optionals isLinux (with pkgs; [
          webkitgtk_4_1
          gtk3
          librsvg
          libsoup_3
          glib
          libayatana-appindicator
        ]);
        # cpal (audio capture) and the hotkey/overlay stack's other Linux deps.
        runtimeLibs = pkgs.lib.optionals isLinux (with pkgs; [
          alsa-lib
          libxkbcommon
          wayland
          # The overlay's layer-shell surface on wlr-layer-shell compositors
          # (Hyprland, etc.) — see crates/walkie-app/src/overlay/linux.rs.
          gtk-layer-shell
        ]);
      in {
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            # Rust toolchain (rust-toolchain.toml pins the channel; mise pins
            # the exact version for CI/macOS — close enough here).
            rustc
            cargo
            rustfmt
            clippy
            rust-analyzer
            # whisper-rs: CMake build of whisper.cpp + bindgen (needs libclang).
            cmake
            pkg-config
            clang
            llvmPackages.libclang
            # The Svelte UI (crates/walkie-app/ui) and the Tauri CLI.
            bun
            cargo-tauri
            openssl
          ] ++ tauriLibs ++ runtimeLibs ++ pkgs.lib.optionals isLinux [
            xdotool
            patchelf
          ];

          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";

          shellHook = pkgs.lib.optionalString isLinux ''
            # So `cargo tauri dev`/`run` can find the webview's shared libs
            # and GTK's schemas/icons at runtime.
            export LD_LIBRARY_PATH=${pkgs.lib.makeLibraryPath (tauriLibs ++ runtimeLibs)}:$LD_LIBRARY_PATH
            export XDG_DATA_DIRS=${pkgs.gtk3}/share/gsettings-schemas/${pkgs.gtk3.name}:${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:$XDG_DATA_DIRS

            # @biomejs/biome's prebuilt x86_64-linux binary (bun installs it
            # as an optional dependency) is linked for a generic distro, not
            # NixOS: it has no /lib64/ld-linux. Re-point it at the Nix store's
            # loader/libs so `make fmt`/`make lint` work without the host
            # needing `programs.nix-ld`. Idempotent; a no-op once installed.
            biome="crates/walkie-app/ui/node_modules/@biomejs/cli-linux-x64/biome"
            if [ -f "$biome" ] && patchelf --print-interpreter "$biome" >/dev/null 2>&1; then
              patchelf \
                --set-interpreter ${pkgs.glibc}/lib/ld-linux-x86-64.so.2 \
                --set-rpath ${pkgs.lib.makeLibraryPath [ pkgs.glibc pkgs.stdenv.cc.cc.lib ]} \
                "$biome" 2>/dev/null || true
            fi
          '';
        };
      });
}
