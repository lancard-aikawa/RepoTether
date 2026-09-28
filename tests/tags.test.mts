// タグの操作 (src/lib/tags.ts) のテスト。`pnpm test` で実行する (Node の型除去で .ts をそのまま読む)
import assert from "node:assert/strict";
import { test } from "node:test";
import * as t from "../src/lib/tags.ts";

const base = (): any => ({
  tags: { A: ["自作/ツール"], B: ["自作/ツール/ポート"], C: ["仕事"] },
  tagDefs: [],
});
test("definedTags は上の階層も含み、上の階層を先に置く", () => {
  assert.deepEqual([...t.definedTags(base())].sort(), ["仕事", "自作", "自作/ツール", "自作/ツール/ポート"].sort());
  const d = t.definedTags(base());
  assert.ok(d.indexOf("自作") < d.indexOf("自作/ツール"));
});

test("definedTags は tagDefs の順を保つ", () => {
  const c = { tags: {}, tagDefs: ["b", "a", "b/y", "b/x"] } as any;
  assert.deepEqual(t.definedTags(c), ["b", "a", "b/y", "b/x"]);
});

// 同じ階層の子の並び (表示の順)
const kids = (c: any, parent: string | null) =>
  t.definedTags(c).filter((x: string) => t.parentOf(x) === parent);

test("placeTag: before / after で同じ階層の中を並べ替える", () => {
  const c0 = { tags: {}, tagDefs: ["A", "B", "C"] } as any;
  assert.deepEqual(kids(t.placeTag(c0, "C", "A", "before"), null), ["C", "A", "B"]);
  assert.deepEqual(kids(t.placeTag(c0, "A", "B", "after"), null), ["B", "A", "C"]);
});

test("placeTag: 下の階層にあるタグを一番上の階層のタグの前へ出す (中に入れてしまったのを戻す)", () => {
  const c0 = { tags: { P: ["RoundCube/私事"] }, tagDefs: ["仕事", "RoundCube", "RoundCube/私事"] } as any;
  const c = t.placeTag(c0, "RoundCube/私事", "仕事", "before");
  assert.deepEqual(kids(c, null), ["私事", "仕事", "RoundCube"]);
  assert.deepEqual(c.tags.P, ["私事"]);
});

test("placeTag: into は中へ。下の階層も一緒に動き、自分の下へは入れられない", () => {
  const c0 = { tags: {}, tagDefs: ["A", "A/x", "B"] } as any;
  const c = t.placeTag(c0, "A", "B", "into");
  assert.ok(t.definedTags(c).includes("B/A/x"));
  assert.throws(() => t.placeTag(c0, "A", "A/x", "before"), /自分の下/);
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
