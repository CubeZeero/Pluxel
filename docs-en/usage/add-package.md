---
icon: material/package-variant
---

# Adding packages

A **`.ppf` (Pluxel Package File)** is a file exported as a package from
Pluxel. It's a zip under the hood, bundling scripts or plugins together with
manifest information (name, author, version, type, and so on).

## How to add

- **Drag & drop** — Drop a `.ppf` file onto the window
- **From the menu** — Top menu **File → Add package (.ppf)**

Before importing, a preview shows the packages, bundles and included files.
Choose **Import** to confirm or cancel to leave the library unchanged.

Confirmation registers the items and their manifest information (banner, author, version, type and so on).
Normal distribution packages do not contain personal tags. A full-library backup restores tags as well.
If a duplicate exists, choose whether to skip it or replace the existing item as an update.
Importing does not execute scripts or installers.

## Packages and backups

- Individual packages can be shared and distributed as `.ppf`.
- To export your **entire** library at once, see
  [Creating packages](create-package.md).

!!! tip
    How to create a `.ppf` is covered in [Creating packages](create-package.md).
