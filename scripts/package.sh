#!/bin/sh
# Build an ad-hoc-signed walkie.app and zip it for distribution. Used by CI
# and the release workflow; prints the zip's path on the last line.
#
#   scripts/package.sh
#
# Ad-hoc ("-") instead of the "walkie local signing" identity from
# tauri.conf.json: CI has no keychain with that cert. Users have to
# right-click → Open on first launch.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
version=$(sed -n 's/^  "version": "\(.*\)",$/\1/p' "$root/crates/walkie-app/tauri.conf.json")
arch=$(uname -m)
bundle="$root/target/release/bundle/macos"
zip="$root/target/release/bundle/walkie-$version-macos-$arch.zip"

# The UI itself is built by tauri's beforeBuildCommand; it needs its packages.
(cd "$root/crates/walkie-app/ui" && bun install --frozen-lockfile)
(cd "$root/crates/walkie-app" &&
    cargo tauri build --config '{"bundle":{"macOS":{"signingIdentity":"-"}}}')

rm -f "$zip"
# ditto keeps the bundle's symlinks, xattrs and signature intact; zip doesn't.
ditto -c -k --keepParent "$bundle/walkie.app" "$zip"
echo "$zip"
