#!/bin/sh
# Build an ad-hoc-signed hearme.app and zip it for distribution. Used by CI
# and the release workflow; prints the zip's path on the last line.
#
#   scripts/package.sh
#
# Ad-hoc ("-") instead of the "hearme local signing" identity from
# tauri.conf.json: CI has no keychain with that cert. Users have to
# right-click → Open on first launch.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
version=$(sed -n 's/^  "version": "\(.*\)",$/\1/p' "$root/crates/hearme-app/tauri.conf.json")
arch=$(uname -m)
bundle="$root/target/release/bundle/macos"
zip="$root/target/release/bundle/hearme-$version-macos-$arch.zip"

(cd "$root/crates/hearme-app" &&
    cargo tauri build --config '{"bundle":{"macOS":{"signingIdentity":"-"}}}')

rm -f "$zip"
# ditto keeps the bundle's symlinks, xattrs and signature intact; zip doesn't.
ditto -c -k --keepParent "$bundle/hearme.app" "$zip"
echo "$zip"
