# RepoTether を入れたら、つなぐ道具もすぐ使える

2026-11-13 に決めた (ユーザーの選択は「B」)。

## なぜ

RepoTether がつなぐ道具は 4 つあり、使う人はそれぞれを別に用意していた。

| 道具 | 作り | これまでの用意 |
|---|---|---|
| Offstage | Rust (このリポジトリの `apps/offstage`) | 別の Release のインストーラー |
| LockWatch | Python (標準ライブラリだけ、画面は tkinter) | git clone・Python 3.11 以上・uv sync・osv-scanner・設定に場所 |
| ToolKeeper | Python (同上) | git clone・Python・設定に場所 |
| SessionVault | Python (標準ライブラリだけ) | git clone か exe・設定に場所 |

手間の本体はダウンロードの回数より、「使う PC に Python があり、リポジトリを clone してある」前提のほう。

## 決めたこと

1. **Offstage は RepoTether に同梱する** (済み、0.18.0 から)。同じリポジトリで、同じ版でないと困る
   (古い Offstage は `--refresh` を知らずに窓を開き、RepoTether が待ち続ける) ので、同じソースから作って
   `offstage.exe` を `RepoTether.exe` の隣に入れる。RepoTether は隣を最初に探す。
   - release.yml: Offstage を Tauri の CLI で作り (`cargo build` だと画面の無い exe になる)、`src-tauri/bundle-extra/` に写し、
     `--config src-tauri/tauri.bundle.json` でインストーラーに入れる。target/release を直接指さないのは、
     tauri-build が resources を target/release へ写すので、同じファイルを自分自身へ写すことになるから
   - インストーラーの一番上に `offstage.exe` があることを `7z l -slt` で確かめる
   - 手元: `run-repotether.bat` が起動前に `run-offstage.bat ensure` (無いか古ければ作るだけ) を呼ぶ
   - Offstage だけの Release (`offstage-v*`) は、Offstage だけ使う人のために残す
2. **LockWatch・ToolKeeper は RepoTether の中から入れる** (0.18.0 から)。単独でも使う道具で、出す間隔も別なので同梱しない。
   - 各リポジトリの Release に、それだけで動く exe を固めた zip (`LockWatch-<版>-win-x64.zip` など) を付ける
     (LockWatch の design.md §9.1、ToolKeeper の §9.3)。コマンド用の `<名前>.exe` と、窓を出さない `<名前>w.exe` (画面と定期実行) の 2 つ
   - 脆弱性タブの LockWatch の欄と、道具タブの ToolKeeper の欄に「入れる」(`core/tools.rs`、部品は `ToolInstall.svelte`)。
     GitHub の Release の最新版から zip を取り、API が返す SHA-256 (`digest`) で確かめ、`%LOCALAPPDATA%\RepoTether\tools\<名前>\` に展開する。
     場所が空なら、その場所を設定に書いて使い始める。新しい版があれば「更新」、ほかの場所を使っていれば「使う」
   - 取る先は `core/tools.rs` に書いた道具・リポジトリだけ (画面から任意の URL は受けない)。SHA-256 の無いファイルは入れない
   - 更新は、zip の中身だけを入れ替える (設定・データ・`osv-scanner.exe`・入れた印 `repotether-tool.json` は残す)。
     古いものを全部脇へ動かせてから新しいものを置き、動かせないもの (開いている画面の exe) があれば何も変えずにやめる
   - LockWatch を入れるとき、PATH に osv-scanner が無ければ、版を固定した 2.6.0 を GitHub から取って隣に置く (56 MB。回線によっては数分)
   - 今の「場所」(clone したフォルダ) の設定はそのまま使える (開発する人向け)。RepoTether は場所が exe の形 (`<名前>.exe` と `<名前>w.exe` がある) か、
     リポジトリ (`src/<名前>`) かを見て呼び方を変える
   - SessionVault は Release が無く、RepoTether は読むだけの検査 (`verify`) にしか使わないので、まだ入れられない (今は自分で置く)

### 確かめたこと (2026-11-13)

- 手元で固めた LockWatch 0.7.0・ToolKeeper 0.3.0 の zip を作業用のフォルダに入れ (入れる・更新の 2 回)、RepoTether の呼び方で `status` が動いた
  (`cargo test --lib real_zips -- --ignored`、`TOOLS_DIR` と `TOOLS_ZIP_*` を入れる)
- 固めた exe の画面 (`lockwatchw.exe gui`・`toolkeeperw.exe gui`) が開き、窓の題が版つきで出て、黒い窓は出なかった
- osv-scanner 2.6.0 を GitHub から取り、SHA-256 を確かめて置けた (`cargo test --lib real_osv -- --ignored`。3〜4 分かかった)
- まだ: exe の zip を付けた Release が無いので、画面の「入れる」から GitHub を通した入れ方は動かしていない
