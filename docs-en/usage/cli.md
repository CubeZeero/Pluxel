---
icon: material/console
---

# Creating packages from the command line

Pluxel can **create `.ppf` packages from the command line** without opening the
GUI — handy for build and distribution automation. The generated `.ppf` imports
like any other via [Adding packages](add-package.md).

## Usage

```
pluxel package [OPTIONS] <FILE|FOLDER>...
```

**Write the options (`--name`, etc.) first, then list the files and folders to include at the very end.**

```
pluxel package --name "MyEffect" --version 1.0.0 --out ./MyEffect.ppf  effect.jsx preset.ffx
```

## Choosing which files to include

Just list the files and folders you want, separated by spaces, **after the options**. Multiple entries are allowed.

```
# A single file
pluxel package --name Wiggle --out Wiggle.ppf  wiggle.jsx

# Multiple files (a script plus companion files)
pluxel package --name Wiggle --out Wiggle.ppf  wiggle.jsx wiggle.ffx icon.png

# A whole folder (its contents are bundled, structure preserved)
pluxel package --name Wiggle --out Wiggle.ppf  ./WiggleFiles/
```

- Pass a **folder** to bundle its contents (folder structure is preserved).
- Quote paths that contain **spaces**: `"My Script.jsx"`.
- If none of the entries is a supported type (`.jsx`, etc.), it errors (see below).

## Where the executable is

=== "macOS"

    ```
    /Applications/Pluxel.app/Contents/MacOS/pluxel package …
    ```

    Set up an alias if you use it often.

=== "Windows"

    ```
    pluxel.exe package …
    ```

## Options

| Option | Description |
| --- | --- |
| `--name <NAME>` | Package name (**required**) |
| `--version <VERSION>` | Version string (e.g. `1.0.0`) |
| `--author <AUTHOR>` | Author |
| `--description <TEXT>` | Description |
| `--homepage <URL>` | Homepage URL |
| `--kind <KIND>` | `script` / `script-ui-panel` / `plugin` / `zxp` / `installer` (**auto-detected** when omitted) |
| `--out <PATH>` | Output path (defaults to `./<name>.ppf`) |
| `-h`, `--help` | Show help |

## Auto-detected kind & supported types

When `--kind` is omitted, the kind is auto-detected from the input file extensions.

| Kind | Extensions |
| --- | --- |
| Script | `.jsx` / `.js` / `.jsxbin` |
| Effect plugin | `.plugin` / `.aex` |
| CEP extension | `.zxp` |
| Installer | `.pkg` / `.exe` / `.msi` |

- If **none** of the inputs is a supported type, it errors (you can pass `--kind` explicitly).
- Companion files such as `.ffx` presets and images can be **bundled together** with a supported file.

## Example

```
# Bundle a script with its companion files into one package
pluxel package --name "Wiggle Pro" --version 2.1.0 --author cubezeero \
  --out ./WigglePro.ppf wiggle.jsx presets/ icon.png
```
