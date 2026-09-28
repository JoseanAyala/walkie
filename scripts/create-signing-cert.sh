#!/bin/sh
# Creates a self-signed code-signing identity ("walkie local signing") in the
# login keychain. Run once per machine, before `cargo tauri build`.
#
# Why: without an identity the bundle is only linker-signed ad hoc, so macOS
# identifies it by its code hash. Every rebuild is a "new app" to TCC and the
# Microphone / Accessibility / Input Monitoring grants silently stop applying,
# even though System Settings still shows the toggles on. A stable certificate
# makes the designated requirement `identifier + certificate leaf`, which
# survives rebuilds.
set -eu

NAME="walkie local signing"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"
# LibreSSL on purpose: Homebrew's OpenSSL 3 writes PKCS#12 files that
# `security import` rejects unless you pass -legacy.
OPENSSL=/usr/bin/openssl

if security find-certificate -c "$NAME" "$KEYCHAIN" >/dev/null 2>&1; then
    echo "\"$NAME\" already exists in the login keychain — nothing to do."
    exit 0
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

cat >"$tmp/cert.cnf" <<EOF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $NAME
[ext]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
EOF

"$OPENSSL" req -x509 -newkey rsa:2048 -nodes -days 3650 \
    -config "$tmp/cert.cnf" -keyout "$tmp/key.pem" -out "$tmp/cert.pem" 2>/dev/null
"$OPENSSL" pkcs12 -export -inkey "$tmp/key.pem" -in "$tmp/cert.pem" \
    -name "$NAME" -passout pass:walkie -out "$tmp/id.p12"

# -T lets codesign use the key without a keychain prompt on every build.
security import "$tmp/id.p12" -k "$KEYCHAIN" -P walkie -T /usr/bin/codesign

echo "Created \"$NAME\". Rebuild with: cd crates/walkie-app && cargo tauri build"
