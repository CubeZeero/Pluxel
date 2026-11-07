---
icon: material/shield-check
---

# Integrity, revisions & history

Vault files and files installed into After Effects are managed separately.

## Check installed file integrity

1. Choose **Check integrity** in an item's detail panel.
2. Review each install location. To check the entire library, use the management section in Settings.

| Result | Meaning |
| --- | --- |
| Match | Matches the contents recorded at installation |
| Missing | A recorded file cannot be found |
| Changed | Contents or structure differ from the recorded baseline |
| Unverified | No comparison baseline has been recorded yet |
| Unreadable | Permissions or another read error prevented checking |
| Unsupported | Pluxel cannot inspect the installed contents, as with external installers |

Items installed by an earlier version may appear as **Unverified**. Reinstalling records a baseline for future checks.
A check does not repair files. Use Update in the detail panel when needed.

## Install conflict warnings

Before overwriting existing files or paths owned by another package, Pluxel asks you to review the conflict.
Installed files that were modified or have no baseline also require confirmation.

- Check the paths and ownership, then overwrite or cancel.
- If selected packages conflict with each other, you cannot continue with that selection. Change the selection first.
- Conflict approval is separate from the operating system's administrator confirmation.

## Restore an earlier vault revision

Pluxel saves the current vault revision before file replacement, package updates and certain other changes.

1. Open **Vault revisions** in the item's detail panel.
2. Review the version, save time and reason, then select a revision.
3. Choose **Restore this vault revision**.
4. Optionally select **Reinstall to recorded install locations**.

Without reinstallation, only vault files change; installed AE files remain untouched.
The current revision is saved before restoration so you can restore it again later. Your tags and current install records are preserved.
If reinstallation is cancelled or fails, the vault restoration and the installed-file result are reported separately.

!!! note "Restoring installers"
    For `.pkg` / `.exe` / `.msi`, only the stored installer files are restored.
    Software already installed into the OS is not rolled back. Run the restored installer individually if needed.

Automatic revisions do not replace a full-library backup.
Use [Backing up the whole library](create-package.md#backing-up-the-whole-library) to move between machines or protect against loss of the vault itself.

## Activity history

Open **Activity history** in the management section of Settings to review imports, installs, updates,
uninstalls, revision restores, file additions and removals, and other recorded operations.
The latest 1,000 events are kept in newest-first order. Viewing history does not undo an operation.
