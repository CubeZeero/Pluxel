#!/usr/bin/env bash
# Build locally; sign inside-out; notarize/staple before creating updater files.
# Never enable shell tracing: the environment contains signing credentials.
set +x
set -euo pipefail
cd "$(dirname "$0")/.."

SIGN_ONLY=false
case "${1:-}" in
  '') ;;
  --sign-only) SIGN_ONLY=true; shift ;;
  *) echo 'Usage: scripts/build-macos-signed.sh [--sign-only]' >&2; exit 2 ;;
esac
[[ $# -eq 0 ]] || { echo 'Unexpected arguments' >&2; exit 2; }
[[ "$(uname -s)" == Darwin ]] || { echo 'macOS required' >&2; exit 1; }
for f in .env.signing .env; do
  if [[ -f "$f" ]]; then set -a; . "./$f"; set +a; break; fi
done
set +x
: "${APPLE_SIGNING_IDENTITY:?Set APPLE_SIGNING_IDENTITY (see SIGNING.md)}"
TEAM=BVC35LQRJR
if [[ "${APPLE_TEAM_ID:-$TEAM}" != "$TEAM" ]]; then
  echo 'APPLE_TEAM_ID must match the native helper team BVC35LQRJR' >&2; exit 1
fi
if ! $SIGN_ONLY; then
  : "${APPLE_ID:?Set APPLE_ID or use --sign-only}"
  : "${APPLE_PASSWORD:?Set APPLE_PASSWORD or use --sign-only}"
  export APPLE_TEAM_ID="$TEAM"
  # The CLI reads these via environment, never command-line secret arguments.
  if [[ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]]; then
    export TAURI_SIGNING_PRIVATE_KEY_PATH="${TAURI_SIGNING_PRIVATE_KEY_PATH:-$HOME/.tauri/pluxel-updater.key}"
    [[ -f "$TAURI_SIGNING_PRIVATE_KEY_PATH" ]] || { echo 'Updater key missing' >&2; exit 1; }
  fi
  if [[ -z "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD+x}" ]]; then
    [[ -f "$HOME/.tauri/pluxel-updater.pass" ]] || { echo 'Updater password missing (see SIGNING.md)' >&2; exit 1; }
    TAURI_SIGNING_PRIVATE_KEY_PASSWORD=$(cat "$HOME/.tauri/pluxel-updater.pass")
    export TAURI_SIGNING_PRIVATE_KEY_PASSWORD
  fi
fi

APP=src-tauri/target/release/bundle/macos/Pluxel.app
HELPER="$APP/Contents/MacOS/pluxel-helper"
PLIST="$APP/Contents/Library/LaunchDaemons/com.cubezeero.pluxel.install-helper.plist"
ARCHIVE="${APP}.tar.gz"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
# Remove stale updater files so failed/sign-only runs cannot leave a release pair.
rm -f "$ARCHIVE" "$ARCHIVE.sig"
echo '▶ Building unsigned app (automatic updater artifacts disabled)'
npm run tauri -- build --bundles app --no-sign --config '{"bundle":{"createUpdaterArtifacts":false}}'
[[ -f "$HELPER" && -f "$PLIST" ]] || { echo 'Embedded helper or LaunchDaemon plist missing' >&2; exit 1; }
[[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleName' "$APP/Contents/Info.plist")" == Pluxel ]] || {
  echo 'Production application name must be Pluxel' >&2; exit 1;
}
plutil -lint "$PLIST"
echo '▶ Signing helper, then outer app with hardened runtime'
codesign --force --sign "$APPLE_SIGNING_IDENTITY" --options runtime --timestamp \
  --identifier com.cubezeero.pluxel.install-helper "$HELPER"
codesign --verify --strict --verbose=2 "$HELPER"
codesign --force --sign "$APPLE_SIGNING_IDENTITY" --options runtime --timestamp "$APP"
# Do not use --deep for signing: it can overwrite nested designated requirements.
codesign --verify --deep --strict --verbose=2 "$APP"
for target in "$HELPER" "$APP"; do
  metadata=$(codesign -d --verbose=4 "$target" 2>&1)
  expected=com.cubezeero.pluxel
  [[ "$target" != "$HELPER" ]] || expected=com.cubezeero.pluxel.install-helper
  [[ "$metadata" == *"Identifier=$expected"$'\n'* && "$metadata" == *"TeamIdentifier=$TEAM"$'\n'* && "$metadata" == *'(runtime)'* ]] || {
    echo 'Unexpected signing identifier, team, or hardened runtime' >&2; exit 1;
  }
  codesign --verify --strict -R="anchor apple generic and identifier \"$expected\" and certificate leaf[subject.OU] = \"$TEAM\"" "$target"
  echo "✔ $expected / team $TEAM / hardened runtime"
done
if $SIGN_ONLY; then
  echo "✔ Signed and locally verified: $APP (not notarized; no updater artifacts)"
  exit 0
fi

echo '▶ Submitting complete signed app for notarization'
ditto -c -k --keepParent "$APP" "$WORK/Pluxel.zip"
# Capture all notary output privately; print only submission metadata, never credentials.
if ! xcrun notarytool submit "$WORK/Pluxel.zip" --apple-id "$APPLE_ID" \
  --password "$APPLE_PASSWORD" --team-id "$TEAM" --wait --output-format json \
  > "$WORK/notary.json" 2> "$WORK/notary.err"; then
  case "$(cat "$WORK/notary.err") $(cat "$WORK/notary.json")" in
    *403*|*'required agreement'*|*'public developer contract'*)
      echo 'Notarization blocked by Apple Developer agreement (HTTP 403); no updater artifacts. See SIGNING.md.' >&2 ;;
    *) echo 'Notarization failed; no updater artifacts. Check credentials and Developer agreements (see SIGNING.md).' >&2 ;;
  esac
  exit 1
fi
node - "$WORK/notary.json" <<'NODE'
const fs = require('fs');
const result = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
if (typeof result.id === 'string' && /^[0-9a-f-]+$/i.test(result.id)) console.log(`Submission: ${result.id}`);
if (result.status !== 'Accepted') { console.error('Notarization was not Accepted; no updater artifacts.'); process.exit(1); }
console.log('Notarization: Accepted');
NODE
xcrun stapler staple "$APP"
xcrun stapler validate "$APP"
codesign --verify --deep --strict --verbose=2 "$APP"
spctl --assess --type execute --verbose=2 "$APP"

echo '▶ Creating updater archive from stapled app'
tar -czf "$WORK/Pluxel.app.tar.gz" -C "$(dirname "$APP")" "$(basename "$APP")"
# Signer stdout includes signature text; keep it private and report only file paths.
npm run tauri -- signer sign "$WORK/Pluxel.app.tar.gz" > "$WORK/signer.log" 2>&1
[[ -s "$WORK/Pluxel.app.tar.gz.sig" ]] || { echo 'Updater signature missing' >&2; exit 1; }
mv "$WORK/Pluxel.app.tar.gz" "$ARCHIVE"
mv "$WORK/Pluxel.app.tar.gz.sig" "$ARCHIVE.sig"
echo "✔ Notarized/stapled: $APP"
echo "✔ Updater: $ARCHIVE + .sig"
echo 'DMG is a separate later step: scripts/make-dmg.sh'
