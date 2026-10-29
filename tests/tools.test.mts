// ToolKeeper の結果の並べ方と言葉 (src/lib/tools.ts) のテスト。`pnpm test` で実行する
import assert from "node:assert/strict";
import { test } from "node:test";
import * as t from "../src/lib/tools.ts";

const item = (key: string, tier: string, over: Record<string, unknown> = {}): any => ({
  key,
  tier,
  name: key,
  installed: "1.0",
  latest: "1.1",
  updatable: true,
  eol: null,
  new: false,
  notes: [],
  packages: null,
  ...over,
});

const check: any = {
  schema: 1,
  toolkeeper: "0.1.0",
  checkedAt: "2026-10-28T19:40:30+09:00",
  summary: { updatable: 13, eol: 1, new: 0, failed: 0 },
  items: [
    item("winget:OpenJS.NodeJS.20", "E", { eol: { product: "nodejs", cycle: "20", date: "2026-04-30", state: "ended" } }),
    item("cygwin", "A", { updatable: false, packages: { installed: 80, outdated: 0, names: [] } }),
    item("php", "A", { notes: ["manual"] }),
    item("winget:Gyan.FFmpeg", "B", { new: true }),
    item("winget:Microsoft.WSL", "C"),
    item("winget:Microsoft.OpenJDK.17", "C", { eol: { product: "x", cycle: "17", date: "2027-01-15", state: "soon" } }),
    item("choco:gh", "D", { notes: ["twin:winget:GitHub.cli"] }),
  ],
  failed: [],
};

test("tabCount は更新できる数とサポート終了の数の合計", () => {
  assert.equal(t.tabCount(check), 14);
  assert.equal(t.tabCount(null), 0);
});

test("visibleItems は既定で C・D を隠し、サポート終了の物は出す。並びは変えない", () => {
  assert.deepEqual(
    t.visibleItems(check, false).map((x) => x.key),
    ["winget:OpenJS.NodeJS.20", "cygwin", "php", "winget:Gyan.FFmpeg", "winget:Microsoft.OpenJDK.17"],
  );
  assert.equal(t.visibleItems(check, true).length, 7);
  assert.equal(t.hiddenCount(check), 2);
});

test("canUpdate は A・B で新しい版があり、手で入れた PHP ではないもの", () => {
  const can = Object.fromEntries(check.items.map((x: any) => [x.key, t.canUpdate(x)]));
  assert.deepEqual(can, {
    "winget:OpenJS.NodeJS.20": false,
    cygwin: false,
    php: false,
    "winget:Gyan.FFmpeg": true,
    "winget:Microsoft.WSL": false,
    "winget:Microsoft.OpenJDK.17": false,
    "choco:gh": false,
  });
});

test("noteParts は ToolKeeper の言葉にそろえる", () => {
  assert.deepEqual(t.noteParts(check.items[0]), ["サポート終了 2026-04-30"]);
  assert.deepEqual(t.noteParts(check.items[1]), ["古いパッケージ 0/80"]);
  assert.deepEqual(t.noteParts(check.items[2]), ["手で入れた物 (winget へ乗り換え)"]);
  assert.deepEqual(t.noteParts(check.items[6]), ["winget でも見えている (GitHub.cli)"]);
  assert.deepEqual(t.noteParts(item("winget:(pinned)", "C", { notes: ["pinned:1"] })), ["pin 1 件"]);
  assert.deepEqual(t.noteParts(item("cygwin", "A", { notes: ["running:mintty.exe"] })), ["動いている: mintty.exe"]);
  assert.deepEqual(t.noteParts(item("winget:Microsoft.OpenJDK.21", "B", { updatable: false, notes: ["same"] })), [
    "winget は更新ありと出すが、版の書き方が違うだけで最新",
  ]);
});

test("isStale は 8 日より前を止まっているとみなす", () => {
  const at = "2026-10-28T09:00:00+09:00";
  const day = 24 * 3600 * 1000;
  assert.equal(t.isStale(at, Date.parse(at) + 7 * day), false);
  assert.equal(t.isStale(at, Date.parse(at) + 9 * day), true);
  assert.equal(t.isStale(null, Date.now()), false);
});
