#!/usr/bin/env bash
#
# Build, sign, notarize and staple the macOS app. Tauri does all of it natively
# from these environment variables — no provisioning profile or custom
# entitlements are required (the app uses no restricted capabilities).
#
# Secrets come from the environment / a git-ignored `.env.signing` — never
# commit them. See SIGNING.md.
#   APPLE_SIGNING_IDENTITY   Developer ID Application identity (name or SHA-1)
#   APPLE_ID                 Apple ID email            (required to notarize)
#   APPLE_PASSWORD           app-specific password     (required to notarize)
#   APPLE_TEAM_ID            Team ID (e.g. BVC35LQRJR) (required to notarize)
#
set -euo pipefail
cd "$(dirname "$0")/.."

for f in .env.signing .env; do
  if [[ -f "$f" ]]; then set -a; . "./$f"; set +a; echo "▶ loaded $f"; break; fi
done

: "${APPLE_SIGNING_IDENTITY:?Set APPLE_SIGNING_IDENTITY (env or .env.signing — see SIGNING.md)}"
if [[ -n "${APPLE_ID:-}" ]]; then
  echo "▶ Notarization: enabled (Apple ID $APPLE_ID, team ${APPLE_TEAM_ID:-?})"
else
  echo "▶ Notarization: SKIPPED (APPLE_ID unset — local signing only)"
fi

# Tauri signs with APPLE_SIGNING_IDENTITY and, when APPLE_ID/APPLE_PASSWORD/
# APPLE_TEAM_ID are set, notarizes + staples automatically.
npm run tauri build

APP="src-tauri/target/release/bundle/macos/Pluxel.app"
if [[ -d "$APP" ]]; then
  echo "== codesign --verify =="; codesign --verify --deep --strict --verbose=2 "$APP" || true
  echo "== spctl (Gatekeeper) assessment =="; spctl -a -vvv "$APP" || true
fi
echo "✔ Done. Artifacts under src-tauri/target/release/bundle/"
