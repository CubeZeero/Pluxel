---
icon: material/history
---

# Changelog

Changes for each Pluxel release.

## 1.2.1

- Added `--banner <PATH>` when creating PPF packages from the command line

## 1.2.0

### Added

- macOS privileged helper with Touch ID; authentication is shared across packages for the app session
- Set banner images from the clipboard
- Include PDF manuals and other documentation for storage and distribution, without installing them into AE
- Preview contents before importing
- Check installed file integrity
- Warn about install conflicts and overwrites
- Restore earlier vault revisions, with optional reinstallation
- View activity history

### Improved & fixed

- Monochrome light and dark interface
- Simplified selection bar; bundle creation and adding to an existing bundle now share a dialog
- Search the bundle list
- Fixed stale edit fields after updates and restores
- Fixed target selection after AE detection and custom folder changes
- Protected primary assets and their parent folders from accidental deletion
- Improved recovery after install failures and handling of shared install paths
- Fixed Windows path handling during import and export
- Preserve existing output files if PPF export fails

See [Integrity, revisions & history](usage/maintenance.md), [Editing details](usage/edit-details.md) and [Appearance & colors](usage/appearance.md) for instructions.

## 1.1.0

- Create PPF packages from the command line
- Install into all detected After Effects versions at once

## 1.0.1

- **Show the absolute install path** (scripts, plugins, etc.)
- Added **copy path** and **open in Finder / Explorer** buttons
- Fixed filter chips wrapping into a lopsided second row on narrow windows
- (Windows) Fixed opening the install location opening the Documents folder instead

## 1.0.0 — Initial release

- Store scripts, plugins, and extensions in a library (drag & drop)
- Auto-detect After Effects and install by type
- Install companion files (`.ffx` presets, image folders, etc.) together
- Update / uninstall based on recorded install locations
- Batch install via multi-select
- Edit details (name, version, author, tags, banner image, install type, file structure)
- Export / import packages (`.ppf`), bundles, and full-library backups
- In-app auto-update (via GitHub Releases)
- Japanese / English UI, theme and accent color settings
- macOS (Apple Silicon) / Windows (x64)
