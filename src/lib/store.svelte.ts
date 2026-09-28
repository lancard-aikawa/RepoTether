// アプリ全体の状態。設定・取り込み結果・更新中の表示。

import * as api from "./api";
import type { Config, Snapshot } from "./types";
import * as tags_ from "./tags";

export const app = $state({
  config: null as Config | null,
  snapshot: null as Snapshot | null,
  busy: false,
  progress: "",
  error: "",
  toast: "",
});

/** 画面の好み (その端末だけ)。失われても困らないものだけ置く */
export const prefs = $state({
  tab: "state" as Tab,
  includeAutomated: false,
  /** 状態タブの表示形式 */
  stateView: "time" as StateViewMode,
  /** ツリーで畳んでいるノード ("folder:<id>" / "tag:<id>") */
  collapsed: [] as string[],
  /** 詳細パネルで最後に開いたタブ */
  detailTab: "summary" as DetailTab,
  /** 日報の表示: 編集 / 並べて表示 / プレビュー */
  reportView: "split" as ReportViewMode,
  /** 未クローンのフォーク・アーカイブを状態タブで隠す */
  hideForkArchived: false,
  /** 設定で最後に開いたタブ */
  settingsTab: "roots" as "roots" | "authors" | "accounts" | "other" | "hidden" | "errors",
});

export type ReportViewMode = "edit" | "split" | "preview";

export type DetailTab = "summary" | "git" | "commits" | "claude" | "readme";

export type StateViewMode = "time" | "folder" | "tag";

export type Tab = "state" | "history" | "graph" | "report" | "settings";

const PREFS_KEY = "repotether.prefs";

function loadPrefs() {
  try {
    const raw = localStorage.getItem(PREFS_KEY);
    if (raw) Object.assign(prefs, JSON.parse(raw));
  } catch {
    // 読めなければ既定値のまま
  }
}

export function savePrefs() {
  try {
    localStorage.setItem(PREFS_KEY, JSON.stringify(prefs));
  } catch {
    // 保存できなくても動作には影響しない
  }
}

let toastTimer: ReturnType<typeof setTimeout> | undefined;

export function toast(msg: string) {
  app.toast = msg;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (app.toast = ""), 4000);
}

export function errorText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/** 起動時: キャッシュをすぐ表示し、裏でローカルを読み直す */
export async function init() {
  loadPrefs();
  await api.onProgress((m) => (app.progress = m));
  try {
    app.config = await api.getConfig();
    app.snapshot = await api.loadSnapshot();
  } catch (e) {
    app.error = errorText(e);
  }
  // 初回 (キャッシュなし) はリモートも取る
  await refresh(app.snapshot == null);
}

export async function refresh(includeRemote: boolean) {
  if (app.busy) return;
  app.busy = true;
  app.error = "";
  app.progress = includeRemote ? "リモートも含めて更新しています" : "更新しています";
  try {
    app.snapshot = await api.refresh(includeRemote);
  } catch (e) {
    app.error = errorText(e);
  } finally {
    app.busy = false;
    app.progress = "";
  }
}

export async function updateConfig(next: Config, opts: { refresh?: boolean; remote?: boolean } = {}) {
  app.config = await api.saveConfig(next);
  if (opts.refresh) await refresh(opts.remote ?? false);
}

// タグの操作の本体は tags.ts。ここでは設定に反映するだけ
export { isAncestorTag, normalizeTag, tidyTags } from "./tags";

async function applyTags(change: (cfg: Config) => Config) {
  if (!app.config) return;
  await updateConfig(change(app.config));
}

export async function setTags(key: string, tags: string[]) {
  await applyTags((c) => tags_.setProjectTags(c, key, tags));
}

/**
 * タグ表示のドラッグ: from のタグから to のタグへ移す。copy なら from を残す。
 * from / to が null は「タグなし」。何も変わらなければ false
 */
export async function moveTag(
  key: string,
  current: string[],
  from: string | null,
  to: string | null,
  copy: boolean,
): Promise<boolean> {
  if (from === to) return false;
  let next = [...current];
  if (from && !copy) next = next.filter((t) => t !== from);
  if (to && !next.includes(to)) next.push(to);
  next = tags_.tidyTags(next);
  const before = tags_.tidyTags(current);
  if (next.length === before.length && next.every((t) => before.includes(t))) return false;
  await setTags(key, next);
  return true;
}

/** 空のタグを作る */
export const createTag = (raw: string) => applyTags((c) => tags_.createTag(c, raw));
/** 名前の変更・別の階層への移動 (下の階層とプロジェクトのタグも一緒に) */
export const renameTag = (from: string, to: string) => applyTags((c) => tags_.renameTag(c, from, to));
/** 削除。付いていたプロジェクトは親のタグへ */
export const deleteTag = (path: string) => applyTags((c) => tags_.deleteTag(c, path));

/** 存在するタグすべて (候補の表示用) */
export function allTags(): string[] {
  return app.config ? tags_.definedTags(app.config) : [];
}

/** プロジェクトを一覧から外す / 戻す */
export async function setHidden(key: string, hidden: boolean) {
  if (!app.config) return;
  const set = new Set(app.config.hidden);
  if (hidden) set.add(key);
  else set.delete(key);
  await updateConfig({ ...app.config, hidden: [...set] });
}
