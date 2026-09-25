<script lang="ts">
  import { onMount } from "svelte";
  import { app, prefs, init, refresh, savePrefs, type Tab } from "$lib/store.svelte";
  import { buildProjects } from "$lib/derive";
  import { inTauri } from "$lib/api";
  import { relative, toMs } from "$lib/format";
  import StateView from "$lib/components/StateView.svelte";
  import HistoryView from "$lib/components/HistoryView.svelte";
  import GraphView from "$lib/components/GraphView.svelte";
  import ReportView from "$lib/components/ReportView.svelte";
  import SettingsView from "$lib/components/SettingsView.svelte";

  const tabs: { id: Tab; label: string }[] = [
    { id: "state", label: "状態" },
    { id: "history", label: "履歴" },
    { id: "graph", label: "グラフ" },
    { id: "report", label: "日報" },
    { id: "settings", label: "設定" },
  ];

  const projects = $derived(
    app.snapshot && app.config
      ? buildProjects(app.snapshot, app.config, { includeAutomated: prefs.includeAutomated })
      : [],
  );
  const visible = $derived(prefs.showHidden ? projects : projects.filter((p) => !p.hidden));

  // 相対時刻の表示を 1 分ごとに進める
  let now = $state(Date.now());

  onMount(() => {
    init();
    const t = setInterval(() => (now = Date.now()), 60000);
    return () => clearInterval(t);
  });

  function selectTab(t: Tab) {
    prefs.tab = t;
    savePrefs();
  }
</script>

<div class="shell">
  <header>
    <div class="brand">RepoTether</div>
    <div class="tabs" role="tablist">
      {#each tabs as t (t.id)}
        <button
          role="tab"
          class="tab"
          class:on={prefs.tab === t.id}
          aria-selected={prefs.tab === t.id}
          onclick={() => selectTab(t.id)}>{t.label}</button
        >
      {/each}
    </div>
    <div class="status">
      {#if app.busy}
        <span class="spinner" aria-hidden="true"></span>
        <span>{app.progress}</span>
      {:else if app.snapshot}
        <span class="muted" title={app.snapshot.generatedAt}
          >更新 {relative(toMs(app.snapshot.generatedAt), now)}</span
        >
      {/if}
      {#if !inTauri}<span class="badge info">ブラウザ表示 (読み取りのみ)</span>{/if}
      <button onclick={() => refresh(false)} disabled={app.busy} title="ローカルのリポジトリと Claude のセッションを読み直す"
        >更新</button
      >
      <button
        onclick={() => refresh(true)}
        disabled={app.busy || !app.config?.accounts.some((a) => a.enabled)}
        title="GitHub / Gogs のリポジトリ一覧も取り直す">リモートも更新</button
      >
    </div>
  </header>

  {#if app.error}
    <div class="error-bar" role="alert">
      <span>{app.error}</span>
      <button class="ghost" onclick={() => (app.error = "")}>閉じる</button>
    </div>
  {/if}

  <main>
    {#if !app.snapshot || !app.config}
      <div class="empty">
        {#if app.busy}
          <p>{app.progress || "読み込んでいます"}</p>
          <p class="muted">初回は Claude のセッションログをすべて読むため、30 秒ほどかかります。</p>
        {:else}
          <p>データがありません。「更新」を押してください。</p>
        {/if}
      </div>
    {:else if prefs.tab === "state"}
      <StateView projects={visible} {now} />
    {:else if prefs.tab === "history"}
      <HistoryView projects={visible} />
    {:else if prefs.tab === "graph"}
      <GraphView projects={visible} {now} />
    {:else if prefs.tab === "report"}
      <ReportView projects={visible} />
    {:else}
      <SettingsView {projects} />
    {/if}
  </main>

  {#if app.toast}
    <div class="toast" role="status">{app.toast}</div>
  {/if}
</div>

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }

  header {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 0 12px;
    height: 44px;
    border-bottom: 1px solid var(--line);
    background: var(--surface);
    flex: none;
  }

  .brand {
    font-weight: 700;
    font-size: 14px;
    letter-spacing: 0.02em;
  }

  .tabs {
    display: flex;
    gap: 2px;
    align-self: stretch;
  }

  .tab {
    border: none;
    border-radius: 0;
    background: transparent;
    padding: 0 14px;
    color: var(--muted);
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
  }

  .tab:hover:not(:disabled) {
    background: var(--hover);
    color: var(--ink);
  }

  .tab.on {
    color: var(--ink);
    font-weight: 600;
    border-bottom-color: var(--accent);
  }

  .status {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .status > span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .spinner {
    width: 12px;
    height: 12px;
    border: 2px solid var(--line-strong);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
    flex: none;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .error-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    background: color-mix(in srgb, var(--st-high) 12%, var(--surface));
    border-bottom: 1px solid var(--line);
  }

  .error-bar span {
    flex: 1;
  }

  main {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .empty {
    margin: auto;
    text-align: center;
  }

  .toast {
    position: fixed;
    bottom: 16px;
    left: 50%;
    transform: translateX(-50%);
    background: var(--ink);
    color: var(--bg);
    padding: 8px 16px;
    border-radius: var(--radius);
    max-width: 80vw;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.2);
    z-index: 100;
  }
</style>
