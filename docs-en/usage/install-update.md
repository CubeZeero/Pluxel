---
icon: material/cog-play
---

# Install & update

## Install into After Effects

1. Select an item from your library and open the detail panel.
2. Choose the **install target (After Effects version)**.
   Installed After Effects versions are detected automatically.
3. Press **Install** — the item is placed in the correct folder for its type.

| Type | Install target |
| --- | --- |
| Script | Scripts |
| ScriptUI panel | Scripts › ScriptUI Panels |
| Effect plugin | Plug-ins |
| CEP extension | CEP Extensions |
| Installer | Run (individually) |

!!! note "Custom folders"
    Add a custom install folder in Settings to place items somewhere other
    than After Effects.

!!! tip "Install into every After Effects at once"
    When several After Effects versions are detected, choose
    **"All After Effects"** as the target to install into all of them at once
    (normally with a single confirmation even when administrator privileges are needed).

## Batch install

Select items in multi-select mode and install them all in one action. Even
when administrator privileges are required, a single confirmation normally covers the operation.

!!! info "Installers run individually"
    Installers such as `.pkg` / `.exe` / `.msi` are **run individually**,
    not in a batch.

## Administrator privileges & passwords

When installing (or uninstalling) to a location that **requires administrator
privileges to write** — such as After Effects' system folders — you'll be asked
to confirm. No confirmation is needed for locations you can write to (your home
folder or custom folders).

=== "macOS"

    From v1.2.0, you can use a privileged helper with **Touch ID** on macOS 13 or later.

    1. Install the app at **/Applications/Pluxel.app**.
    2. In Pluxel, choose **Settings → Administrator auth → Set up helper**.
    3. Allow **Pluxel** in **System Settings → General → Login Items & Extensions**. Initial administrator approval is required.
    4. Choose **Refresh status** in Pluxel and confirm that the helper is **Enabled**.

    - The first administrator operation per launch asks for Touch ID or the OS password.
    - Further installs, updates and removals across packages require no additional authentication until you quit the app or manually lock the session.
    - Helper mode keeps only an authorization flag in memory; Pluxel does not store your password or biometric data.
    - Use **Lock authentication session** to lock manually, or **Remove helper** to return to the legacy mode.

    Privileged helper writes are limited to standard AE **Scripts / Plug-ins** destinations, including ScriptUI Panels and Effects subfolders.
    Snapshots are limited to **128 MiB** per batch. For protected custom destinations, mixed CEP batches or larger snapshots, remove the helper and use legacy authorization.
    User-writable destinations do not need administrator authorization.

    Without the helper, the legacy mode can keep your administrator password in memory for the app session.
    External installers such as `.pkg` may present their own separate authentication prompt.

=== "Windows"

    A **User Account Control (UAC)** confirmation dialog appears.

    - UAC can't be cached like a password, so it appears for each operation that needs elevation.
    - For a **batch install**, a single confirmation covers the whole batch.

## Updating

After swapping files, press **Update** to **reinstall the latest files** into
the recorded install location.

## How "installed" is determined

For scripts and plugins, Pluxel records the location after files are successfully placed.
External installers are marked as installed once they are successfully launched.
Pluxel cannot verify what an external installer installed or whether its setup completed.

## Checking the install location

For installed items, the **absolute path** where the files were placed is
shown in the detail panel.

- **:material-content-copy: Copy** — Copy the path to the clipboard
- **:material-folder-open: Open** — Open the folder in Finder (macOS) /
  Explorer (Windows) with the file selected

The same actions are available for each entry under "Other install locations".

## Uninstalling

From the install history in the detail panel, uninstall per location. The
placed files are removed and the record is cleared.

## Overwrites, integrity and older revisions

Existing files and install conflicts require confirmation before overwriting.
See [Integrity, revisions & history](maintenance.md) to check for modified or missing files and to reinstall an older revision.

PDF manuals and similar documentation are not copied into AE.
You may store `.aex` on macOS or `.plugin` on Windows, but cannot install a platform-specific effect on the other OS.

## Multi-select mode

The selection bar shows the selection count, Clear, Install and Bundle actions.
Choose **Bundle actions**, then create a new bundle or add the selection to an existing bundle.
See [Creating packages](create-package.md) for details.
