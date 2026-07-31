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

## Batch install

Select items in multi-select mode and install them all in one action. Even
when admin privileges are required, you are prompted only once.

!!! info "Installers run individually"
    Installers such as `.pkg` / `.exe` / `.msi` are **run individually**,
    not in a batch.

## Administrator privileges & passwords

When installing (or uninstalling) to a location that **requires administrator
privileges to write** — such as After Effects' system folders — you'll be asked
to confirm. No confirmation is needed for locations you can write to (your home
folder or custom folders).

=== "macOS"

    You're prompted for your administrator **password**.

    - The password is kept **in memory only while the app is running** and is never written to disk.
    - Enter it **once per launch**; the rest of the session installs without prompting.
    - You can lock the auth session manually from **Administrator auth** in Settings.

=== "Windows"

    A **User Account Control (UAC)** confirmation dialog appears.

    - UAC can't be cached like a password, so it appears for each operation that needs elevation.
    - For a **batch install**, a single confirmation covers the whole batch.

## Updating

After swapping files, press **Update** to **reinstall the latest files** into
the recorded install location.

## How "installed" is determined

Pluxel records install locations. Once you press the install button, the item
is treated as **installed** and the history appears in the detail panel.

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
