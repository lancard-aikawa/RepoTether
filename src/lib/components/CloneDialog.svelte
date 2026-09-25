<script lang="ts">
  import type { RemoteRepo } from "$lib/types";
  import * as api from "$lib/api";
  import { app, errorText, refresh, toast, updateConfig } from "$lib/store.svelte";

  let { remote, onclose }: { remote: RemoteRepo; onclose: () => void } = $props();

  const urls = $derived(
    [
      remote.cloneUrl ? { label: "HTTPS", url: remote.cloneUrl } : null,
      remote.sshUrl ? { label: "SSH", url: remote.sshUrl } : null,
    ].filter((x) => x != null),
  );

  // 初期値を一度だけ取る (props の変化には追従しない)
  const init = (() => ({
    url: remote.cloneUrl ?? remote.sshUrl ?? "",
    parent: app.config?.cloneRoot || app.config?.roots[0] || "",
    name: remote.name,
  }))();
  let url = $state(init.url);
  let parent = $state(init.parent);
  let name = $state(init.name);
  let addRoot = $state(true);
  let running = $state(false);
  let error = $state("");

  function trimSep(p: string): string {
    return p.length > 1 ? p.replace(/[\\/]+$/, "") : p;
  }

  const dest = $derived(parent && name ? `${trimSep(parent)}${api.sep}${name}` : "");
  const underRoot = $derived(
    (app.config?.roots ?? []).some((r) => dest.toLowerCase().startsWith(trimSep(r).toLowerCase() + api.sep)),
  );

  async function browse() {
    const p = await api.pickFolder("クローン先の親フォルダ", parent || undefined);
    if (p) parent = p;
  }

  async function run() {
    running = true;
    error = "";
    try {
      const path = await api.cloneRepo(url, dest);
      const cfg = app.config!;
      const roots = !underRoot && addRoot ? [...cfg.roots, parent] : cfg.roots;
      await updateConfig({ ...cfg, roots, cloneRoot: parent });
      toast(`${path} にクローンしました`);
      onclose();
      await refresh(false);
    } catch (e) {
      error = errorText(e);
    } finally {
      running = false;
    }
  }
</script>

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && !running && onclose()}>
  <div class="dialog panel" role="dialog" aria-modal="true" aria-labelledby="clone-title">
    <h2 id="clone-title">{remote.fullName} をクローン</h2>

    <div class="field">
      <span>URL</span>
      <div class="row">
        {#if urls.length > 1}
          <select bind:value={url}>
            {#each urls as u (u.url)}
              <option value={u.url}>{u.label}</option>
            {/each}
          </select>
        {/if}
        <input type="text" class="mono grow" bind:value={url} />
      </div>
    </div>

    <label class="field">
      <span>親フォルダ</span>
      <div class="row">
        <input type="text" class="grow" bind:value={parent} />
        <button onclick={browse}>参照</button>
      </div>
    </label>

    <label class="field">
      <span>フォルダ名</span>
      <input type="text" bind:value={name} />
    </label>

    <p class="muted mono">{dest}</p>

    {#if dest && !underRoot}
      <label class="check">
        <input type="checkbox" bind:checked={addRoot} />
        親フォルダを探索先に追加する (追加しないと、次の更新で一覧に出ません)
      </label>
    {/if}

    {#if error}<p class="err">{error}</p>{/if}

    <div class="buttons">
      <button onclick={onclose} disabled={running}>やめる</button>
      <button class="primary" onclick={run} disabled={running || !url || !dest}>
        {running ? "クローンしています…" : "クローン"}
      </button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.35);
    display: grid;
    place-items: center;
    z-index: 50;
  }

  .dialog {
    width: min(560px, calc(100vw - 32px));
    padding: 16px 20px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .field > span {
    color: var(--ink-2);
    font-size: 12px;
  }

  .row {
    display: flex;
    gap: 6px;
  }

  .grow {
    flex: 1;
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .err {
    color: var(--st-high);
    white-space: pre-wrap;
    margin: 0;
  }

  p {
    margin: 0;
  }
</style>
