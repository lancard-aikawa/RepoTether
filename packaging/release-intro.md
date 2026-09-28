<!-- release.yml が Release の本文の後ろに付ける「初めての方へ」 -->
## 初めての方へ

どちらかを入れてください (Windows 10 / 11、64 bit)。

- **`RepoTether_<版>_x64-setup.exe`** — インストーラー。スタートメニューに登録します。WebView2 が無ければ入れます
- **`RepoTether-<版>-win-x64.zip`** — 展開した `RepoTether.exe` を起動するだけです

署名していないので、初回は SmartScreen が「PC が保護されました」と出します。「詳細情報」→「実行」で起動してください。

`git` コマンド (Git for Windows) が必要です。GitHub の一覧を gh のログインで取るなら GitHub CLI も入れて、
`gh auth login --web` でログインしておいてください。使い方は [README](https://github.com/lancard-aikawa/RepoTether#readme) にあります。
