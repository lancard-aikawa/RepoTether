<script lang="ts">
  import type { Project } from "$lib/derive";
  import { hasLeftovers, leftoverWeight } from "$lib/derive";
  import type { RemoteKind } from "$lib/remotes";
  import { relative } from "$lib/format";
  import { folderTree, tagTree, type TreeNode } from "$lib/tree";
  import { prefs, savePrefs, type StateViewMode } from "$lib/store.svelte";
  import ProjectDetail from "./ProjectDetail.svelte";
  import ProjectRow from "./ProjectRow.svelte";

  let { projects, now }: { projects: Project[]; now: number } = $props();

  type Filter = "all" | "leftovers" | "remote" | "error" | "folder";
  type Sort = "recent" | "stale" | "weight" | "name";
  type Visibility = "shown" | "hidden" | "all";
  type RemoteFilter = "" | RemoteKind | "none";

  const filters: { id: Filter; label: string }[] = [
    { id: "all", label: "すべて" },
    { id: "leftovers", label: "取り残しあり" },
    { id: "remote", label: "未クローン" },
    { id: "folder", label: "git 以外" },
    { id: "error", label: "読めない" },
  ];

  const views: { id: StateViewMode; label: string }[] = [
    { id: "time", label: "時系列" },
    { id: "folder", label: "フォルダ" },
    { id: "tag", label: "タグ" },
  ];

  const remoteLabels: Record<Exclude<RemoteFilter, "">, string> = {
    github: "GitHub",
    gogs: "Gogs",
    gitea: "Gitea",
    gitlab: "GitLab",
    backlog: "Backlog",
    other: "その他",
    none: "なし",
  };

  let filter = $state<Filter>("all");
  let sort = $state<Sort>("recent");
  let visibility = $state<Visibility>("shown");
  let remoteFilter = $state<RemoteFilter>("");
  let query = $state("");
  let selectedKey = $state<string | null>(null);

  function matchesRemote(p: Project, k: RemoteFilter): boolean {
    if (!k) return true;
    if (k === "none") return p.kind === "local" && p.links.length === 0 && !p.local?.error;
    return p.links.some((l) => l.kind === k);
  }

  function matchesFilter(p: Project, f: Filter): boolean {
    switch (f) {
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
  }

  // 表示 / 非表示で絞ったもの。ほかの絞り込みの件数はこれを母数にする
  const byVisibility = $derived(
    projects.filter((p) => (visibility === "all" ? true : visibility === "hidden" ? p.hidden : !p.hidden)),
  );

  const counts = $derived(
    Object.fromEntries(
      filters.map((f) => [f.id, byVisibility.filter((p) => matchesRemote(p, remoteFilter) && matchesFilter(p, f.id)).length]),
    ) as Record<Filter, number>,
  );

  /** 実際に出てくるリモートの種類だけを選択肢にする */
  const remoteOptions = $derived.by(() => {
    const base = byVisibility.filter((p) => matchesFilter(p, filter));
    return (Object.keys(remoteLabels) as Exclude<RemoteFilter, "">[])
      .map((k) => ({ id: k, label: remoteLabels[k], count: base.filter((p) => matchesRemote(p, k)).length }))
      .filter((o) => o.count > 0 || o.id === remoteFilter);
  });

  const hiddenCount = $derived(projects.filter((p) => p.hidden).length);

  function sortProjects(xs: Project[]): Project[] {
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
  }

  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    let xs = byVisibility.filter((p) => matchesFilter(p, filter) && matchesRemote(p, remoteFilter));
    if (q) {
      xs = xs.filter(
        (p) =>
          p.name.toLowerCase().includes(q) ||
          (p.path ?? "").toLowerCase().includes(q) ||
          p.tags.some((t) => t.toLowerCase().includes(q)) ||
          p.remotes.some((r) => r.fullName.toLowerCase().includes(q)) ||
          p.links.some((l) => l.label.toLowerCase().includes(q) || l.url.toLowerCase().includes(q)),
      );
    }
    return xs;
  });

  const flat = $derived(prefs.stateView === "time" ? sortProjects(shown) : []);
  const tree = $derived(
    prefs.stateView === "folder"
      ? folderTree(shown, sortProjects)
      : prefs.stateView === "tag"
        ? tagTree(shown, sortProjects)
        : null,
  );
  const hasAnyTag = $derived(projects.some((p) => p.tags.length));

  const selected = $derived(projects.find((p) => p.key === selectedKey) ?? null);

  // ---- 畳む ----
  const collapsed = $derived(new Set(prefs.collapsed));
  const cKey = (n: TreeNode) => `${prefs.stateView}:${n.id}`;

  function toggle(n: TreeNode) {
    const k = cKey(n);
    prefs.collapsed = collapsed.has(k) ? prefs.collapsed.filter((x) => x !== k) : [...prefs.collapsed, k];
    savePrefs();
  }

  function setAll(open: boolean) {
    if (!tree) return;
    const keys: string[] = [];
    const walk = (n: TreeNode) => n.children.forEach((c) => (keys.push(cKey(c)), walk(c)));
    walk(tree);
    const rest = prefs.collapsed.filter((k) => !keys.includes(k));
    prefs.collapsed = open ? rest : [...rest, ...keys];
    savePrefs();
  }

  function setView(v: StateViewMode) {
    prefs.stateView = v;
    savePrefs();
  }

  function select(p: Project) {
    selectedKey = p.key === selectedKey ? null : p.key;
  }
</script>

{#snippet branch(n: TreeNode, depth: number)}
  {#each n.children as c (c.id)}
    {@const open = !collapsed.has(cKey(c))}
    <li class="group" style="--depth: {depth}">
      <button class="group-head" class:special={c.special} aria-expanded={open} onclick={() => toggle(c)}>
        <svg class="chev" class:open viewBox="0 0 16 16" aria-hidden="true"><path d="M6 4l4 4-4 4" /></svg>
        <span class="g-label">{c.label}</span>
        <span class="g-count num">{c.total}</span>
        {#if c.leftovers}<span class="badge mid">取り残し {c.leftovers}</span>{/if}
        <span class="g-when muted">{relative(c.lastActivity, now)}</span>
      </button>
      {#if open}
        <ul class="sub">
          {@render branch(c, depth + 1)}
          {#each c.projects as p (p.key)}
            <ProjectRow
              project={p}
              {now}
              selected={p.key === selectedKey}
              onselect={() => select(p)}
              showTags={prefs.stateView !== "tag"}
            />
          {/each}
        </ul>
      {/if}
    </li>
  {/each}
{/snippet}

<div class="layout">
  <section class="list-pane">
    <div class="bar">
      <div class="toolbar">
        <input type="search" placeholder="名前・パス・タグで絞り込み" bind:value={query} />
        <div class="segmented" role="group" aria-label="表示形式">
          {#each views as v (v.id)}
            <button class:on={prefs.stateView === v.id} onclick={() => setView(v.id)}>{v.label}</button>
          {/each}
        </div>
        <label class="inline">
          <span class="muted">並び</span>
          <select bind:value={sort}>
            <option value="recent">最近の作業順</option>
            <option value="stale">取り残し・放置が長い順</option>
            <option value="weight">取り残しが重い順</option>
            <option value="name">名前順</option>
          </select>
        </label>
        {#if tree}
          <button class="ghost" onclick={() => setAll(true)}>すべて開く</button>
          <button class="ghost" onclick={() => setAll(false)}>すべて畳む</button>
        {/if}
      </div>
      <div class="toolbar">
        <div class="segmented" role="group" aria-label="状態で絞り込み">
          {#each filters as f (f.id)}
            <button class:on={filter === f.id} onclick={() => (filter = f.id)}
              >{f.label} <span class="muted num">{counts[f.id]}</span></button
            >
          {/each}
        </div>
        <label class="inline">
          <span class="muted">リモート</span>
          <select bind:value={remoteFilter}>
            <option value="">すべて</option>
            {#each remoteOptions as o (o.id)}
              <option value={o.id}>{o.label} ({o.count})</option>
            {/each}
          </select>
        </label>
        <label class="inline">
          <span class="muted">表示</span>
          <select bind:value={visibility}>
            <option value="shown">表示中</option>
            <option value="hidden">非表示 ({hiddenCount})</option>
            <option value="all">すべて</option>
          </select>
        </label>
      </div>
    </div>

    {#if prefs.stateView === "tag" && !hasAnyTag}
      <p class="hint muted">
        タグはまだありません。プロジェクトを選ぶと、右の詳細パネルで「仕事/客先/案件」のように / 区切りで 3 階層まで付けられます。
      </p>
    {/if}

    <ul class="list">
      {#if tree}
        {@render branch(tree, 0)}
        {#if !tree.children.length}<li class="none muted">該当するプロジェクトはありません</li>{/if}
      {:else}
        {#each flat as p (p.key)}
          <ProjectRow project={p} {now} selected={p.key === selectedKey} onselect={() => select(p)} />
        {:else}
          <li class="none muted">該当するプロジェクトはありません</li>
        {/each}
      {/if}
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
    padding: 8px 12px;
    border-bottom: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .bar input[type="search"] {
    width: 220px;
  }

  .inline {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .hint {
    margin: 8px 14px 0;
    font-size: 12px;
  }

  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    flex: 1;
  }

  .sub {
    list-style: none;
    margin: 0;
    padding: 0 0 0 16px;
  }

  .group {
    list-style: none;
  }

  .group-head {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    border: none;
    border-radius: 0;
    border-bottom: 1px solid var(--line);
    background: var(--surface-2);
    padding: 5px 12px 5px 8px;
    text-align: left;
    position: sticky;
    top: calc(var(--depth) * 31px);
    z-index: calc(10 - var(--depth));
  }

  .group-head:hover:not(:disabled) {
    background: color-mix(in srgb, var(--ink) 7%, var(--surface-2));
  }

  .group-head.special .g-label {
    color: var(--ink-2);
    font-weight: 500;
  }

  .chev {
    width: 14px;
    height: 14px;
    flex: none;
    transition: transform 0.12s;
  }

  .chev path {
    fill: none;
    stroke: var(--ink-2);
    stroke-width: 1.6;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .chev.open {
    transform: rotate(90deg);
  }

  .g-label {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .g-count {
    font-size: 12px;
    color: var(--ink-2);
    background: var(--surface);
    border-radius: 999px;
    padding: 0 7px;
  }

  .g-when {
    margin-left: auto;
    font-size: 12px;
    white-space: nowrap;
  }

  .none {
    padding: 24px;
    text-align: center;
  }
</style>
