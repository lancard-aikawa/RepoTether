// アプリ全体の状態。設定・取り込み結果・更新中の表示。

import * as api from "./api";
import type { Config, Snapshot } from "./types";

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
});

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

/** タグを "a / b / c" → "a/b/c" に整える。3 階層を超える・空なら null */
export function normalizeTag(raw: string): string | null {
  const parts = raw
    .split("/")
    .map((x) => x.trim())
    .filter(Boolean);
  if (parts.length === 0 || parts.length > 3) return null;
  return parts.join("/");
}

export async function setTags(key: string, tags: string[]) {
  if (!app.config) return;
  const next = { ...(app.config.tags ?? {}) };
  const uniq = [...new Set(tags)];
  if (uniq.length) next[key] = uniq;
  else delete next[key];
  await updateConfig({ ...app.config, tags: next });
}

/** 使われているタグすべて (候補の表示用) */
export function allTags(): string[] {
  const set = new Set<string>();
  for (const ts of Object.values(app.config?.tags ?? {})) for (const t of ts) set.add(t);
  return [...set].sort((a, b) => a.localeCompare(b, "ja"));
}

/** プロジェクトを一覧から外す / 戻す */
export async function setHidden(key: string, hidden: boolean) {
  if (!app.config) return;
  const set = new Set(app.config.hidden);
  if (hidden) set.add(key);
  else set.delete(key);
  await updateConfig({ ...app.config, hidden: [...set] });
}
