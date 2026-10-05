// LockWatch の結果の絞り込み (src/lib/vulns.ts) のテスト。`pnpm test` で実行する
import assert from "node:assert/strict";
import { test } from "node:test";
import * as v from "../src/lib/vulns.ts";

const f = (pkg: string, id: string, severity: string, informational: string | null = null): any => ({
  lockfile: "Cargo.lock",
  ecosystem: "crates.io",
  package: pkg,
  version: "1.0.0",
  id,
  aliases: [],
  severity,
  score: null,
  fixed: [],
  informational,
  summary: "",
});

const sample = [
  f("unic-common", "RUSTSEC-2025-0080", "unknown", "unmaintained"),
  f("h2", "RUSTSEC-2026-0258", "unknown"),
  f("glib", "RUSTSEC-2024-0429", "medium", "unsound"),
  f("vite", "GHSA-b", "high"),
  f("axios", "GHSA-a", "critical"),
  f("cookie", "GHSA-c", "low"),
];

test("visible は重い順に並べ、隠すものを除く", () => {
  assert.deepEqual(
    v.visible(sample, []).map((x) => x.package),
    ["axios", "vite", "glib", "cookie", "h2", "unic-common"],
  );
  // 保守終了だけを隠しても、知らせでない unknown (h2) は残る
  assert.deepEqual(
    v.visible(sample, ["unmaintained"]).map((x) => x.package),
    ["axios", "vite", "glib", "cookie", "h2"],
  );
  // 深刻度でも隠せる
  assert.deepEqual(
    v.visible(sample, ["low", "unknown"]).map((x) => x.package),
    ["axios", "vite", "glib"],
  );
});

test("heavyCount は隠すものを除いた critical と high", () => {
  assert.equal(v.heavyCount(sample, []), 2);
  assert.equal(v.heavyCount(sample, ["critical"]), 1);
});

test("counts は 0 件の深刻度を出さない", () => {
  assert.deepEqual(v.counts([f("a", "1", "high"), f("b", "2", "high"), f("c", "3", "low")]), [
    ["high", 2],
    ["low", 1],
  ]);
});

test("hideChoices はその結果に出てくるものだけ。表に無い知らせの種類も選べる", () => {
  assert.deepEqual(v.hideChoices(sample), ["critical", "high", "medium", "low", "unknown", "unmaintained", "unsound"]);
  assert.deepEqual(v.hideChoices([f("a", "1", "unknown", "yanked")]), ["unknown", "yanked"]);
  assert.equal(v.choiceLabel("unmaintained"), "保守終了");
  assert.equal(v.choiceLabel("yanked"), "yanked");
});

test("注意の文は LockWatch の表示と同じ。知らない種類もそのまま出す", () => {
  assert.equal(v.noticeText({ kind: "unpinned", detail: ">=1.24.1" }), "版を固定していません (>=1.24.1)。照合は不正確か、行われていません");
  assert.equal(v.noticeText({ kind: "unpinned", detail: "" }), "版の指定がありません。照合されていません");
  assert.equal(v.noticeText({ kind: "not-registry", detail: "git+ssh://github.com/x/y.git" }), "レジストリ以外から取得: git+ssh://github.com/x/y.git");
  assert.equal(v.noticeText({ kind: "no-integrity", detail: "" }), "ハッシュ (integrity) がありません");
  assert.match(v.noticeText({ kind: "no-integrity", detail: "577" }), /^577 個のパッケージ/);
  assert.match(v.noticeText({ kind: "recent", detail: "2026-10-03T08:00:00Z" }), /7 日たっていません \(公開 2026-10-03T08:00:00Z\)/);
  assert.equal(v.noticeText({ kind: "future", detail: "x" }), "x");
  assert.equal(v.noticeLabel("recent"), "公開直後の版");
  assert.equal(v.noticeLabel("future"), "future");
});
