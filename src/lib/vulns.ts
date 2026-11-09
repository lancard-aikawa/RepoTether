// LockWatch の結果 (脆弱性) の絞り込みと数え上げ。画面 (詳細パネルの脆弱性タブ・一覧の印) から使う。
//
// 「隠す」は、深刻度 (low など) か知らせの種類 (unmaintained など) を選ぶ。LockWatch の report --hide と同じ考え方

import type { Assessed, AutoJudgment, VulnRuntime, VulnFinding, VulnNotice, VulnReport, VulnSeverity } from "./types";

export const SEVERITIES: VulnSeverity[] = ["critical", "high", "medium", "low", "unknown"];
/** RustSec の知らせの種類 (脆弱性ではないもの) */
export const INFORMATIONAL = ["unmaintained", "unsound", "notice"];

export const SEVERITY_LABEL: Record<string, string> = {
  critical: "緊急",
  high: "高",
  medium: "中",
  low: "低",
  unknown: "不明",
};

export const INFORMATIONAL_LABEL: Record<string, string> = {
  unmaintained: "保守終了",
  unsound: "安全性の欠陥",
  notice: "お知らせ",
};

/** 悪意あるコードの記録 (OSV の MAL-) に付ける印。LockWatch が深刻度を「緊急」にして渡す */
export const MALICIOUS_LABEL = "悪意あるコード";

/** 画面の印の色 (app.css の .badge.high など) */
export const SEVERITY_BADGE: Record<string, string> = {
  critical: "high",
  high: "high",
  medium: "mid",
  low: "low",
  unknown: "low",
};

export function isHidden(f: VulnFinding, hide: readonly string[]): boolean {
  return hide.includes(f.severity) || (f.informational != null && hide.includes(f.informational));
}

/** 隠すものを除き、重い順 (同じ重さならパッケージ名・ID の順) に並べる */
export function visible(findings: readonly VulnFinding[], hide: readonly string[]): VulnFinding[] {
  const order = (s: string) => {
    const i = SEVERITIES.indexOf(s as VulnSeverity);
    return i < 0 ? SEVERITIES.length : i;
  };
  return findings
    .filter((f) => !isHidden(f, hide))
    .sort((a, b) => order(a.severity) - order(b.severity) || a.package.localeCompare(b.package) || a.id.localeCompare(b.id));
}

/** 深刻度ごとの件数 (0 件のものは入れない。重い順) */
export function counts(findings: readonly VulnFinding[]): [VulnSeverity, number][] {
  return SEVERITIES.map((s) => [s, findings.filter((f) => f.severity === s).length] as [VulnSeverity, number]).filter(
    ([, n]) => n > 0,
  );
}

/** 一覧の行に印を出す数: 隠すものを除いた critical と high */
export function heavyCount(findings: readonly VulnFinding[], hide: readonly string[]): number {
  return findings.filter((f) => !isHidden(f, hide) && (f.severity === "critical" || f.severity === "high")).length;
}

/** そのリポジトリの脆弱性 (隠す前のすべて)。手元に無い・対象でない・まだ照合していないなら空。repoId は LocalRepo.id */
export function findingsOf(report: VulnReport | null | undefined, repoId: string | null | undefined): VulnFinding[] {
  return (repoId != null ? report?.byRepo[repoId]?.result?.findings : undefined) ?? [];
}

/** そのリポジトリの印の数 (一覧の行・マス・上部のタブで同じ数になるように、ここだけで数える) */
export function heavyOf(report: VulnReport | null | undefined, repoId: string | null | undefined, hide: readonly string[]): number {
  return heavyCount(findingsOf(report, repoId), hide);
}

/** その結果に出てくる「隠す」の選択肢 (深刻度 → 知らせの種類の順) */
export function hideChoices(findings: readonly VulnFinding[]): string[] {
  const sev = SEVERITIES.filter((s) => findings.some((f) => f.severity === s));
  const info = INFORMATIONAL.filter((k) => findings.some((f) => f.informational === k));
  // 表に無い知らせの種類が来ても、隠せるようにする
  const other = [...new Set(findings.map((f) => f.informational).filter((k): k is string => !!k && !INFORMATIONAL.includes(k)))];
  return [...sev, ...info, ...other];
}

export function choiceLabel(k: string): string {
  return SEVERITY_LABEL[k] ?? INFORMATIONAL_LABEL[k] ?? k;
}

// ---- 脆弱性タブの絞り込み (名前などの文字とタグ) ----

/** タグの絞り込みで「タグの無いもの」を選んだとき */
export const NO_TAG = "#none";

/** 大文字小文字と、パスの区切り (\ と /) を区別しない (format.ts の searchKey と同じ) */
const key = (s: string) => s.toLowerCase().replace(/\\/g, "/");

/**
 * 文字の絞り込み。texts (名前・パス・タグ・リモートなど) か、脆弱性のパッケージ名・ID・別名のどれかに q が入っていれば当たり。
 * q が空ならすべて当たり
 */
export function queryMatches(texts: readonly (string | null | undefined)[], findings: readonly VulnFinding[], q: string): boolean {
  const k = key(q.trim());
  if (!k) return true;
  const hit = (s: string | null | undefined) => !!s && key(s).includes(k);
  return texts.some(hit) || findings.some((f) => hit(f.package) || hit(f.id) || (f.aliases ?? []).some(hit));
}

/** タグの絞り込み。"仕事" を選ぶと "仕事/客先" の付いたものも当たる。"" はすべて、NO_TAG はタグの無いもの */
export function tagMatches(tags: readonly string[], tag: string): boolean {
  if (!tag) return true;
  if (tag === NO_TAG) return tags.length === 0;
  return tags.some((t) => t === tag || t.startsWith(tag + "/"));
}

/**
 * タグの選択肢: 行に付いているタグとその上の階層を、order (設定のタグの並び) の順に、当たる行の数と一緒に。
 * order に無いものは名前順で後ろに。選んでいるタグ (selected) は 0 件でも残す
 */
export function tagOptions(tagLists: readonly (readonly string[])[], order: readonly string[], selected = ""): { tag: string; count: number }[] {
  const seen = new Set<string>();
  for (const ts of tagLists)
    for (const t of ts) {
      const parts = t.split("/");
      parts.forEach((_, i) => seen.add(parts.slice(0, i + 1).join("/")));
    }
  if (selected && selected !== NO_TAG) seen.add(selected);
  const rank = (t: string) => {
    const i = order.indexOf(t);
    return i < 0 ? order.length : i;
  };
  return [...seen]
    .sort((a, b) => rank(a) - rank(b) || a.localeCompare(b, "ja"))
    .map((tag) => ({ tag, count: tagLists.filter((ts) => tagMatches(ts, tag)).length }));
}

// ---- lock ファイルの健全性の注意 (LockWatch の design.md §3.5)。文は LockWatch の hygiene.describe と同じにする

export const NOTICE_LABEL: Record<string, string> = {
  unpinned: "版を固定していない",
  "not-registry": "レジストリ以外から取得",
  "no-integrity": "ハッシュなし",
  recent: "公開直後の版",
};

/** 公開直後の版とみなす日数 (LockWatch の hygiene.RECENT_DAYS) */
const RECENT_DAYS = 7;

export function noticeLabel(kind: string): string {
  return NOTICE_LABEL[kind] ?? kind;
}

/** 注意 1 件の説明の文 */
export function noticeText(n: Pick<VulnNotice, "kind" | "detail">): string {
  const d = n.detail;
  switch (n.kind) {
    case "unpinned":
      return d ? `版を固定していません (${d})。照合は不正確か、行われていません` : "版の指定がありません。照合されていません";
    case "not-registry":
      return `レジストリ以外から取得: ${d}`;
    case "no-integrity":
      return d
        ? `${d} 個のパッケージにハッシュ (integrity) がありません (ハッシュを書かない古い形式の lock ファイル)`
        : "ハッシュ (integrity) がありません";
    case "recent":
      return `公開から ${RECENT_DAYS} 日たっていません (公開 ${d})`;
    default:
      return d;
  }
}

/** LockWatch の版 ("0.5.0") が major.minor 以上か。使える機能を版で決める (古い LockWatch では押せないようにする) */
export function lockwatchAtLeast(version: string, major: number, minor: number): boolean {
  const [a, b] = version.split(".").map((x) => Number.parseInt(x, 10) || 0);
  return a > major || (a === major && (b ?? 0) >= minor);
}

/** 「影響を調べる」(lockwatch assess) は LockWatch 0.5.0 から */
export const ASSESS_SINCE = "0.5.0";
export const hasAssess = (version: string) => lockwatchAtLeast(version, 0, 5);

// ---- 診断の後に増えた脆弱性の自動の判断 (LockWatch の design.md §7.4) ----

/** 急ぎの札の字 (単語 1 つ)。LockWatch の urgency */
export const URGENCY_WORD: Record<AutoJudgment["urgency"], string> = { now: "急ぎ", soon: "近日", later: "後回し", none: "不要" };
/** どこで使うかの字 (単語 1 つ)。LockWatch の where */
export const WHERE_WORD: Record<string, string> = { release: "リリース", build: "ビルド", dev: "開発", unused: "未使用", unknown: "不明" };
/** 札の色 (app.css の .badge と同じ名前) */
export const URGENCY_TONE: Record<AutoJudgment["urgency"], string> = { now: "high", soon: "mid", later: "low", none: "low" };

const judgeKey = (lockfile: string, pkg: string, version: string, id: string) => `${lockfile}\u0000${pkg}\u0000${version}\u0000${id}`;

/** (lock ファイル, パッケージ, 版, ID) → 判断。同じものが 2 つあれば後の (新しい) 方 */
export function judgmentMap(auto: AutoJudgment[] | undefined): Map<string, AutoJudgment> {
  const m = new Map<string, AutoJudgment>();
  for (const j of auto ?? []) for (const id of j.ids) m.set(judgeKey(j.lockfile, j.package, j.version, id), j);
  return m;
}

export function judgmentOf(m: Map<string, AutoJudgment>, f: Pick<VulnFinding, "lockfile" | "package" | "version" | "id">): AutoJudgment | undefined {
  return m.get(judgeKey(f.lockfile, f.package, f.version, f.id));
}

/** 今の脆弱性のうち、自動で判断したものの数と、そのうち急ぐもの (now・soon) の数。もう消えた脆弱性の判断は数えない */
export function judgedCounts(m: Map<string, AutoJudgment>, findings: VulnFinding[]): { judged: number; urgent: number } {
  let judged = 0;
  let urgent = 0;
  for (const f of findings) {
    const j = judgmentOf(m, f);
    if (!j) continue;
    judged++;
    if (j.urgency === "now" || j.urgency === "soon") urgent++;
  }
  return { judged, urgent };
}

/** 札の tooltip。理由と、だれが判断したか */
export function judgmentTitle(j: AutoJudgment): string {
  const who = j.source === "reused" ? "前回の判断を使った (Claude は見ていない)" : "Claude が調べた";
  const when = j.madeAt ? ` ${j.madeAt.slice(0, 10)}` : "";
  const where = j.where ? `どこで使うか: ${WHERE_WORD[j.where] ?? j.where}\n` : "";
  return `${where}${j.reason}\n(${who}${when})`;
}

/** assess --last の答えで、調べ直しを勧めるか。増えたものがあるときだけ (消えただけなら判断は古くならない)。
 * added の無い古い LockWatch は changed で決める */
export function assessStale(last: Assessed | undefined): boolean {
  if (!last?.exists) return false;
  if (last.added != null) return last.added > 0;
  return !!last.changed;
}

// ---- 言語そのものの版 (LockWatch の design.md §3.6) ----

export const RUNTIME_LABEL: Record<string, string> = { php: "PHP", node: "Node.js" };

/** 一覧で印を付けるもの: サポート終了・終了が近い・修正版がある */
export function runtimeAttention(r: VulnRuntime): boolean {
  return r.status === "ended" || r.status === "soon" || r.security.length > 0;
}

/** 印の文字 (例: "PHP 8.1 終了") */
export function runtimeBrief(r: VulnRuntime): string {
  const what = r.status === "ended" ? "終了" : r.status === "soon" ? "終了間近" : "修正版あり";
  return `${RUNTIME_LABEL[r.name] ?? r.name} ${r.cycle ?? r.version} ${what}`;
}

/** そのプロジェクトで版を聞く言語。RepoTether が判定した言語から (PHP → php、JavaScript・TypeScript → node) */
export function expectedRuntimes(languages: readonly string[]): string[] {
  const out: string[] = [];
  if (languages.includes("PHP")) out.push("php");
  if (languages.some((l) => l === "JavaScript" || l === "TypeScript")) out.push("node");
  return out;
}
