// LockWatch の結果 (脆弱性) の絞り込みと数え上げ。画面 (詳細パネルの脆弱性タブ・一覧の印) から使う。
//
// 「隠す」は、深刻度 (low など) か知らせの種類 (unmaintained など) を選ぶ。LockWatch の report --hide と同じ考え方

import type { VulnFinding, VulnNotice, VulnReport, VulnSeverity } from "./types";

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
