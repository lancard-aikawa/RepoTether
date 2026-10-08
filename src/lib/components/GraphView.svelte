<script lang="ts">
  import type { DailyCounts, Project } from "$lib/derive";
  import { buildActivity, hasLeftovers, historyItems } from "$lib/derive";
  import { addDays, dayKey, formatDayLabel, formatTime, parseDayKey, startOfDay, weekStart } from "$lib/format";
  import { openProject, openTranscript, prefs, savePrefs } from "$lib/store.svelte";

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

  // ---- 押したマス・棒の中身 (右のパネル) ----
  /** 押した範囲。id は印を付けるマス・棒を見分ける ("day:2026-10-01" / "week:<開始>" / "cell:<プロジェクト>:<開始>") */
  let picked = $state<{ id: string; from: number; to: number; label: string; projectKey?: string } | null>(null);

  function pick(id: string, from: number, days: number, label: string, projectKey?: string) {
    hideTip();
    // 同じ所をもう一度押したら閉じる
    picked = picked?.id === id ? null : { id, from, to: addDays(from, days), label, projectKey };
  }
  const pickDay = (day: string) => pick(`day:${day}`, parseDayKey(day).getTime(), 1, formatDayLabel(day));
  const pickWeek = (start: number) => pick(`week:${start}`, start, 7, weekLabel(start));
  const pickCell = (p: Project, start: number) => pick(`cell:${p.key}:${start}`, start, 7, `${p.name} / ${weekLabel(start)}`, p.key);

  /** 押した範囲の出来事を、プロジェクトごとに (グラフの数え方と同じ: 自分のコミット・選んだ指標) */
  const pickedGroups = $derived.by(() => {
    const sel = picked;
    if (!sel) return [];
    const items = historyItems(sel.projectKey ? projects.filter((p) => p.key === sel.projectKey) : projects, {
      from: sel.from,
      to: sel.to,
      mineOnly: true,
      includeAutomated: prefs.includeAutomated,
      kinds: { commit: metric !== "prompt", session: metric !== "commit" },
    });
    const groups: { project: Project; items: typeof items; commits: number; prompts: number }[] = [];
    for (const i of items) {
      let g = groups.find((x) => x.project.key === i.project.key);
      if (!g) groups.push((g = { project: i.project, items: [], commits: 0, prompts: 0 }));
      g.items.push(i);
      if (i.kind === "commit") g.commits++;
      else g.prompts += i.prompts;
    }
    return groups.sort((a, b) => b.commits + b.prompts - (a.commits + a.prompts));
  });
  const pickedTotal = $derived({
    commits: pickedGroups.reduce((n, g) => n + g.commits, 0),
    prompts: pickedGroups.reduce((n, g) => n + g.prompts, 0),
  });

  /** 1 日だけなら時刻、週なら日付も */
  function when(at: number): string {
    if (picked && picked.to - picked.from <= 86400000) return formatTime(at);
    const d = new Date(at);
    return `${d.getMonth() + 1}/${d.getDate()} ${formatTime(at)}`;
  }

  function counted(commits: number, prompts: number): string {
    return [commits ? `コミット ${commits} 件` : "", prompts ? `プロンプト ${prompts} 回` : ""].filter(Boolean).join("・") || "なし";
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

  <div class="body">
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
      <h2>日ごとの{metricLabel[metric]} <span class="muted small">過去 1 年・マスを押すとその日の中身</span></h2>
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
              class:on={c.v > 0}
              class:sel={picked?.id === `day:${c.day}`}
              role="presentation"
              onclick={() => c.v > 0 && pickDay(c.day)}
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
      <h2>週ごとの{metricLabel[metric]} <span class="muted small">過去 {WEEKS} 週・棒を押すとその週の中身</span></h2>
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
              class:on={w.v > 0}
              class:sel={picked?.id === `week:${w.start}`}
              role="presentation"
              onclick={() => w.v > 0 && pickWeek(w.start)}
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
      <h2>プロジェクトごとの{metricLabel[metric]} <span class="muted small">過去 {WEEKS} 週・最近触った順・マスを押すとその週の中身</span></h2>
      {#if matrix.rows.length}
        <div class="hscroll">
          <div class="matrix" style="--cols: {WEEKS}">
            {#each matrix.rows as row (row.project.key)}
              <button class="m-name" title="状態タブでこのプロジェクトを開く" onclick={() => openProject(row.project.key)}
                >{row.project.name}</button
              >
              <div class="m-cells">
                {#each row.levels as l, i (i)}
                  <span
                    class="m-cell l{l}"
                    class:on={row.cells[i] > 0}
                    class:sel={picked?.id === `cell:${row.project.key}:${weeks[i].start}`}
                    role="presentation"
                    onclick={() => row.cells[i] > 0 && pickCell(row.project, weeks[i].start)}
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
              <button class="name" title="状態タブでこのプロジェクトを開く" onclick={() => openProject(s.project.key)}
                >{s.project.name}</button
              >
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

  {#if picked}
    <aside class="picked">
      <div class="p-head">
        <div>
          <div class="p-title">{picked.label}</div>
          <div class="muted small">{counted(pickedTotal.commits, pickedTotal.prompts)}</div>
        </div>
        <button onclick={() => (picked = null)}>閉じる</button>
      </div>
      <div class="p-scroll">
        {#each pickedGroups as g (g.project.key)}
          <div class="p-group">
            <div class="p-proj">
              <button class="name" title="状態タブでこのプロジェクトを開く" onclick={() => openProject(g.project.key)}
                >{g.project.name}</button
              >
              <span class="muted small">{counted(g.commits, g.prompts)}</span>
            </div>
            <ul>
              {#each g.items as i, idx (idx)}
                <li>
                  <span class="time num muted">{when(i.at)}</span>
                  {#if i.kind === "commit"}
                    <span class="kind">commit</span>
                    <span class="text">{i.commit.subject}</span>
                  {:else}
                    <span class="kind claude">Claude</span>
                    <span class="text">
                      <button class="s-title" onclick={() => openTranscript(i.session)} title="会話の全文を見る"
                        >{i.session.title ?? i.session.firstPrompt ?? "(無題)"}</button
                      >
                      {#if i.prompts}<span class="muted small"> プロンプト {i.prompts} 回</span>{/if}
                    </span>
                  {/if}
                </li>
              {/each}
            </ul>
          </div>
        {:else}
          <p class="muted">この期間の出来事はありません</p>
        {/each}
      </div>
    </aside>
  {/if}
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

  .body {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  .scroll {
    overflow-y: auto;
    flex: 1;
    min-width: 0;
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

  .hit.on {
    cursor: pointer;
  }

  .hit.on:hover {
    fill: var(--hover);
  }

  .hit.on:hover + .bar {
    filter: brightness(1.15);
  }

  .hit.sel {
    fill: var(--hover);
    stroke: var(--accent);
    stroke-width: 1;
  }

  /* 押せるマス (活動のある日・週) は、乗せると枠が出る。選んだマスは枠を残す */
  rect.on,
  .m-cell.on {
    cursor: pointer;
  }

  rect.on:not(.hit):hover,
  rect.sel:not(.hit) {
    stroke: var(--ink);
    stroke-width: 1.5;
  }

  .m-cell.on:hover,
  .m-cell.sel {
    outline: 1.5px solid var(--ink);
    outline-offset: -1px;
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
    text-align: left;
    padding: 0 6px;
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

  .name {
    font-weight: 600;
    padding: 0 6px;
  }

  .picked {
    width: 400px;
    flex: none;
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-left: 1px solid var(--line);
    background: var(--surface);
  }

  .p-head {
    display: flex;
    justify-content: space-between;
    align-items: start;
    gap: 8px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--line);
  }

  .p-title {
    font-weight: 600;
    font-size: 15px;
  }

  .p-scroll {
    overflow-y: auto;
    flex: 1;
    padding: 4px 14px 24px;
  }

  .p-group {
    padding: 10px 0;
    border-bottom: 1px solid var(--line);
  }

  .p-proj {
    display: flex;
    align-items: baseline;
    gap: 8px;
    flex-wrap: wrap;
    margin-bottom: 4px;
  }

  .p-group ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .p-group li {
    display: flex;
    gap: 8px;
    align-items: baseline;
    font-size: 13px;
  }

  .time {
    flex: none;
    font-size: 12px;
  }

  .kind {
    flex: none;
    font-size: 11px;
    color: var(--ink-2);
  }

  .text {
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .s-title {
    text-align: left;
    padding: 0 6px;
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
