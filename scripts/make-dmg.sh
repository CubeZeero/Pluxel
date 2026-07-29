#!/usr/bin/env bash
#
# Build a distributable .dmg from the already-built, signed & notarized
# Pluxel.app. Tauri's bundled create-dmg (bundle_dmg.sh) fails on macOS 26, so
# we assemble a plain DMG (app + /Applications alias) with hdiutil, then sign,
# notarize and staple it. Run after `scripts/build-macos-signed.sh` has produced
# the notarized .app.
#
# Secrets come from `.env.signing` (see SIGNING.md). Without APPLE_ID the DMG is
# signed but not notarized.
#
set -euo pipefail
cd "$(dirname "$0")/.."

for f in .env.signing .env; do
  if [[ -f "$f" ]]; then set -a; . "./$f"; set +a; echo "▶ loaded $f"; break; fi
done
: "${APPLE_SIGNING_IDENTITY:?Set APPLE_SIGNING_IDENTITY (.env.signing — see SIGNING.md)}"

VER=$(node -p "require('./src-tauri/tauri.conf.json').version")
APP="src-tauri/target/release/bundle/macos/Pluxel.app"
OUT="src-tauri/target/release/bundle/dmg/Pluxel_${VER}_aarch64.dmg"
[[ -d "$APP" ]] || { echo "✖ missing $APP — run scripts/build-macos-signed.sh first"; exit 1; }

mkdir -p "$(dirname "$OUT")"
rm -f "$OUT"

STAGING=$(mktemp -d)
trap 'rm -rf "$STAGING"' EXIT
cp -R "$APP" "$STAGING/"
ln -s /Applications "$STAGING/Applications"

echo "▶ Creating DMG…"
hdiutil create -volname "Pluxel" -srcfolder "$STAGING" -ov -format UDZO "$OUT"

echo "▶ Signing DMG…"
codesign --force --sign "$APPLE_SIGNING_IDENTITY" "$OUT"

if [[ -n "${APPLE_ID:-}" ]]; then
  echo "▶ Notarizing DMG (this can take a few minutes)…"
  xcrun notarytool submit "$OUT" \
    --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" --team-id "$APPLE_TEAM_ID" --wait
  xcrun stapler staple "$OUT"
else
  echo "▶ Notarization SKIPPED (APPLE_ID unset — signed only)"
fi

echo "== spctl (Gatekeeper) =="; spctl -a -vvv -t open --context context:primary-signature "$OUT" 2>&1 | head -3 || true
echo "✔ $OUT ($(du -h "$OUT" | cut -f1))"
