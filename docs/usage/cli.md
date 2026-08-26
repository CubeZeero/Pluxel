---
icon: material/console
---

# コマンドラインでパッケージ作成

Pluxel は、GUI を開かずに **コマンドラインから `.ppf` パッケージを作成**できます。
ビルドや配布の自動化に便利です。生成した `.ppf` は、通常どおり
[パッケージの追加](add-package.md) から読み込めます。

## 使い方

```
pluxel package [オプション] <ファイル|フォルダ>...
```

**オプション(`--name` など)を先に書き、いちばん最後に、パッケージへ含めたいファイルやフォルダを並べます。**

```
pluxel package --name "MyEffect" --version 1.0.0 --out ./MyEffect.ppf  effect.jsx preset.ffx
```

## 含めるファイルの指定

オプションのうしろに、**入れたいファイルやフォルダをスペース区切りで並べる**だけです。複数指定できます。

```
# ファイル 1 つ
pluxel package --name Wiggle --out Wiggle.ppf  wiggle.jsx

# ファイル複数(スクリプト + 付随ファイル)
pluxel package --name Wiggle --out Wiggle.ppf  wiggle.jsx wiggle.ffx icon.png

# フォルダごと(中身を構造ごと同梱)
pluxel package --name Wiggle --out Wiggle.ppf  ./WiggleFiles/
```

- **フォルダ**を指定すると、その中身がフォルダ構造ごと同梱されます。
- パスに**スペースが含まれる**場合は引用符で囲みます: `"My Script.jsx"`。
- 指定した中に対応形式(`.jsx` など)が 1 つも無いとエラーになります(下記参照)。

## 実行ファイルの場所

=== "macOS"

    ```
    /Applications/Pluxel.app/Contents/MacOS/pluxel package …
    ```

    よく使う場合はエイリアスを設定すると便利です。

=== "Windows"

    ```
    pluxel.exe package …
    ```

## オプション

| オプション | 説明 |
| --- | --- |
| `--name <名前>` | パッケージ名(**必須**) |
| `--version <バージョン>` | バージョン(例: `1.0.0`) |
| `--author <作者>` | 作者 |
| `--description <説明>` | 説明 |
| `--homepage <URL>` | ホームページ |
| `--kind <種類>` | `script` / `script-ui-panel` / `plugin` / `zxp` / `installer`(省略時は**自動判別**) |
| `--out <パス>` | 出力先(省略時は `./<name>.ppf`) |
| `-h`, `--help` | ヘルプを表示 |

## 種類の自動判別と対応形式

`--kind` を省略すると、入力ファイルの拡張子から種類を自動判別します。

| 種類 | 拡張子 |
| --- | --- |
| スクリプト | `.jsx` / `.js` / `.jsxbin` |
| エフェクトプラグイン | `.plugin` / `.aex` |
| CEP 拡張 | `.zxp` |
| インストーラー | `.pkg` / `.exe` / `.msi` |

- 対応形式のファイルが **1 つも無い**場合はエラーになります(`--kind` で明示指定も可能)。
- `.ffx` プリセットや画像などの**付随ファイルは、対応形式のファイルと一緒に同梱**できます。

## 例

```
# スクリプト + 付随ファイルをまとめて 1 つのパッケージに
pluxel package --name "Wiggle Pro" --version 2.1.0 --author cubezeero \
  --out ./WigglePro.ppf wiggle.jsx presets/ icon.png
```
