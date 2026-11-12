# 同梱したライブラリ

Offstage は外と通信しないので、CDN から読み込まず、ここに置いたものだけを使う（CSP の `script-src 'self'`）。
中身は RepoTether の `node_modules` にある版をそのまま写した。差し替えたら、下の SHA-256 と
`apps/offstage/src-tauri/tests/no_network.rs` の `VENDOR` を同時に直す（試験が中身を確かめる）。

| ファイル | 元 | 版 | ライセンス | SHA-256 |
|---|---|---|---|---|
| `marked.umd.js` | `node_modules/marked/lib/marked.umd.js` | marked 18.0.13 | MIT（`LICENSE-marked`） | `b147274a9ce27d17276587167e49483d719f6893eeca3a3667a59797661d3556` |
| `purify.min.js` | `node_modules/dompurify/dist/purify.min.js` | DOMPurify 3.4.17 | Apache-2.0 または MPL-2.0（`LICENSE-dompurify`・`LICENSE-dompurify-MPL`） | `cd37898f9782adf54820c413191b5691803f0f137c799d6edcfd38523b4c2c31` |

使い方は `app.js` の `md()`: marked で HTML にし、DOMPurify で必ず無害化し、画像は代わりの文字に、リンクは移動しない文字にする。
