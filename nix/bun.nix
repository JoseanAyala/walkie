# nixos-unstable's `bun` (1.3.13 as of this writing) is older than what
# crates/walkie-app/ui/bun.lock was written with (mise.toml pins bun
# 1.4.2): bun bumped its lockfile format ("lockfileVersion": 2) in a way
# 1.3.13 can't read, so `bun install --frozen-lockfile` fails outright on
# it ("Unknown lockfile version"). Rather than wait for nixpkgs to catch
# up, grab the upstream release directly the same way nixpkgs' own bun
# package does (pkgs/by-name/bu/bun/package.nix) — just a newer version.
#
# To bump: update `version` and re-fetch each hash, e.g.
#   nix-prefetch-url --type sha256 \
#     https://github.com/oven-sh/bun/releases/download/bun-v<version>/bun-linux-x64-baseline.zip
# (and the aarch64 build for completeness). Once nixpkgs' own `bun` is at
# least this new, this file can go away and callers can go back to the
# plain `pkgs.bun`.
{ pkgs }:

pkgs.bun.overrideAttrs (old: {
  version = "1.4.2";
  __intentionallyOverridingVersion = true;
  passthru = old.passthru // {
    sources = {
      x86_64-linux = pkgs.fetchurl {
        url = "https://github.com/oven-sh/bun/releases/download/bun-v1.4.2/bun-linux-x64-baseline.zip";
        sha256 = "13vjpyvam9p6gp4nm03jv8r1l1acrv8cndwxhgml017y2h7h8y66";
      };
      aarch64-linux = pkgs.fetchurl {
        url = "https://github.com/oven-sh/bun/releases/download/bun-v1.4.2/bun-linux-aarch64.zip";
        sha256 = "19zxr0d8yxc1xvjhjjg3750vi0vsqmk4sm1ci6ghr3lw5ny8ncjl";
      };
    };
  };
})
