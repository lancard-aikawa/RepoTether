<script lang="ts">
  import type { Project } from "$lib/derive";
  import { hasLeftovers, leftoverWeight } from "$lib/derive";
  import { relative } from "$lib/format";
  import ProjectActions from "./ProjectActions.svelte";
  import ProjectDetail from "./ProjectDetail.svelte";

  let { projects, now }: { projects: Project[]; now: number } = $props();

  type Filter = "all" | "leftovers" | "remote" | "error" | "folder";
  type Sort = "recent" | "stale" | "weight" | "name";

  const filters: { id: Filter; label: string }[] = [
    { id: "all", label: "すべて" },
    { id: "leftovers", label: "取り残しあり" },
    { id: "remote", label: "未クローン" },
    { id: "folder", label: "git 以外" },
    { id: "error", label: "読めない" },
  ];

  let filter = $state<Filter>("all");
  let sort = $state<Sort>("recent");
  let query = $state("");
  let selectedKey = $state<string | null>(null);

  const counts = $derived({
    all: projects.length,
    leftovers: projects.filter(hasLeftovers).length,
    remote: projects.filter((p) => p.kind === "remote").length,
    folder: projects.filter((p) => p.kind === "folder").length,
    error: projects.filter((p) => p.local?.error).length,
  });

  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    let xs = projects.filter((p) => {
      switch (filter) {
        case "leftovers":
          return hasLeftovers(p);
        case "remote":
          return p.kind === "remote";
        case "folder":
          return p.kind === "folder";
        case "error":
          return !!p.local?.error;
        default:
          return true;
      }
    });
    if (q) {
      xs = xs.filter(
        (p) =>
          p.name.toLowerCase().includes(q) ||
          (p.path ?? "").toLowerCase().includes(q) ||
          p.remotes.some((r) => r.fullName.toLowerCase().includes(q)),
      );
    }
    const byRecent = (a: Project, b: Project) => (b.lastActivity ?? 0) - (a.lastActivity ?? 0);
    switch (sort) {
      case "recent":
        return [...xs].sort(byRecent);
      case "stale":
        // 取り残しがあるものを、放置が長い順に先頭へ
        return [...xs].sort(
          (a, b) =>
            Number(hasLeftovers(b)) - Number(hasLeftovers(a)) || (a.lastActivity ?? 0) - (b.lastActivity ?? 0),
        );
      case "weight":
        return [...xs].sort((a, b) => leftoverWeight(b) - leftoverWeight(a) || byRecent(a, b));
      default:
        return [...xs].sort((a, b) => a.name.localeCompare(b.name, "ja"));
    }
  });

  const selected = $derived(projects.find((p) => p.key === selectedKey) ?? null);

  function sessionLine(p: Project): string | null {
    const s = p.lastSession;
    if (!s) return null;
    const title = s.title ?? s.firstPrompt ?? "";
    const last = s.lastPrompt && s.lastPrompt !== s.firstPrompt ? ` — ${s.lastPrompt}` : "";
    return `${title}${last}`;
  }

  function kindLabel(p: Project): string | null {
    if (p.kind === "remote") return null;
    if (p.kind === "folder") return "git 以外";
    return null;
  }
</script>

<div class="layout">
  <section class="list-pane">
    <div class="toolbar bar">
      <input type="search" placeholder="名前・パスで絞り込み" bind:value={query} />
      <div class="segmented" role="group" aria-label="絞り込み">
        {#each filters as f (f.id)}
          <button class:on={filter === f.id} onclick={() => (filter = f.id)}
            >{f.label} <span class="muted num">{counts[f.id]}</span></button
          >
        {/each}
      </div>
      <label class="sort">
        <span class="muted">並び</span>
        <select bind:value={sort}>
          <option value="recent">最近の作業順</option>
          <option value="stale">取り残し・放置が長い順</option>
          <option value="weight">取り残しが重い順</option>
          <option value="name">名前順</option>
        </select>
      </label>
    </div>

    <ul class="list">
      {#each shown as p (p.key)}
        <li class:selected={p.key === selectedKey}>
          <div
            class="row"
            role="button"
            tabindex="0"
            onclick={() => (selectedKey = p.key === selectedKey ? null : p.key)}
            onkeydown={(e) => (e.key === "Enter" || e.key === " ") && (e.preventDefault(), (selectedKey = p.key))}
          >
            <div class="line1">
              <span class="name">{p.name}</span>
              {#each p.hosts as h (h)}<span class="host">{h}</span>{/each}
              {#if kindLabel(p)}<span class="host">{kindLabel(p)}</span>{/if}
              {#if p.local?.branch && p.local.branch !== p.local.defaultBranch}
                <span class="branch mono">{p.local.branch}</span>
              {/if}
              <span class="when muted" title={p.lastActivity ? new Date(p.lastActivity).toLocaleString() : ""}
                >{relative(p.lastActivity, now)}</span
              >
            </div>
            {#if p.path}<div class="path mono muted">{p.path}</div>{/if}
            {#if p.kind === "remote" && p.remotes[0]?.description}
              <div class="path muted">{p.remotes[0].description}</div>
            {/if}
            {#if p.leftovers.length}
              <div class="badges">
                {#each p.leftovers as l (l.kind)}
                  <span class="badge {l.severity}" title={l.detail ?? ""}>{l.label}</span>
                {/each}
              </div>
            {/if}
            {#if sessionLine(p)}
              <div class="session"><span class="tag">Claude</span>{sessionLine(p)}</div>
            {/if}
          </div>
          <div class="row-actions">
            <ProjectActions project={p} compact />
          </div>
        </li>
      {:else}
        <li class="none muted">該当するプロジェクトはありません</li>
      {/each}
    </ul>
  </section>

  {#if selected}
    <ProjectDetail project={selected} {now} onclose={() => (selectedKey = null)} />
  {/if}
</div>

<style>
  .layout {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  .list-pane {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .bar {
    padding: 10px 12px;
    border-bottom: 1px solid var(--line);
  }

  .bar input[type="search"] {
    width: 220px;
  }

  .sort {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    flex: 1;
  }

  li {
    position: relative;
    border-bottom: 1px solid var(--line);
  }

  li.selected {
    background: var(--accent-wash);
  }

  .row {
    padding: 8px 12px 8px 14px;
    cursor: pointer;
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }

  .row:hover {
    background: var(--hover);
  }

  .row:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }

  .line1 {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }

  .name {
    font-weight: 600;
    font-size: 14px;
  }

  .host {
    font-size: 11px;
    color: var(--ink-2);
    border: 1px solid var(--line-strong);
    border-radius: 4px;
    padding: 0 5px;
  }

  .branch {
    font-size: 11px;
    color: var(--ink-2);
  }

  .when {
    margin-left: auto;
    white-space: nowrap;
    font-size: 12px;
  }

  .path {
    font-size: 11.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .badges {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .session {
    color: var(--ink-2);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tag {
    font-size: 10.5px;
    color: var(--muted);
    border: 1px solid var(--line-strong);
    border-radius: 3px;
    padding: 0 4px;
    margin-right: 6px;
  }

  /* 操作ボタンは行にカーソルを乗せたときだけ出す */
  .row-actions {
    position: absolute;
    right: 12px;
    bottom: 8px;
    display: none;
    background: var(--surface);
    border-radius: var(--radius);
    padding: 2px;
    box-shadow: 0 0 0 1px var(--line);
  }

  li:hover .row-actions,
  li:focus-within .row-actions {
    display: block;
  }

  .none {
    padding: 24px;
    text-align: center;
  }
</style>
