#!/bin/sh
# Cut a release: bump the version, check, commit, tag, push. The tag push
# triggers .github/workflows/release.yml, which builds and publishes.
#
#   scripts/release.sh 0.2.0      (or: make release VERSION=0.2.0)
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
conf=crates/walkie-app/tauri.conf.json
current=$(sed -n 's/^  "version": "\(.*\)",$/\1/p' "$conf")

version=${1:-}
if [ -z "$version" ]; then
    echo "usage: make release VERSION=x.y.z   (current: $current)" >&2
    exit 1
fi
if ! echo "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
    echo "error: VERSION must look like 1.2.3, got '$version'" >&2
    exit 1
fi
tag="v$version"

[ "$(git branch --show-current)" = main ] || { echo "error: not on main" >&2; exit 1; }
[ -z "$(git status --porcelain)" ] || { echo "error: working tree not clean" >&2; exit 1; }
git fetch --quiet --tags origin
[ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] ||
    { echo "error: main isn't in sync with origin/main" >&2; exit 1; }
if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
    echo "error: tag $tag already exists" >&2
    exit 1
fi

sed -i '' "s/^  \"version\": \"$current\",$/  \"version\": \"$version\",/" "$conf"
# First `version =` in the manifest is the package's own.
sed -i '' "1,/^version = /s/^version = \".*\"$/version = \"$version\"/" crates/walkie-app/Cargo.toml

make check # also refreshes Cargo.lock

git commit -qam "release: $tag"
git tag -a "$tag" -m "walkie $tag"

printf 'Push main and %s to origin (starts the release build)? [y/N] ' "$tag"
read -r answer
if [ "$answer" != y ] && [ "$answer" != Y ]; then
    echo "Not pushed. Undo with: git tag -d $tag && git reset --hard HEAD~1"
    exit 1
fi
git push --atomic origin main "$tag"
echo "Released $tag — watch it with: gh run watch \$(gh run list -w Release -L1 --json databaseId -q '.[0].databaseId')"
