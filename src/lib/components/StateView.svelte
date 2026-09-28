<script lang="ts">
  import type { Project } from "$lib/derive";
  import { hasLeftovers, leftoverWeight } from "$lib/derive";
  import type { RemoteKind } from "$lib/remotes";
  import { relative, searchKey } from "$lib/format";
  import { folderTree, tagTree, type TreeNode } from "$lib/tree";
  import {
    allTags,
    app,
    createTag,
    deleteTag,
    errorText,
    isAncestorTag,
    moveTag,
    prefs,
    renameTag,
    savePrefs,
    toast,
    type StateViewMode,
  } from "$lib/store.svelte";
  import { depthOf, leafOf, MAX_DEPTH, parentOf, usageCount } from "$lib/tags";
  import ProjectDetail from "./ProjectDetail.svelte";
  import ProjectRow from "./ProjectRow.svelte";

  let { projects, now }: { projects: Project[]; now: number } = $props();

  const isMacLike = typeof navigator !== "undefined" && /Mac/.test(navigator.userAgent);

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
    projects.filter(
      (p) =>
        (visibility === "all" ? true : visibility === "hidden" ? p.hidden : !p.hidden) &&
        // クローンして作業しているフォークは隠さない。隠すのは未クローンのものだけ
        !(prefs.hideForkArchived && p.kind === "remote" && (p.isFork || p.isArchived)),
    ),
  );
  const forkArchivedCount = $derived(projects.filter((p) => p.kind === "remote" && (p.isFork || p.isArchived)).length);

  function setHideForkArchived(v: boolean) {
    prefs.hideForkArchived = v;
    savePrefs();
  }

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
    const q = searchKey(query.trim());
    let xs = byVisibility.filter((p) => matchesFilter(p, filter) && matchesRemote(p, remoteFilter));
    if (q) {
      const hit = (s: string | null | undefined) => !!s && searchKey(s).includes(q);
      xs = xs.filter(
        (p) =>
          hit(p.name) ||
          hit(p.path) ||
          p.tags.some(hit) ||
          p.remotes.some((r) => hit(r.fullName)) ||
          p.links.some((l) => hit(l.label) || hit(l.url)),
      );
    }
    return xs;
  });

  /** 既定以外の絞り込みをしているか (検索も含む) */
  const filtering = $derived(filter !== "all" || remoteFilter !== "" || visibility !== "shown" || query.trim() !== "");

  const flat = $derived(prefs.stateView === "time" ? sortProjects(shown) : []);
  const tree = $derived(
    prefs.stateView === "folder"
      ? folderTree(shown, sortProjects)
      : prefs.stateView === "tag"
        ? tagTree(shown, sortProjects, filtering ? [] : allTags())
        : null,
  );
  // 作っただけで、まだどのプロジェクトにも付けていないタグも数える
  const hasAnyTag = $derived(allTags().length > 0);

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

  function clearFilters() {
    filter = "all";
    remoteFilter = "";
    visibility = "shown";
    query = "";
  }

  function setView(v: StateViewMode) {
    prefs.stateView = v;
    savePrefs();
  }

  // ---- タグ表示のドラッグ (プロジェクトの付け替えと、タグ自体の移動) ----
  const UNTAGGED = "#untagged";
  type Drag = { kind: "project"; project: Project; from: string | null } | { kind: "tag"; tag: string };
  let dragging = $state<Drag | null>(null);
  let dropId = $state<string | null>(null);

  const tagOfNode = (id: string) => (id === UNTAGGED ? null : id);

  function startDrag(e: DragEvent, p: Project, nodeId: string) {
    dragging = { kind: "project", project: p, from: tagOfNode(nodeId) };
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = "copyMove";
      e.dataTransfer.setData("text/plain", p.name);
    }
  }

  function startTagDrag(e: DragEvent, n: TreeNode) {
    e.stopPropagation();
    dragging = { kind: "tag", tag: n.id };
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = "move";
      e.dataTransfer.setData("text/plain", n.id);
    }
  }

  function endDrag() {
    dragging = null;
    dropId = null;
  }

  // ドラッグ中に一覧の上端・下端へ近づけたら自動でスクロールする (遠くのタグへ運べるように)
  let listEl = $state<HTMLUListElement | null>(null);
  $effect(() => {
    if (!dragging || !listEl) return;
    const el = listEl;
    let speed = 0;
    const EDGE = 60;
    const onOver = (e: DragEvent) => {
      const r = el.getBoundingClientRect();
      if (e.clientY < r.top + EDGE) speed = -Math.ceil((r.top + EDGE - e.clientY) / 2);
      else if (e.clientY > r.bottom - EDGE) speed = Math.ceil((e.clientY - (r.bottom - EDGE)) / 2);
      else speed = 0;
    };
    const timer = setInterval(() => speed && el.scrollBy(0, speed), 16);
    // グループ側で伝播を止めるので、捕捉段階で拾う
    document.addEventListener("dragover", onOver, true);
    return () => {
      clearInterval(timer);
      document.removeEventListener("dragover", onOver, true);
    };
  });

  const isCopy = (e: DragEvent) => e.ctrlKey || e.altKey;

  /** タグを n の下へ移せるか (自分自身・自分の下の階層・今の親には落とせない) */
  function canDropTag(tag: string, n: TreeNode): boolean {
    const target = tagOfNode(n.id);
    if (target === null) return tag.includes("/"); // 一番上へ。既に一番上なら何もしない
    return target !== tag && !isAncestorTag(tag, target) && target !== parentOf(tag);
  }

  function overGroup(e: DragEvent, n: TreeNode) {
    if (!dragging) return;
    // 入れ子のグループでは一番内側だけが受ける
    e.stopPropagation();
    if (dragging.kind === "tag" && !canDropTag(dragging.tag, n)) {
      dropId = null;
      return;
    }
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = dragging.kind === "project" && isCopy(e) ? "copy" : "move";
    dropId = n.id;
  }

  async function dropOnGroup(e: DragEvent, n: TreeNode) {
    if (!dragging) return;
    e.stopPropagation();
    e.preventDefault();
    const d = dragging;
    const to = tagOfNode(n.id);
    endDrag();
    try {
      if (d.kind === "tag") {
        if (!canDropTag(d.tag, n)) return;
        const dest = to === null ? leafOf(d.tag) : `${to}/${leafOf(d.tag)}`;
        await renameTag(d.tag, dest);
        toast(`タグ「${d.tag}」を「${dest}」へ移しました`);
        return;
      }
      const { project, from } = d;
      const copy = isCopy(e) && to !== null;
      const changed = await moveTag(project.prefKey, project.tags, from, to, copy);
      if (changed) {
        toast(
          to === null
            ? `${project.name} からタグ「${from}」を外しました`
            : copy || from === null
              ? `${project.name} にタグ「${to}」を付けました`
              : `${project.name} を「${from}」から「${to}」へ移しました`,
        );
      }
    } catch (err) {
      toast(errorText(err));
    }
  }

  // ---- タグの追加・名前の変更・削除 (タグ表示の見出しで) ----
  /** 編集中のもの。add の parent が null なら一番上 */
  type Edit = { mode: "add"; parent: string | null } | { mode: "rename"; tag: string };
  let editing = $state<Edit | null>(null);
  let editValue = $state("");
  let editError = $state("");

  function startAdd(parent: string | null) {
    editing = { mode: "add", parent };
    editValue = "";
    editError = "";
    // 子を足すなら、見えるように親を開く
    if (parent) {
      prefs.collapsed = prefs.collapsed.filter((k) => k !== `tag:${parent}`);
      savePrefs();
    }
  }

  function startRename(tag: string) {
    editing = { mode: "rename", tag };
    editValue = leafOf(tag);
    editError = "";
  }

  function cancelEdit() {
    editing = null;
    editError = "";
  }

  async function commitEdit() {
    const e = editing;
    if (!e) return;
    const name = editValue.trim();
    if (!name) return cancelEdit();
    try {
      if (e.mode === "add") {
        const path = e.parent ? `${e.parent}/${name}` : name;
        await createTag(path);
        toast(`タグ「${path}」を作りました`);
      } else {
        // 名前の変更は同じ階層のまま。"/" を含めれば別の階層へ移せる
        const parent = parentOf(e.tag);
        const dest = name.includes("/") ? name : parent ? `${parent}/${name}` : name;
        if (dest === e.tag) return cancelEdit();
        await renameTag(e.tag, dest);
        toast(`タグ「${e.tag}」を「${dest}」にしました`);
      }
      editing = null;
    } catch (err) {
      editError = errorText(err);
    }
  }

  async function removeTag(tag: string) {
    const n = app.config ? usageCount(app.config, tag) : 0;
    const parent = parentOf(tag);
    const msg =
      `タグ「${tag}」と、その下の階層のタグを削除します。` +
      (n ? `\n付いているプロジェクト ${n} 件は${parent ? `「${parent}」` : "「タグなし」"}へ移ります。` : "");
    if (!confirm(msg)) return;
    try {
      await deleteTag(tag);
      toast(`タグ「${tag}」を削除しました`);
    } catch (err) {
      toast(errorText(err));
    }
  }

  function editKey(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.isComposing) commitEdit();
    else if (e.key === "Escape") cancelEdit();
  }

  function select(p: Project) {
    selectedKey = p.key === selectedKey ? null : p.key;
  }
</script>

{#snippet branch(n: TreeNode, depth: number)}
  {#each n.children as c (c.id)}
    {@const open = !collapsed.has(cKey(c))}
    {@const tagMode = prefs.stateView === "tag" && !c.special}
    <li
      class="group"
      class:drop={dropId === c.id}
      style="--depth: {depth}"
      ondragover={(e) => prefs.stateView === "tag" && overGroup(e, c)}
      ondrop={(e) => prefs.stateView === "tag" && dropOnGroup(e, c)}
    >
      <div
        class="group-head"
        class:special={c.special}
        class:tag-draggable={tagMode && !editing}
        role="group"
        aria-label={c.label}
        draggable={tagMode && !editing ? "true" : undefined}
        ondragstart={tagMode ? (e) => startTagDrag(e, c) : undefined}
        ondragend={endDrag}
      >
        {#if editing?.mode === "rename" && editing.tag === c.id}
          <svg class="chev" class:open viewBox="0 0 16 16" aria-hidden="true"><path d="M6 4l4 4-4 4" /></svg>
          <!-- svelte-ignore a11y_autofocus -->
          <input class="tag-input" bind:value={editValue} onkeydown={editKey} onblur={cancelEdit} autofocus />
          {#if editError}<span class="edit-err">{editError}</span>{/if}
        {:else}
          <button class="toggle" aria-expanded={open} onclick={() => toggle(c)}>
            <svg class="chev" class:open viewBox="0 0 16 16" aria-hidden="true"><path d="M6 4l4 4-4 4" /></svg>
            <span class="g-label">{c.label}</span>
            <span class="g-count num">{c.total}</span>
            {#if c.leftovers}<span class="badge mid">取り残し {c.leftovers}</span>{/if}
          </button>
          {#if tagMode}
            <span class="g-actions">
              {#if depthOf(c.id) < MAX_DEPTH}
                <button class="ghost icon-btn" title="「{c.id}」の下にタグを追加" onclick={() => startAdd(c.id)}>＋ 下に追加</button>
              {/if}
              <button class="ghost icon-btn" onclick={() => startRename(c.id)}>名前を変更</button>
              <button class="ghost icon-btn" onclick={() => removeTag(c.id)}>削除</button>
            </span>
          {/if}
          <span class="g-when muted">{relative(c.lastActivity, now)}</span>
        {/if}
      </div>
      {#if open}
        <ul class="sub">
          {#if editing?.mode === "add" && editing.parent === c.id}
            <li class="add-row">
              <!-- svelte-ignore a11y_autofocus -->
              <input
                class="tag-input"
                placeholder="新しいタグの名前"
                bind:value={editValue}
                onkeydown={editKey}
                onblur={cancelEdit}
                autofocus
              />
              {#if editError}<span class="edit-err">{editError}</span>{/if}
            </li>
          {/if}
          {@render branch(c, depth + 1)}
          {#each c.projects as p (p.key)}
            <ProjectRow
              project={p}
              {now}
              selected={p.key === selectedKey}
              onselect={() => select(p)}
              showTags={prefs.stateView !== "tag"}
              ondragstart={prefs.stateView === "tag" ? (e) => startDrag(e, p, c.id) : undefined}
              ondragend={endDrag}
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
      <!-- 1 行目: 検索と件数 -->
      <div class="bar-row">
        <span class="caption">検索</span>
        <div class="controls">
          <input class="search" type="search" placeholder="名前・パス・タグで検索" bind:value={query} aria-label="検索" />
          <span class="result muted num">{shown.length} / {projects.length} 件</span>
        </div>
      </div>

      <!-- 2 行目: 見え方 (表示形式と並び替え) -->
      <div class="bar-row">
        <span class="caption">表示</span>
        <div class="controls">
          <div class="segmented" role="group" aria-label="表示形式">
            {#each views as v (v.id)}
              <button class:on={prefs.stateView === v.id} onclick={() => setView(v.id)}>{v.label}</button>
            {/each}
          </div>
          <label class="field">
            <span class="muted">並び</span>
            <select bind:value={sort}>
              <option value="recent">最近の作業順</option>
              <option value="stale">取り残し・放置が長い順</option>
              <option value="weight">取り残しが重い順</option>
              <option value="name">名前順</option>
            </select>
          </label>
          {#if prefs.stateView === "tag"}
            <button class="small" onclick={() => startAdd(null)}>タグを追加</button>
          {/if}
          {#if tree}
            <span class="spacer"></span>
            <button class="ghost small" onclick={() => setAll(true)}>すべて開く</button>
            <button class="ghost small" onclick={() => setAll(false)}>すべて畳む</button>
          {/if}
        </div>
      </div>

      <!-- 3 行目: 絞り込み (対象を減らすものはすべてここ) -->
      <div class="bar-row">
        <span class="caption">絞り込み</span>
        <div class="controls">
          <div class="segmented" role="group" aria-label="状態で絞り込み">
            {#each filters as f (f.id)}
              <button class:on={filter === f.id} onclick={() => (filter = f.id)}
                >{f.label} <span class="muted num">{counts[f.id]}</span></button
              >
            {/each}
          </div>
          <label class="field">
            <span class="muted">リモート</span>
            <select bind:value={remoteFilter} class:active={remoteFilter !== ""}>
              <option value="">すべて</option>
              {#each remoteOptions as o (o.id)}
                <option value={o.id}>{o.label} ({o.count})</option>
              {/each}
            </select>
          </label>
          <label class="field">
            <span class="muted">非表示</span>
            <select bind:value={visibility} class:active={visibility !== "shown"}>
              <option value="shown">除く</option>
              <option value="hidden">だけ ({hiddenCount})</option>
              <option value="all">含める</option>
            </select>
          </label>
          {#if forkArchivedCount}
            <label class="check small-text" title="クローンして作業しているフォークは隠しません">
              <input
                type="checkbox"
                checked={prefs.hideForkArchived}
                onchange={(e) => setHideForkArchived(e.currentTarget.checked)}
              />
              未クローンのフォーク・アーカイブを隠す <span class="muted num">({forkArchivedCount})</span>
            </label>
          {/if}
          {#if filtering}
            <button class="ghost small" onclick={clearFilters}>条件を解除</button>
          {/if}
        </div>
      </div>
    </div>

    {#if prefs.stateView === "tag"}
      <p class="hint muted">
        {#if !hasAnyTag}
          タグはまだありません。プロジェクトを選ぶと、右の詳細パネルで「仕事/客先/案件」のように / 区切りで 3 階層まで付けられます。
        {:else}
          行をタグへドラッグすると付け替え ({isMacLike ? "Option" : "Ctrl"} を押しながらだと元のタグも残す)、「タグなし」へ落とすと外せます。
          タグの見出しもドラッグで別のタグの下へ移せます。見出しにカーソルを乗せると、下に追加・名前の変更・削除ができます。
        {/if}
      </p>
    {/if}

    <ul class="list" bind:this={listEl}>
      {#if tree}
        {#if editing?.mode === "add" && editing.parent === null}
          <li class="add-row top">
            <!-- svelte-ignore a11y_autofocus -->
            <input
              class="tag-input"
              placeholder="新しいタグ (「仕事/客先」のように / で下の階層も作れる)"
              bind:value={editValue}
              onkeydown={editKey}
              onblur={cancelEdit}
              autofocus
            />
            {#if editError}<span class="edit-err">{editError}</span>{/if}
          </li>
        {/if}
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
    background: var(--surface);
  }

  .bar-row {
    display: flex;
    align-items: flex-start;
    gap: 8px;
  }

  /* 見出しの右側。折り返しても見出しの下にはみ出さない */
  .controls {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 14px;
    min-height: 28px;
  }

  /* 行の見出し。幅をそろえて、表示と絞り込みの操作の頭を縦に並べる */
  .caption {
    width: 56px;
    flex: none;
    line-height: 28px;
    font-size: 12px;
    font-weight: 600;
    color: var(--ink-2);
  }

  .search {
    width: 320px;
    max-width: 100%;
  }

  .result {
    margin-left: auto;
    font-size: 12px;
  }

  .field {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  /* 既定から変えている絞り込みは枠を強調する */
  select.active {
    border-color: var(--accent);
    background: var(--accent-wash);
  }

  .spacer {
    flex: 1;
  }

  .small-text {
    font-size: 12px;
  }

  .small {
    font-size: 12px;
    padding: 2px 8px;
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
    border-bottom: 1px solid var(--line);
    background: var(--surface-2);
    padding: 0 12px 0 0;
    min-height: 31px;
    position: sticky;
    top: calc(var(--depth) * 31px);
    z-index: calc(10 - var(--depth));
  }

  .group-head:hover {
    background: color-mix(in srgb, var(--ink) 7%, var(--surface-2));
  }

  .group-head.tag-draggable {
    cursor: grab;
  }

  /* 畳む / 開くボタン */
  .toggle {
    display: flex;
    align-items: center;
    gap: 8px;
    border: none;
    border-radius: 0;
    background: transparent;
    padding: 5px 0 5px 8px;
    text-align: left;
    min-width: 0;
    flex: 0 1 auto;
  }

  .toggle:hover:not(:disabled) {
    background: transparent;
  }

  /* 見出しの操作はカーソルを乗せたときだけ */
  .g-actions {
    display: none;
    gap: 2px;
  }

  .group-head:hover .g-actions,
  .group-head:focus-within .g-actions {
    display: inline-flex;
  }

  .icon-btn {
    font-size: 12px;
    padding: 1px 7px;
    color: var(--ink-2);
  }

  .tag-input {
    font-size: 13px;
    padding: 2px 6px;
    min-width: 200px;
  }

  .group-head > .tag-input {
    margin: 3px 0;
  }

  .group-head > .chev {
    margin-left: 8px;
  }

  .add-row {
    list-style: none;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 12px 5px 30px;
    border-bottom: 1px solid var(--line);
    background: var(--accent-wash);
  }

  .add-row.top {
    padding-left: 12px;
  }

  .add-row .tag-input {
    width: 360px;
    max-width: 100%;
  }

  .edit-err {
    font-size: 12px;
    color: var(--st-high);
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

  /* ドラッグ中の落とし先 */
  .group.drop > .group-head {
    background: var(--accent-wash);
    box-shadow: inset 0 0 0 2px var(--accent);
  }

  .group.drop > .sub {
    background: color-mix(in srgb, var(--accent) 5%, transparent);
  }

  .none {
    padding: 24px;
    text-align: center;
  }
</style>
