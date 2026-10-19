// リモートの表示 (src/lib/remotes.ts) のテスト。`pnpm test` で実行する
import assert from "node:assert/strict";
import { test } from "node:test";
import { plainHttpHost, repoPathOf, webUrlOf } from "../src/lib/remotes.ts";

test("plainHttpHost: 暗号化なし (http://) で外へ出る URL だけを知らせる", () => {
  assert.equal(plainHttpHost("http://gogs.example.com/"), "gogs.example.com");
  assert.equal(plainHttpHost(" http://192.168.1.10:3000 "), "192.168.1.10:3000");
  assert.equal(plainHttpHost("HTTP://Git.Example.com/api/v3"), "git.example.com");
  // https と、空 (GitHub の既定)・書きかけは知らせない
  assert.equal(plainHttpHost("https://git.example.com"), null);
  assert.equal(plainHttpHost(""), null);
  assert.equal(plainHttpHost("git.example.com"), null);
  // この PC の中だけの通信は知らせない
  for (const u of ["http://localhost:3000", "http://127.0.0.1:3000/", "http://[::1]:3000", "http://gogs.localhost"]) {
    assert.equal(plainHttpHost(u), null, u);
  }
  // 127 で始まるだけの名前は別のホスト
  assert.equal(plainHttpHost("http://127.example.com"), "127.example.com");
});

test("repoPathOf: いろいろな形の URL から「所有者/リポジトリ名」を大文字小文字のまま取り出す", () => {
  assert.equal(repoPathOf("https://github.com/lancard-aikawa/LPortMan.git"), "lancard-aikawa/LPortMan");
  assert.equal(repoPathOf("https://user@github.com/Foo/Bar"), "Foo/Bar");
  assert.equal(repoPathOf("git@github.com:Foo/Bar.git"), "Foo/Bar");
  assert.equal(repoPathOf("ssh://git@gogs.example.com:2222/RoundCube/Plugin.git"), "RoundCube/Plugin");
  // Gogs をサブパスに置いた https
  assert.equal(repoPathOf("https://git.example.com:3000/gogs/aikawa/Sasayui.git"), "aikawa/Sasayui");
  // Backlog の ssh
  assert.equal(repoPathOf("space@space.git.backlog.jp:/PROJ/app.git"), "PROJ/app");
});

test("repoPathOf: ローカルのパスを remote にしている場合も末尾 2 階層", () => {
  assert.equal(repoPathOf(String.raw`C:\repos\bare\app.git`), "bare/app");
  assert.equal(repoPathOf("/srv/git/app.git"), "git/app");
});

test("webUrlOf: Backlog は git のホストから Web の URL を作る", () => {
  assert.equal(
    webUrlOf("space@space.git.backlog.jp:/PROJ/app.git", "space.git.backlog.jp/proj/app"),
    "https://space.backlog.jp/git/proj/app",
  );
  assert.equal(webUrlOf("https://u@github.com/a/b.git", "github.com/a/b"), "https://github.com/a/b");
});
