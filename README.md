# RepoTether

ローカルの git リポジトリ、Claude Code のセッション、GitHub / Gogs / Gitea のリポジトリ一覧をまとめて、
「いつ何をやったか」「どこに時間を使っているか」「いま何が取り残されているか」を見る個人用のデスクトップアプリ (Windows)。

プロジェクトを手で登録しない。探すフォルダとアカウントを決めれば、あとは自動で集まる。

## 動作環境

- Windows 10 / 11 (64 bit)。WebView2 (Windows 11 と、更新済みの Windows 10 には入っている) が要る
- [Git for Windows](https://gitforwindows.org/) (`git` コマンド)
- 任意: [GitHub CLI](https://cli.github.com/) (`gh`)。GitHub の一覧を gh のログインで取るとき
- 任意: [Claude Code](https://claude.com/claude-code)。セッションの履歴を見るとき

macOS 向けのコードもあるが、実機では確認していない (下の「macOS」を参照)。

## インストール

[Releases](https://github.com/lancard-aikawa/RepoTether/releases) から、どちらかを入れる。

- `RepoTether_<版>_x64-setup.exe` — インストーラー。スタートメニューに登録される。WebView2 が無ければ入れる
- `RepoTether-<版>-win-x64.zip` — 展開した `RepoTether.exe` を起動するだけ。どこに置いてもよい

署名していないので、初回は Windows の SmartScreen が「PC が保護されました」と出す。
「詳細情報」→「実行」で起動する。

## 使い始め

1. 起動すると、よくある場所 (`~\Repos`、`C:\Repos` など) からリポジトリを探して並べる
2. 「設定 → 探す場所」で、リポジトリを置いているフォルダを足す
3. 「設定 → 自分のコミット」で、自分のメールアドレスを選ぶ (履歴・グラフ・日報は、これで自分のコミットを数える)
4. GitHub の一覧も見るなら「設定 → アカウント → GitHub を追加」。`gh auth login --web` でログイン済みなら、そのまま使える

## 画面

| タブ | 見せるもの | 答える問い |
|---|---|---|
| **状態** | プロジェクトごとの取り残し (未コミット・未 push・未マージのブランチ・stash など)、Claude / git / 変更の時刻、直前の Claude セッション | いま何が取り残されているか |
| **履歴** | コミットと Claude セッションを、日 → プロジェクトの順にまとめて並べる | いつ、何をやったか |
| **グラフ** | 日ごとのカレンダー (1 年)、週ごとの棒 (26 週)、プロジェクト × 週、止まっているプロジェクト | どこに時間を使っているか |
| **日報** | 日報 / 週報の Markdown。プレビューを見ながら手直しし、コピー・保存できる | 報告に何を書くか |
| **設定** | 探す場所、自分のメールアドレス、アカウント、表示 (テーマ・密度・端末・自動更新) | |

状態タブ:

- 見せ方は「時系列 / フォルダ / タグ」。タグは自分で作る分類で、`仕事/客先/案件` のように `/` 区切りで 3 階層まで。
  タグ表示では、プロジェクトやタグの見出しをドラッグして付け替え・並べ替え・移動でき、見出しから追加・名前の変更・削除ができる。
  間違えたら直後に出る「元に戻す」で戻せる
- 絞り込みは、スター・状態 (取り残しあり / 未クローン / git 以外 / 読めない)・リモートの種類・非表示
- 各行から VS Code・端末・エクスプローラーで開ける。リモートの種類とパスの横のアイコンでブラウザを開く。
  リモートにだけあるリポジトリはクローンできる
- 行を選ぶと詳細パネル (概要 / リポジトリ / コミット / Claude / README)。コミットは取り込み期間より前も 10 件ずつ読める

## 集めるもの

- **ローカルのリポジトリ** — 「探す場所」を 3 階層 (変更可) まで探す。
  Claude のセッションで使ったフォルダが git リポジトリなら、探す場所の外でも対象にする
- **git の状態** — `git status --porcelain=v2`、`for-each-ref`、`log` を読む。
  読み取りだけで、`GIT_OPTIONAL_LOCKS=0` で index のロックも取らない
- **コミット** — ブランチ・リモート追跡・タグから届くもの (stash は除く)。既定は過去 365 日
- **Claude Code のセッション** — `~/.claude/projects/*/*.jsonl`。タイトル、最初と最後のプロンプト、最後の返答、
  プロンプトの時刻を抜き出す。SDK からの自動実行は既定で活動に数えない
- **リモートの一覧** — GitHub / Gogs / Gitea の API。ローカルの remote URL と `host/owner/name` で照合する
- **git fetch (任意・既定はしない)** — リモートを更新するとき、各リポジトリで `git fetch --all --prune` もする。
  裏で動くので認証の画面は出さず、1 つ 20 秒で打ち切る

自動更新は、ローカル (既定 15 分) とリモート (既定 60 分) で別々に間隔を選べる。ウィンドウが見えていないあいだは止める。

「自分のコミット」は設定のメールアドレスで判定する。GitHub の Web 上でのコミットは
`...@users.noreply.github.com` になるので、設定画面の候補から足しておく。

## 認証とトークン

- **GitHub** — 既定は GitHub CLI のログインを借りる (一覧を取るたびに `gh auth token` を呼ぶ)。RepoTether にはトークンを置かない。
  トークンを直接使うこともできる
- **Gogs / Gitea** — トークンが要る (Gogs は「ユーザー設定 → アプリケーション」で作る)

トークンは設定ファイルに書かず、OS の資格情報の保管庫に置く
(Windows は資格情報マネージャーの `RepoTether:<アカウント ID>`、macOS はキーチェーン)。
画面にも返さず「保存済み」とだけ出す。同じユーザーとして動くプログラムからは読めるので、読み取り権限だけのトークンを使う。

## データの置き場所

| もの | Windows | macOS |
|---|---|---|
| 設定 (探す場所・タグ・スター・非表示など) | `%APPDATA%\com.repotether.app\config.json` | `~/Library/Application Support/com.repotether.app/config.json` |
| トークン | 資格情報マネージャー | ログインキーチェーン |
| 取り込み結果・セッション要約のキャッシュ | `%LOCALAPPDATA%\com.repotether.app\` | `~/Library/Caches/com.repotether.app/` |

Claude のログは全体で数百 MB になることがあるので、ファイルの更新時刻とサイズが同じなら前回の要約を使う
(初回は 30 秒ほど、2 回目以降は数秒)。設定ファイルが壊れて読めないときは、`config.broken.json` に退避してから既定値で起動する。

## 読めないリポジトリ

- **所有者チェック (dubious ownership)** — 別のドライブなどで、git がフォルダの所有者を確認できないもの。
  詳細パネルの「safe.directory に追加」で `git config --global --add safe.directory <パス>` を実行する
  (自分のフォルダであることを確かめてから)
- **壊れた `.git`** — `.git` はあるが中身が無いもの。表示するだけ

## やらないこと

サーバーモード、マルチユーザー、チケット管理、git のブランチのツリー表示 (GitKraken や VS Code の Git Graph で見られる)。

## macOS

コードは OS ごとに分けてあり、macOS 向けの自前のコードがコンパイルできることは確認済み (`aarch64-apple-darwin`)。
アプリ全体のビルドと動作は Mac の実機で確認していない。

| 項目 | Windows | macOS |
|---|---|---|
| VS Code で開く | Code.exe を直接起動 | `open -a "Visual Studio Code"` |
| 端末 | Windows Terminal / PowerShell / cmd / Git Bash / WSL から選ぶ | ターミナル / iTerm |
| フォルダ | エクスプローラー | Finder |
| トークン | 資格情報マネージャー | キーチェーン |

Mac 用のビルドは Mac の上で行う (`pnpm tauri build`)。Linux は対象外。

## 開発

Tauri v2 + SvelteKit (adapter-static) + Svelte 5 + TypeScript。パッケージは pnpm、Rust は stable。

```sh
pnpm install
pnpm tauri dev                 # 起動 (画面の変更はすぐ反映、Rust の変更は自動で再ビルド)
pnpm check                     # 型チェック
pnpm test                      # フロントのテスト (tests/*.test.mts)
cd src-tauri && cargo test     # Rust のテスト
pnpm tauri build               # 配布物 (exe とインストーラー)
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
`static/dev-snapshot.json` を置いて `pnpm dev` をブラウザで開くと、その内容で画面を確認できる
(読み取りだけ)。このファイルは自分のコミットやプロンプトを含むので git には入れない (`.gitignore` 済み)。

### 構成

```
src-tauri/src/
  lib.rs            Tauri のコマンド
  launch.rs         VS Code / 端末 / フォルダ / 資格情報の保管庫を開く (OS ごと)
  core/             取り込み。Tauri に依存しない
    discover.rs     リポジトリを探す
    git.rs          git の状態・コミット・fetch・クローン
    sessions.rs     Claude のセッションの要約とキャッシュ
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
```

### リリース

`package.json`・`src-tauri/Cargo.toml`・`src-tauri/tauri.conf.json` の版を上げ、`CHANGELOG.md` にその版の節を書いてから、
`v<版>` のタグを push する。GitHub Actions がビルドして、zip とインストーラーを付けた Release を下書きで作る。

## ライセンス

[MIT](LICENSE)
