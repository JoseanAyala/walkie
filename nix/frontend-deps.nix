# Fixed-output derivation: `bun install`s crates/walkie-app/ui's
# dependencies so the sandboxed Tauri build (no network) has a
# node_modules/ to build the Svelte UI from.
#
# To update after crates/walkie-app/ui/bun.lock changes: set `outputHash`
# below to `lib.fakeHash`, run `nix build .#walkie`, and copy the hash Nix
# reports as "got:" back in. The hash is per-system because bun installs
# platform-specific optional deps (e.g. @biomejs/cli-linux-x64).
{
  lib,
  stdenv,
  pkgs,
}:
let
  # nixos-unstable's `bun` can't read our bun.lock (format bump); see
  # bun.nix for why.
  bun = import ./bun.nix { inherit pkgs; };
in
stdenv.mkDerivation {
  pname = "walkie-ui-deps";
  version = "0.7.0";

  src = lib.fileset.toSource {
    root = ../crates/walkie-app/ui;
    fileset = lib.fileset.unions [
      ../crates/walkie-app/ui/package.json
      ../crates/walkie-app/ui/bun.lock
    ];
  };

  nativeBuildInputs = [ bun ];

  dontConfigure = true;

  buildPhase = ''
    runHook preBuild
    export HOME="$TMPDIR"
    export BUN_INSTALL_CACHE_DIR="$TMPDIR/bun-cache"
    bun install --frozen-lockfile --ignore-scripts --no-progress
    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall
    mkdir -p "$out"
    cp -R node_modules "$out"/node_modules
    runHook postInstall
  '';

  dontFixup = true;

  outputHashMode = "recursive";
  outputHash =
    {
      x86_64-linux = "sha256-nKLQwiiUFkflEWddxX/pVWCv9J3SBbCtxYNpi2aPlo4=";
      # Not yet verified on aarch64-linux (no hardware to test on); bun
      # installs some platform-specific optional deps, so this almost
      # certainly needs its own real hash — see this file's header comment.
      aarch64-linux = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
    }
    .${stdenv.hostPlatform.system} or (throw
      "walkie-ui-deps: no bun.lock outputHash pinned for ${stdenv.hostPlatform.system}");
}
