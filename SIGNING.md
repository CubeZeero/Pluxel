# macOS packaging, signing and notarization

Run this workflow on a Mac with a Developer ID Application certificate in the
keychain. Credentials belong in the ignored `.env.signing` at the repository
root (fallback: `.env`), or the environment. Never commit them or run with shell
tracing. The script does not print Apple ID/password or raw notarization output.

```sh
APPLE_SIGNING_IDENTITY="<Developer ID Application certificate name or SHA-1>"
APPLE_ID="<Apple ID>"
APPLE_PASSWORD="<app-specific password>"
APPLE_TEAM_ID="BVC35LQRJR"
```

Use `security find-identity -v -p codesigning` to find the certificate. If names
are ambiguous, use its SHA-1. Create an app-specific password in your Apple
account. The signing certificate must belong to team `BVC35LQRJR`, matching the
native helper's compiled trust requirements.

## Local build without notarization

```sh
./scripts/build-macos-signed.sh --sign-only
```

This builds a fresh app, signs the embedded helper first and the outer app
second with hardened runtime and secure timestamps, then strictly verifies
both signatures, identifiers and team requirements. It skips notarization and
creates no updater archive/signature. Signing timestamps may require network
access. A locally valid signature does not establish Gatekeeper approval or
successful privileged-helper registration.

The existing app/config version is 1.2.0; the script does not bump versions,
commit, publish or push. It replaces the app at
`src-tauri/target/release/bundle/macos/Pluxel.app`. Each run removes any old
`Pluxel.app.tar.gz` and `.sig` at that location to avoid stale release artifacts.

## Full packaging

Updater signing uses the existing `~/.tauri/pluxel-updater.key` and
`~/.tauri/pluxel-updater.pass`. Overrides use the CLI environment names:
`TAURI_SIGNING_PRIVATE_KEY` (key contents), `TAURI_SIGNING_PRIVATE_KEY_PATH`
(key file), and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (password, including an
explicit empty value for an unencrypted key). The password file is read only
when the password environment variable is unset. Secrets are never passed to
the updater signer on its command line.

```sh
./scripts/build-macos-signed.sh
# Separate later step, after successful app notarization:
./scripts/make-dmg.sh
```

The build command is:

```sh
npm run tauri -- build --bundles app --no-sign --config '{"bundle":{"createUpdaterArtifacts":false}}'
```

Credentials remain available, but `--no-sign` disables Tauri's automatic signing
and notarization. `build.rs` builds `target/native-helper/pluxel-helper` and the
bundle configuration embeds it at `Contents/MacOS/pluxel-helper`, alongside
`Contents/Library/LaunchDaemons/com.cubezeero.pluxel.install-helper.plist`.
Manual inside-out signing handles the extra helper explicitly, using its
identifier `com.cubezeero.pluxel.install-helper`. No recursive `--deep` signing
is used; strict recursive verification is mandatory and failures stop the run.

The complete signed app is zipped and submitted with `notarytool --wait`.
Only an explicit `Accepted` result allows stapling, staple validation,
strict codesign verification and Gatekeeper assessment. Only after those pass
is the stapled app archived as `Pluxel.app.tar.gz` and signed using
`npm run tauri -- signer sign`. Final archive/signature files are placed next
to the app. Failure stops the workflow; no unnotarized updater is produced.
The separate, unchanged `make-dmg.sh` creates/signs/notarizes/staples the DMG;
the app script does not call it automatically.

## Notarization status and troubleshooting

On 2026-11-07, after the account holder accepted the updated agreement, both
the 1.2.0 app and DMG were accepted by Apple's notarization service. Stapling
and Gatekeeper validation passed for both, including the installed app copy.

If Apple returns **403 / missing public developer contract**, the account
holder must check pending agreements and related account requirements in the
Apple Developer portal. There is no signing flag or local bypass for Apple's
notarization service. `--sign-only` supports local signature validation but
does not replace notarization.

A failed submission prints a generic failure without credential-bearing logs.
Successful submissions print only their ID and Accepted status. For a rejected
submission, inspect its notarization log privately with `xcrun notarytool log`
and your credentials; never paste secrets into issue reports. Fix the cause and
rerun the full script; it does not reuse approval for an older app.

Syntax-only validation (does not build or sign):

```sh
bash -n scripts/build-macos-signed.sh
```

Windows Authenticode signing remains a separate workflow.

## Helper authentication

The helper remains registered until the user removes it in Settings. Pluxel
uses LocalAuthentication on the first privileged operation in each app process.
After success, an in-memory flag permits further installs and removals across
packages without another prompt. Quitting the app, manually locking its
authentication session, or removing helper approval clears that flag. Passwords,
biometric data and authentication tokens are never persisted by this helper
mode. Installing a native external installer still uses that installer's own
OS authorization flow; it is not covered by this session.

The installed application is `/Applications/Pluxel.app`. Its background item
uses the application's normal name, Pluxel. The helper's stable service label
and signing identifier remain `com.cubezeero.pluxel.install-helper`; do not
rename these when replacing an older local test app. Unregister the helper
from the old app first, quit it, then install and register from Pluxel.app.
Keep a backup of any previous installed app before replacing it. Archives of
older local test apps should be kept outside Applications to avoid duplicate
Launch Services entries.

The helper intentionally limits administrator writes to validated standard AE
Scripts and Plug-ins destinations, with bounded 128 MiB in-memory snapshots.
User-writable installs continue without elevation. For protected custom paths,
mixed CEP batches or larger snapshots, disable the helper and use the legacy
administrator authorization path. These boundaries are security/resource
policies; they must not be removed merely to change an application label.
