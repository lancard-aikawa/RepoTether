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
  /** トーストに付けるボタン (元に戻す など) */
  toastAction: null as { label: string; run: () => void } | null,
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
  /** テーマ: OS に合わせる / ライト / ダーク */
  theme: "system" as Theme,
  /** 設定で最後に開いたタブ */
  settingsTab: "roots" as "roots" | "authors" | "accounts" | "other" | "hidden" | "errors",
});

export type ReportViewMode = "edit" | "split" | "preview";

export type DetailTab = "summary" | "git" | "commits" | "claude" | "readme";

export type StateViewMode = "time" | "folder" | "tag";
export type Theme = "system" | "light" | "dark";

/** テーマを画面とウィンドウのタイトルバーに当てる。system なら OS の設定に任せる */
export function applyTheme() {
  const t = prefs.theme;
  if (t === "system") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = t;
  api.setWindowTheme(t === "system" ? null : t).catch(() => {
    // タイトルバーの色が変わらないだけなので、失敗しても続ける
  });
}

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

export function toast(msg: string, action?: { label: string; run: () => void }) {
  app.toast = msg;
  app.toastAction = action ?? null;
  clearTimeout(toastTimer);
  // ボタン付きは押す時間を見て長めに出す
  toastTimer = setTimeout(() => ((app.toast = ""), (app.toastAction = null)), action ? 8000 : 4000);
}

/** 設定を prev に戻せるボタン付きのトースト (タグの操作の取り消し用) */
export function toastUndo(msg: string, prev: Config | undefined) {
  if (!prev) return toast(msg);
  toast(msg, {
    label: "元に戻す",
    run: async () => {
      try {
        await updateConfig(prev);
        toast("元に戻しました");
      } catch (e) {
        toast(errorText(e));
      }
    },
  });
}

export function errorText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/** 起動時: キャッシュをすぐ表示し、裏でローカルを読み直す */
export async function init() {
  loadPrefs();
  applyTheme();
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

/** タグの操作を設定に反映する。取り消せるように、変更前の設定を返す */
async function applyTags(change: (cfg: Config) => Config): Promise<Config | undefined> {
  if (!app.config) return;
  const prev = JSON.parse(JSON.stringify(app.config)) as Config;
  await updateConfig(change(app.config));
  return prev;
}

export async function setTags(key: string, tags: string[]) {
  return applyTags((c) => tags_.setProjectTags(c, key, tags));
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
): Promise<Config | false | undefined> {
  if (from === to) return false;
  let next = [...current];
  if (from && !copy) next = next.filter((t) => t !== from);
  if (to && !next.includes(to)) next.push(to);
  next = tags_.tidyTags(next);
  const before = tags_.tidyTags(current);
  if (next.length === before.length && next.every((t) => before.includes(t))) return false;
  return setTags(key, next);
}

/** 空のタグを作る */
export const createTag = (raw: string) => applyTags((c) => tags_.createTag(c, raw));
/** 名前の変更・別の階層への移動 (下の階層とプロジェクトのタグも一緒に) */
export const renameTag = (from: string, to: string) => applyTags((c) => tags_.renameTag(c, from, to));
/** 削除。付いていたプロジェクトは親のタグへ */
export const deleteTag = (path: string) => applyTags((c) => tags_.deleteTag(c, path));
/** 別のタグの前・後 (並べ替え) か中 (下の階層へ) に動かす */
export const placeTag = (tag: string, target: string, pos: tags_.Placement) =>
  applyTags((c) => tags_.placeTag(c, tag, target, pos));

/** 存在するタグすべて (自分で並べた順) */
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
