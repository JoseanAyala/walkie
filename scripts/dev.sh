#!/bin/sh
# Build, install and relaunch /Applications/hearme.app, then follow its log.
#
#   scripts/dev.sh           build + install + launch + tail log
#   scripts/dev.sh --debug   same, with HEARME_DEBUG_EVENTS=1 (dumps key events)
#   scripts/dev.sh --install-only   build + install, don't launch (used by e2e/run-app-tests.sh)
#
# Launches through `open` on purpose: running the binary from a terminal makes
# the terminal the "responsible process", so macOS checks the *terminal's*
# Microphone / Input Monitoring grants instead of hearme's.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
app=/Applications/hearme.app
log="$HOME/Library/Logs/hearme/hearme.log"

if ! security find-certificate -c "hearme local signing" >/dev/null 2>&1; then
    "$root/scripts/create-signing-cert.sh"
fi

# The UI itself is built by tauri's beforeBuildCommand; it needs its packages.
(cd "$root/crates/hearme-app/ui" && bun install --frozen-lockfile)

# HEARME_FEATURES=test-hooks is set by e2e/run-app-tests.sh only.
(cd "$root/crates/hearme-app" && cargo tauri build ${HEARME_FEATURES:+--features "$HEARME_FEATURES"})

pkill -x hearme 2>/dev/null && sleep 0.5 || true
rm -rf "$app"
cp -R "$root/target/release/bundle/macos/hearme.app" "$app"
[ "${1:-}" = "--install-only" ] && exit 0

mkdir -p "$(dirname "$log")"
touch "$log"
lines=$(wc -l <"$log")

if [ "${1:-}" = "--debug" ]; then
    open -a "$app" --env HEARME_DEBUG_EVENTS=1
else
    open -a "$app"
fi

echo "hearme launched — following $log (Ctrl-C stops following, app keeps running)"
exec tail -n +"$((lines + 1))" -f "$log"
