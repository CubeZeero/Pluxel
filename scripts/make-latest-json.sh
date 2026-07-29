#!/usr/bin/env bash
#
# Generate `latest.json` for the Tauri updater from both platforms' build
# artifacts. The updater fetches this file from the GitHub Release and compares
# its `version` against the running app.
#
# Collect these files (from `npm run tauri build` on each platform, built with
# the updater signing key) into one staging directory, then run this script:
#
#   macOS  : Pluxel.app.tar.gz            + Pluxel.app.tar.gz.sig
#   Windows: Pluxel_<ver>_x64-setup.exe   + Pluxel_<ver>_x64-setup.exe.sig
#
# Usage:
#   scripts/make-latest-json.sh <staging-dir> ["release notes"]
#
# Writes <staging-dir>/latest.json. Upload it — together with the two artifacts
# above — as assets of the `v<ver>` GitHub Release.
#
set -euo pipefail
cd "$(dirname "$0")/.."

DIR="${1:?Usage: make-latest-json.sh <staging-dir> [notes]}"
NOTES="${2:-}"
REPO="CubeZeero/Pluxel"

VER=$(node -p "require('./src-tauri/tauri.conf.json').version")
BASE="https://github.com/$REPO/releases/download/v$VER"
PUBDATE=$(date -u +%Y-%m-%dT%H:%M:%SZ)

MAC_SIG="$DIR/Pluxel.app.tar.gz.sig"
WIN_SIG="$DIR/Pluxel_${VER}_x64-setup.exe.sig"
[[ -f "$MAC_SIG" ]] || { echo "✖ missing $MAC_SIG (build macOS with the updater key)"; exit 1; }
[[ -f "$WIN_SIG" ]] || { echo "✖ missing $WIN_SIG (build Windows with the updater key)"; exit 1; }

node -e '
const fs = require("fs");
const [dir, ver, base, notes, pub] = process.argv.slice(1);
const sig = (p) => fs.readFileSync(p, "utf8").trim();
const out = {
  version: ver,
  notes: notes || `Pluxel ${ver}`,
  pub_date: pub,
  platforms: {
    "darwin-aarch64": {
      signature: sig(`${dir}/Pluxel.app.tar.gz.sig`),
      url: `${base}/Pluxel.app.tar.gz`,
    },
    "windows-x86_64": {
      signature: sig(`${dir}/Pluxel_${ver}_x64-setup.exe.sig`),
      url: `${base}/Pluxel_${ver}_x64-setup.exe`,
    },
  },
};
fs.writeFileSync(`${dir}/latest.json`, JSON.stringify(out, null, 2) + "\n");
' "$DIR" "$VER" "$BASE" "$NOTES" "$PUBDATE"

echo "✔ wrote $DIR/latest.json (v$VER)"
echo "  darwin-aarch64 → $BASE/Pluxel.app.tar.gz"
echo "  windows-x86_64 → $BASE/Pluxel_${VER}_x64-setup.exe"
