// LockWatch の結果 (脆弱性) の絞り込みと数え上げ。画面 (詳細パネルの脆弱性タブ・一覧の印) から使う。
//
// 「隠す」は、深刻度 (low など) か知らせの種類 (unmaintained など) を選ぶ。LockWatch の report --hide と同じ考え方

import type { VulnFinding, VulnSeverity } from "./types";

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
