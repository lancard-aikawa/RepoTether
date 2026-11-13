# Offstage 構想

2026-11-12。git の外にある個人のファイルと、git の中の資料を、リポジトリに結び付けて探せるようにする。
RepoTether の隣に置く（同じワークスペースの別 crate・別 exe）。

## 目的

| | 1-1 個人のファイル | 1-2 資料 |
|---|---|---|
| 何か | gitignore に入れて手元だけに置くもの（`sandbox/`・`slack/`・`.work/`・`data/`） | git で管理している md・画像・動画・音声・Office 系 |
| 守ること | **外に出さない・失わない**。バックアップはしない | 守るのは git に任せる |
| 欲しいこと | どこに何があるか・消えていないか・Claude に渡したか | 探せる・会話のどの場面で出たか |

1 つのアプリで、全リポジトリと関連プロジェクトを横断して扱う。

## 実物（2026-11-12、手元の約 25 リポジトリ）

gitignore の中身は個人のファイルと生成物が混ざっている。全部拾うと生成物で埋まるので、分類の規則が本体になる。
（このリポジトリは公開しているので、手元のファイル名は書かない。種類だけ挙げる）

- 個人: `sandbox/` の作業ファイル、API のトークンやリフレッシュトークンの JSON、Workers の `.dev.vars`、道具が溜めた `data/`
- 生成物: 古い版の `*.vsix`、`.playwright-mcp/`（6 リポジトリ）、`src-tauri/gen/`、`.wrangler/`、`.dart_tool/`
- 判断が要る: 道具の状態や履歴の JSON、試験で撮った画像、`.claude/settings.local.json`

## 分類

拾い方は `git ls-files --others --ignored --exclude-standard --directory`（リポジトリごと）。

| 区分 | 扱い | 規則の初版 |
|---|---|---|
| 生成物 | 索引に入れない（数だけ数える） | `node_modules/` `.venv/` `target/` `build/` `dist/` `out/` `__pycache__/` `.svelte-kit/` `gen/` `.dart_tool/` `.wrangler/` `.playwright-mcp/` `test-results/` `cache/` `.cache/` `playwright-report/` `*.vsix` `*.pid` `*.db-wal` `*.db-shm` |
| 機密の疑い | 個人として扱い、印を付ける | 名前に `token` `secret` `credential` `password`、`.env*`、`.dev.vars`、`*.pem` `*.key` `*.p12`、`google-services.json`、`*service-account*.json`。**`Local State` を持つフォルダ**（Chromium・WebView2・Electron の利用者データ）は中を並べず 1 件に畳む |
| 道具のデータ | 索引に入れるが、個人と分けて見せる（2026-11-12 に足した） | フォルダ `log/` `logs/` `history/` `vault/` `results/` `reports/` `snapshots/`、管理しているフォルダ `.claude/` `.github/` `.vscode/` の中で無視されているもの、`state.json`、`*.log` `*.jsonl` `*.sqlite` `*.db` |
| 個人 | 索引に入れる | `sandbox/` `slack/` `.work/` `data/` `tmp/` と、上のどれにも当たらないもの |
| 未分類 | 画面で人が決める | 規則の結果を人が上書きしたもの・新しく現れたフォルダ |

決める順は、生成物のフォルダの下 → 生成物の名前 → 機密らしい名前 → 道具のデータ → 個人。
機密らしい名前は道具のデータのフォルダの中でも機密の疑いにする（漏れの印を落とさない）。生成物のフォルダの下では生成物のまま。
Flutter が作り直すもの（`local.properties`・`Generated.xcconfig`・`GeneratedPluginRegistrant.*` など）は生成物。

規則は設定ファイルに置き、人の上書き（このフォルダは生成物、など）はパスで覚える。

### 初版を手元で試した結果（2026-11-12、約 25 リポジトリ、1.8 秒）

`cargo run -p offstage-core --example scan -- <フォルダ> --summary` で、ファイル名を出さずにフォルダごと・規則ごとの件数だけを見た。

- 名前に `token` で当たった 18 件のうち 11 件は、ブラウザの利用者データの中（`Trust Tokens`・パスワードの強さの辞書など）だった。`Local State` のフォルダを畳む規則を足して、機密の疑いは 7 件（どれも当たるべきもの）になった
- 個人 2,206 件の多くは、人が書いたものではなく道具が溜めたもの: Claude のセッションの写し（約 1,400）、試験の撮影（約 150）、脆弱性の結果、VS Code の拡張が `.github/` の下に作るもの、Flutter が作る `android/`・`ios/` の一部。作り直せないものもあるので生成物にはせず、4 つ目の区分「道具のデータ」を足した（ユーザーの判断）。足したあとは、生成物 209・道具のデータ 1,602・個人 580・機密の疑い 7
- 残る個人 580 件のうち、規則では分けにくいもの: 試験の撮影を溜める `_` で始まるフォルダ、道具の設定のフォルダ `config/` の一部、デモの環境。人の上書きで分ける

## 中身を読むのは許可制

- 既定で記録するのは、名前・パス・日付・大きさ・ハッシュだけ。**中身は索引に入れない**
- 中身（全文検索・見出し）を索引に入れるのは、人がフォルダ単位で許可したところだけ。許可の一覧は設定ファイルに書く
- 許可していても、**機密の疑いのファイルは中身を入れない**（変えるにはファイル単位で別に許可する）
- 許可を外したら、そのフォルダの中身を索引から消す（`VACUUM` まで行う）
- 1-2（git の中の資料）は許可なしで中身を読む

## 消えたら気づく

- 取り込むたびに、パス・大きさ・更新時刻を前回と比べる。ハッシュ（SHA-256）は大きさか更新時刻が変わったときだけ計算し直す
- 前回はあって今回無いものは「消えた」とする。同じハッシュが別の場所にあれば「移動」として分ける
- 消えたものは、名前とハッシュと最後に見た日付を残す（中身は残らない）。人が「消してよかった」と付けたら印を消す
- リポジトリごと無くなった（フォルダの移動・ドライブを外した）場合は、ファイルごとの「消えた」を出さず、リポジトリ 1 件の印にまとめる
- 「消えた」は、前回あって今回の洗い出しに無く、**ディスクにも無い**もの。ディスクにあるのに洗い出しに無いもの（gitignore から外して git の管理に入った・規則が変わって生成物になった）は記録から外すだけ。洗い出しに失敗したリポジトリと、今回洗い出さなかったリポジトリは比べない
- ハッシュを取るのは個人と機密の疑いのファイルだけ（道具のデータはセッションの写しなど大きく、よく変わるので、大きさと更新時刻だけ）。移動は個人と機密の疑いだけ見分けられる
- 実装は `crates/offstage-core/src/store.rs`（SQLite、`rusqlite` の bundled と `sha2`。どちらも通信しない）。手元の約 25 リポジトリで 2,313 件を記録して 1.3 秒、2 回目は 1.0 秒で違い 0 件（2026-11-12）
- ⛔ git は、無視されたフォルダの中に自分用の `.gitignore` を持つフォルダがあると、親のフォルダとその中のパスを両方返す（VS Code の拡張が `.github/` の下に作るもので起きた）。親は自分で降りるので、中のほうを外す。外さないと同じファイルが 2 度入り、何も変えていないのに毎回「追加」が出た

## Claude の会話との結び付け

- `~/.claude/projects/*/*.jsonl` から、ファイルのパスと「どう出たか」を拾う。拾い方は Open Outside の `tools/try_extract.js` と RepoTether の `core/sessions.rs` を元にする

| 出方 | 意味 |
|---|---|
| Read | 中身が Anthropic に送られた |
| Edit / Write | Claude が中身を書いた（書いた分は送られている） |
| Bash のコマンドに出た | 中身が出力として送られた可能性がある |
| 本文に書かれただけ | 名前とパスだけが送られた |

- 記録するのは、セッション・時刻・発言の位置・出方。押すと RepoTether の会話の全文表示の、その場面を開く
- ツールのほかにも中身が送られる経路がある（2026-11-12 に記録で確かめた）: `attachment` の `file`（@ で渡した）・`edited_text_file`（外で直した）・`instructions`（CLAUDE.md・memory）、発言の `<ide_selection>`（エディタで選んだ範囲）。いずれも `attached` にする。`compact_file_reference` と `<ide_opened_file>` は名前だけ（`text`）。`<system-reminder>` の中は飛ばす
- パスはディスクではなく、Offstage の記録（消えたものも含む）と git の資料の一覧に当てる。消えたファイルが前に会話に出ていたかを知るため
- 1 件に畳んだフォルダ（ブラウザの利用者データ）の中のファイルは、そのフォルダに結び付ける。生成物のフォルダの中は結び付けない
- 実装は `crates/offstage-core/src/link.rs`。手元の記録 685 本・2.7 GB・37 万行で 8.2 秒、2 回目は増えた分だけで 1.0 秒。当たり 3,792 件（2026-11-12）
- `bash` は「中身が出たかもしれない」として画面では印にする（`Get-Content … | Select-String` のように、Read を使わずにコマンドでファイルを読むことがある）
- ⛔ 末尾を 1 文字ずつ縮めて探すとき、区切りで終わる所（`out\`）まで縮めると、フォルダの記録に当たってしまう（初版で、生成物のフォルダの中の画像の Read 67 件をフォルダに当てていた）。区切りまで来たらやめる
- ⛔ 候補の先頭をバイト数で切ると、日本語で始まる候補で落ちる（`str::get` で切る）
- 1-2 は、md・画像・動画・音声・Office 系に絞る（スクリプトは結び付けない）
- 1-1 は全種類を結び付ける（漏れたかどうかを見る目的なので、絞らない）
- **機密の疑いがあるファイルが Read・Bash で出たら、RepoTether に印を出す**

## 外に出さないための決まり（コードで守る）

- `offstage-core` と Offstage の exe は、通信するクレート（reqwest・hyper・ureq など）に依存しない。`cargo tree` の結果を試験で確かめる
- Offstage の Tauri の capabilities に、http・shell・opener の外部 URL を入れない
- 索引の DB は `%USERPROFILE%\.offstage\index.sqlite`。クラウド同期のフォルダ（OneDrive など）の下だったら、起動時に止める
- 索引の DB と設定は、どのリポジトリにも入れない
- RepoTether は公開リポジトリなので、試験の材料・文書・画面の見本に、手元の実際のファイル名を使わない（架空のものを作る。Open Outside の `demo/` と同じ）

## 形

RepoTether を Cargo のワークスペースにする（Sasayui と同じ形）。

```
RepoTether/
  Cargo.toml                 [workspace]
  src-tauri/                 RepoTether（今のまま）
  crates/offstage-core/      取り込み・分類・索引・会話の結び付け。Tauri と通信に依存しない
  apps/offstage/src-tauri/   Offstage の画面（別の exe、別ウィンドウ）
```

- RepoTether は `offstage-core` を読むだけで使い、印（消えた・機密が会話に出た・未分類が増えた）と件数だけを行に出す。押すと Offstage の画面を、そのリポジトリで絞った状態で開く
- 画面を別の exe にする理由: RepoTether は通信する（GitHub など）ので、同じプロセスに置くと「通信しない」をコードで保証できない
- 独立させるときは、`crates/offstage-core` と `apps/offstage` をそのまま別のリポジトリへ抜く
- タグは RepoTether のタグを読む（Offstage 側では作らない）
- ビルドの出力先は `.cargo/config.toml` で `src-tauri/target` のまま（release.yml の exe のパスと CI のキャッシュがそこを見る）
- ⛔ RepoTether の exe を `cargo build --release` で作らない。画面が埋め込まれず開発サーバを読みに行く exe になり、しかもソースより新しいので `run-repotether.bat` が作り直さない。必ず `pnpm tauri build --no-bundle`（2026-11-12 に一度踏んで作り直した）

## 検索

ファイル名・日付・タグ・git 管理の有無・リポジトリ・区分（個人 / 機密の疑い / 資料）・ファイルの種類・会話に出たか。
全文検索は SQLite の FTS5 trigram（Backlogup で日本語に使えた形）。

## 進め方

1. ✅ RepoTether をワークスペースにする（2026-11-12。試験 68 件と `pnpm tauri build --no-bundle` が通り、exe が起動して窓が出た）
2. 🔶 `offstage-core`: 取り込みと分類（2026-11-12 に分類・洗い出し・通信のクレートの見張りの試験まで。道具が溜めたものの扱いが残り）
3. ✅ 消えたら気づく（前回との比較。2026-11-12。消えた・移動・リポジトリをまたぐ移動・戻った・消してよかった・リポジトリごと無いを試験で確かめた。いつ取り込むかは未定）
4. ✅ 会話との結び付け（2026-11-12。増えた分だけ読む・機密の疑いで中身が送られたものの一覧まで）
5. 🔶 Offstage の画面（2026-11-12。`apps/offstage`、素の HTML/JS（`ui/`）+ Tauri。印のカード（消失・移動・送信・不明）・絞り込み（検索・区分・状態・会話・リポジトリ・種類）・一覧・詳細（場所・規則・付け替え・会話の場面と題）・開く・了解・区分の変更。中身を読む許可の設定は 7 と一緒に作る）
   - 探すフォルダは RepoTether の設定（`%APPDATA%\com.repotether.app\config.json` の `roots` と `scanDepth` だけ）から借りる
   - 起動は `run-offstage.bat`（Tauri の CLI で作る。`cargo build` は画面を埋め込まない）
   - 見本の画面: `python -I apps/offstage/tools/make_demo.py <フォルダ>` で架空のリポジトリ 2 つと会話を作り、`OFFSTAGE_DB`・`OFFSTAGE_ROOTS`・`OFFSTAGE_PROJECTS`・`OFFSTAGE_AUTOREFRESH=1`・`OFFSTAGE_SELECT=<リポジトリ名>/<パス>` を付けて起動、`apps/offstage/tools/shoot.ps1` で撮る。実際のファイル名を画像に写さない
   - 見本で確かめたこと: 1 回目の取り込みで記録と「送信 1」、ファイルを消す・動かしてからの 2 回目で「消失 1」「移動 1」が出て先頭に並ぶ、行を選ぶと詳細が出る。画面の上で「了解」「変更」「戻す」「開く」を押す確かめはまだ（core の試験では確かめた）
   - 通信しない: 依存（通信するクレート・opener/shell/http の plugin）、CSP（`connect-src` は `ipc:` だけ）、権限（`core:default` だけ）、`ui/` に外の URL や fetch が無いことを `apps/offstage/src-tauri/tests/no_network.rs` で確かめる
   - ⛔ 窓のアプリから git を起動すると、1 回ごとに黒い窓が開く（2026-11-12、「更新」で操作できなくなった。見本の 32 リポジトリで 23 個を数えた）。`scan::git_command()`（`CREATE_NO_WINDOW`）だけを使い、`tests/no_console_window.rs` で他の `Command::new` を止める。直したあとは 0 個。git はリポジトリごとに 6 本ずつ並べて回す
   - 会話の場面を押すと、前後のやり取り（直前の人の発言から、場面の後 8 こま）を窓で見せる（2026-11-12）。`link::excerpt`。開くのは Offstage が読んだ記録のファイル（`session_logs` にあるもの）だけ。結果は 1 こま 4,000 文字で切る。同じ行に道具の呼び出しが並ぶので、強調はそのファイル名を含むこまに絞る
   - 人と Claude の文は Markdown として組む（2026-11-12）。marked 18.0.13 と DOMPurify 3.4.17 を `ui/vendor/` に同梱（CDN から読まない。SHA-256 を `tests/no_network.rs` で確かめる）。必ず DOMPurify を通し、画像は読み込まず代わりの文字に、リンクは移動しない文字にする（行き先は吹き出し）。「原文」で元の文字に戻せる
   - 表示は「一覧」と「ツリー」を切り替える（2026-11-12。選んだほうは localStorage に覚える）。ツリーは今の絞り込みの結果をリポジトリ → フォルダ → ファイルに組み直し、フォルダの行に中の数と印（消失・移動・中身が会話に出た）の数を出す。200 件以下なら全部開く。「展開」「畳む」で一度に開閉。ツリーでは 5,000 件まで読む（一覧は 1,000 件）
   - 種類で絞る（2026-11-12）: 一覧の「会話」で出方（読込・添付・検索・編集・命令・道具・作成・結果・本文）を選べる（`Query.kinds`）。会話の窓では、人・Claude・道具・結果・添付のボタンで、こまを隠す・出す（この場面のこまは隠さない）。詳細の欄の場面の一覧も、出方のボタン（件数付き）で隠す・出す。隠した出方は別のファイルを選んでも保つ
   - ⛔ 日本語のフォント（Yu Gothic UI）だと、パスの `\` が `¥` に見える。英数字は Segoe UI を先に指定する
   - ⛔ 見本を撮るときは、撮るプロセスを ID で指定し、窓に自分を描かせる（`shoot.ps1 -ProcessId`、PrintWindow）。2026-11-12、名前で最初の Offstage の窓を選んだら、ユーザーが開いていた実際の Offstage を撮り、実際のファイル名が会話に入った
   - ⛔ エクスプローラーに `/` の混じったパス（`…\sandbox/x.md`）を `/select,` で渡すと、デスクトップが開く（2026-11-12、「開く」でホームしか開かなかった）。`\` にそろえ、`/select,"…"` を `raw_arg` で渡す（`Command::arg` は空白を含む引数を全体ごと引用符で囲み、`/select` が読まれない）。`main.rs` の `explorer_arg` と試験
   - ⛔ Windows のパスの上限（260 文字）: 見本の会話のフォルダ名を、本物と同じく cwd から作ったら超えた。Offstage は記録の中の `cwd` を見るので、フォルダ名は何でもよい
6. ✅ RepoTether の行に印を出す（2026-11-12。取り込みは (b) RepoTether の起動時に裏で、とユーザーが決めた）
   - Offstage: `--refresh` で窓を出さずに取り込んで終わる（Tauri を起動しない）。`--repo <フォルダ>` でそのリポジトリに絞って開く。取り込むたび・了解・区分の変更のたびに、記録の隣へ `summary.json`（リポジトリのパスと消失・移動・送信・不明の数だけ。ファイル名は書かない）を、一時ファイルから置き換えて書く
   - RepoTether: 起動して一覧を読み直したあと、`offstage.exe --refresh` を裏で走らせ（`CREATE_NO_WINDOW`。同時に 2 つは走らせない）、終わったら `summary.json` を読み直す。0 でない行にだけ「消失 n」「送信 n」「移動 n」「Offstage: 不明」の札を出し、行と詳細に「Offstage」ボタン（そのリポジトリで絞って開く）。RepoTether は Offstage の記録の DB にも offstage-core にも依存しない
   - Offstage を探す順: 環境変数 `OFFSTAGE_EXE`、RepoTether の exe と同じフォルダ（同じリポジトリでビルドすると並ぶ）、`%LOCALAPPDATA%\Offstage`・`%ProgramFiles%\Offstage`（インストーラーの入れ先。実機では未確認）。見つからなければボタンも札も出さない（設定は要らない）
   - 記録の DB は `busy_timeout` 60 秒（画面と裏の取り込みが同時に書いても待つ）
   - ⛔ RepoTether と同じフォルダの offstage.exe が `--refresh` を知らない古い版だと、引数を無視して窓を開き、RepoTether はそれが閉じられるまで待つ。両方を同じ版で作り直す
7. 1-2 の全文検索

## 決めていないこと

- 取り込みの起動のしかた（RepoTether の起動時・Offstage の起動時・タスクスケジューラ）
- C:\work\claude のように、リポジトリの外に置いた資料の扱い（RepoTether の一覧に入っていれば対象）
- 人の上書きと規則の追加をどこに書くか（`%USERPROFILE%\.offstage\` の設定ファイルを想定）
