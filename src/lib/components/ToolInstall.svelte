<script lang="ts">
  // RepoTether から道具 (LockWatch・ToolKeeper) を入れる・更新する欄 (docs/tools-install.md)。
  // GitHub の Release の、Python の要らない exe の zip を SHA-256 で確かめて %LOCALAPPDATA%\RepoTether\tools に入れる。
  // 入れたら onUse(入れた場所) を呼ぶ (呼ぶ側が「場所」の設定に書いて保存する)。
  import { onMount } from "svelte";
  import * as api from "$lib/api";
  import { errorText, toast } from "$lib/store.svelte";

  let {
    id,
    current,
    note = "",
    onUse,
  }: {
    id: api.ToolId;
    /** 今の「場所」の設定 (空なら使っていない) */
    current: string;
    /** 入れる前に出す、中身の説明 */
    note?: string;
    onUse: (dir: string) => Promise<void> | void;
  } = $props();

  let info = $state<api.ToolInfo | null>(null);
  let latest = $state<api.ToolLatest | null>(null);
  let latestError = $state("");
  let busy = $state(false);
  let error = $state("");

  const same = (a: string, b: string) => a.trim().replace(/[\\/]+$/, "").toLowerCase() === b.trim().replace(/[\\/]+$/, "").toLowerCase();
  const inUse = $derived(!!info && !!current.trim() && same(current, info.dir));
  const updatable = $derived(!!info?.installed && !!latest && latest.version !== info.installed);
  const mb = (n: number) => `${(n / 1024 / 1024).toFixed(1)} MB`;

  onMount(async () => {
    try {
      info = await api.toolInfo(id);
    } catch (e) {
      error = errorText(e);
      return;
    }
    try {
      latest = await api.toolLatest(id);
    } catch (e) {
      latestError = errorText(e);
    }
  });

  async function install() {
    if (busy) return;
    busy = true;
    error = "";
    try {
      const was = info?.installed;
      info = await api.toolInstall(id);
      // 使っていなかった (場所が空) か、入れた場所を使っていたなら、そのまま使う
      if (!current.trim() || inUse) await onUse(info.dir);
      toast(was ? `${info.name} を ${info.installed} に更新しました` : `${info.name} ${info.installed} を入れました`);
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }
</script>

{#if info}
  <div class="tool-install">
    {#if info.installed}
      <span class="badge good" title="RepoTether が入れた版 ({info.dir})">入れた版 {info.installed}</span>
      {#if updatable}
        <button class="primary" onclick={install} disabled={busy} title="{info.name} {latest?.version} に入れ替える (設定とデータは残る)">
          {busy ? "更新しています…" : `↻ 更新 ${latest?.version}`}
        </button>
      {:else if latest}
        <span class="badge" title="GitHub の最新版と同じ">最新</span>
      {/if}
      {#if !inUse}
        <button onclick={() => onUse(info!.dir)} disabled={busy} title="「場所」を、RepoTether が入れた {info.name} ({info.dir}) にする">✓ 使う</button>
      {/if}
    {:else}
      <button class="primary" onclick={install} disabled={busy || !latest} title={latest ? `${latest.asset} を GitHub から取って ${info.dir} に入れる` : latestError}>
        {busy ? "入れています…" : "↓ 入れる"}
      </button>
      {#if latest}
        <span class="note">{info.name} {latest.version} ({mb(latest.size)}){note ? `。${note}` : ""}</span>
      {/if}
    {/if}
    {#if busy}
      <span class="note">GitHub から取っています。回線によっては数分かかります</span>
    {/if}
  </div>
  {#if latestError && !info.installed}
    <p class="note">入れられる版を調べられません: {latestError}</p>
  {/if}
{/if}
{#if error}
  <p class="err">{error}</p>
{/if}

<style>
  .tool-install {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin: 8px 0;
  }
  .tool-install .note {
    margin: 0;
  }
</style>
