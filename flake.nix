{
  description = "walkie — local-only dictation for any agent";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  # The dev shell works on both platforms (its Linux-only packages/hooks are
  # skipped on darwin, so the flake still evaluates there), but the `walkie`
  # package and NixOS module are Linux-only: see nix/package.nix and
  # nix/module.nix. The `packages` output below is built with a *separate*
  # `eachSystem` over just the Linux systems — not an `optionalAttrs isLinux`
  # merged into the eachDefaultSystem result — because merging attrsets
  # with `//` forces enough of the right-hand side to see its keys, which
  # would force `pkgs.stdenv` (hence `isLinux`) for every default system,
  # including darwin ones. That matters here because this nixpkgs pin has
  # dropped x86_64-darwin support outright (`import nixpkgs { system =
  # "x86_64-darwin"; }` throws as soon as its stdenv is forced) — so
  # anything that forces stdenv for that system, even just to check
  # `isLinux`, breaks evaluation there. Keeping `packages` out of the `//`
  # entirely avoids ever constructing `pkgs` for a darwin system.
  outputs = { self, nixpkgs, flake-utils }:
    let
      linuxSystems = [ "x86_64-linux" "aarch64-linux" ];
    in
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
          # whisper.cpp's Vulkan backend (GPU transcription).
          vulkan-loader
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
            # Building whisper.cpp's Vulkan shaders.
            vulkan-headers
            shaderc
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
      }) // flake-utils.lib.eachSystem linuxSystems (system:
      let
        pkgs = import nixpkgs { inherit system; };
      in {
        packages = rec {
          walkie = pkgs.callPackage ./nix/package.nix { inherit self; };
          default = walkie;
        };
      }) // {
      nixosModules.default = import ./nix/module.nix;
      # So `programs.walkie.package`'s default (`pkgs.walkie`) resolves for
      # anyone who adds this to their own `nixpkgs.overlays`; the module
      # itself doesn't force this (overlay bodies aren't evaluated just by
      # existing), so it's free even for darwin consumers.
      overlays.default = final: prev: prev.lib.optionalAttrs prev.stdenv.hostPlatform.isLinux {
        walkie = final.callPackage ./nix/package.nix { inherit self; };
      };
    };
}
