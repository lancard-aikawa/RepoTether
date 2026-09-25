<script lang="ts">
  import type { HistoryItem, Project } from "$lib/derive";
  import { historyItems } from "$lib/derive";
  import { addDays, dayKey, formatDayLong, formatTime, startOfDay } from "$lib/format";
  import { prefs, savePrefs } from "$lib/store.svelte";

  let { projects }: { projects: Project[] } = $props();

  let range = $state(14);
  let projectKey = $state("");
  let query = $state("");
  let mineOnly = $state(true);
  let showCommits = $state(true);
  let showSessions = $state(true);

  const ranges = [
    { days: 1, label: "今日" },
    { days: 7, label: "7 日" },
    { days: 14, label: "14 日" },
    { days: 30, label: "30 日" },
    { days: 90, label: "90 日" },
  ];

  const pickable = $derived(
    projects.filter((p) => p.commits.length || p.sessions.length).sort((a, b) => a.name.localeCompare(b.name, "ja")),
  );

  const items = $derived.by(() => {
    const to = addDays(startOfDay(Date.now()), 1);
    const from = addDays(to, -range);
    const scope = projectKey ? projects.filter((p) => p.key === projectKey) : projects;
    let xs = historyItems(scope, {
      from,
      to,
      mineOnly,
      includeAutomated: prefs.includeAutomated,
      kinds: { commit: showCommits, session: showSessions },
    });
    const q = query.trim().toLowerCase();
    if (q) xs = xs.filter((i) => textOf(i).toLowerCase().includes(q) || i.project.name.toLowerCase().includes(q));
    return xs;
  });

  /** 日 -> プロジェクト -> 出来事 */
  const days = $derived.by(() => {
    const out: { day: string; groups: { project: Project; items: HistoryItem[] }[] }[] = [];
    for (const i of items) {
      const d = dayKey(i.at);
      let day = out[out.length - 1];
      if (!day || day.day !== d) out.push((day = { day: d, groups: [] }));
      let g = day.groups.find((x) => x.project.key === i.project.key);
      if (!g) day.groups.push((g = { project: i.project, items: [] }));
      g.items.push(i);
    }
    return out;
  });

  function textOf(i: HistoryItem): string {
    if (i.kind === "commit") return i.commit.subject;
    const s = i.session;
    return [s.title, s.firstPrompt, s.lastPrompt].filter(Boolean).join(" ");
  }

  function toggleAutomated() {
    prefs.includeAutomated = !prefs.includeAutomated;
    savePrefs();
  }
</script>

<div class="wrap">
  <div class="toolbar bar">
    <div class="segmented" role="group" aria-label="期間">
      {#each ranges as r (r.days)}
        <button class:on={range === r.days} onclick={() => (range = r.days)}>{r.label}</button>
      {/each}
    </div>
    <select bind:value={projectKey} aria-label="プロジェクト">
      <option value="">すべてのプロジェクト</option>
      {#each pickable as p (p.key)}
        <option value={p.key}>{p.name}</option>
      {/each}
    </select>
    <input type="search" placeholder="コミット・プロンプトを検索" bind:value={query} />
    <label class="check"><input type="checkbox" bind:checked={showCommits} /> コミット</label>
    <label class="check"><input type="checkbox" bind:checked={showSessions} /> Claude</label>
    <label class="check"><input type="checkbox" bind:checked={mineOnly} /> 自分のコミットだけ</label>
    <label class="check"
      ><input type="checkbox" checked={prefs.includeAutomated} onchange={toggleAutomated} /> 自動実行のセッションも</label
    >
    <span class="muted num">{items.length} 件</span>
  </div>

  <div class="scroll">
    {#each days as d (d.day)}
      <section class="day">
        <h2 class="day-head">{formatDayLong(d.day)}</h2>
        {#each d.groups as g (g.project.key)}
          <div class="group">
            <div class="g-name">{g.project.name}</div>
            <ul>
              {#each g.items as i, idx (idx)}
                <li>
                  <span class="time num muted">
                    {#if i.kind === "session" && i.firstAt !== i.lastAt}
                      {formatTime(i.firstAt)}〜{formatTime(i.lastAt)}
                    {:else}
                      {formatTime(i.at)}
                    {/if}
                  </span>
                  {#if i.kind === "commit"}
                    <span class="kind">commit</span>
                    <span class="text">
                      {i.commit.subject}
                      {#if !mineOnly}<span class="muted small"> {i.commit.authorName}</span>{/if}
                    </span>
                  {:else}
                    <span class="kind claude">Claude</span>
                    <span class="text">
                      <span>{i.session.title ?? i.session.firstPrompt ?? "(無題)"}</span>
                      {#if i.prompts}<span class="muted small"> プロンプト {i.prompts} 回</span>{/if}
                      {#if !i.session.interactive}<span class="muted small"> (自動)</span>{/if}
                      {#if i.session.lastPrompt}
                        <span class="sub muted small">最後: {i.session.lastPrompt}</span>
                      {/if}
                    </span>
                  {/if}
                </li>
              {/each}
            </ul>
          </div>
        {/each}
      </section>
    {:else}
      <p class="none muted">この期間の出来事はありません</p>
    {/each}
  </div>
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

  .bar select {
    max-width: 220px;
  }

  .scroll {
    overflow-y: auto;
    flex: 1;
    padding: 4px 16px 32px;
  }

  .day {
    max-width: 980px;
  }

  .day-head {
    position: sticky;
    top: 0;
    background: var(--bg);
    padding: 12px 0 6px;
    margin: 0;
    border-bottom: 1px solid var(--line);
    z-index: 1;
  }

  .group {
    display: grid;
    grid-template-columns: 180px 1fr;
    gap: 12px;
    padding: 8px 0;
    border-bottom: 1px solid var(--line);
  }

  .g-name {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  li {
    display: grid;
    grid-template-columns: 96px 52px 1fr;
    gap: 8px;
    align-items: baseline;
  }

  .kind {
    font-size: 10.5px;
    color: var(--muted);
    border: 1px solid var(--line-strong);
    border-radius: 3px;
    padding: 0 4px;
    text-align: center;
  }

  .kind.claude {
    border-color: var(--accent);
    color: var(--ink-2);
  }

  .text {
    min-width: 0;
  }

  .sub {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .small {
    font-size: 12px;
  }

  .none {
    padding: 32px;
    text-align: center;
  }
</style>
