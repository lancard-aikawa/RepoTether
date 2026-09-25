# RepoTether

ローカルの git リポジトリ、Claude Code のセッション、GitHub / Gogs / Gitea のリポジトリ一覧をまとめて、
「いつ何をやったか」「どこに時間を使っているか」「いま何が取り残されているか」を見る個人用ツール。

プロジェクトを手で登録しない。探す場所とアカウントを決めれば、あとは自動で集まる。

## 画面

| タブ | 見せるもの | 答える問い |
|---|---|---|
| **状態** | プロジェクトごとの取り残し (未コミット・未 push・未マージのブランチ・stash など) と、直前の Claude セッション | いま何が取り残されているか |
| **履歴** | コミットと Claude セッションを、日 → プロジェクトの順にまとめて並べる | いつ、何をやったか |
| **グラフ** | 日ごとのカレンダー (1 年)、週ごとの棒 (26 週)、プロジェクト × 週、止まっているプロジェクト | どこに時間を使っているか、どれが止まっているか |
| **日報** | 日報 / 週報の Markdown。手直ししてコピー・保存できる | 報告に何を書くか |
| **設定** | 探す場所、自分のメールアドレス、リモートのアカウント | |

状態タブの各行から、VS Code・端末・エクスプローラー・ブラウザ (リモートのページ) で開ける。
リモートにだけあるリポジトリはクローンできる。

## 集めるもの

- **ローカルのリポジトリ** — 設定の「探す場所」を 3 階層 (変更可) まで探す。
  Claude のセッションで使ったフォルダが git リポジトリなら、探す場所の外でも対象にする
- **git の状態** — `git status --porcelain=v2`、`for-each-ref`、`log` を読む (libgit2 は使わない)。
  読み取りだけで、`GIT_OPTIONAL_LOCKS=0` で index のロックも取らない
- **コミット** — ブランチ・リモート追跡・タグから届くもの (stash は除く)。既定は過去 365 日
- **Claude Code のセッション** — `~/.claude/projects/*/*.jsonl`。タイトル (`ai-title`)、最初と最後のプロンプト、
  最後の返答、プロンプトの時刻を抜き出す。SDK からの自動実行 (`entrypoint` が `sdk-*`) は既定で活動に数えない
- **リモートの一覧** — トークンがあれば `/user/repos` (見られる全部)、無ければ `/users/{user}/repos` (公開分)。
  Gogs はトークンが必須。ローカルの remote URL と `host/owner/name` で照合する

「自分のコミット」は設定のメールアドレスで判定する。GitHub の Web 上でのコミットは
`...@users.noreply.github.com` になるので、設定画面の候補から足しておく。

## データの置き場所

| もの | 場所 |
|---|---|
| 設定 | `%APPDATA%\com.repotether.app\config.json` (トークンも平文でここに入る) |
| 取り込み結果のキャッシュ | `%LOCALAPPDATA%\com.repotether.app\snapshot.json` |
| セッション要約のキャッシュ | `%LOCALAPPDATA%\com.repotether.app\sessions-cache.json` |

Claude のログは全体で数百 MB あるので、ファイルの更新時刻とサイズが同じなら前回の要約を使う。
初回は 30 秒ほど、2 回目以降は 5 秒ほど (96 リポジトリ・330 セッションで計測)。

設定ファイルが壊れて読めないときは、`config.broken.json` に退避してから既定値で起動する。

## 読めないリポジトリ

- **所有者チェック (dubious ownership)** — 別のドライブなどで、git がフォルダの所有者を確認できないもの。
  詳細パネルの「safe.directory に追加」で、`git config --global --add safe.directory <パス>` を実行する
  (自分のフォルダであることを確かめてから)
- **壊れた `.git`** — `.git` はあるが中身が無いもの。表示するだけ

## やらないこと

サーバーモード、マルチユーザー、認証、チケット管理。
(PMManage が広がりすぎた反省。必要になってから考える)

git のコミットグラフ (ブランチのツリー表示) も入れない。GitKraken や VS Code の Git Graph で見られるため。

開発サーバーの起動は LocalLauncher (`../LocalLauncher`) の担当。

## 開発

Tauri v2 + SvelteKit (adapter-static) + Svelte 5 + TypeScript。パッケージは pnpm。

```sh
pnpm install
pnpm tauri dev                 # デスクトップアプリ起動
pnpm check                     # 型チェック
cd src-tauri && cargo test     # Rust のテスト
pnpm tauri build --no-bundle   # exe だけ作る (src-tauri/target/release/repotether.exe)
```

ポートは LPortMan の台帳に登録済み: Vite dev `11420` (Tauri の devUrl)、HMR `11421` (`TAURI_DEV_HOST` 指定時のみ)。

### アプリを起動せずに取り込みを試す

```sh
cd src-tauri
cargo run --release --example dump -- ../static/dev-snapshot.json            # ローカルだけ
cargo run --release --example dump -- ../static/dev-snapshot.json --remote   # リモートも
```

設定は `%APPDATA%\com.repotether.app\config.json` を読む。環境変数 `REPOTETHER_CONFIG` で別の設定ファイルを渡せる。

`static/dev-snapshot.json` を置いて `pnpm dev` をブラウザで開くと、その内容で画面を確認できる
(読み取りだけ。開く・クローンなどの操作は使えない)。このファイルは自分のコミットやプロンプトを含むので git には入れない。

### 構成

```
src-tauri/src/
  lib.rs            Tauri のコマンド (設定・更新・クローン・開く・保存ダイアログ)
  core/             取り込み。Tauri に依存しない
    discover.rs     リポジトリを探す
    git.rs          git の状態とコミット、クローン
    sessions.rs     Claude のセッションの要約とキャッシュ
    remote.rs       GitHub / Gogs / Gitea の一覧
    model.rs        画面に渡すデータの形
src/lib/
  derive.ts         取り込み結果をプロジェクト単位にまとめる。取り残しの判定、履歴・活動の集計
  report.ts         日報 / 週報の Markdown
  components/       各タブの画面
```

## 今後の候補

- セッションの要約を LLM で作る (使うなら無料の経路: `claude -p` のサブスク枠 / ローカルの gemma)
- Gogs / Gitea のトークンありでの取得は未検証 (公開分の GitHub と、Gogs の 403 応答までは確認済み)
- トークンを Windows の資格情報マネージャーに置く
