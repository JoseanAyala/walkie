#!/bin/sh
# Builds + installs /Applications/hearme.app, then runs the OS-level e2e
# suite (e2e/tests/app*.rs) against it. Your running hearme is stopped for
# the run and restarted afterwards; your config and history aren't touched.
#
#   e2e/run-app-tests.sh              all tests
#   e2e/run-app-tests.sh hold_fn      only tests matching a name
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
was_running=$(pgrep -x hearme || true)
restore() {
    pkill -x hearme 2>/dev/null || true
    if [ -n "$was_running" ]; then open -a /Applications/hearme.app; fi
}
trap restore EXIT

# The installed app is a test-hooks build until the next `make install`.
HEARME_FEATURES=test-hooks "$root/scripts/dev.sh" --install-only
cd "$root"
# Every e2e/tests/app*.rs file is an OS-level suite.
tests=$(for f in e2e/tests/app*.rs; do printf -- '--test %s ' "$(basename "$f" .rs)"; done)
# shellcheck disable=SC2086
cargo test -p hearme-e2e --features os-tests $tests -- --test-threads=1 "$@"
