// 日報 / 週報の Markdown を作る。材料は履歴と同じ (自分のコミット + 対話したセッション)。

import type { HistoryItem, Project } from "./derive";
import { historyItems, hasLeftovers } from "./derive";
import { addDays, dayKey, formatDayLong, formatTime, parseDayKey } from "./format";

export interface ReportOptions {
  /** 最終日 "YYYY-MM-DD" */
  day: string;
  /** 何日分か (日報 1、週報 7) */
  days: number;
  includeAutomated: boolean;
  /** 触ったプロジェクトの取り残し (未 push など) も書く */
  includeLeftovers: boolean;
  /** Claude の最後の返答の抜粋も書く */
  includeReplies: boolean;
}

export function buildReport(projects: Project[], o: ReportOptions): string {
  const to = addDays(parseDayKey(o.day).getTime(), 1);
  const from = addDays(to, -o.days);
  const items = historyItems(projects, {
    from,
    to,
    mineOnly: true,
    includeAutomated: o.includeAutomated,
    kinds: { commit: true, session: true },
  });

  const lines: string[] = [];
  const title =
    o.days === 1 ? `日報 ${formatDayLong(o.day)}` : `週報 ${formatDayLong(dayKey(from))} 〜 ${formatDayLong(o.day)}`;
  lines.push(`# ${title}`, "");

  if (items.length === 0) {
    lines.push("記録された作業はありません。", "");
    return lines.join("\n");
  }

  const commitCount = items.filter((i) => i.kind === "commit").length;
  const sessionIds = new Set(items.filter((i) => i.kind === "session").map((i) => i.kind === "session" && i.session.id));
  const projectKeys = new Set(items.map((i) => i.project.key));
  lines.push(`- プロジェクト ${projectKeys.size} 件 / コミット ${commitCount} 件 / Claude セッション ${sessionIds.size} 件`, "");

  if (o.days === 1) {
    writeProjects(lines, items, o, "##");
  } else {
    // 週報は日ごとに見出しを切る
    const byDay = new Map<string, HistoryItem[]>();
    for (const i of items) {
      const k = dayKey(i.at);
      byDay.set(k, [...(byDay.get(k) ?? []), i]);
    }
    for (const k of [...byDay.keys()].sort()) {
      lines.push(`## ${formatDayLong(k)}`, "");
      writeProjects(lines, byDay.get(k)!, o, "###");
    }
  }

  if (o.includeLeftovers) {
    const touched = projects.filter((p) => projectKeys.has(p.key) && hasLeftovers(p));
    if (touched.length) {
      lines.push("## 残っていること", "");
      for (const p of touched) {
        const ls = p.leftovers.filter((l) => l.severity !== "info").map((l) => l.label);
        lines.push(`- **${p.name}**: ${ls.join("、")}`);
      }
      lines.push("");
    }
  }
  return lines.join("\n");
}

function writeProjects(lines: string[], items: HistoryItem[], o: ReportOptions, h: string) {
  const byProject = new Map<string, HistoryItem[]>();
  for (const i of items) byProject.set(i.project.key, [...(byProject.get(i.project.key) ?? []), i]);

  // 作業量の多い順
  const ordered = [...byProject.values()].sort((a, b) => weight(b) - weight(a));
  for (const group of ordered) {
    const p = group[0].project;
    const times = group.flatMap((i) => (i.kind === "session" ? [i.firstAt, i.lastAt] : [i.at]));
    const span = o.days === 1 ? `  (${formatTime(Math.min(...times))}〜${formatTime(Math.max(...times))})` : "";
    lines.push(`${h} ${p.name}${span}`, "");

    const commits = group.filter((i) => i.kind === "commit").sort((a, b) => a.at - b.at);
    const sessions = group.filter((i) => i.kind === "session").sort((a, b) => a.at - b.at);
    if (commits.length) {
      lines.push(`コミット ${commits.length} 件`);
      for (const c of commits) if (c.kind === "commit") lines.push(`- ${c.commit.subject}`);
      lines.push("");
    }
    if (sessions.length) {
      lines.push(`Claude とのやりとり ${sessions.length} 件`);
      for (const s of sessions) {
        if (s.kind !== "session") continue;
        const name = s.session.title ?? s.session.firstPrompt ?? "(無題)";
        const n = s.prompts ? ` (プロンプト ${s.prompts} 回)` : "";
        lines.push(`- ${name}${n}`);
        if (o.includeReplies && s.session.lastReply) {
          lines.push(`  - 最後の返答: ${oneLine(s.session.lastReply, 120)}`);
        }
      }
      lines.push("");
    }
  }
}

function weight(group: HistoryItem[]): number {
  return group.reduce((a, i) => a + (i.kind === "commit" ? 3 : Math.max(1, i.prompts)), 0);
}

function oneLine(s: string, max: number): string {
  const t = s.replace(/\s+/g, " ").trim();
  return t.length > max ? t.slice(0, max) + "…" : t;
}
