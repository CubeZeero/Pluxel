---
icon: material/file-plus
---

# Adding scripts & plugins

Register scripts and plugins into your Pluxel library.

## How to add

- **Drag & drop** — Drop files or folders onto the Pluxel window
- **From the menu** — Top menu **File → Add scripts & plugins**

Drop a whole folder to register its contents as a single item.

## Supported file types

| Type | Extensions |
| --- | --- |
| Script | `.jsx` / `.js` / `.jsxbin` |
| ScriptUI panel | `.jsx` / `.jsxbin` |
| Effect plugin | `.plugin` / `.aex` |
| CEP extension | `.zxp` |
| Installer | `.pkg` / `.exe` / `.msi` |

The type is detected automatically on add. If it's wrong, change the
**install type** in [Editing details](edit-details.md).

## Bundling companion files

A `.jsx` script often ships with `.ffx` presets or image folders for its
panel. Keep these **in the same item** and they are placed together at
install time.

- To add companion files later, use **Add files / Add folder** in the detail panel.
- See [Editing details](edit-details.md) for more.

## Script vs. ScriptUI panel

Even for the same `.jsx`, After Effects distinguishes files that go in
**Scripts** from those in **ScriptUI Panels**. Pluxel manages them by type
and installs each into the correct folder.
