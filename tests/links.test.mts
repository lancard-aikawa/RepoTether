// 関連ページ (src/lib/links.ts) のテスト。`pnpm test` で実行する
import assert from "node:assert/strict";
import { test } from "node:test";
import { hostOf, normalizeUrl, titleCandidate } from "../src/lib/links.ts";

test("normalizeUrl: http / https だけを通し、スキームが無ければ https を補う", () => {
  assert.equal(normalizeUrl(" https://resend.com/emails "), "https://resend.com/emails");
  assert.equal(normalizeUrl("dash.cloudflare.com/abc/pages"), "https://dash.cloudflare.com/abc/pages");
  assert.equal(normalizeUrl("http://localhost:4000/"), "http://localhost:4000/");
  assert.equal(normalizeUrl(""), null);
  assert.equal(normalizeUrl("メモ"), null);
  assert.equal(normalizeUrl("a b.com"), null);
  assert.equal(normalizeUrl("javascript:alert(1)"), null);
  assert.equal(normalizeUrl("file:///C:/Windows/win.ini"), null);
});

test("titleCandidate: 取れたタイトルを使い、空やログイン画面のものはホスト名にする", () => {
  const url = "https://console.firebase.google.com/project/x/overview";
  assert.equal(titleCandidate("lancard-aikawa/LockWatch", "https://github.com/lancard-aikawa/LockWatch"), "lancard-aikawa/LockWatch");
  assert.equal(titleCandidate("", url), "console.firebase.google.com");
  assert.equal(titleCandidate("Sign in - Google Accounts", url), "console.firebase.google.com");
  assert.equal(titleCandidate("Log in | Cloudflare", "https://dash.cloudflare.com/"), "dash.cloudflare.com");
  assert.equal(titleCandidate("ログイン - Resend", "https://resend.com/emails"), "resend.com");
  // "login" を含むだけの語は巻き込まない
  assert.equal(titleCandidate("Blogging tips", "https://example.com/"), "Blogging tips");
  assert.equal(hostOf("not a url"), "not a url");
});
