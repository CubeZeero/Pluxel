# macOS code signing & notarization

Signing + notarizing lets users open Pluxel without the "unidentified
developer" block. The app uses **no restricted entitlements** (no keychain
sharing, sandbox, etc.), so no provisioning profile or custom entitlements are
needed — plain Developer ID signing + notarization is enough.

Signing happens on **your Mac** with **your** certificate. No secrets live in
this repo; they're passed via environment variables (or a git-ignored
`.env.signing`) at build time.

## One-time setup

1. **Certificate** — a *Developer ID Application* certificate in your login
   keychain (Apple Developer Program). List what you have:

   ```sh
   security find-identity -v -p codesigning
   ```

   If two rows share the same name, use the **SHA-1 hash** (first column) as the
   identity to avoid an "ambiguous identity" error.

2. **App-specific password** for notarization — create one at
   <https://appleid.apple.com> → Sign-In & Security → App-Specific Passwords.

## Build

Put your values in a git-ignored `.env.signing` at the repo root:

```sh
APPLE_SIGNING_IDENTITY="<Developer ID Application: cert SHA-1 hash or name>"
APPLE_ID="you@example.com"
APPLE_PASSWORD="abcd-efgh-ijkl-mnop"    # app-specific password
APPLE_TEAM_ID="XXXXXXXXXX"              # your 10-char Team ID
```

then:

```sh
./scripts/build-macos-signed.sh
```

Tauri signs, notarizes and staples automatically. Omit `APPLE_ID` /
`APPLE_PASSWORD` / `APPLE_TEAM_ID` to sign only (no network). Artifacts land in
`src-tauri/target/release/bundle/` (`.app`, `.dmg`).

## Troubleshooting

- `ambiguous (matches N identities)` → use the certificate's SHA-1 hash for
  `APPLE_SIGNING_IDENTITY`.
- Notarization rejected → `xcrun notarytool log <submission-id>`.

## Windows

Windows signing is separate (Authenticode, an EV/OV certificate) and optional —
it removes SmartScreen warnings. Not covered here.
