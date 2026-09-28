<script lang="ts">
  import type { Project } from "$lib/derive";
  import * as api from "$lib/api";
  import { errorText, prefs, toast } from "$lib/store.svelte";
  import CloneDialog from "./CloneDialog.svelte";

  let { project, compact = false }: { project: Project; compact?: boolean } = $props();

  let cloning = $state(false);

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
