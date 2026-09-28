// 状態タブのツリー表示 (フォルダ / タグ)。

import type { Project } from "./derive";
import { hasLeftovers } from "./derive";

export interface TreeNode {
  /** 畳んだ状態を覚えるための ID */
  id: string;
  label: string;
  children: TreeNode[];
  /** このノードに直接ぶら下がるプロジェクト */
  projects: Project[];
  /** 下の階層も含めたプロジェクト数 */
  total: number;
  /** 下の階層も含めた、取り残しのあるプロジェクト数 */
  leftovers: number;
  /** 下の階層も含めた最新の作業 */
  lastActivity: number | null;
  /** 「タグなし」「未クローン」などの特別な入れ物 (末尾に置く) */
  special?: boolean;
}

function node(id: string, label: string, special = false): TreeNode {
  return { id, label, children: [], projects: [], total: 0, leftovers: 0, lastActivity: null, special };
}

function child(parent: TreeNode, id: string, label: string): TreeNode {
  let c = parent.children.find((x) => x.id === id);
  if (!c) parent.children.push((c = node(id, label)));
  return c;
}

/** 数を下から集計し、子を並べる。特別な入れ物は最後。keepOrder なら子は作った順 (タグの並び) のまま */
function finish(n: TreeNode, sortProjects: (xs: Project[]) => Project[], keepOrder = false): TreeNode {
  n.children.forEach((c) => finish(c, sortProjects, keepOrder));
  n.children.sort(
    (a, b) =>
      Number(!!a.special) - Number(!!b.special) || (keepOrder ? 0 : a.label.localeCompare(b.label, "ja")),
  );
  n.projects = sortProjects(n.projects);
  const ids = new Set<string>();
  let left = 0;
  let last: number | null = null;
  const visit = (x: TreeNode) => {
    for (const p of x.projects) {
      if (ids.has(p.key)) continue;
      ids.add(p.key);
      if (hasLeftovers(p)) left++;
      if (p.lastActivity != null && (last == null || p.lastActivity > last)) last = p.lastActivity;
    }
    x.children.forEach(visit);
  };
  visit(n);
  n.total = ids.size;
  n.leftovers = left;
  n.lastActivity = last;
  return n;
}

/**
 * フォルダ (親フォルダ) の階層。1 つの子しか持たない階層は "C:\Repos\mywork" のようにまとめる。
 * パスの無いもの (未クローン) は末尾の入れ物に入れる。
 */
export function folderTree(projects: Project[], sortProjects: (xs: Project[]) => Project[]): TreeNode {
  const root = node("", "");
  const remoteOnly = node("#remote", "未クローン (リモートのみ)", true);
  for (const p of projects) {
    if (!p.path) {
      remoteOnly.projects.push(p);
      continue;
    }
    const sep = p.path.includes("\\") ? "\\" : "/";
    const segs = p.path.split(/[\\/]/).filter(Boolean);
    // macOS / Linux の絶対パスは "/" から
    if (p.path.startsWith("/")) segs.unshift("");
    let cur = root;
    let id = "";
    for (const s of segs.slice(0, -1)) {
      id = id ? `${id}${sep}${s}` : s || "/";
      cur = child(cur, id.toLowerCase(), s || "/");
    }
    cur.projects.push(p);
  }
  if (remoteOnly.projects.length) root.children.push(remoteOnly);
  compress(root, root);
  return finish(root, sortProjects);
}

/** 子が 1 つだけで、自分にプロジェクトがない階層を子とつなげる */
function compress(n: TreeNode, root: TreeNode) {
  for (let i = 0; i < n.children.length; i++) {
    let c = n.children[i];
    while (!c.special && c.projects.length === 0 && c.children.length === 1 && !c.children[0].special) {
      const only = c.children[0];
      const sep = c.label === "/" || c.label.endsWith("\\") ? "" : c.label.includes("/") ? "/" : "\\";
      only.label = `${c.label}${sep}${only.label}`;
      c = only;
    }
    n.children[i] = c;
    compress(c, root);
  }
}

/**
 * 自分で付けたタグの階層 ("仕事/客先/案件")。複数のタグがあれば、それぞれの場所に出る。
 * タグの無いものは末尾の「タグなし」に入れる。
 */
export function tagTree(
  projects: Project[],
  sortProjects: (xs: Project[]) => Project[],
  /** 作ったタグ (並べた順)。プロジェクトが無くても見出しとして出し、この順で並べる */
  defs: string[] = [],
): TreeNode {
  const root = node("", "");
  const untagged = node("#untagged", "タグなし", true);
  const ensure = (t: string) => {
    let cur = root;
    let id = "";
    for (const s of t.split("/").slice(0, 3)) {
      id = id ? `${id}/${s}` : s;
      cur = child(cur, id, s);
    }
    return cur;
  };
  defs.forEach(ensure);
  for (const p of projects) {
    if (!p.tags.length) {
      untagged.projects.push(p);
      continue;
    }
    // 上の階層のタグは、下の階層のタグがあれば飛ばす (同じプロジェクトが親子の両方に出ないように)
    for (const t of p.tags.filter((t) => !p.tags.some((u) => u.startsWith(t + "/")))) {
      const cur = ensure(t);
      if (!cur.projects.includes(p)) cur.projects.push(p);
    }
  }
  if (untagged.projects.length) root.children.push(untagged);
  // タグは自分で並べた順 (defs の順) で出す
  return finish(root, sortProjects, true);
}
