// ToolKeeper の結果 (PC に入れた道具の版) の並べ方と言葉。道具のタブから使う。
//
// 区分と印の意味は ToolKeeper の docs/design.md §2・§9.1。言葉は ToolKeeper の labels.py にそろえる

import type { ToolCheck, ToolItem } from "./types";

export const TIER_LABEL: Record<ToolItem["tier"], string> = {
  A: "A 専用",
  B: "B winget",
  C: "C winget",
  D: "D choco",
  E: "E 乗り換え",
};

export const TIER_HINT: Record<ToolItem["tier"], string> = {
  A: "ToolKeeper が専用の手順で上げる (PHP・Ghostscript・Cygwin)",
  B: "winget の開発の道具。ToolKeeper の画面で選んで上げられる",
  C: "winget の残り (自分で更新するアプリ・大きく変わるもの)。見るだけ",
  D: "choco。見るだけ",
  E: "系列のサポートが終わった。乗り換えは人が決める",
};

/** 定期実行 (週 1 回) が止まっているとみなす日数 */
export const STALE_DAYS = 8;

/** タブに出す数: 更新できる A・B と、サポート終了・もうすぐ終了の合計 */
export function tabCount(check: ToolCheck | null | undefined): number {
  return check ? check.summary.updatable + check.summary.eol : 0;
}

const eolBad = (it: ToolItem) => !!it.eol && it.eol.state !== "ok";

/** ToolKeeper の画面で更新のボタンが付くもの (A・B で新しい版があり、手で入れた PHP ではない) */
export function canUpdate(it: ToolItem): boolean {
  return (it.tier === "A" || it.tier === "B") && it.updatable && !it.notes.includes("manual");
}

/** 表に出す行。並びは check.json のまま (E → A → B → C → D)。既定では C・D を隠す (サポート終了の物は出す) */
export function visibleItems(check: ToolCheck | null | undefined, showAll: boolean): ToolItem[] {
  return (check?.items ?? []).filter((it) => showAll || !(it.tier === "C" || it.tier === "D") || eolBad(it));
}

/** 隠している C・D の数 */
export function hiddenCount(check: ToolCheck | null | undefined): number {
  return (check?.items.length ?? 0) - visibleItems(check, false).length;
}

/** 注意の欄の言葉 (ToolKeeper の labels.note_text と同じ) */
export function noteParts(it: ToolItem): string[] {
  const parts: string[] = [];
  if (it.eol && it.eol.state !== "ok") {
    parts.push(`${it.eol.state === "ended" ? "サポート終了" : "もうすぐ終了"}${it.eol.date ? ` ${it.eol.date}` : ""}`);
  }
  for (const n of it.notes) {
    const i = n.indexOf(":");
    const kind = i < 0 ? n : n.slice(0, i);
    const val = i < 0 ? "" : n.slice(i + 1);
    parts.push(
      kind === "manual"
        ? "手で入れた物 (winget へ乗り換え)"
        : kind === "twin"
          ? `winget でも見えている (${val.split(":").slice(1).join(":") || val})`
          : kind === "old"
            ? `古い版 ${val} も入っている`
            : kind === "running"
              ? `動いている: ${val}`
              : kind === "same"
                ? "winget は更新ありと出すが、版の書き方が違うだけで最新"
                : kind === "pinned"
                ? val
                  ? `pin ${val} 件`
                  : "pin"
                : n,
    );
  }
  if (it.packages) parts.push(`古いパッケージ ${it.packages.outdated}/${it.packages.installed}`);
  return parts;
}

/** 最後に調べてから STALE_DAYS 日より経っているか (定期実行が止まっている) */
export function isStale(checkedAt: string | null | undefined, now: number): boolean {
  if (!checkedAt) return false;
  const t = Date.parse(checkedAt);
  return Number.isFinite(t) && now - t > STALE_DAYS * 24 * 3600 * 1000;
}
