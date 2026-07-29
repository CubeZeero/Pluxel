# GitHub 開発ワークフロー（再利用テンプレート）

Tally で確立した開発・リリースの流れを、**他のソフトウェア開発にもそのまま流用**
できるようまとめたもの。各リポジトリのルートにコピーして使う。`OWNER/REPO` や
アプリ名などは自分のプロジェクトに読み替える。

---

## 1. 狙い

- **小さな変更を feature ブランチで作り、main に統合**（履歴が追える）
- **リリースは `stable` ブランチ ＋ `vX.Y.Z` タグ**で固定（利用者が安全に更新・戻せる）
- **バージョンは 1 か所（`package.json` 等）を真実**とし、そこから表示・更新チェックが導出される
- **ドキュメントは別ブランチ／別デプロイ**（アプリ本体に同梱しない）

---

## 2. ブランチ構成

| ブランチ | 役割 | 誰が使う |
| --- | --- | --- |
| `main` | 開発の統合先（デフォルト） | 開発者 |
| `feature/<名前>` | 1 変更 = 1 ブランチ。完了したら main へマージ | 開発者 |
| `stable` | **利用者が clone する安定版**。リリース時に main から進める | 利用者・本番 |
| `wiki`（任意） | ドキュメント専用。本体には同梱しない（別途デプロイ） | 開発者 |

```mermaid
gitGraph
   commit tag: "v1.0.0"
   branch feature/x
   commit
   commit
   checkout main
   merge feature/x
   commit tag: "v1.1.0"
   branch stable
   checkout stable
   merge main
```

> **なぜ `stable` を分けるか**：`main` は開発中で壊れることがある。利用者は
> `stable` を追えば、検証済みのバージョンだけを受け取れる。

---

## 3. 日々の開発サイクル（feature → main）

```bash
# 1. main を最新化して feature ブランチを切る
git switch main && git pull
git switch -c feature/<変更内容>

# 2. 変更してコミット（小さく、意味のある単位で）
git add -A
git commit -m "<何をしたか>"

# 3. push して（必要なら）PR、レビュー後に main へマージ
git push -u origin feature/<変更内容>

# 4. main へマージ（マージコミットを残すと feature 単位が履歴に残る）
git switch main && git pull
git merge --no-ff feature/<変更内容> -m "Merge feature/<変更内容> into main"
git push

# 5. 使い終えた feature ブランチは削除
git branch -d feature/<変更内容>
git push origin --delete feature/<変更内容>
```

- **`--no-ff`**（no fast-forward）で「Merge feature/x into main」という
  マージコミットを残すのがこのフローの流儀（機能のまとまりが履歴に残る）。
- コミットメッセージは命令形の要約（例: `Add update-available notification`）。
  **バージョンを上げたコミットには末尾に `(vX.Y.Z)`** を付ける（例:
  `Auto-save settings, accent color, and UI fixes (v1.1.2)`）。

---

## 4. バージョニング（セマンティックバージョニング）

`vMAJOR.MINOR.PATCH`（例: `v1.1.2`）。

| 上げる桁 | いつ |
| --- | --- |
| **MAJOR** | 後方互換を壊す変更（データ移行が必要、API 破壊 等） |
| **MINOR** | 後方互換のある機能追加 |
| **PATCH** | バグ修正・小さな改善 |

**バージョンの真実は 1 か所だけ**にする（例: `package.json` の `"version"`）。
アプリはそこから読む：

```ts
// version.ts — package.json を単一の真実にする
import pkg from "../../package.json";
export const APP_VERSION: string = pkg.version;
```

---

## 5. リリース手順（タグ付け）

```bash
# 1. main 上でバージョンを上げる（真実の 1 か所を更新）
#    package.json の "version" を x.y.z に。CHANGELOG があれば追記。
git switch main && git pull
git add -A && git commit -m "<リリース内容の要約> (vX.Y.Z)"
git push

# 2. 注釈付きタグを打って push（GitHub Release は必須ではない。タグだけでよい）
git tag -a vX.Y.Z -m "vX.Y.Z"
git push origin vX.Y.Z

# 3. stable を main の内容へ進める（利用者向けの安定版を更新）
git switch stable && git pull
git merge --ff-only main       # main が stable の先にある前提。分岐していれば merge/rebase
git push
git switch main
```

- **タグ名は `vX.Y.Z`**、`package.json` の `version` は `X.Y.Z`（`v` なし）で揃える。
- GitHub の「Release」オブジェクトは作らなくてよい。**タグを push すれば
  更新チェック（後述）が拾う**。

---

## 6. タグベースの「アップデート」フロー

### 6-1. 利用者側の更新・固定・ロールバック

```bash
# 通常の更新（stable を追っている場合）
git pull
docker compose up -d --build        # ビルド/再起動はプロジェクトに応じて

# 特定バージョンに固定 / 巻き戻し
git checkout vX.Y.Z
docker compose up -d --build
```

- 起動時にスキーマの**追加的マイグレーションを自動適用**する設計にしておくと、
  データを保持したまま更新できる（例: 起動スクリプトで `ADD COLUMN` を冪等適用）。

### 6-2. アプリ内「新バージョンあり」通知（GitHub tags API 比較）

**GitHub Release は不要**。`/tags` を取得して semver で比較するだけ。

```ts
// 走っている APP_VERSION と、GitHub 上の最新タグを比較する
const TAGS_URL = `https://api.github.com/repos/OWNER/REPO/tags?per_page=100`;

function parseSemver(v: string): [number, number, number] | null {
  const m = /^v?(\d+)\.(\d+)\.(\d+)/.exec(v.trim());
  return m ? [Number(m[1]), Number(m[2]), Number(m[3])] : null;
}
function isNewer(a: string, b: string): boolean {
  const pa = parseSemver(a), pb = parseSemver(b);
  if (!pa || !pb) return false;
  for (let i = 0; i < 3; i++) if (pa[i] !== pb[i]) return pa[i] > pb[i];
  return false;
}

async function latestTag(): Promise<string | null> {
  const res = await fetch(TAGS_URL, {
    headers: { Accept: "application/vnd.github+json", "User-Agent": "app-update-check" },
    // 結果はキャッシュ推奨（例: 6 時間）。API レート制限を避ける。
  });
  if (!res.ok) return null;
  const tags = (await res.json()) as { name?: string }[];
  let latest: string | null = null;
  for (const t of tags) {
    if (!t.name || !parseSemver(t.name)) continue;
    if (!latest || isNewer(t.name, latest)) latest = t.name;
  }
  return latest ? latest.replace(/^v/, "") : null;
}

// updateAvailable = latest && isNewer(latest, APP_VERSION)
```

実装の勘所（Tally での運用）:

- **キャッシュは 6 時間**程度。1 インスタンスあたり API コールは最小限に。
- **手動チェック**はキャッシュを無視（`cache: "no-store"`）して即時反映。
- **オフスイッチ**を用意（例: 環境変数 `UPDATE_CHECK=0` で無効化）。自己ホスト
  勢が外部通信を止められるように。
- 未認証の GitHub API はレート制限が緩め（60 req/h/IP）だが、キャッシュ前提なら十分。

---

## 7. ドキュメント（`wiki` ブランチ）

- ドキュメントは **`wiki` ブランチ**に置き、**アプリ本体（`main`/`stable`）には
  同梱しない**（配布物を小さく保つ）。
- 静的サイトジェネレータ（MkDocs / Zensical 等）でビルドし、**Cloudflare Pages
  等へデプロイ**。多言語なら言語ごとに別ビルド（例: `docs/`＝日本語、
  `docs-en/`＝英語）。
- アプリの About 画面などから公開ドキュメント URL へリンク。

---

## 8. リリース前チェックリスト

- [ ] 型チェック／ビルドが通る（例: `tsc --noEmit` ＋ 本番ビルド）
- [ ] 主要フローを実際に動かして確認（テストだけでなく E2E で挙動を見る）
- [ ] `package.json` の `version` を更新（＝真実の 1 か所）
- [ ] 破壊的変更がある場合、移行手順を用意（起動時マイグレーション等）
- [ ] CHANGELOG / wiki を更新
- [ ] `main` にマージ済み → タグ `vX.Y.Z` を push → `stable` を進める
- [ ] タグ push 後、更新チェックが新バージョンを拾うか確認

---

## 9. 他プロジェクトへの適用（最短手順）

1. リポジトリを作り、`main` をデフォルトに。
2. `stable` ブランチを作成（初回は main と同一）。
   ```bash
   git switch -c stable && git push -u origin stable && git switch main
   ```
3. バージョンの真実を 1 か所に（`package.json` 等）＋それを読む `APP_VERSION`。
4. 変更は必ず `feature/*` → `--no-ff` で `main` へ。
5. リリースは §5 の手順（version 更新 → `vX.Y.Z` タグ → `stable` を進める）。
6. 必要なら §6-2 の更新チェックと §7 の `wiki` ブランチを導入。
7. この `DEVELOPMENT_WORKFLOW.md` をリポジトリに置いておく。

---

### 付録: 使うコマンド早見

```bash
# feature 開始 / 統合
git switch main && git pull && git switch -c feature/x
git switch main && git merge --no-ff feature/x -m "Merge feature/x into main" && git push

# リリース
git commit -m "… (vX.Y.Z)" && git tag -a vX.Y.Z -m "vX.Y.Z" && git push --follow-tags
git switch stable && git merge --ff-only main && git push && git switch main

# 利用者の更新 / 固定
git pull                 # 最新へ
git checkout vX.Y.Z      # 特定版に固定・ロールバック
```
