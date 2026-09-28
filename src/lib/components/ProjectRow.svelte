<script lang="ts">
  import type { Project } from "$lib/derive";
  import { relative } from "$lib/format";
  import { errorText, setHidden, toast } from "$lib/store.svelte";
  import ProjectActions from "./ProjectActions.svelte";
  import RemoteBadges from "./RemoteBadges.svelte";

  let {
    project: p,
    now,
    selected,
    onselect,
    showTags = true,
  }: {
    project: Project;
    now: number;
    selected: boolean;
    onselect: () => void;
    /** タグ表示ではタグ自体が見出しになるので出さない */
    showTags?: boolean;
  } = $props();

  function sessionLine(p: Project): string | null {
    const s = p.lastSession;
    if (!s) return null;
    const title = s.title ?? s.firstPrompt ?? "";
    const last = s.lastPrompt && s.lastPrompt !== s.firstPrompt ? ` — ${s.lastPrompt}` : "";
    return `${title}${last}`;
  }

  async function toggleHidden() {
    try {
      await setHidden(p.prefKey, !p.hidden);
      toast(p.hidden ? `${p.name} を表示に戻しました` : `${p.name} を非表示にしました`);
    } catch (e) {
      toast(errorText(e));
    }
  }
</script>

<li class:selected class:hidden={p.hidden}>
  <div
    class="row"
    role="button"
    tabindex="0"
    onclick={onselect}
    onkeydown={(e) => (e.key === "Enter" || e.key === " ") && (e.preventDefault(), onselect())}
  >
    <div class="line1">
      <span class="name">{p.name}</span>
      <RemoteBadges links={p.links} showNone={p.kind === "local" && !p.local?.error} />
      {#if p.kind === "folder"}<span class="host">git 以外</span>{/if}
      {#if p.hidden}<span class="host">非表示</span>{/if}
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
    {#if p.leftovers.length || (showTags && p.tags.length)}
      <div class="badges">
        {#each p.leftovers as l (l.kind)}
          <span class="badge {l.severity}" title={l.detail ?? ""}>{l.label}</span>
        {/each}
        {#if showTags}
          {#each p.tags as t (t)}<span class="tag-chip"># {t}</span>{/each}
        {/if}
      </div>
    {/if}
    {#if sessionLine(p)}
      <div class="session"><span class="tag">Claude</span>{sessionLine(p)}</div>
    {/if}
  </div>
  <div class="row-actions">
    <ProjectActions project={p} compact />
    <button class="hide-btn" onclick={toggleHidden}>{p.hidden ? "表示に戻す" : "非表示"}</button>
  </div>
</li>

<style>
  li {
    position: relative;
    border-bottom: 1px solid var(--line);
    list-style: none;
  }

  li.selected {
    background: var(--accent-wash);
  }

  li.hidden .row {
    opacity: 0.6;
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
    align-items: center;
    gap: 6px 8px;
    flex-wrap: wrap;
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

  .tag-chip {
    font-size: 12px;
    color: var(--ink-2);
    padding: 1px 6px;
    border-radius: 4px;
    border: 1px solid var(--line-strong);
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
    gap: 4px;
    align-items: center;
    background: var(--surface);
    border-radius: var(--radius);
    padding: 2px;
    box-shadow: 0 0 0 1px var(--line);
  }

  li:hover .row-actions,
  li:focus-within .row-actions {
    display: flex;
  }

  .hide-btn {
    padding: 2px 8px;
    font-size: 12px;
  }
</style>
