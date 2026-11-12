<!-- release-offstage.yml が Release の本文の後ろに付ける「初めての方へ」 -->
## 初めての方へ

Offstage は [RepoTether](https://github.com/lancard-aikawa/RepoTether) と同じリポジトリの別アプリです。どちらかを入れてください (Windows 10 / 11、64 bit)。

- **`Offstage_<版>_x64-setup.exe`** — インストーラー。スタートメニューに登録します。WebView2 が無ければ入れます
- **`Offstage-<版>-win-x64.zip`** — 展開した `Offstage.exe` を起動するだけです

署名していないので、初回は SmartScreen が「PC が保護されました」と出します。「詳細情報」→「実行」で起動してください。

`git` コマンド (Git for Windows) が必要です。探すフォルダは RepoTether の設定から読みます。RepoTether を使っていなければ、
環境変数 `OFFSTAGE_ROOTS` に探すフォルダを `;` 区切りで入れてください。使い方は [README](https://github.com/lancard-aikawa/RepoTether/blob/main/apps/offstage/README.md) にあります。
