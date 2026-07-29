<div align="center">
  <img src="icon/icons_app.png" width="128" alt="Pluxel" />
  <h1>Pluxel</h1>
  <p>
    <b>Collect, organize, and install your After Effects scripts and plugins.</b><br>
    A library and installer — a desktop app for macOS and Windows
  </p>
  <p><a href="README.md">日本語</a> ・ <b>English</b></p>
</div>

---

Pluxel is a desktop app that **keeps all your After Effects scripts, effect plugins, and extensions in a single library and installs them with one click.** It centralizes scripts and plugins that tend to scatter across AE versions, and lets you track install locations, update, and uninstall.

## Features

- **Store** — Register scripts, plugins, and extensions into your library by drag & drop
- **Auto-install** — Detects installed After Effects versions and places each item in the correct folder for its type
- **Companion files included** — Bundle `.ffx` presets and panel image folders that ship alongside a `.jsx`, and install them together
- **Update / uninstall** — Records where each item was installed, so you can reinstall (update) or remove it later
- **Batch install** — Install multiple selected items in a single action (with just one admin-privilege prompt)
- **Clear overview** — Organize items aescripts-style with banner images, author, version, and tags
- **Backup** — Export your entire library to a single package file (`.ppf`) and restore it anywhere

## Supported file types

| Type | Extensions | Install target |
| --- | --- | --- |
| Script | `.jsx` / `.js` / `.jsxbin` | Scripts |
| ScriptUI panel | `.jsx` / `.jsxbin` | Scripts › ScriptUI Panels |
| Effect plugin | `.plugin` / `.aex` | Plug-ins |
| CEP extension | `.zxp` | CEP Extensions |
| Installer | `.pkg` / `.exe` / `.msi` | Run (individually) |

## Download

Grab the installer for your platform from the [**Releases**](../../releases) page.

| Platform | File |
| --- | --- |
| macOS (Apple Silicon) | `Pluxel_1.0.0_aarch64.dmg` |
| Windows (x64) | `Pluxel_1.0.0_x64-setup.exe` (or `.msi`) |

## Usage

1. **Drag & drop** script or plugin files / folders onto the window (or use the top menu **File → Add**) to register them.
2. Select an item from the list, then choose the **install target (After Effects version)** in the detail panel.
3. Press **Install** — each item is placed in the correct folder for its type.
4. Later, you can **Update** (reinstall with the latest files) or **Uninstall**.

> **Backup:** From the settings window, export your whole library to a `.ppf` file and load it on another machine to restore it.

## Development

Built with Tauri v2 (Rust) + React / TypeScript.

```bash
npm install
npm run tauri dev      # run in development
npm run tauri build    # build for distribution (.dmg / .app / .msi / .exe)
```

### Tests

```bash
cd src-tauri && cargo test --lib   # verify core logic (import → export → install)
```

## License

Copyright (C) 2026 cubezeero

This software is released under the **[GNU General Public License v3.0](LICENSE) or later** (GPL-3.0-or-later).

A list of the main libraries used and their licenses is available in the app's **About** screen.
