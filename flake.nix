{
  description = "walkie — local-only dictation for any agent";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  # V1: dev shell only (Linux package + home-manager module land in v1.x,
  # per the spec's platform tiering). On macOS prefer rustup + system clang;
  # this shell is primarily for the NixOS machine.
  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let pkgs = import nixpkgs { inherit system; };
      in {
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            rustc cargo rustfmt clippy rust-analyzer
            cmake pkg-config
          ] ++ pkgs.lib.optionals pkgs.stdenv.isLinux [
            alsa-lib
            libxkbcommon
            gtk3
            webkitgtk_4_1
            libayatana-appindicator
            xdotool
          ];
        };
      });
}
