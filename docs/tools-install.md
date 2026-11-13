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
2. **Python の 3 つは RepoTether の中から入れる** (未着手)。単独でも使う道具で、出す間隔も別なので同梱しない。
   - 各リポジトリの Release に、それだけで動く exe を固めた zip を付ける (PyInstaller 6.22.3 以上。Tk 9 の zipfs のため)
   - RepoTether に「道具」の欄を作り、「入れる」で GitHub の Release から zip を取り、API が返す SHA-256 (`digest`) で確かめ、
     `%LOCALAPPDATA%\RepoTether\tools\<名前>\` に展開し、設定の「場所」を自動で書く。新しい版があれば「更新」
   - 取る先は RepoTether に書いた決まった道具・決まったリポジトリだけ (任意の URL は受けない)
   - LockWatch の osv-scanner も同じ形で取る
   - 今の「場所」(clone したフォルダ) の設定はそのまま使えるようにする (開発する人向け)
