// アプリ全体の状態。設定・取り込み結果・更新中の表示。

import * as api from "./api";
import type { Config, Link, LockwatchStatus, Session, Snapshot, VulnReport } from "./types";
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
  /** 全文を開いているセッション */
  openSession: null as Session | null,
  /** 状態タブのフォルダ表示で選んでいる階層。あれば「更新」はその下だけを読み直す */
  focusFolder: null as { path: string; label: string } | null,
  /** LockWatch の結果 (脆弱性)。LockWatch を使わない設定なら configured: false */
  vulns: null as VulnReport | null,
  /** LockWatch が使える状態か (osv-scanner・定期実行など)。脆弱性タブの案内に使う。まだ確かめていなければ null */
  lockwatchStatus: null as LockwatchStatus | null,
});

/** 画面の好み (その端末だけ)。失われても困らないものだけ置く */
export const prefs = $state({
  tab: "state" as Tab,
  includeAutomated: false,
  /** 状態タブの分類: なし / フォルダ / タグ */
  stateGroup: "none" as StateGroup,
  /** 状態タブの見せ方: 一覧 / エクスプローラ (左に木、右に表) */
  stateLayout: "list" as StateLayout,
  /** ツリーで畳んでいるノード ("folder:<id>" / "tag:<id>"。エクスプローラの木は "explorer-folder:<id>" など) */
  collapsed: [] as string[],
  /** エクスプローラ表示で開いている階層 (木のノードの ID。"" はすべて) */
  explorerNode: "",
  /** エクスプローラ表示で、下の階層のプロジェクトもまとめて出す */
  explorerDeep: false,
  /** 詳細パネルで最後に開いたタブ */
  detailTab: "summary" as DetailTab,
  /** 日報の表示: 編集 / 並べて表示 / プレビュー */
  reportView: "split" as ReportViewMode,
  /** 未クローンのフォーク・アーカイブを状態タブで隠す */
  hideForkArchived: false,
  /** 自動更新の間隔 (分)。0 はしない。ローカル = リポジトリ・コミット・セッション、リモート = GitHub / Gogs の一覧 */
  autoLocalMin: 15,
  autoRemoteMin: 60,
  /** 自動更新 (と起動時) に git から読み直すのは、最近この日数に作業したものだけ。0 はすべて */
  autoActiveDays: 90,
  /** リモートを更新するとき、各リポジトリで git fetch もする (既定はしない) */
  fetchOnRemote: false,
  /** テーマ: OS に合わせる / ライト / ダーク */
  theme: "system" as Theme,
  /** 開く端末 (空なら自動: Windows Terminal、無ければ PowerShell) */
  terminal: "",
  /** 一覧の密度: 標準 / コンパクト (行を詰め、Claude の一行要約を省く) */
  density: "normal" as "normal" | "compact",
  /** 設定で最後に開いたタブ */
  settingsTab: "roots" as "roots" | "authors" | "accounts" | "vulns" | "other" | "hidden" | "errors",
  /** 脆弱性で隠すもの (深刻度 low など、知らせの種類 unmaintained など)。一覧の印にも効く */
  vulnHide: [] as string[],
});

export type ReportViewMode = "edit" | "split" | "preview";

export type DetailTab = "summary" | "git" | "commits" | "claude" | "vulns" | "readme";

export type StateGroup = "none" | "folder" | "tag";
export type StateLayout = "list" | "explorer";
export type Theme = "system" | "light" | "dark";

/** テーマと密度を画面に当てる (テーマはウィンドウのタイトルバーにも)。system なら OS の設定に任せる */
export function applyTheme() {
  document.documentElement.dataset.density = prefs.density;
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
    if (!raw) return;
    const saved = JSON.parse(raw);
    // 0.2.0 までの表示形式 (時系列 / フォルダ / タグ) を分類に読み替える
    if (typeof saved.stateView === "string" && saved.stateGroup == null) {
      saved.stateGroup = saved.stateView === "time" ? "none" : saved.stateView;
    }
    delete saved.stateView;
    Object.assign(prefs, saved);
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

// ---- Claude ----

/** セッションの会話の全文を開く */
export function openTranscript(s: Session) {
  app.openSession = s;
}

/** セッションを再開する (選んだ端末で claude -r)。fork なら元の会話を残す */
export async function resumeSession(s: Session, fork: boolean) {
  if (!s.cwd) return toast("このセッションのフォルダが分かりません");
  try {
    await api.claudeResume(s.cwd, s.id, fork, prefs.terminal);
    toast(fork ? "分岐して再開しました (端末を見てください)" : "再開しました (端末を見てください)");
  } catch (e) {
    toast(errorText(e));
  }
}

/** そのフォルダで claude を起動する */
export async function openClaude(path: string) {
  try {
    await api.claudeOpen(path, prefs.terminal);
  } catch (e) {
    toast(errorText(e));
  }
}

/** リモートの一覧を取るアカウントがあるか */
export function hasRemoteAccounts(): boolean {
  return !!app.config?.accounts.some((a) => a.enabled);
}

/**
 * 自動更新。前回の更新 (取り込み結果に残る時刻) から間隔が過ぎていれば、裏で読み直す。
 * ウィンドウが見えていないあいだはしない (見えたときに、過ぎていればすぐ更新する)
 */
export function startAutoRefresh(): () => void {
  const check = () => {
    if (app.busy || document.hidden || !app.snapshot) return;
    const now = Date.now();
    const due = (min: number, at: string | null | undefined) =>
      min > 0 && (!at || now - Date.parse(at) >= min * 60000);
    // リモートを取るときはローカルも一緒に読み直すので、先に見る
    if (hasRemoteAccounts() && due(prefs.autoRemoteMin, app.snapshot.remoteFetchedAt)) refresh(true, autoScope());
    else if (due(prefs.autoLocalMin, app.snapshot.generatedAt)) refresh(false, autoScope());
  };
  const timer = setInterval(check, 30000);
  document.addEventListener("visibilitychange", check);
  return () => {
    clearInterval(timer);
    document.removeEventListener("visibilitychange", check);
  };
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
  // 初回 (キャッシュなし) はリモートも取る。前回の結果が無ければ、範囲を絞ってもすべて読む
  await refresh(app.snapshot == null, autoScope());
}

/** 自動更新と起動時に読み直す範囲: 最近作業したものだけ (設定で「すべて」にもできる) */
export function autoScope(): api.RefreshScope {
  return prefs.autoActiveDays > 0 ? { kind: "active", days: prefs.autoActiveDays } : { kind: "all" };
}

/** 読み直す。scope を省くと (手動の「更新」) すべてのリポジトリを git から読み直す */
export async function refresh(includeRemote: boolean, scope: api.RefreshScope = { kind: "all" }) {
  if (app.busy) return;
  app.busy = true;
  app.error = "";
  app.progress = includeRemote
    ? "リモートも含めて更新しています"
    : scope.kind === "under"
      ? "選んだフォルダの下を更新しています"
      : "更新しています";
  try {
    // fetch はリモートを更新するときだけ (ローカルの読み直しは速さを優先)
    app.snapshot = await api.refresh(includeRemote, includeRemote && prefs.fetchOnRemote, scope);
  } catch (e) {
    app.error = errorText(e);
  } finally {
    app.busy = false;
    app.progress = "";
  }
  // 結果は LockWatch の定期実行で変わるので、更新のたびに読み直す (targets.json も今の一覧にそろう)
  await loadVulns();
}

// ---- 脆弱性 (LockWatch) ----

/** LockWatch の結果を読み直す。失敗は結果の error に入る (画面の更新は止めない) */
export async function loadVulns() {
  try {
    app.vulns = await api.lockwatchResults();
    // 脆弱性タブの案内 (osv-scanner が無い・定期実行が未登録) に使う。一度確かめれば足りる
    if (app.vulns.configured && !app.vulns.error && !app.lockwatchStatus) void loadLockwatchStatus();
  } catch (e) {
    app.vulns = {
      configured: true,
      error: errorText(e),
      locations: null,
      scannedAt: null,
      osvScanner: null,
      dbDownloadedAt: null,
      byRepo: {},
    };
  }
}

/** LockWatch の状態を確かめ直す (設定の場所)。使わない設定・失敗なら null */
export async function loadLockwatchStatus() {
  if (!app.config?.lockwatchPath?.trim()) {
    app.lockwatchStatus = null;
    return;
  }
  try {
    app.lockwatchStatus = await api.lockwatchStatus();
  } catch {
    // 使えない理由は app.vulns.error に出るので、ここでは黙る
    app.lockwatchStatus = null;
  }
}

/** そのリポジトリだけ照合し直す。終わったら結果を差し替える。fresh ならキャッシュを使わない */
export async function scanVulns(repoId: string, fresh = false) {
  app.vulns = await api.lockwatchScan(repoId, fresh);
}

export async function updateConfig(next: Config, opts: { refresh?: boolean; remote?: boolean } = {}) {
  const lockwatchMoved = (next.lockwatchPath ?? "") !== (app.config?.lockwatchPath ?? "");
  app.config = await api.saveConfig(next);
  // LockWatch の場所が変わったら、状態は次の読み込みで確かめ直す
  if (lockwatchMoved) app.lockwatchStatus = null;
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

/** プロジェクトの関連ページを置き換える (空にしたらキーごと消す) */
export async function setLinks(key: string, links: Link[]) {
  if (!app.config) return;
  const all = { ...(app.config.links ?? {}) };
  if (links.length) all[key] = links;
  else delete all[key];
  await updateConfig({ ...app.config, links: all });
}

/** スター (お気に入り) を付ける / 外す */
export async function setStarred(key: string, on: boolean) {
  if (!app.config) return;
  const set = new Set(app.config.starred ?? []);
  if (on) set.add(key);
  else set.delete(key);
  await updateConfig({ ...app.config, starred: [...set] });
}

/** プロジェクトを一覧から外す / 戻す */
export async function setHidden(key: string, hidden: boolean) {
  if (!app.config) return;
  const set = new Set(app.config.hidden);
  if (hidden) set.add(key);
  else set.delete(key);
  await updateConfig({ ...app.config, hidden: [...set] });
}
