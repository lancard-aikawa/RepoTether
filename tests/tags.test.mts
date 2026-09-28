// タグの操作 (src/lib/tags.ts) のテスト。`pnpm test` で実行する (Node の型除去で .ts をそのまま読む)
import assert from "node:assert/strict";
import { test } from "node:test";
import * as t from "../src/lib/tags.ts";

const base = (): any => ({
  tags: { A: ["自作/ツール"], B: ["自作/ツール/ポート"], C: ["仕事"] },
  tagDefs: [],
});
test("definedTags は上の階層も含む", () => {
  assert.deepEqual(t.definedTags(base()), ["仕事", "自作", "自作/ツール", "自作/ツール/ポート"]);
});

test("createTag: 空のタグが一覧に残る / 重複と 4 階層は拒否", () => {
  const c = t.createTag(base(), " 趣味 / 写真 ");
  assert.ok(t.definedTags(c).includes("趣味/写真"));
  assert.throws(() => t.createTag(c, "趣味/写真"), /既にあります/);
  assert.throws(() => t.createTag(c, "a/b/c/d"), /3 階層/);
});

test("renameTag: 名前の変更で下の階層とプロジェクトも変わる", () => {
  const c = t.renameTag(base(), "自作/ツール", "自作/道具");
  assert.deepEqual(c.tags.A, ["自作/道具"]);
  assert.deepEqual(c.tags.B, ["自作/道具/ポート"]);
  assert.ok(!t.definedTags(c).includes("自作/ツール"));
});

test("renameTag: 別の階層へ移す", () => {
  const c = t.renameTag(base(), "自作/ツール", "仕事/ツール");
  assert.deepEqual(c.tags.B, ["仕事/ツール/ポート"]);
  // 移した元の親 (自作) は、ほかに無ければ消えてよい。移動先の親 (仕事) は残る
  assert.ok(t.definedTags(c).includes("仕事"));
});

test("renameTag: 3 階層を超える移動と、自分の下への移動は拒否", () => {
  assert.throws(() => t.renameTag(base(), "自作/ツール", "仕事/x/ツール"), /3 階層/);
  assert.throws(() => t.renameTag(base(), "自作", "自作/ツール/中"), /自分の下/);
});

test("renameTag: 既にあるタグへは合流する", () => {
  const c = t.renameTag(base(), "仕事", "自作/ツール");
  assert.deepEqual(c.tags.C, ["自作/ツール"]);
});

test("deleteTag: プロジェクトは親へ、親は残る", () => {
  const c = t.deleteTag(base(), "自作/ツール");
  assert.deepEqual(c.tags.A, ["自作"]);
  assert.deepEqual(c.tags.B, ["自作"]);
  assert.ok(t.definedTags(c).includes("自作"));
  assert.ok(!t.definedTags(c).some((x: string) => x.startsWith("自作/ツール")));
});

test("deleteTag: 一番上を消すとタグなしになる", () => {
  const c = t.deleteTag(base(), "仕事");
  assert.equal(c.tags.C, undefined);
  assert.ok(!t.definedTags(c).includes("仕事"));
});

test("usageCount は下の階層も数える", () => {
  assert.equal(t.usageCount(base(), "自作"), 2);
  assert.equal(t.usageCount(base(), "自作/ツール/ポート"), 1);
});

test("setProjectTags: 付けたタグは一覧に残り、外しても消えない", () => {
  let c = t.setProjectTags(base(), "D", ["新規/タグ"]);
  c = t.setProjectTags(c, "D", []);
  assert.equal(c.tags.D, undefined);
  assert.ok(t.definedTags(c).includes("新規/タグ"));
});
