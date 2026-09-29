<script lang="ts">
  // 状態タブのエクスプローラ表示。左に分類 (フォルダ / タグ) の木、右に選んだ階層の中身を表で出す。
  // 分類なしのときは木を出さず、すべてを 1 枚の表で出す。
  import type { Project } from "$lib/derive";
  import { relative } from "$lib/format";
  import { prefs, savePrefs, type StateGroup } from "$lib/store.svelte";
  import type { TreeNode } from "$lib/tree";
  import ProjectActions from "./ProjectActions.svelte";
  import RemoteBadges from "./RemoteBadges.svelte";
  import StarButton from "./StarButton.svelte";

  type Sort = "recent" | "stale" | "weight" | "name";

  let {
    tree,
    group,
    now,
    sort,
    onsort,
    sortProjects,
    selectedKey,
    onselect,
  }: {
    tree: TreeNode;
    group: StateGroup;
    now: number;
    sort: Sort;
    onsort: (s: Sort) => void;
    sortProjects: (xs: Project[]) => Project[];
    selectedKey: string | null;
    onselect: (p: Project) => void;
  } = $props();

  const hasTree = $derived(group !== "none");

  /** 選んだノードまでの道筋 (先頭は木の根)。見つからなければ (絞り込みで消えたなど) 根だけ */
  const trail = $derived.by(() => {
    const find = (n: TreeNode, path: TreeNode[]): TreeNode[] | null => {
      if (n.id === prefs.explorerNode) return path;
      for (const c of n.children) {
        const r = find(c, [...path, c]);
        if (r) return r;
      }
      return null;
    };
    return find(tree, [tree]) ?? [tree];
  });
  const current = $derived(trail[trail.length - 1]);

  /** 下の階層も含めたプロジェクト (タグでは同じものが複数の階層に出るので重複を除く) */
  function allUnder(n: TreeNode): Project[] {
    const seen = new Map<string, Project>();
    const visit = (x: TreeNode) => {
      x.projects.forEach((p) => seen.set(p.key, p));
      x.children.forEach(visit);
    };
    visit(n);
    return sortProjects([...seen.values()]);
  }

  const folders = $derived(prefs.explorerDeep ? [] : current.children);
  const items = $derived(prefs.explorerDeep ? allUnder(current) : current.projects);

  // ---- 左の木の開け閉め ----
  const collapsed = $derived(new Set(prefs.collapsed));
  const cKey = (n: TreeNode) => `explorer-${group}:${n.id}`;

  function toggle(n: TreeNode) {
    const k = cKey(n);
    prefs.collapsed = collapsed.has(k) ? prefs.collapsed.filter((x) => x !== k) : [...prefs.collapsed, k];
    savePrefs();
  }

  /** 階層を開く。左の木でも見えるように、上の階層を開いておく */
  function go(n: TreeNode) {
    prefs.explorerNode = n.id;
    const path = trailTo(n);
    const open = new Set(path.slice(0, -1).map(cKey));
    prefs.collapsed = prefs.collapsed.filter((k) => !open.has(k));
    savePrefs();
  }

  function trailTo(target: TreeNode): TreeNode[] {
    const find = (n: TreeNode, path: TreeNode[]): TreeNode[] | null => {
      if (n === target) return path;
      for (const c of n.children) {
        const r = find(c, [...path, c]);
        if (r) return r;
      }
      return null;
    };
    return find(tree, [tree]) ?? [tree];
  }

  function setDeep(v: boolean) {
    prefs.explorerDeep = v;
    savePrefs();
  }

  /** 木の階層にカーソルを乗せたときの説明 */
  function tipOf(n: TreeNode): string {
    const name = n === tree ? "すべて" : n.id && !n.special ? n.id : n.label;
    return `${name}
${n.total} 件 (取り残しのあるもの ${n.leftovers} 件)`;
  }

  /** 場所の列: 親フォルダ。フォルダで分類しているときは、今の階層から見た相対 */
  function placeOf(p: Project): string {
    if (!p.path) return "";
    const dir = p.path.replace(/[\\/][^\\/]*$/, "");
    const base = current.id;
    if (group !== "folder" || !base || current.special) return dir;
    if (dir.toLowerCase() === base) return "";
    return dir.toLowerCase().startsWith(base) ? dir.slice(base.length).replace(/^[\\/]/, "") : dir;
  }

  const showPlace = $derived(group !== "folder" || prefs.explorerDeep || !current.id || !!current.special);

  const columns: { id: Sort | null; label: string; cls: string }[] = [
    { id: "name", label: "名前", cls: "c-name" },
    { id: null, label: "ブランチ", cls: "c-branch" },
    { id: "weight", label: "取り残し", cls: "c-left" },
    { id: "recent", label: "最新の作業", cls: "c-when" },
  ];

  function rowKey(e: KeyboardEvent, run: () => void) {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      run();
    }
  }
</script>

<!-- 階層の数: 取り残しのあるプロジェクト数と、全体の件数 -->
{#snippet counts(n: TreeNode)}
  {#if n.leftovers}<span class="badge mid n-left">取り残し {n.leftovers}</span>{/if}
  <span class="n-count num">{n.total} 件</span>
{/snippet}

{#snippet branch(n: TreeNode, depth: number)}
  {#each n.children as c (c.id)}
    {@const open = !collapsed.has(cKey(c))}
    <li>
      <div class="node" class:on={c.id === current.id && c !== tree} class:special={c.special} style="--depth: {depth}">
        {#if c.children.length}
          <button class="chev-btn" aria-expanded={open} aria-label={open ? "畳む" : "開く"} onclick={() => toggle(c)}>
            <svg class="chev" class:open viewBox="0 0 16 16" aria-hidden="true"><path d="M6 4l4 4-4 4" /></svg>
          </button>
        {:else}
          <span class="chev-btn"></span>
        {/if}
        <button class="node-label" title={tipOf(c)} onclick={() => go(c)}>
          <span class="n-label">{c.label}</span>
          {@render counts(c)}
        </button>
      </div>
      {#if open && c.children.length}
        <ul>{@render branch(c, depth + 1)}</ul>
      {/if}
    </li>
  {/each}
{/snippet}

<div class="explorer">
  {#if hasTree}
  <nav class="side" aria-label="階層">
    <ul class="side-tree">
      <li>
        <div class="node" class:on={current === tree} style="--depth: 0">
          <span class="chev-btn"></span>
          <button class="node-label" title={tipOf(tree)} onclick={() => go(tree)}>
            <span class="n-label">すべて</span>
            {@render counts(tree)}
          </button>
        </div>
      </li>
      {@render branch(tree, 0)}
    </ul>
  </nav>
  {/if}

  <section class="main">
    {#if hasTree}
    <div class="main-head">
      <div class="crumbs" aria-label="今の階層">
        {#each trail as n, i (n.id)}
          {#if i > 0}<span class="sep muted">›</span>{/if}
          {#if i === trail.length - 1}
            <span class="crumb cur">{i === 0 ? "すべて" : n.label}</span>
          {:else}
            <button class="crumb ghost" onclick={() => go(n)}>{i === 0 ? "すべて" : n.label}</button>
          {/if}
        {/each}
      </div>
      <label class="check deep" title="選んだ階層の下にあるプロジェクトを、階層を分けずにすべて並べる">
        <input type="checkbox" checked={prefs.explorerDeep} onchange={(e) => setDeep(e.currentTarget.checked)} />
        下の階層もまとめて出す
      </label>
    </div>
    {/if}

    <div class="table-wrap">
      <table>
        <thead>
          <tr>
            <th class="c-star"></th>
            {#each columns as col (col.label)}
              <th class={col.cls}>
                {#if col.id}
                  {@const id = col.id}
                  <button class="sort" class:on={sort === id} onclick={() => onsort(id)}>
                    {col.label}{#if sort === id}<span class="arrow" aria-hidden="true">▼</span>{/if}
                  </button>
                {:else}
                  {col.label}
                {/if}
              </th>
            {/each}
            {#if showPlace}<th class="c-place">場所</th>{/if}
            <th class="c-act"></th>
          </tr>
        </thead>
        <tbody>
          {#each folders as f (f.id)}
            <tr class="folder" tabindex="0" onclick={() => go(f)} onkeydown={(e) => rowKey(e, () => go(f))}>
              <td class="c-star"><span class="f-icon" aria-hidden="true">▸</span></td>
              <td class="c-name"><span class="name">{f.label}</span> <span class="muted small">{f.total} 件</span></td>
              <td class="c-branch"></td>
              <td class="c-left">{#if f.leftovers}<span class="badge mid">取り残し {f.leftovers}</span>{/if}</td>
              <td class="c-when muted">{relative(f.lastActivity, now)}</td>
              {#if showPlace}<td class="c-place"></td>{/if}
              <td class="c-act"></td>
            </tr>
          {/each}
          {#each items as p (p.key)}
            <tr
              class="project"
              class:selected={p.key === selectedKey}
              class:hidden={p.hidden}
              data-key={p.key}
              tabindex="0"
              onclick={() => onselect(p)}
              onkeydown={(e) => rowKey(e, () => onselect(p))}
            >
              <td class="c-star"><span class="star-slot" class:starred={p.starred}><StarButton project={p} /></span></td>
              <td class="c-name">
                <span class="name">{p.name}</span>
                <RemoteBadges links={p.links} showPath={false} showNone={p.kind === "local" && !p.local?.error} />
                {#if p.kind === "folder"}<span class="host">git 以外</span>{/if}
                {#if p.isFork}<span class="host">フォーク</span>{/if}
                {#if p.isArchived}<span class="host">アーカイブ</span>{/if}
                {#if p.hidden}<span class="host">非表示</span>{/if}
              </td>
              <td class="c-branch mono">
                {#if p.local?.branch}<span class:off-default={p.local.branch !== p.local.defaultBranch}>{p.local.branch}</span>{/if}
              </td>
              <td class="c-left">
                {#each p.leftovers as l (l.kind)}
                  <span class="badge {l.severity}" title={l.detail ?? ""}>{l.label}</span>
                {/each}
              </td>
              <td class="c-when" title={p.lastActivity ? new Date(p.lastActivity).toLocaleString() : ""}>
                {relative(p.lastActivity, now)}
              </td>
              {#if showPlace}<td class="c-place mono muted" title={p.path ?? ""}>{placeOf(p)}</td>{/if}
              <!-- 操作ボタンを押しても行の選択は変えない -->
              <td class="c-act">
                <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
                <span class="acts" onclick={(e) => e.stopPropagation()}><ProjectActions project={p} compact /></span>
              </td>
            </tr>
          {/each}
          {#if !folders.length && !items.length}
            <tr><td class="none muted" colspan={showPlace ? 7 : 6}>この階層にはプロジェクトがありません</td></tr>
          {/if}
        </tbody>
      </table>
    </div>
  </section>
</div>

<style>
  .explorer {
    display: flex;
    flex: 1;
    min-width: 0;
    min-height: 0;
  }

  /* 左の木。右端をドラッグして幅を変えられる */
  .side {
    width: 260px;
    min-width: 160px;
    max-width: 60%;
    resize: horizontal;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--line);
    background: var(--surface);
  }

  .side-tree,
  .side-tree ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .side-tree {
    overflow: auto;
    flex: 1;
    padding: 4px 0;
  }

  .node {
    display: flex;
    align-items: center;
    padding-left: calc(var(--depth) * 14px + 4px);
    padding-right: 6px;
  }

  .node:hover {
    background: color-mix(in srgb, var(--ink) 6%, transparent);
  }

  .node.on {
    background: var(--accent-wash);
  }

  .node.on .n-label {
    font-weight: 600;
  }

  .node.special .n-label {
    color: var(--ink-2);
  }

  .chev-btn {
    width: 20px;
    height: 24px;
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: none;
    background: transparent;
    padding: 0;
  }

  .chev-btn:hover:not(:disabled) {
    background: transparent;
  }

  .chev {
    width: 14px;
    height: 14px;
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

  .node-label {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 6px;
    border: none;
    border-radius: 0;
    background: transparent;
    padding: 3px 0;
    text-align: left;
  }

  .node-label:hover:not(:disabled) {
    background: transparent;
  }

  .n-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .n-count {
    flex: none;
    font-size: 11px;
    color: var(--ink-2);
  }

  /* 数は右寄せ。取り残しが無ければ件数だけが右端に来る */
  .n-left {
    margin-left: auto;
    font-size: 11px;
    padding: 0 6px 0 5px;
  }

  .n-label + .n-count {
    margin-left: auto;
  }

  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .main-head {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 6px 12px;
    border-bottom: 1px solid var(--line);
    background: var(--surface-2);
  }

  .crumbs {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 2px 4px;
  }

  .crumb {
    font-size: 13px;
    padding: 1px 6px;
  }

  .crumb.cur {
    font-weight: 600;
  }

  .deep {
    font-size: 12px;
    white-space: nowrap;
  }

  .table-wrap {
    flex: 1;
    overflow: auto;
  }

  table {
    width: 100%;
    border-collapse: collapse;
  }

  th {
    position: sticky;
    top: 0;
    z-index: 1;
    background: var(--surface);
    border-bottom: 1px solid var(--line);
    text-align: left;
    font-size: 12px;
    font-weight: 600;
    color: var(--ink-2);
    padding: 4px 8px;
    white-space: nowrap;
  }

  .sort {
    border: none;
    background: transparent;
    padding: 0;
    font: inherit;
    color: inherit;
  }

  .sort:hover:not(:disabled) {
    background: transparent;
    color: var(--ink);
  }

  .sort.on {
    color: var(--accent);
  }

  .arrow {
    font-size: 9px;
    margin-left: 3px;
  }

  td {
    border-bottom: 1px solid var(--line);
    padding: 4px 8px;
    vertical-align: middle;
  }

  tbody tr {
    cursor: pointer;
  }

  tbody tr:hover {
    background: color-mix(in srgb, var(--ink) 4%, transparent);
  }

  tr.selected,
  tr.selected:hover {
    background: var(--accent-wash);
  }

  tr.hidden td {
    opacity: 0.6;
  }

  .c-star {
    width: 28px;
    padding-right: 0;
    text-align: center;
  }

  .f-icon {
    color: var(--ink-2);
  }

  tr.folder .name {
    font-weight: 600;
  }

  .c-name .name {
    font-weight: 600;
    margin-right: 4px;
  }

  .c-name {
    min-width: 180px;
    white-space: nowrap;
  }

  .c-branch {
    font-size: 12px;
    white-space: nowrap;
    color: var(--ink-2);
  }

  .off-default {
    color: var(--ink);
  }

  .c-left .badge {
    margin: 1px 4px 1px 0;
  }

  .c-when {
    font-size: 12px;
    white-space: nowrap;
  }

  .c-place {
    font-size: 12px;
    max-width: 280px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .c-act {
    width: 1%;
    white-space: nowrap;
    text-align: right;
  }

  /* 操作ボタンは行にカーソルを乗せたときだけ */
  .acts {
    visibility: hidden;
  }

  .acts :global(.actions) {
    flex-wrap: nowrap;
  }

  tr:hover .acts,
  tr:focus-within .acts {
    visibility: visible;
  }

  /* スターは付いていないときはカーソルを乗せたときだけ */
  .star-slot:not(.starred) {
    visibility: hidden;
  }

  tr:hover .star-slot,
  tr:focus-within .star-slot {
    visibility: visible;
  }

  .host {
    font-size: 11px;
    color: var(--ink-2);
    border: 1px solid var(--line);
    border-radius: 4px;
    padding: 0 5px;
    margin-left: 4px;
  }

  .small {
    font-size: 12px;
  }

  .none {
    padding: 24px;
    text-align: center;
    cursor: default;
  }

  :global(:root[data-density="compact"]) td {
    padding-top: 2px;
    padding-bottom: 2px;
  }
</style>
