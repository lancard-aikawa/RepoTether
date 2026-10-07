<script lang="ts">
  // 状態タブのセル表示の 1 マス。エクスプローラの「大アイコン」のように、アイコンと名前を並べる。
  // 押すと詳細パネルを開く (開く・非表示などの操作は詳細パネルで)。
  import type { Project } from "$lib/derive";
  import { relative } from "$lib/format";
  import { app, prefs } from "$lib/store.svelte";
  import { heavyOf } from "$lib/vulns";
  import RepoIcon from "./RepoIcon.svelte";
  import StarButton from "./StarButton.svelte";

  let {
    project: p,
    now,
    selected,
    onselect,
    ondragstart,
    ondragend,
  }: {
    project: Project;
    now: number;
    selected: boolean;
    onselect: () => void;
    /** 渡されたときだけドラッグできる (タグ表示) */
    ondragstart?: (e: DragEvent) => void;
    ondragend?: () => void;
  } = $props();

  /** 重い脆弱性 (緊急・高) の数。「隠す」で隠したものは数えない */
  const heavy = $derived(heavyOf(app.vulns, p.local?.id, prefs.vulnHide));

  /** カーソルを乗せたときの説明: 場所・ブランチ・タグ */
  const tip = $derived(
    [
      p.name,
      p.path,
      p.local?.branch ? `ブランチ: ${p.local.branch}` : null,
      p.tags.length ? `タグ: ${p.tags.join(", ")}` : null,
      p.lastActivity ? `最新の作業: ${new Date(p.lastActivity).toLocaleString()}` : null,
    ]
      .filter(Boolean)
      .join("\n"),
  );
</script>

<li
  data-key={p.key}
  class:selected
  class:hidden={p.hidden}
  class:draggable={!!ondragstart}
  draggable={ondragstart ? "true" : undefined}
  {ondragstart}
  {ondragend}
>
  <div
    class="cell"
    role="button"
    tabindex="0"
    title={tip}
    onclick={onselect}
    onkeydown={(e) => (e.key === "Enter" || e.key === " ") && (e.preventDefault(), onselect())}
  >
    <RepoIcon name={p.name} path={p.path} size={48} />
    <span class="name">{p.name}</span>
    <span class="when muted">{relative(p.lastActivity, now)}</span>
    {#if p.leftovers.length || heavy || p.kind === "folder" || p.hidden}
      <span class="marks">
        {#each p.leftovers as l (l.kind)}
          <span class="badge {l.severity}" title={l.detail ?? ""}>{l.label}</span>
        {/each}
        {#if heavy}<span class="badge high" title="LockWatch の結果。緊急・高の脆弱性の数">脆弱性 {heavy}</span>{/if}
        {#if p.kind === "folder"}<span class="host">git 以外</span>{/if}
        {#if p.hidden}<span class="host">非表示</span>{/if}
      </span>
    {/if}
  </div>
  <span class="star-slot" class:starred={p.starred}><StarButton project={p} /></span>
</li>

<style>
  li {
    position: relative;
    list-style: none;
    display: flex;
  }

  /* 押せるマス: 枠と面を付け、乗せると枠が濃くなる */
  .cell {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
    padding: 12px 6px 8px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface);
    cursor: pointer;
    text-align: center;
  }

  .cell:hover {
    border-color: var(--line-strong);
    background: color-mix(in srgb, var(--ink) 5%, var(--surface));
  }

  .cell:active {
    background: color-mix(in srgb, var(--ink) 9%, var(--surface));
  }

  .cell:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  li.selected .cell {
    border-color: var(--accent);
    background: var(--accent-wash);
  }

  li.draggable .cell {
    cursor: grab;
  }

  li.hidden .cell {
    opacity: 0.6;
  }

  /* 名前は 2 行まで。長い名前は途中でも折り返す */
  .name {
    margin-top: 3px;
    font-weight: 600;
    line-height: 1.3;
    max-width: 100%;
    overflow-wrap: anywhere;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .when {
    font-size: 11.5px;
  }

  .marks {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 3px;
    margin-top: 2px;
    max-width: 100%;
  }

  /* 長い札はマスからはみ出さないように折り返す */
  .marks .badge {
    font-size: 11px;
    padding: 0 6px 0 5px;
    white-space: normal;
    text-align: left;
  }

  .host {
    font-size: 11px;
    color: var(--ink-2);
    border: 1px solid var(--line-strong);
    border-radius: 4px;
    padding: 0 5px;
  }

  /* スターは付いていれば常に、付いていなければカーソルを乗せたときだけ出す */
  .star-slot {
    position: absolute;
    top: 4px;
    left: 5px;
    visibility: hidden;
  }

  .star-slot.starred,
  li:hover .star-slot,
  li:focus-within .star-slot {
    visibility: visible;
  }
</style>
