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

test("findingsOf / heavyOf は、結果が無い・対象でない・まだ照合していないリポジトリを 0 件として扱う", () => {
  const repo = (findings: any[] | null): any => ({
    targetId: "x",
    visibility: "private",
    result: findings && { status: "ok", mode: "offline", scannedAt: null, lockfiles: [], findings, notices: [], error: null },
    new: [],
  });
  const report: any = { configured: true, error: null, byRepo: { a: repo(sample), pending: repo(null) } };
  assert.equal(v.findingsOf(report, "a").length, sample.length);
  assert.equal(v.heavyOf(report, "a", []), 2);
  assert.equal(v.heavyOf(report, "a", ["high"]), 1);
  // まだ照合していない・一覧に無い・手元に無い (id が無い)・結果そのものが無い
  assert.equal(v.heavyOf(report, "pending", []), 0);
  assert.equal(v.heavyOf(report, "missing", []), 0);
  assert.equal(v.heavyOf(report, undefined, []), 0);
  assert.deepEqual(v.findingsOf(null, "a"), []);
});

test("LockWatch の版で使える機能を決める", () => {
  assert.equal(v.hasAssess("0.4.0"), false);
  assert.equal(v.hasAssess("0.5.0"), true);
  assert.equal(v.hasAssess("0.10.1"), true); // 文字の順ではなく数で比べる
  assert.equal(v.hasAssess("1.0"), true);
  assert.equal(v.lockwatchAtLeast("0.2.0", 0, 2), true);
  assert.equal(v.lockwatchAtLeast("0.1.9", 0, 2), false);
});

// ---- 脆弱性タブの絞り込み (文字とタグ) ----

const finding = (pkg: string, id: string, aliases: string[] = []): any => ({ package: pkg, id, aliases });

test("queryMatches は名前・パスなどと、パッケージ名・ID・別名に当たる", () => {
  const fs = [finding("lodash", "GHSA-aaaa", ["CVE-2021-1"])];
  assert.ok(v.queryMatches(["RepoTether", "C:\\Repos\\mywork\\RepoTether"], fs, ""));
  assert.ok(v.queryMatches(["RepoTether"], fs, "repot"));
  assert.ok(v.queryMatches(["x", "C:\\Repos\\mywork\\RepoTether"], fs, "repos/mywork"));
  assert.ok(v.queryMatches(["x"], fs, "LODASH"));
  assert.ok(v.queryMatches(["x"], fs, "ghsa-aaaa"));
  assert.ok(v.queryMatches(["x"], fs, "cve-2021"));
  assert.ok(!v.queryMatches(["x", null, undefined], fs, "react"));
});

test("tagMatches は下の階層も含める", () => {
  assert.ok(v.tagMatches(["仕事/客先"], "仕事"));
  assert.ok(v.tagMatches(["仕事"], "仕事"));
  assert.ok(!v.tagMatches(["仕事場"], "仕事"));
  assert.ok(!v.tagMatches(["仕事"], "仕事/客先"));
  assert.ok(v.tagMatches([], ""));
  assert.ok(v.tagMatches([], v.NO_TAG));
  assert.ok(!v.tagMatches(["a"], v.NO_TAG));
});

test("tagOptions は上の階層を足し、設定の順に並べて数える", () => {
  const lists = [["仕事/客先"], ["仕事"], ["自作"], []];
  assert.deepEqual(v.tagOptions(lists, ["自作", "仕事", "仕事/客先"]), [
    { tag: "自作", count: 1 },
    { tag: "仕事", count: 2 },
    { tag: "仕事/客先", count: 1 },
  ]);
  // 選んでいるタグは 0 件でも残す
  assert.deepEqual(v.tagOptions([[]], [], "消えた"), [{ tag: "消えた", count: 0 }]);
});
