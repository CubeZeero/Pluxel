import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";

export type Lang = "ja" | "en";

const LANG_KEY = "pluxel.lang";

export const LANGS: { value: Lang; label: string }[] = [
  { value: "ja", label: "日本語" },
  { value: "en", label: "English" },
];

type Dict = Record<string, string>;

const JA: Dict = {
  "common.cancel": "キャンセル",
  "common.save": "保存",
  "common.edit": "編集",
  "common.close": "閉じる",
  "common.delete": "削除",

  "tab.packages": "パッケージ",
  "tab.bundles": "バンドル",
  "select.mode": "選択モード",
  "select.count": "{count} 件選択中",
  "select.install": "選択をインストール",
  "multiInstall.title": "選択したパッケージをインストール",
  "multiInstall.pick": "{count} 件のインストール先(After Effects)を選んでください。既にインストール済みのものは更新されます。",
  "select.createBundle": "バンドルを作成",
  "select.chooseBundle": "バンドルを選択…",
  "select.addToBundle": "追加",
  "select.clear": "解除",
  "bundle.new": "バンドル作成",
  "bundle.untitled": "新しいバンドル",
  "bundle.emptyTitle": "バンドルがありません",
  "bundle.emptyBody": "パッケージをシリーズとしてまとめ、一括インストール・書き出しできます(インストーラーを除く)。",
  "bundle.selectHint": "バンドルを選択すると詳細が表示されます",
  "bundle.namePlaceholder": "バンドル名",
  "bundle.description": "説明",
  "bundle.members": "メンバー ({count})",
  "bundle.addHint": "パッケージの追加は、パッケージ一覧の選択モードから行えます。",
  "bundle.install": "シリーズをインストール",
  "bundle.export": "バンドルを書き出す (.ppf)",
  "bundle.delete": "バンドルを削除",
  "bundle.count": "{count} 項目",
  "bundle.showMore": "もっと表示 (+{count})",
  "bundle.showLess": "折りたたむ",
  "bundle.deleteTitle": "バンドルの削除",
  "bundle.deleteMsg": "バンドル「{name}」を削除しますか?(中のパッケージは残ります)",
  "toast.bundleCreated": "バンドルを作成しました",
  "toast.bundleAdded": "{count} 件をバンドルに追加しました",
  "toast.bundleDeleted": "バンドルを削除しました",
  "toast.bundleInstalled": "{count} 件をインストールしました",
  "toast.bundleExported": "{name} を書き出しました",
  "drop.title": "ここにドロップして追加",
  "drop.sub": ".jsx / .plugin / .aex / .zxp / .ppf / フォルダ",

  "toolbar.search": "名前・作者・タグで検索…",
  "toolbar.add": "追加",
  "toolbar.addPackage": "パッケージ (.ppf)",
  "toolbar.addAssets": "スクリプト・プラグイン",
  "toolbar.gridView": "グリッド表示",
  "toolbar.listView": "リスト表示",
  "toolbar.settings": "設定",
  "toolbar.about": "Pluxel について",

  "filter.all": "すべて",
  "filter.bundled": "バンドル済み",
  "kind.script": "スクリプト",
  "kind.scriptui": "ScriptUI",
  "kind.plugin": "エフェクト",
  "kind.zxp": "プラグイン (.zxp)",
  "kind.installer": "インストーラー",
  "kind.unknown": "その他",

  "installer.run": "インストーラーを実行",
  "installer.rerun": "再実行",
  "installer.unmark": "インストール済みを解除",
  "installer.hint": "この項目は .pkg / .exe 形式のインストーラーです。実行するとOSのインストーラーが起動します。",
  "elevate.title": "管理者権限が必要",
  "elevate.msg": "この場所への書き込みには管理者権限が必要です。パスワード / Touch ID の認証で再試行しますか?",
  "elevate.ok": "認証してインストール",
  "elevate.uninstallMsg": "この場所からの削除には管理者権限が必要です。パスワード / Touch ID の認証で続行しますか?",
  "elevate.uninstallOk": "認証して削除",
  "confirm.runTitle": "インストーラーの実行",
  "confirm.runMsg": "「{name}」のインストーラーを実行しますか?",
  "confirm.runOk": "実行",
  "toast.installerLaunched": "インストーラーを起動しました",

  "empty.title": "ライブラリは空です",
  "empty.body":
    "「スクリプト・プラグイン追加」から .jsx / .plugin / .aex / .zxp を追加するか、ウィンドウにドラッグ&ドロップ、または「パッケージ追加」で .ppf を読み込んでください。",
  "empty.noMatch": "条件に一致するパッケージがありません。",

  "untitled": "無題",
  "installed": "インストール済み",

  "detail.selectHint": "パッケージを選択すると詳細が表示されます",
  "detail.changeBanner": "バナー変更",
  "detail.removeBanner": "バナー削除",
  "toast.bannerRemoved": "バナーを削除しました",
  "detail.bannerSizeTip": "推奨バナーサイズ",
  "field.version": "バージョン",
  "field.author": "作者",
  "field.homepage": "ホームページ",
  "field.description": "説明",
  "field.tags": "タグ (カンマ区切り)",
  "field.kind": "種別 / 既定のインストール先",
  "section.install": "インストール",
  "install.customFolder": "カスタム",
  "install.allAe": "すべての After Effects ({count})",
  "install.button": "インストール",
  "install.aexUnsupported": ".aex は Windows 用のエフェクトです。macOS ではインストールできません(保管は可能です)。",
  "install.pluginUnsupported": ".plugin は macOS 用のエフェクトです。Windows ではインストールできません(保管は可能です)。",
  "admin.title": "管理者認証",
  "admin.explain": "このインストールには管理者権限が必要です。パスワードはこの起動中のみメモリに保持され、ディスクには保存されません。",
  "admin.password": "管理者パスワード",
  "admin.unlock": "認証",
  "install.installed": "インストール済み",
  "install.reveal": "Finder / エクスプローラーで表示",
  "install.copyPath": "パスをコピー",
  "install.otherTargets": "その他のインストール先",
  "install.updateBtn": "更新",
  "install.update": "更新 (再インストール)",
  "install.uninstall": "アンインストール",
  "section.files": "ファイル ({count})",
  "files.addFiles": "ファイル",
  "files.addFolder": "フォルダ",
  "files.replace": "差し替え",
  "files.folderTag": "フォルダ",
  "files.mainTag": "本体",
  "files.removeTip": "削除",
  "toast.fileRemoved": "同梱ファイルを削除しました",
  "files.companionHint":
    "スクリプトに付随する .ffx やパネル用の画像フォルダなどを「ファイル / フォルダ」で同梱できます。",
  "section.distribute": "配布",
  "distribute.export": "配布用に書き出す (.ppf)",
  "detail.deleteFromLibrary": "ライブラリから削除",

  "bannerGuide.title": "推奨バナーサイズ",
  "bannerGuide.ratio": "アスペクト比",
  "bannerGuide.ratioVal": "16 : 9(カード・詳細ともこの比率で表示)",
  "bannerGuide.recommended": "推奨サイズ",
  "bannerGuide.minimum": "最低サイズ",
  "bannerGuide.format": "形式",
  "bannerGuide.note":
    "表示は「切り抜き(cover)」です。比率が異なる画像は中央基準でトリミングされるため、文字やロゴは中央寄りに配置すると切れにくくなります。",

  "settings.title": "設定",
  "settings.language": "言語",
  "settings.theme": "テーマ",
  "settings.themeDark": "ダーク",
  "settings.themeLight": "ライト",
  "settings.accent": "アクセントカラー",
  "settings.kindColors": "種別カラー",
  "settings.reset": "既定に戻す",
  "settings.effectsFolder": "エフェクトの配置先",
  "settings.effectsFolderToggle": "エフェクトを「Plug-ins/Effects」フォルダに入れる",
  "settings.effectsFolderHint": "オンにすると、エフェクトプラグインを Plug-ins 直下ではなく Effects サブフォルダにインストールします。",
  "settings.customInstall": "カスタムインストール先",
  "settings.customInstallHint": "指定すると、詳細パネルのインストール先メニューにこのフォルダを選べるようになります。",
  "settings.customInstallNone": "未設定",
  "settings.customInstallChoose": "フォルダを選択",
  "settings.customInstallClear": "解除",
  "settings.adminAuth": "管理者認証",
  "settings.adminAuthHint": "権限が必要なインストールは、起動中に1回パスワードを入力すれば、以降そのまま実行できます(パスワードはメモリのみ、終了で破棄)。",
  "settings.adminLock": "認証セッションをロック",
  "settings.aeDetect": "After Effects の検出",
  "settings.aeNone": "After Effects は検出されていません。",
  "settings.redetect": "再検出",
  "settings.backup": "ライブラリのバックアップ",
  "settings.backupHint":
    "保管中の {count} 件をまとめて 1 つの .ppf に書き出します(読み込みは上の「パッケージ追加」から)。",
  "settings.exportAll": "ライブラリ全体を .ppf に書き出す",
  "settings.dataDir": "データ保管場所",
  "settings.updates": "アップデート",
  "settings.autoUpdate": "起動時に自動でアップデートを確認",
  "settings.checkUpdate": "今すぐ確認",
  "update.title": "アップデート",
  "update.available": "新しいバージョン {version} が利用可能です。",
  "update.upToDate": "お使いのバージョンは最新です。",
  "update.failed": "アップデートの確認に失敗しました。",
  "update.downloading": "ダウンロード中",
  "update.installNow": "更新する",
  "update.later": "後で",
  "settings.openFolder": "フォルダを開く",

  "about.desc":
    "After Effects のスクリプト・エフェクトプラグインの保管庫兼インストーラーです。.jsx / .plugin / .aex / .zxp に対応し、macOS・Windows で動作します。",
  "about.list1": "スクリプト・プラグインをライブラリに保管",
  "about.list2": "After Effects を自動検出してインストール",
  "about.list3": "パッケージ (.ppf) で入出力",
  "about.website": "公式サイト",
  "about.libs": "主要ライブラリ / ライセンス",
  "about.license": "ライセンス: GPL-3.0-or-later",
  "about.built": "Built with Tauri + React",

  "menu.export": "配布用に書き出す",
  "menu.delete": "削除",

  "dup.title": "既に存在します",
  "dup.msg": "「{name}」は既にライブラリにあります。アップデートして置き換えますか?",
  "dup.replace": "アップデートして置き換え",
  "dup.cancel": "取り込まない",

  "toast.added": "{count} 件を追加しました",
  "toast.importCancelled": "取り込みをキャンセルしました",
  "toast.importedZip": "{count} 件を読み込みました",
  "toast.nothingToExport": "エクスポートするパッケージがありません",
  "toast.exported": "{count} 件を書き出しました",
  "toast.exportedPackage": "{name} を配布用に書き出しました",
  "toast.aeDetected": "{count} 個の After Effects を検出",
  "toast.aeNotFound": "After Effects が見つかりませんでした",
  "toast.installed": "{name} をインストール: {label}",
  "toast.installedAllAe": "{name} を {count} 個の After Effects にインストールしました",
  "toast.installedMany": "{count} 件をインストールしました",
  "toast.installedManyUpd": "{count} 件をインストールしました(うち {updated} 件を更新)",
  "toast.uninstalled": "{name} をアンインストールしました",
  "toast.saved": "情報を保存しました",
  "toast.bannerSet": "バナーを設定しました",
  "toast.filesAdded": "{count} 件のファイルを同梱しました",
  "toast.folderAdded": "{count} 件のフォルダを同梱しました",
  "toast.filesReplaced": "ファイルを差し替えました",
  "toast.removed": "{name} を削除しました",

  "confirm.deleteTitle": "削除の確認",
  "confirm.deleteMsg": "「{name}」をライブラリから削除します。この操作は取り消せません。",
  "confirm.deleteOk": "削除",
  "confirm.replaceTitle": "差し替えの確認",
  "confirm.replaceMsg": "このパッケージのファイルを新しいものに差し替えますか?",
  "confirm.replaceOk": "差し替える",
};

const EN: Dict = {
  "common.cancel": "Cancel",
  "common.save": "Save",
  "common.edit": "Edit",
  "common.close": "Close",
  "common.delete": "Delete",

  "tab.packages": "Packages",
  "tab.bundles": "Bundles",
  "select.mode": "Select mode",
  "select.count": "{count} selected",
  "select.install": "Install selected",
  "multiInstall.title": "Install selected packages",
  "multiInstall.pick": "Choose the After Effects to install {count} package(s) into. Already-installed ones are updated.",
  "select.createBundle": "Create bundle",
  "select.chooseBundle": "Select a bundle…",
  "select.addToBundle": "Add",
  "select.clear": "Clear",
  "bundle.new": "New bundle",
  "bundle.untitled": "New Bundle",
  "bundle.emptyTitle": "No bundles yet",
  "bundle.emptyBody": "Group packages into a series to install and export them together (installers excluded).",
  "bundle.selectHint": "Select a bundle to see its details",
  "bundle.namePlaceholder": "Bundle name",
  "bundle.description": "Description",
  "bundle.members": "Members ({count})",
  "bundle.addHint": "Add packages from the package list's selection mode.",
  "bundle.install": "Install series",
  "bundle.export": "Export bundle (.ppf)",
  "bundle.delete": "Delete bundle",
  "bundle.count": "{count} items",
  "bundle.showMore": "Show more (+{count})",
  "bundle.showLess": "Show less",
  "bundle.deleteTitle": "Delete bundle",
  "bundle.deleteMsg": "Delete the bundle “{name}”? (Its packages remain.)",
  "toast.bundleCreated": "Bundle created",
  "toast.bundleAdded": "Added {count} item(s) to the bundle",
  "toast.bundleDeleted": "Bundle deleted",
  "toast.bundleInstalled": "Installed {count} item(s)",
  "toast.bundleExported": "Exported {name}",
  "drop.title": "Drop here to add",
  "drop.sub": ".jsx / .plugin / .aex / .zxp / .ppf / folder",

  "toolbar.search": "Search by name, author, tag…",
  "toolbar.add": "Add",
  "toolbar.addPackage": "Package (.ppf)",
  "toolbar.addAssets": "Script / Plugin",
  "toolbar.gridView": "Grid view",
  "toolbar.listView": "List view",
  "toolbar.settings": "Settings",
  "toolbar.about": "About Pluxel",

  "filter.all": "All",
  "filter.bundled": "Bundled",
  "kind.script": "Script",
  "kind.scriptui": "ScriptUI",
  "kind.plugin": "Effect",
  "kind.zxp": "Plugin (.zxp)",
  "kind.installer": "Installer",
  "kind.unknown": "Other",

  "installer.run": "Run installer",
  "installer.rerun": "Run again",
  "installer.unmark": "Mark as not installed",
  "installer.hint":
    "This item is a .pkg / .exe installer. Running it launches the OS installer.",
  "elevate.title": "Administrator privileges required",
  "elevate.msg": "Writing to this location requires administrator privileges. Retry with password / Touch ID authentication?",
  "elevate.ok": "Authenticate & install",
  "elevate.uninstallMsg": "Removing from this location requires administrator privileges. Continue with password / Touch ID authentication?",
  "elevate.uninstallOk": "Authenticate & remove",
  "confirm.runTitle": "Run installer",
  "confirm.runMsg": "Run the installer for “{name}”?",
  "confirm.runOk": "Run",
  "toast.installerLaunched": "Launched the installer",

  "empty.title": "Your library is empty",
  "empty.body":
    "Add .jsx / .plugin / .aex / .zxp via “Add Script / Plugin”, drag & drop onto the window, or import a .ppf via “Add Package”.",
  "empty.noMatch": "No packages match your filters.",

  "untitled": "Untitled",
  "installed": "Installed",

  "detail.selectHint": "Select a package to see its details",
  "detail.changeBanner": "Change banner",
  "detail.removeBanner": "Remove banner",
  "toast.bannerRemoved": "Banner removed",
  "detail.bannerSizeTip": "Recommended banner size",
  "field.version": "Version",
  "field.author": "Author",
  "field.homepage": "Homepage",
  "field.description": "Description",
  "field.tags": "Tags (comma-separated)",
  "field.kind": "Type / default install location",
  "section.install": "Install",
  "install.customFolder": "Custom",
  "install.allAe": "All After Effects ({count})",
  "install.button": "Install",
  "install.aexUnsupported": ".aex is a Windows effect and cannot be installed on macOS (storage is fine).",
  "install.pluginUnsupported": ".plugin is a macOS effect and cannot be installed on Windows (storage is fine).",
  "admin.title": "Administrator authentication",
  "admin.explain": "This install needs administrator access. Your password is kept in memory only for this session and never written to disk.",
  "admin.password": "Administrator password",
  "admin.unlock": "Unlock",
  "install.installed": "Installed",
  "install.reveal": "Show in Finder / Explorer",
  "install.copyPath": "Copy path",
  "install.otherTargets": "Other install locations",
  "install.updateBtn": "Update",
  "install.update": "Update (reinstall)",
  "install.uninstall": "Uninstall",
  "section.files": "Files ({count})",
  "files.addFiles": "Files",
  "files.addFolder": "Folder",
  "files.replace": "Replace",
  "files.folderTag": "Folder",
  "files.mainTag": "Main",
  "files.removeTip": "Remove",
  "toast.fileRemoved": "Removed the file",
  "files.companionHint":
    "Bundle companion assets like .ffx presets or a panel's image folder via “Files / Folder”.",
  "section.distribute": "Distribute",
  "distribute.export": "Export for distribution (.ppf)",
  "detail.deleteFromLibrary": "Remove from library",

  "bannerGuide.title": "Recommended banner size",
  "bannerGuide.ratio": "Aspect ratio",
  "bannerGuide.ratioVal": "16 : 9 (used for both cards and details)",
  "bannerGuide.recommended": "Recommended",
  "bannerGuide.minimum": "Minimum",
  "bannerGuide.format": "Format",
  "bannerGuide.note":
    "Images are displayed cropped (cover). Off-ratio images are trimmed from the center, so keep text and logos centered to avoid clipping.",

  "settings.title": "Settings",
  "settings.language": "Language",
  "settings.theme": "Theme",
  "settings.themeDark": "Dark",
  "settings.themeLight": "Light",
  "settings.accent": "Accent color",
  "settings.kindColors": "Type colors",
  "settings.reset": "Reset to defaults",
  "settings.effectsFolder": "Effect location",
  "settings.effectsFolderToggle": "Install effects into a “Plug-ins/Effects” folder",
  "settings.effectsFolderHint": "When on, effect plugins are installed into an Effects subfolder instead of directly under Plug-ins.",
  "settings.customInstall": "Custom install folder",
  "settings.customInstallHint": "When set, this folder appears as a target in the detail panel's install menu.",
  "settings.customInstallNone": "Not set",
  "settings.customInstallChoose": "Choose folder",
  "settings.customInstallClear": "Clear",
  "settings.adminAuth": "Administrator auth",
  "settings.adminAuthHint": "Enter the administrator password once per launch and the rest of the session installs without prompting (kept in memory only, cleared on quit).",
  "settings.adminLock": "Lock auth session",
  "settings.aeDetect": "After Effects detection",
  "settings.aeNone": "No After Effects installations detected.",
  "settings.redetect": "Re-detect",
  "settings.backup": "Library backup",
  "settings.backupHint":
    "Export all {count} package(s) to a single .ppf (import via “Add Package” above).",
  "settings.exportAll": "Export entire library to .ppf",
  "settings.dataDir": "Data location",
  "settings.updates": "Updates",
  "settings.autoUpdate": "Check for updates on startup",
  "settings.checkUpdate": "Check now",
  "update.title": "Update",
  "update.available": "Version {version} is available.",
  "update.upToDate": "You're on the latest version.",
  "update.failed": "Failed to check for updates.",
  "update.downloading": "Downloading",
  "update.installNow": "Update",
  "update.later": "Later",
  "settings.openFolder": "Open folder",

  "about.desc":
    "A vault and installer for After Effects scripts and effect plugins. Supports .jsx / .plugin / .aex / .zxp and runs on macOS and Windows.",
  "about.list1": "Store scripts and plugins in a library",
  "about.list2": "Auto-detect After Effects and install",
  "about.list3": "Import / export packages (.ppf)",
  "about.website": "Website",
  "about.libs": "Key libraries & licenses",
  "about.license": "License: GPL-3.0-or-later",
  "about.built": "Built with Tauri + React",

  "menu.export": "Export for distribution",
  "menu.delete": "Delete",

  "dup.title": "Already exists",
  "dup.msg": "“{name}” already exists in your library. Update and replace it?",
  "dup.replace": "Update & replace",
  "dup.cancel": "Don't import",

  "toast.added": "Added {count} item(s)",
  "toast.importCancelled": "Import cancelled",
  "toast.importedZip": "Imported {count} item(s)",
  "toast.nothingToExport": "No packages to export",
  "toast.exported": "Exported {count} item(s)",
  "toast.exportedPackage": "Exported {name} for distribution",
  "toast.aeDetected": "Detected {count} After Effects installation(s)",
  "toast.aeNotFound": "No After Effects found",
  "toast.installed": "Installed {name}: {label}",
  "toast.installedAllAe": "Installed {name} to {count} After Effects versions",
  "toast.installedMany": "Installed {count} package(s)",
  "toast.installedManyUpd": "Installed {count} package(s) ({updated} updated)",
  "toast.uninstalled": "Uninstalled {name}",
  "toast.saved": "Saved",
  "toast.bannerSet": "Banner updated",
  "toast.filesAdded": "Added {count} file(s)",
  "toast.folderAdded": "Added {count} folder(s)",
  "toast.filesReplaced": "Files replaced",
  "toast.removed": "Removed {name}",

  "confirm.deleteTitle": "Confirm deletion",
  "confirm.deleteMsg": "Remove “{name}” from the library. This cannot be undone.",
  "confirm.deleteOk": "Delete",
  "confirm.replaceTitle": "Confirm replace",
  "confirm.replaceMsg": "Replace this package's files with new ones?",
  "confirm.replaceOk": "Replace",
};

const DICTS: Record<Lang, Dict> = { ja: JA, en: EN };

export type TFunc = (key: string, params?: Record<string, string | number>) => string;

function translate(lang: Lang, key: string, params?: Record<string, string | number>): string {
  let s = DICTS[lang][key] ?? DICTS.ja[key] ?? key;
  if (params) {
    for (const [k, v] of Object.entries(params)) s = s.split(`{${k}}`).join(String(v));
  }
  return s;
}

function initialLang(): Lang {
  const saved = localStorage.getItem(LANG_KEY);
  if (saved === "ja" || saved === "en") return saved;
  return navigator.language?.toLowerCase().startsWith("ja") ? "ja" : "en";
}

interface I18nValue {
  lang: Lang;
  setLang: (l: Lang) => void;
  t: TFunc;
}

const I18nContext = createContext<I18nValue | null>(null);

export function I18nProvider({ children }: { children: ReactNode }) {
  const [lang, setLang] = useState<Lang>(initialLang);

  useEffect(() => {
    localStorage.setItem(LANG_KEY, lang);
    document.documentElement.lang = lang;
  }, [lang]);

  const value = useMemo<I18nValue>(
    () => ({ lang, setLang, t: (key, params) => translate(lang, key, params) }),
    [lang],
  );

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18nValue {
  const ctx = useContext(I18nContext);
  if (!ctx) throw new Error("useI18n must be used within I18nProvider");
  return ctx;
}

/** Convenience hook returning just the translate function. */
export function useT(): TFunc {
  return useI18n().t;
}
