---
icon: material/package-up
---

# Creating packages

Export a registered item as a **`.ppf` (Pluxel Package File)** to share,
distribute, or back up. A `.ppf` is a zip that includes the files and manifest
information (name, author, version, type, and so on).

## Exporting a single package

From the detail panel, export the item as `.ppf`. For distribution, internal
data such as tags is omitted from the output.

## Bundles (multiple items)

Group several items into one bundle and export it as `.ppf`.

- Bundleable types: **Script / ScriptUI panel / Effect plugin / CEP extension**
- **Installers** (`.pkg` / `.exe` / `.msi`) cannot be bundled (handled individually)

## Backing up the whole library

From the Settings window, export your **entire** library as a single package
(`.ppf` / zip). Import it on another machine via [Adding packages](add-package.md)
to restore everything.

## Importing

To import an exported `.ppf`, see [Adding packages](add-package.md).
