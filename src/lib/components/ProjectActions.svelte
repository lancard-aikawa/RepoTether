<script lang="ts">
  import type { Project } from "$lib/derive";
  import * as api from "$lib/api";
  import { app, errorText, openClaude, prefs, toast } from "$lib/store.svelte";
  import CloneDialog from "./CloneDialog.svelte";

  let { project, compact = false }: { project: Project; compact?: boolean } = $props();

  let cloning = $state(false);

  // 設定の外部ツール (名前とプログラムが入っているものだけ)
  const tools = $derived((app.config?.externalTools ?? []).filter((t) => t.label.trim() && t.command.trim()));

  async function openTool(id: string) {
    if (!project.path) return;
    try {
      await api.openExternal(id, project.path);
    } catch (e) {
      toast(errorText(e));
    }
  }

  async function open(target: api.OpenTarget) {
    if (!project.path) return;
    try {
      await api.openIn(target, project.path, prefs.terminal);
    } catch (e) {
      toast(errorText(e));
    }
  }
</script>

<div class="actions" class:compact>
  {#if project.path}
    <button onclick={() => open("vscode")} title="VS Code で開く">VS Code</button>
    <button onclick={() => open("terminal")} title="ターミナルを開く">端末</button>
    <button onclick={() => open("explorer")} title="エクスプローラーで開く">フォルダ</button>
    <button onclick={() => openClaude(project.path!)} title="このフォルダで claude を起動する (設定の端末で)">Claude</button>
    {#each tools as t (t.id)}
      <button onclick={() => openTool(t.id)} title="{t.label} で開く ({t.command})">{t.label}</button>
    {/each}
  {/if}
  {#if project.kind === "remote"}
    <button class="primary" onclick={() => (cloning = true)}>クローン</button>
  {/if}
</div>

{#if cloning && project.remotes[0]}
  <CloneDialog remote={project.remotes[0]} onclose={() => (cloning = false)} />
{/if}

<style>
  .actions {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }

  .compact button {
    padding: 2px 8px;
    font-size: 12px;
  }
</style>
