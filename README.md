# RepoTether

**登録ゼロで、全リポジトリの取り残しと Claude との作業の続きが一目で分かる。**

ローカルの git リポジトリ、Claude Code のセッション、GitHub / Gogs / Gitea のリポジトリ一覧をまとめて、
「いま何が取り残されているか」「いつ何をやったか」「どこに時間を使っているか」を見る個人用のデスクトップアプリ (Windows)。

プロジェクトを手で登録しない。探すフォルダとアカウントを決めれば、あとは自動で集まる。

![状態タブ](docs/images/01_state_time.png)

**操作マニュアル (画面付き): [docs/manual.md](docs/manual.md)**

## できること

- **状態** — プロジェクトごとの取り残し (未コミット・未 push・未マージのブランチ・stash など) と、
  Claude / git / 変更の時刻、直前の Claude セッション。フォルダ / 自分で付けるタグ (3 階層) で分類でき、一覧のほか、
  左に木・右に表のエクスプローラ形式でも見られる。
  スター・絞り込み・詳細パネル (リポジトリ / コミット / Claude / README) がある。VS Code・端末・エクスプローラー・Claude で開ける
- **Claude** — セッションの会話の全文を読み、端末で `claude -r` を動かして続きから再開できる。
  SessionVault があれば、セッションのログが壊れていないかを検査できる (読むだけ)
- **脆弱性** — LockWatch があれば、手元のリポジトリの一覧を渡し、lock ファイルの脆弱性の結果を詳細パネルと一覧に出す。
  非公開のリポジトリのパッケージ名は外に出さない (LockWatch が手元の DB で照合する)
- **履歴** — コミットと Claude セッションを日ごとに並べる
- **グラフ** — 日ごとのカレンダー、週ごとの棒、プロジェクト × 週、止まっているプロジェクト
- **日報** — 日報 / 週報の Markdown をプレビューを見ながら手直しし、コピー・保存
- **リモート** — GitHub (GitHub CLI のログインを借りられる) / Gogs / Gitea の一覧と照合し、未クローンのものはクローンできる

リポジトリは読むだけ。書き込むのは、自分で操作したとき (クローン・safe.directory の追加・任意の git fetch) だけ。
トークンは設定ファイルに書かず、Windows の資格情報マネージャーに置く。

## 動作環境

- Windows 10 / 11 (64 bit)。WebView2 (Windows 11 と、更新済みの Windows 10 には入っている)
- [Git for Windows](https://gitforwindows.org/) (`git` コマンド)
- 任意: [GitHub CLI](https://cli.github.com/) (`gh`)、[Claude Code](https://claude.com/claude-code)

macOS 向けのコードもあるが、実機では確認していない。

## インストール

[Releases](https://github.com/lancard-aikawa/RepoTether/releases) から、どちらかを入れる。

- `RepoTether_<版>_x64-setup.exe` — インストーラー。スタートメニューに登録される。WebView2 が無ければ入れる
- `RepoTether-<版>-win-x64.zip` — 展開した `RepoTether.exe` を起動するだけ

署名していないので、初回は SmartScreen が「PC が保護されました」と出す。「詳細情報」→「実行」で起動する。

使い始めの手順は [マニュアルの「使い始め」](docs/manual.md#2-使い始め) を参照。

## 開発

Tauri v2 + SvelteKit (adapter-static) + Svelte 5 + TypeScript。パッケージは pnpm、Rust は stable。

```sh
pnpm install
pnpm tauri dev                 # 起動 (画面の変更はすぐ反映、Rust の変更は自動で再ビルド)
pnpm check                     # 型チェック
pnpm test                      # フロントのテスト (tests/*.test.mts)
cd src-tauri && cargo test     # Rust のテスト
pnpm tauri build               # 配布物 (exe とインストーラー)
pnpm manual-shots              # マニュアルの画像をデモ環境で撮る (docs/images/)
```

Windows では `run-repotether.bat` でも起動できる (exe がソースより古ければ作り直してから起動。`dev` / `build` の引数あり)。
開発サーバーのポートは `vite.config.js` の 11420 (HMR は 11421)。

### アプリを起動せずに取り込みを試す

```sh
cd src-tauri
cargo run --release --example dump -- ../static/dev-snapshot.json                     # ローカルだけ
cargo run --release --example dump -- ../static/dev-snapshot.json --remote --fetch    # リモートと fetch も
```

設定はアプリと同じ場所を読む。環境変数 `REPOTETHER_CONFIG` で別の設定ファイルを渡せる。
`static/dev-snapshot.json` を置いて `pnpm dev` をブラウザで開くと、その内容で画面を確認できる (読み取りだけ)。
このファイルは自分のコミットやプロンプトを含むので git には入れない (`.gitignore` 済み)。

### 構成

```
src-tauri/src/
  lib.rs            Tauri のコマンド
  launch.rs         VS Code / 端末 / フォルダ / 資格情報の保管庫を開く (OS ごと)
  core/             取り込み。Tauri に依存しない
    discover.rs     リポジトリを探す
    git.rs          git の状態・コミット・fetch・クローン
    sessions.rs     Claude のセッションの要約とキャッシュ
    sessionvault.rs SessionVault の verify を呼んでログを検査する (読むだけ)
    remote.rs       GitHub / Gogs / Gitea の一覧
    gh.rs           GitHub CLI のログインを借りる
    secrets.rs      トークンの保存
    model.rs        画面に渡すデータの形
src/lib/
  derive.ts         取り込み結果をプロジェクト単位にまとめる (取り残しの判定、履歴・活動の集計)
  tags.ts           タグの操作 (作成・名前の変更・移動・並べ替え・削除)
  remotes.ts        リモートの種類とブラウザ用の URL
  report.ts         日報 / 週報の Markdown
  components/       各タブの画面
tools/
  manual-shots.mjs  マニュアルの画像をデモ環境で撮る
```

### リリース

`package.json`・`src-tauri/Cargo.toml`・`src-tauri/tauri.conf.json` の版を上げ、`CHANGELOG.md` にその版の節を書いてから、
`v<版>` のタグを push する。GitHub Actions がビルドして、zip とインストーラーを付けた Release を下書きで作る。

## ライセンス

[MIT](LICENSE)
