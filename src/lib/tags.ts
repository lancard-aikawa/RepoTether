// タグの操作。タグは "仕事/客先/案件" のような / 区切りの文字列で、3 階層まで。
//
// - config.tags: プロジェクト (prefKey) ごとに付けたタグ
// - config.tagDefs: 作ったタグの一覧。プロジェクトが無くなってもタグは残す (削除するまで)
//
// 操作は Config を受け取って新しい Config を返す純粋な関数。問題があれば Error を投げる。

import type { Config } from "./types";

export const MAX_DEPTH = 3;

/** "a / b / c" → "a/b/c"。空や 3 階層を超えるものは null */
export function normalizeTag(raw: string): string | null {
  const parts = raw
    .split("/")
    .map((x) => x.trim())
    .filter(Boolean);
  if (parts.length === 0 || parts.length > MAX_DEPTH) return null;
  return parts.join("/");
}

/** a が b の上の階層か ("自作" と "自作/アプリ") */
export function isAncestorTag(a: string, b: string): boolean {
  return b.startsWith(a + "/");
}

/** 重複を除き、下の階層のタグがあるときは上の階層のタグを落とす (下の階層が上も表しているので) */
export function tidyTags(tags: string[]): string[] {
  const uniq = [...new Set(tags)];
  return uniq.filter((t) => !uniq.some((u) => isAncestorTag(t, u)));
}

export const depthOf = (t: string) => t.split("/").length;
export const parentOf = (t: string) => (t.includes("/") ? t.slice(0, t.lastIndexOf("/")) : null);
export const leafOf = (t: string) => t.slice(t.lastIndexOf("/") + 1);

/** t の上の階層をすべて ("a/b/c" → ["a", "a/b"]) */
function ancestorsOf(t: string): string[] {
  const parts = t.split("/");
  return parts.slice(0, -1).map((_, i) => parts.slice(0, i + 1).join("/"));
}

/** 存在するタグすべて (作ったもの + 使っているもの + それらの上の階層) */
export function definedTags(cfg: Config): string[] {
  const set = new Set<string>();
  const add = (t: string) => {
    set.add(t);
    ancestorsOf(t).forEach((a) => set.add(a));
  };
  (cfg.tagDefs ?? []).forEach(add);
  Object.values(cfg.tags ?? {}).forEach((ts) => ts.forEach(add));
  return [...set].sort((a, b) => a.localeCompare(b, "ja"));
}

/** path か、その下の階層のタグが付いているプロジェクトの数 */
export function usageCount(cfg: Config, path: string): number {
  return Object.values(cfg.tags ?? {}).filter((ts) => ts.some((t) => t === path || isAncestorTag(path, t))).length;
}

function withTagDefs(cfg: Config, defs: string[]): string[] {
  return [...new Set(defs)].sort((a, b) => a.localeCompare(b, "ja"));
}

/** プロジェクトのタグを付け替える。付けたタグは一覧にも登録する */
export function setProjectTags(cfg: Config, key: string, tags: string[]): Config {
  const next = { ...(cfg.tags ?? {}) };
  const tidy = tidyTags(tags);
  if (tidy.length) next[key] = tidy;
  else delete next[key];
  return { ...cfg, tags: next, tagDefs: withTagDefs(cfg, [...(cfg.tagDefs ?? []), ...tidy]) };
}

/** 空のタグを作る */
export function createTag(cfg: Config, raw: string): Config {
  const t = normalizeTag(raw);
  if (!t) throw new Error(`「仕事/客先/案件」のように、/ 区切りで ${MAX_DEPTH} 階層までにしてください`);
  if (definedTags(cfg).includes(t)) throw new Error(`「${t}」は既にあります`);
  return { ...cfg, tagDefs: withTagDefs(cfg, [...(cfg.tagDefs ?? []), t]) };
}

/**
 * from を to に付け替える (名前の変更と、別の階層への移動の両方)。下の階層も一緒に動く。
 * to が既にあれば合流する
 */
export function renameTag(cfg: Config, from: string, rawTo: string): Config {
  const to = normalizeTag(rawTo);
  if (!to) throw new Error(`「仕事/客先/案件」のように、/ 区切りで ${MAX_DEPTH} 階層までにしてください`);
  if (to === from) return cfg;
  if (isAncestorTag(from, to)) throw new Error(`「${from}」を自分の下の階層へは移せません`);

  const map = (t: string): string => (t === from ? to : isAncestorTag(from, t) ? to + t.slice(from.length) : t);
  const moved = definedTags(cfg).filter((t) => t === from || isAncestorTag(from, t));
  const tooDeep = moved.map(map).find((t) => depthOf(t) > MAX_DEPTH);
  if (tooDeep) throw new Error(`「${tooDeep}」が ${MAX_DEPTH} 階層を超えるので移せません`);

  const tags: Record<string, string[]> = {};
  for (const [k, ts] of Object.entries(cfg.tags ?? {})) tags[k] = tidyTags(ts.map(map));
  // 上の階層だけが作られていた場合も含めて、動いた先を一覧に残す
  const defs = [...(cfg.tagDefs ?? []).map(map), ...moved.map(map)];
  return { ...cfg, tags, tagDefs: withTagDefs(cfg, defs) };
}

/**
 * タグを消す。下の階層も消える。付いていたプロジェクトは親のタグへ移る (親が無ければタグなし)
 */
export function deleteTag(cfg: Config, path: string): Config {
  const parent = parentOf(path);
  const gone = (t: string) => t === path || isAncestorTag(path, t);
  const tags: Record<string, string[]> = {};
  for (const [k, ts] of Object.entries(cfg.tags ?? {})) {
    const next = tidyTags(ts.flatMap((t) => (gone(t) ? (parent ? [parent] : []) : [t])));
    if (next.length) tags[k] = next;
  }
  const defs = (cfg.tagDefs ?? []).filter((t) => !gone(t));
  // 親はプロジェクトが無くなっても残す
  if (parent) defs.push(parent);
  return { ...cfg, tags, tagDefs: withTagDefs(cfg, defs) };
}
