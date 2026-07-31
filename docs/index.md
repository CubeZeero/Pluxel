---
icon: lucide/rocket
---

<!-- =========================================================================
     HERO 画像はここに配置してください(例: docs/assets/hero.png を用意して
     下の「HERO」コメント部分を  ![](assets/hero.png)  に置き換え)。
     画像は cubezeero さん側で用意予定。
     ========================================================================= -->

<!-- HERO -->

# Pluxel

![](assets/hero.png)

**After Effects のスクリプト・プラグインを、ためて・並べて・入れる。**
<br>保管庫兼インストーラー — macOS / Windows 対応のデスクトップアプリ。

[ダウンロード](https://github.com/CubeZeero/Pluxel/releases){ .md-button .md-button--primary }
[GitHub](https://github.com/CubeZeero/Pluxel){ .md-button }

---

## Pluxel でできること

- **保管** — スクリプト・プラグイン・拡張をドラッグ&ドロップでライブラリに登録
- **自動インストール** — インストール済みの After Effects を自動検出し、種類に応じた正しいフォルダへ配置
- **付随ファイルもまとめて** — `.jsx` に付く `.ffx` プリセットやパネル用画像フォルダなどを同梱して一括インストール
- **更新 / アンインストール** — インストール先を記録し、後から再インストール(更新)や削除が可能
- **複数まとめてインストール** — 選択した複数のアイテムを一度の操作でインストール
- **バックアップ** — ライブラリ全体を 1 つのパッケージファイル (`.ppf`) に書き出し / 復元

## 対応ファイル形式

| 種類 | 拡張子 | インストール先 |
| --- | --- | --- |
| スクリプト | `.jsx` / `.js` / `.jsxbin` | Scripts |
| ScriptUI パネル | `.jsx` / `.jsxbin` | Scripts › ScriptUI Panels |
| エフェクトプラグイン | `.plugin` / `.aex` | Plug-ins |
| CEP 拡張 | `.zxp` | CEP Extensions |
| インストーラー | `.pkg` / `.exe` / `.msi` | 実行(個別) |

## 次のステップ

- [使い方](usage/index.md) — インストールから更新、パッケージ作成までの基本操作
