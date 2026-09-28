// 取り込み結果 (Snapshot) を、画面で使う「プロジェクト」単位にまとめる。
//
// プロジェクトは 3 種類:
// - local: ローカルにある git リポジトリ (リモート一覧と照合できれば remotes が付く)
// - remote: リモート一覧にだけあり、まだクローンしていないもの
// - folder: git リポジトリではないが、Claude のセッションがあるフォルダ

import type { Commit, Config, LocalRepo, RemoteRepo, Session, Snapshot } from "./types";
import { dayKey, toMs } from "./format";
import { linkOfRemote, linksOfLocal, type RemoteLink } from "./remotes";

export type ProjectKind = "local" | "remote" | "folder";
export type Severity = "high" | "mid" | "low" | "info";

export interface Leftover {
  kind: string;
  label: string;
  severity: Severity;
  detail?: string;
}

export interface Project {
  key: string;
  /** 設定 (非表示・タグ) で使うキー */
  prefKey: string;
  /** 自分で付けたタグ */
  tags: string[];
  kind: ProjectKind;
  name: string;
  path: string | null;
  local: LocalRepo | null;
  remotes: RemoteRepo[];
  /** リモート (GitHub / Gogs などの種類と、ブラウザで開く URL)。無ければ空 */
  links: RemoteLink[];
  sessions: Session[];
  commits: Commit[];
  myCommits: Commit[];
  lastSession: Session | null;
  /** 最新の作業 (下の 3 つの一番新しいもの)。並び替えに使う */
  lastActivity: number | null;
  /** Claude と最後にやりとりした時刻 */
  lastClaudeAt: number | null;
  /** 最新のコミット (自分の分。履歴期間に無ければ、誰かの最新コミット) */
  lastGitAt: number | null;
  /** リモートに最後に push された時刻 (未クローンのもの、またはローカルの活動が履歴期間に無いもの) */
  lastPushAt: number | null;
  /** 対応するリモートがフォーク / アーカイブ済みか */
  isFork: boolean;
  isArchived: boolean;
  /** 未コミットの変更があるとき、変更したファイルの一番新しい更新時刻 */
  lastEditAt: number | null;
  leftovers: Leftover[];
  hidden: boolean;
}

export interface BuildOptions {
  /** 自動実行 (SDK) のセッションも活動に数えるか */
  includeAutomated: boolean;
}

export function isMine(email: string, cfg: Config): boolean {
  if (cfg.authorEmails.length === 0) return true;
  const e = email.toLowerCase();
  return cfg.authorEmails.some((a) => a.trim().toLowerCase() === e);
}

export function countsAsActivity(s: Session, opt: BuildOptions): boolean {
  return s.interactive || (opt.includeAutomated && s.promptCount > 0);
}

function pathKey(p: string): string {
  return p.replace(/\//g, "\\").replace(/\\+$/, "").toLowerCase();
}

export function buildProjects(snap: Snapshot, cfg: Config, opt: BuildOptions): Project[] {
  const hidden = new Set(cfg.hidden);
  const tagsOf = (k: string) => cfg.tags?.[k] ?? [];
  const commitsByRepo = groupBy(snap.commits, (c) => c.repoId);
  const sessionsByRepo = groupBy(
    snap.sessions.filter((s) => s.repoId),
    (s) => s.repoId!,
  );

  // リモートのキー -> リモート一覧のリポジトリ
  const remoteByKey = new Map<string, RemoteRepo>();
  for (const r of snap.remoteRepos) remoteByKey.set(r.key, r);
  const matchedRemote = new Set<string>();

  const projects: Project[] = [];

  for (const repo of snap.repos) {
    const remotes: RemoteRepo[] = [];
    for (const rm of repo.remotes) {
      const r = rm.key ? remoteByKey.get(rm.key) : undefined;
      if (r && !remotes.includes(r)) {
        remotes.push(r);
        matchedRemote.add(r.key);
      }
    }
    const commits = commitsByRepo.get(repo.id) ?? [];
    const sessions = sortSessions(sessionsByRepo.get(repo.id) ?? []);
    projects.push(
      finish(
        {
          key: repo.id,
          prefKey: repo.id,
          tags: tagsOf(repo.id),
          kind: "local",
          name: repo.name,
          path: repo.path,
          local: repo,
          remotes,
          links: linksOfLocal(repo, remotes, cfg),
          sessions,
          commits,
          myCommits: commits.filter((c) => isMine(c.authorEmail, cfg)),
          lastSession: null,
          lastActivity: null,
          lastClaudeAt: null,
          lastGitAt: null,
          lastPushAt: null,
          isFork: false,
          isArchived: false,
          lastEditAt: null,
          leftovers: [],
          hidden: hidden.has(repo.id),
        },
        opt,
      ),
    );
  }

  for (const r of snap.remoteRepos) {
    if (matchedRemote.has(r.key)) continue;
    projects.push(
      finish(
        {
          key: `remote:${r.key}`,
          prefKey: r.key,
          tags: tagsOf(r.key),
          kind: "remote",
          name: r.name,
          path: null,
          local: null,
          remotes: [r],
          links: [linkOfRemote(r, cfg)],
          sessions: [],
          commits: [],
          myCommits: [],
          lastSession: null,
          lastActivity: null,
          lastClaudeAt: null,
          lastGitAt: null,
          lastPushAt: null,
          isFork: false,
          isArchived: false,
          lastEditAt: null,
          leftovers: [],
          hidden: hidden.has(r.key),
        },
        opt,
      ),
    );
  }

  // リポジトリ外のフォルダで使われたセッション
  const orphan = groupBy(
    snap.sessions.filter((s) => !s.repoId && s.cwd),
    (s) => pathKey(s.cwd!),
  );
  for (const [key, sessions] of orphan) {
    const path = sessions[0].cwd!;
    projects.push(
      finish(
        {
          key: `folder:${key}`,
          prefKey: `folder:${key}`,
          tags: tagsOf(`folder:${key}`),
          kind: "folder",
          name: path.split(/[\\/]/).filter(Boolean).pop() ?? path,
          path,
          local: null,
          remotes: [],
          links: [],
          sessions: sortSessions(sessions),
          commits: [],
          myCommits: [],
          lastSession: null,
          lastActivity: null,
          lastClaudeAt: null,
          lastGitAt: null,
          lastPushAt: null,
          isFork: false,
          isArchived: false,
          lastEditAt: null,
          leftovers: [],
          hidden: hidden.has(`folder:${key}`),
        },
        opt,
      ),
    );
  }

  return projects;
}

function finish(p: Project, opt: BuildOptions): Project {
  const active = p.sessions.filter((s) => countsAsActivity(s, opt));
  p.lastSession = active[0] ?? null;

  p.lastClaudeAt = toMs(p.lastSession?.endedAt);
  p.lastEditAt = toMs(p.local?.dirtyModifiedAt);
  const mine = toMs(p.myCommits[0]?.at);
  p.lastGitAt = mine;
  let last = maxOf([mine, p.lastClaudeAt, p.lastEditAt]);
  // 履歴期間より前のものしか無ければ、最新コミット (誰の分でも) やリモートの push 時刻で代用する
  if (last == null) {
    p.lastGitAt = toMs(p.local?.lastCommitAt);
    p.lastPushAt = maxOf(p.remotes.map((r) => toMs(r.pushedAt ?? r.updatedAt)));
    last = maxOf([p.lastGitAt, p.lastPushAt]);
  }
  p.isFork = p.remotes.some((r) => r.fork);
  p.isArchived = p.remotes.some((r) => r.archived);
  p.lastActivity = last;
  p.leftovers = leftoversOf(p);
  return p;
}

export function leftoversOf(p: Project): Leftover[] {
  const out: Leftover[] = [];
  if (p.kind === "remote") {
    out.push({ kind: "notCloned", label: "未クローン", severity: "info" });
    return out;
  }
  const r = p.local;
  if (!r) return out;
  if (r.error) {
    const dubious = r.error.includes("dubious ownership");
    out.push({
      kind: dubious ? "dubious" : "error",
      label: dubious ? "所有者チェックで読めない" : "読めない",
      severity: "mid",
      detail: r.error.split("\n")[0],
    });
    return out;
  }
  if (r.conflicted > 0) out.push({ kind: "conflict", label: `競合 ${r.conflicted}`, severity: "high" });
  const dirty = r.staged + r.modified + r.untracked;
  if (dirty > 0) {
    const parts = [];
    if (r.staged) parts.push(`ステージ済み ${r.staged}`);
    if (r.modified) parts.push(`変更 ${r.modified}`);
    if (r.untracked) parts.push(`未追跡 ${r.untracked}`);
    out.push({ kind: "dirty", label: `未コミット ${dirty}`, severity: "mid", detail: parts.join(" / ") });
  }
  if (r.ahead > 0) out.push({ kind: "ahead", label: `未 push ${r.ahead}`, severity: "high" });
  // リモートが無いことは、リモートの欄に「なし」と出すので取り残しには入れない
  if (r.remotes.length > 0 && !r.upstream && r.branch) {
    out.push({
      kind: "noUpstream",
      label: "upstream 未設定",
      severity: "low",
      detail: `${r.branch} に追跡ブランチがありません。push 済みかどうかを判定できません`,
    });
  }
  const unpushedBranches = r.branches.filter((b) => b.name !== r.branch && (b.ahead > 0 || (!b.upstream && !b.merged)));
  if (unpushedBranches.length)
    out.push({
      kind: "branchUnpushed",
      label: `未 push のブランチ ${unpushedBranches.length}`,
      severity: "mid",
      detail: unpushedBranches.map((b) => b.name).join(", "),
    });
  const unmerged = r.branches.filter((b) => !b.merged && b.name !== r.branch);
  if (unmerged.length)
    out.push({
      kind: "unmerged",
      label: `未マージのブランチ ${unmerged.length}`,
      severity: "low",
      detail: unmerged.map((b) => b.name).join(", "),
    });
  const gone = r.branches.filter((b) => b.gone);
  if (gone.length)
    out.push({
      kind: "gone",
      label: `リモートで消えたブランチ ${gone.length}`,
      severity: "low",
      detail: gone.map((b) => b.name).join(", "),
    });
  if (r.stashes > 0) out.push({ kind: "stash", label: `stash ${r.stashes}`, severity: "low" });
  if (r.behind > 0) out.push({ kind: "behind", label: `取り込み待ち ${r.behind}`, severity: "info" });
  if (r.branch == null && r.head) out.push({ kind: "detached", label: "detached HEAD", severity: "low" });
  return out;
}

/** 取り残しに数えないもの。読めないリポジトリは「読めない」で別に絞り込む */
const NOT_LEFTOVER = new Set(["noUpstream", "dubious", "error"]);

/** 「取り残し」とみなすもの (情報だけのものは除く) */
export function hasLeftovers(p: Project): boolean {
  return p.leftovers.some((l) => l.severity !== "info" && !NOT_LEFTOVER.has(l.kind));
}

export function leftoverWeight(p: Project): number {
  const w: Record<Severity, number> = { high: 100, mid: 10, low: 1, info: 0 };
  return p.leftovers.reduce((a, l) => a + w[l.severity], 0);
}

// ---- 活動 (履歴・グラフ・日報の共通の材料) ----

export type ActivityKind = "commit" | "prompt";

/** projectKey -> dayKey -> 件数 */
export type DailyCounts = Map<string, Map<string, number>>;

export interface Activity {
  commits: DailyCounts;
  prompts: DailyCounts;
}

export function buildActivity(projects: Project[], opt: BuildOptions): Activity {
  const commits: DailyCounts = new Map();
  const prompts: DailyCounts = new Map();
  for (const p of projects) {
    for (const c of p.myCommits) {
      const t = toMs(c.at);
      if (t != null) bump(commits, p.key, dayKey(t));
    }
    for (const s of p.sessions) {
      if (!countsAsActivity(s, opt)) continue;
      for (const ts of s.promptTimes) {
        const t = toMs(ts);
        if (t != null) bump(prompts, p.key, dayKey(t));
      }
    }
  }
  return { commits, prompts };
}

function bump(m: DailyCounts, key: string, day: string) {
  let inner = m.get(key);
  if (!inner) m.set(key, (inner = new Map()));
  inner.set(day, (inner.get(day) ?? 0) + 1);
}

// ---- 履歴 ----

export type HistoryItem =
  | { kind: "commit"; at: number; project: Project; commit: Commit }
  | {
      kind: "session";
      at: number;
      project: Project;
      session: Session;
      /** その日のプロンプト数 */
      prompts: number;
      firstAt: number;
      lastAt: number;
    };

export interface HistoryOptions extends BuildOptions {
  from: number;
  to: number;
  mineOnly: boolean;
  kinds: { commit: boolean; session: boolean };
}

/** from <= t < to の出来事。セッションは日ごとに 1 件に分ける */
export function historyItems(projects: Project[], o: HistoryOptions): HistoryItem[] {
  const items: HistoryItem[] = [];
  for (const p of projects) {
    if (o.kinds.commit) {
      for (const c of o.mineOnly ? p.myCommits : p.commits) {
        const t = toMs(c.at);
        if (t != null && t >= o.from && t < o.to) items.push({ kind: "commit", at: t, project: p, commit: c });
      }
    }
    if (o.kinds.session) {
      for (const s of p.sessions) {
        if (!countsAsActivity(s, o)) continue;
        const byDay = new Map<string, number[]>();
        const times = s.promptTimes.length ? s.promptTimes : [s.startedAt].filter(isString);
        for (const ts of times) {
          const t = toMs(ts);
          if (t == null || t < o.from || t >= o.to) continue;
          const k = dayKey(t);
          const arr = byDay.get(k) ?? [];
          arr.push(t);
          byDay.set(k, arr);
        }
        for (const arr of byDay.values()) {
          arr.sort((a, b) => a - b);
          items.push({
            kind: "session",
            at: arr[arr.length - 1],
            project: p,
            session: s,
            prompts: s.promptTimes.length ? arr.length : 0,
            firstAt: arr[0],
            lastAt: arr[arr.length - 1],
          });
        }
      }
    }
  }
  items.sort((a, b) => b.at - a.at);
  return items;
}

// ---- 小物 ----

export function groupBy<T, K>(xs: T[], f: (x: T) => K): Map<K, T[]> {
  const m = new Map<K, T[]>();
  for (const x of xs) {
    const k = f(x);
    const arr = m.get(k);
    if (arr) arr.push(x);
    else m.set(k, [x]);
  }
  return m;
}

function sortSessions(xs: Session[]): Session[] {
  return [...xs].sort((a, b) => (toMs(b.endedAt) ?? 0) - (toMs(a.endedAt) ?? 0));
}

function isString(x: unknown): x is string {
  return typeof x === "string";
}

function maxOf(xs: (number | null)[]): number | null {
  let m: number | null = null;
  for (const x of xs) if (x != null && (m == null || x > m)) m = x;
  return m;
}

/** コミットに現れるメールアドレスを多い順に (設定画面の候補) */
export function emailCandidates(snap: Snapshot): { email: string; name: string; count: number }[] {
  const m = new Map<string, { email: string; name: string; count: number }>();
  for (const c of snap.commits) {
    const k = c.authorEmail.toLowerCase();
    const e = m.get(k) ?? { email: c.authorEmail, name: c.authorName, count: 0 };
    e.count++;
    m.set(k, e);
  }
  return [...m.values()].sort((a, b) => b.count - a.count);
}
