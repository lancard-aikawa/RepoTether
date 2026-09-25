<script lang="ts">
  import type { DailyCounts, Project } from "$lib/derive";
  import { buildActivity, hasLeftovers } from "$lib/derive";
  import { addDays, dayKey, formatDayLabel, parseDayKey, startOfDay, weekStart } from "$lib/format";
  import { prefs, savePrefs } from "$lib/store.svelte";

  let { projects, now }: { projects: Project[]; now: number } = $props();

  type Metric = "both" | "commit" | "prompt";
  let metric = $state<Metric>("both");
  const metricLabel: Record<Metric, string> = {
    both: "活動 (コミット + プロンプト)",
    commit: "自分のコミット",
    prompt: "Claude へのプロンプト",
  };

  const WEEKS = 26;
  const CAL_WEEKS = 53;
  const MATRIX_ROWS = 24;

  const activity = $derived(buildActivity(projects, { includeAutomated: prefs.includeAutomated }));

  /** projectKey -> day -> 件数 (選んだ指標) */
  const counts = $derived.by<DailyCounts>(() => {
    if (metric === "commit") return activity.commits;
    if (metric === "prompt") return activity.prompts;
    const out: DailyCounts = new Map();
    for (const src of [activity.commits, activity.prompts]) {
      for (const [k, days] of src) {
        const m = out.get(k) ?? new Map<string, number>();
        for (const [d, n] of days) m.set(d, (m.get(d) ?? 0) + n);
        out.set(k, m);
      }
    }
    return out;
  });

  /** day -> 全プロジェクト合計 */
  const perDay = $derived.by(() => {
    const m = new Map<string, number>();
    for (const days of counts.values()) for (const [d, n] of days) m.set(d, (m.get(d) ?? 0) + n);
    return m;
  });

  const today = $derived(startOfDay(now));
  const thisWeek = $derived(weekStart(now));

  // ---- 数字 (今週) ----
  const tiles = $derived.by(() => {
    const from = thisWeek;
    const inWeek = (d: string) => parseDayKey(d).getTime() >= from;
    const sum = (m: DailyCounts) => {
      let n = 0;
      for (const days of m.values()) for (const [d, c] of days) if (inWeek(d)) n += c;
      return n;
    };
    const active = new Set<string>();
    for (const src of [activity.commits, activity.prompts])
      for (const [k, days] of src) for (const d of days.keys()) if (inWeek(d)) active.add(k);
    return [
      { label: "今週触ったプロジェクト", value: active.size },
      { label: "今週の自分のコミット", value: sum(activity.commits) },
      { label: "今週の Claude へのプロンプト", value: sum(activity.prompts) },
      { label: "取り残しのあるプロジェクト", value: projects.filter(hasLeftovers).length },
    ];
  });

  // ---- 段階 (0〜4)。0 以外の値の四分位で区切る ----
  function thresholds(values: number[]): number[] {
    const xs = values.filter((v) => v > 0).sort((a, b) => a - b);
    if (!xs.length) return [1, 1, 1];
    const q = (p: number) => xs[Math.min(xs.length - 1, Math.floor(p * xs.length))];
    return [q(0.25), q(0.5), q(0.75)];
  }
  function level(v: number, t: number[]): number {
    if (v <= 0) return 0;
    if (v <= t[0]) return 1;
    if (v <= t[1]) return 2;
    if (v <= t[2]) return 3;
    return 4;
  }

  // ---- カレンダー (53 週 × 7 日) ----
  const CELL = 12;
  const GAP = 2;
  const STEP = CELL + GAP;
  const LEFT = 26;
  const TOP = 18;

  const cal = $derived.by(() => {
    const start = addDays(thisWeek, -(CAL_WEEKS - 1) * 7);
    const cells: { x: number; y: number; day: string; v: number }[] = [];
    const months: { x: number; label: string }[] = [];
    let lastMonth = -1;
    for (let w = 0; w < CAL_WEEKS; w++) {
      const ws = addDays(start, w * 7);
      const m = new Date(ws).getMonth();
      if (m !== lastMonth) {
        months.push({ x: LEFT + w * STEP, label: `${m + 1}月` });
        lastMonth = m;
      }
      for (let d = 0; d < 7; d++) {
        const t = addDays(ws, d);
        if (t > today) continue;
        const k = dayKey(t);
        cells.push({ x: LEFT + w * STEP, y: TOP + d * STEP, day: k, v: perDay.get(k) ?? 0 });
      }
    }
    // 月ラベルが詰まりすぎたら間引く
    const spaced = months.filter((m, i) => i === 0 || m.x - months[i - 1].x >= STEP * 3);
    const t = thresholds(cells.map((c) => c.v));
    return {
      cells: cells.map((c) => ({ ...c, l: level(c.v, t) })),
      months: spaced,
      width: LEFT + CAL_WEEKS * STEP,
      height: TOP + 7 * STEP,
    };
  });

  // ---- 週ごと (26 週) ----
  const weeks = $derived.by(() => {
    const out: { start: number; v: number }[] = [];
    for (let w = WEEKS - 1; w >= 0; w--) out.push({ start: addDays(thisWeek, -w * 7), v: 0 });
    const first = out[0].start;
    for (const [d, n] of perDay) {
      const t = parseDayKey(d).getTime();
      if (t < first) continue;
      const i = Math.floor((weekStart(t) - first) / (7 * 86400000) + 0.5);
      if (out[i]) out[i].v += n;
    }
    return out;
  });

  const BAR_H = 140;
  const BAR_SLOT = 28;
  const BAR_W = 18;
  const barMax = $derived(niceMax(Math.max(1, ...weeks.map((w) => w.v))));

  function niceMax(v: number): number {
    const p = Math.pow(10, Math.floor(Math.log10(v)));
    for (const m of [1, 2, 2.5, 5, 10]) if (m * p >= v) return m * p;
    return 10 * p;
  }

  // ---- プロジェクト × 週 ----
  const matrix = $derived.by(() => {
    const first = weeks[0].start;
    const rows: { project: Project; cells: number[]; total: number; last: number }[] = [];
    for (const p of projects) {
      const days = counts.get(p.key);
      if (!days) continue;
      const cells = new Array(WEEKS).fill(0);
      let total = 0;
      let last = -1;
      for (const [d, n] of days) {
        const t = parseDayKey(d).getTime();
        if (t < first) continue;
        const i = Math.floor((weekStart(t) - first) / (7 * 86400000) + 0.5);
        if (i >= 0 && i < WEEKS) {
          cells[i] += n;
          total += n;
          last = Math.max(last, i);
        }
      }
      if (total > 0) rows.push({ project: p, cells, total, last });
    }
    // 最近まで触っているもの → 合計の多いもの の順
    rows.sort((a, b) => b.last - a.last || b.total - a.total);
    const top = rows.slice(0, MATRIX_ROWS);
    const t = thresholds(top.flatMap((r) => r.cells));
    return {
      rows: top.map((r) => ({ ...r, levels: r.cells.map((v) => level(v, t)) })),
      more: rows.length - top.length,
    };
  });

  /** 4〜12 週前には触っていたが、ここ 4 週は触っていないもの */
  const stalled = $derived.by(() => {
    const recentFrom = addDays(thisWeek, -3 * 7);
    const olderFrom = addDays(thisWeek, -12 * 7);
    const out: { project: Project; lastDay: string; total: number }[] = [];
    for (const p of projects) {
      const days = counts.get(p.key);
      if (!days) continue;
      let recent = 0;
      let older = 0;
      let lastDay = "";
      for (const [d, n] of days) {
        const t = parseDayKey(d).getTime();
        if (t >= recentFrom) recent += n;
        else if (t >= olderFrom) older += n;
        if (d > lastDay) lastDay = d;
      }
      if (recent === 0 && older > 0) out.push({ project: p, lastDay, total: older });
    }
    return out.sort((a, b) => (a.lastDay < b.lastDay ? 1 : -1));
  });

  // ---- ツールチップ ----
  let tip = $state<{ x: number; y: number; title: string; body: string } | null>(null);
  function showTip(e: MouseEvent, title: string, body: string) {
    tip = { x: e.clientX, y: e.clientY, title, body };
  }
  function hideTip() {
    tip = null;
  }

  function weekLabel(start: number): string {
    const d = new Date(start);
    return `${d.getMonth() + 1}/${d.getDate()} の週`;
  }

  function unit(v: number): string {
    return metric === "commit" ? `コミット ${v} 件` : metric === "prompt" ? `プロンプト ${v} 回` : `${v} 件`;
  }

  function toggleAutomated() {
    prefs.includeAutomated = !prefs.includeAutomated;
    savePrefs();
  }
</script>

<div class="wrap">
  <div class="toolbar bar">
    <div class="segmented" role="group" aria-label="指標">
      <button class:on={metric === "both"} onclick={() => (metric = "both")}>合計</button>
      <button class:on={metric === "commit"} onclick={() => (metric = "commit")}>コミット</button>
      <button class:on={metric === "prompt"} onclick={() => (metric = "prompt")}>Claude</button>
    </div>
    <label class="check"
      ><input type="checkbox" checked={prefs.includeAutomated} onchange={toggleAutomated} /> 自動実行のセッションも数える</label
    >
  </div>

  <div class="scroll">
    <div class="tiles">
      {#each tiles as t (t.label)}
        <div class="tile panel">
          <div class="t-label">{t.label}</div>
          <div class="t-value">{t.value.toLocaleString()}</div>
        </div>
      {/each}
    </div>

    <section class="panel card">
      <h2>日ごとの{metricLabel[metric]} <span class="muted small">過去 1 年</span></h2>
      <div class="hscroll">
        <svg width={cal.width} height={cal.height} role="img" aria-label="日ごとの活動量のカレンダー">
          {#each cal.months as m (m.x)}
            <text x={m.x} y={10} class="axis">{m.label}</text>
          {/each}
          {#each [["月", 0], ["水", 2], ["金", 4]] as [label, d] (label)}
            <text x={0} y={TOP + Number(d) * STEP + CELL - 2} class="axis">{label}</text>
          {/each}
          {#each cal.cells as c (c.day)}
            <rect
              x={c.x}
              y={c.y}
              width={CELL}
              height={CELL}
              rx="2"
              class="l{c.l}"
              role="presentation"
              onmouseenter={(e) => showTip(e, formatDayLabel(c.day), c.v ? unit(c.v) : "なし")}
              onmousemove={(e) => showTip(e, formatDayLabel(c.day), c.v ? unit(c.v) : "なし")}
              onmouseleave={hideTip}
            />
          {/each}
        </svg>
      </div>
      <div class="legend muted small">
        少ない
        {#each [0, 1, 2, 3, 4] as l (l)}<svg width="12" height="12" aria-hidden="true"
            ><rect width="12" height="12" rx="2" class="l{l}" /></svg
          >{/each}
        多い
      </div>
    </section>

    <section class="panel card">
      <h2>週ごとの{metricLabel[metric]} <span class="muted small">過去 {WEEKS} 週</span></h2>
      <div class="hscroll">
        <svg width={40 + WEEKS * BAR_SLOT} height={BAR_H + 30} role="img" aria-label="週ごとの活動量">
          {#each [0, 0.5, 1] as f (f)}
            <line x1="36" x2={40 + WEEKS * BAR_SLOT} y1={10 + BAR_H * (1 - f)} y2={10 + BAR_H * (1 - f)} class="grid" />
            <text x="30" y={14 + BAR_H * (1 - f)} class="axis" text-anchor="end">{Math.round(barMax * f)}</text>
          {/each}
          {#each weeks as w, i (w.start)}
            {@const h = (w.v / barMax) * BAR_H}
            {@const x = 40 + i * BAR_SLOT + (BAR_SLOT - BAR_W) / 2}
            <!-- 当たり判定は棒より広く (列全体) -->
            <rect
              x={40 + i * BAR_SLOT}
              y="10"
              width={BAR_SLOT}
              height={BAR_H}
              class="hit"
              role="presentation"
              onmouseenter={(e) => showTip(e, weekLabel(w.start), unit(w.v))}
              onmousemove={(e) => showTip(e, weekLabel(w.start), unit(w.v))}
              onmouseleave={hideTip}
            />
            {#if h > 0}
              <path
                class="bar"
                d={`M${x},${10 + BAR_H} v${-Math.max(0, h - 4)} q0,-4 4,-4 h${BAR_W - 8} q4,0 4,4 v${Math.max(0, h - 4)} z`}
                pointer-events="none"
              />
            {/if}
            {#if i % 4 === 0 || i === WEEKS - 1}
              <text x={x + BAR_W / 2} y={BAR_H + 26} class="axis" text-anchor="middle"
                >{new Date(w.start).getMonth() + 1}/{new Date(w.start).getDate()}</text
              >
            {/if}
          {/each}
        </svg>
      </div>
    </section>

    <section class="panel card">
      <h2>プロジェクトごとの{metricLabel[metric]} <span class="muted small">過去 {WEEKS} 週・最近触った順</span></h2>
      {#if matrix.rows.length}
        <div class="hscroll">
          <div class="matrix" style="--cols: {WEEKS}">
            {#each matrix.rows as row (row.project.key)}
              <div class="m-name" title={row.project.path ?? row.project.name}>{row.project.name}</div>
              <div class="m-cells">
                {#each row.levels as l, i (i)}
                  <span
                    class="m-cell l{l}"
                    role="presentation"
                    onmouseenter={(e) =>
                      showTip(e, `${row.project.name} / ${weekLabel(weeks[i].start)}`, row.cells[i] ? unit(row.cells[i]) : "なし")}
                    onmousemove={(e) =>
                      showTip(e, `${row.project.name} / ${weekLabel(weeks[i].start)}`, row.cells[i] ? unit(row.cells[i]) : "なし")}
                    onmouseleave={hideTip}
                  ></span>
                {/each}
              </div>
              <div class="m-total num muted">{row.total}</div>
            {/each}
          </div>
        </div>
        {#if matrix.more > 0}<p class="muted small">ほか {matrix.more} 件</p>{/if}
      {:else}
        <p class="muted">この期間の活動はありません</p>
      {/if}
    </section>

    <section class="panel card">
      <h2>止まっているプロジェクト <span class="muted small">5〜12 週前には触っていて、今週を含む直近 4 週は触っていない</span></h2>
      {#if stalled.length}
        <ul class="stalled">
          {#each stalled as s (s.project.key)}
            <li>
              <span class="name">{s.project.name}</span>
              <span class="muted small">最後 {formatDayLabel(s.lastDay)}</span>
              {#each s.project.leftovers.filter((l) => l.severity !== "info") as l (l.kind)}
                <span class="badge {l.severity}">{l.label}</span>
              {/each}
            </li>
          {/each}
        </ul>
      {:else}
        <p class="muted">ありません</p>
      {/if}
    </section>
  </div>

  {#if tip}
    <div class="tip" style="left: {tip.x + 12}px; top: {tip.y + 12}px">
      <div class="tip-title">{tip.title}</div>
      <div>{tip.body}</div>
    </div>
  {/if}
</div>

<style>
  .wrap {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }

  .bar {
    padding: 10px 12px;
    border-bottom: 1px solid var(--line);
  }

  .scroll {
    overflow-y: auto;
    flex: 1;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: 12px;
  }

  .tile {
    padding: 12px 16px;
  }

  .t-label {
    color: var(--ink-2);
    font-size: 12px;
  }

  .t-value {
    font-size: 26px;
    font-weight: 600;
  }

  .card {
    padding: 14px 16px;
  }

  .hscroll {
    overflow-x: auto;
  }

  svg {
    display: block;
  }

  .axis {
    fill: var(--muted);
    font-size: 10px;
  }

  .grid {
    stroke: var(--line);
    stroke-width: 1;
  }

  .bar {
    fill: var(--accent);
  }

  .hit {
    fill: transparent;
  }

  .hit:hover + .bar {
    filter: brightness(1.1);
  }

  .l0 {
    fill: var(--heat-0);
    background: var(--heat-0);
  }
  .l1 {
    fill: var(--heat-1);
    background: var(--heat-1);
  }
  .l2 {
    fill: var(--heat-2);
    background: var(--heat-2);
  }
  .l3 {
    fill: var(--heat-3);
    background: var(--heat-3);
  }
  .l4 {
    fill: var(--heat-4);
    background: var(--heat-4);
  }

  .legend {
    display: flex;
    align-items: center;
    gap: 3px;
    margin-top: 6px;
  }

  .legend svg {
    display: inline-block;
  }

  .matrix {
    display: grid;
    grid-template-columns: minmax(120px, 200px) max-content 48px;
    justify-content: start;
    gap: 2px 10px;
    align-items: center;
  }

  .m-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .m-cells {
    display: grid;
    grid-template-columns: repeat(var(--cols), 14px);
    gap: 2px;
  }

  .m-cell {
    height: 14px;
    border-radius: 2px;
  }

  .m-total {
    text-align: right;
    font-size: 12px;
  }

  .stalled {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .stalled li {
    display: flex;
    gap: 8px;
    align-items: baseline;
    flex-wrap: wrap;
  }

  .stalled .name {
    font-weight: 600;
  }

  .small {
    font-size: 12px;
  }

  .tip {
    position: fixed;
    pointer-events: none;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    padding: 6px 10px;
    box-shadow: 0 4px 14px rgba(0, 0, 0, 0.15);
    font-size: 12px;
    z-index: 20;
  }

  .tip-title {
    font-weight: 600;
  }
</style>
