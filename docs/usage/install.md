---
icon: material/download
---

# アプリのインストール

[Releases](https://github.com/CubeZeero/Pluxel/releases) ページから、お使いのプラットフォーム向けのインストーラをダウンロードしてください。

=== "macOS"

    対象: Apple Silicon (aarch64)

    1. `Pluxel_x.y.z_aarch64.dmg` をダウンロード
    2. `.dmg` を開き、**Pluxel を「アプリケーション」フォルダへドラッグ**
    3. Launchpad または「アプリケーション」から起動

=== "Windows"

    対象: 64bit (x64)

    1. `Pluxel_x.y.z_x64-setup.exe` をダウンロード
    2. インストーラを実行
    3. スタートメニューから起動

## データの保存場所

ライブラリや設定は**アプリ本体とは別の場所**に保存されます。そのため、
アプリを更新・再インストールしても登録済みのデータは保持されます。

| OS | 保存場所 |
| --- | --- |
| macOS | `~/Library/Application Support/com.cubezeero.pluxel/` |
| Windows | `%APPDATA%\com.cubezeero.pluxel\` |
