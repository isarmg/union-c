#!/bin/sh
set -eu

: "${BINARY:?set BINARY to the signed universal unionc-agent binary}"
: "${VERSION:?set VERSION}"

root="$(mktemp -d)"
trap 'rm -rf "$root"' EXIT
install -d "$root/usr/local/libexec" "$root/Library/LaunchDaemons" \
  "$root/Library/Application Support/UnionC Agent"
install -m 0755 "$BINARY" "$root/usr/local/libexec/unionc-agent"
install -m 0644 packaging/macos/com.unionc.agent.plist \
  "$root/Library/LaunchDaemons/com.unionc.agent.plist"
install -m 0600 config.example.json \
  "$root/Library/Application Support/UnionC Agent/config.example.json"

pkgbuild --root "$root" --scripts packaging/macos/scripts \
  --identifier com.unionc.agent --version "$VERSION" \
  "unionc-agent-$VERSION.pkg"
