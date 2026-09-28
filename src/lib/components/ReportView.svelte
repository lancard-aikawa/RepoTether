<script lang="ts">
  import { untrack } from "svelte";
  import type { Project } from "$lib/derive";
  import { buildReport } from "$lib/report";
  import { addDays, dayKey, parseDayKey, startOfDay } from "$lib/format";
  import * as api from "$lib/api";
  import { errorText, prefs, savePrefs, toast, type ReportViewMode } from "$lib/store.svelte";
  import Markdown from "./Markdown.svelte";

  let { projects }: { projects: Project[] } = $props();

  let day = $state(dayKey(Date.now()));
  let days = $state(1);
  let includeLeftovers = $state(true);
  let includeReplies = $state(false);

  // 生成した文面。手で直せるように、条件が変わったときだけ作り直す
  let text = $state("");
  let edited = $state(false);

  const generated = $derived(
    buildReport(projects, {
      day,
      days,
      includeAutomated: prefs.includeAutomated,
      includeLeftovers,
      includeReplies,
    }),
  );

  // 日付や条件を変えたら、手直し中でも作り直す
  $effect(() => {
    void [day, days, includeLeftovers, includeReplies];
    untrack(() => {
      edited = false;
      text = generated;
    });
  });

  // 手で直している最中は、裏の更新で上書きしない (「作り直す」で反映する)
  $effect(() => {
    const g = generated;
    if (!untrack(() => edited)) text = g;
  });

  function shift(n: number) {
    day = dayKey(addDays(parseDayKey(day).getTime(), n * days));
  }

  async function copy() {
    try {
      await navigator.clipboard.writeText(text);
      toast("コピーしました");
    } catch (e) {
      toast(errorText(e));
    }
  }

  async function save() {
    const name = days === 1 ? `日報_${day}.md` : `週報_${day}.md`;
    try {
      const p = await api.saveTextWithDialog(name, text);
      if (p) toast(`${p} に保存しました`);
    } catch (e) {
      toast(errorText(e));
    }
  }

  const viewModes: { id: ReportViewMode; label: string }[] = [
    { id: "edit", label: "編集" },
    { id: "split", label: "並べて表示" },
    { id: "preview", label: "プレビュー" },
  ];

  function setViewMode(m: ReportViewMode) {
    prefs.reportView = m;
    savePrefs();
  }

  const isToday = $derived(day === dayKey(startOfDay(Date.now())));
</script>

<div class="wrap">
  <div class="toolbar bar">
    <div class="segmented" role="group" aria-label="種類">
      <button class:on={days === 1} onclick={() => (days = 1)}>日報</button>
      <button class:on={days === 7} onclick={() => (days = 7)}>週報 (7 日)</button>
    </div>
    <button onclick={() => shift(-1)} aria-label="前へ">前へ</button>
    <input type="date" bind:value={day} aria-label={days === 1 ? "日付" : "最終日"} />
    <button onclick={() => shift(1)} disabled={isToday} aria-label="次へ">次へ</button>
    {#if !isToday}<button class="ghost" onclick={() => (day = dayKey(Date.now()))}>今日</button>{/if}
    <label class="check"><input type="checkbox" bind:checked={includeLeftovers} /> 残っていること</label>
    <label class="check"><input type="checkbox" bind:checked={includeReplies} /> Claude の最後の返答</label>
    <span class="spacer"></span>
    <div class="segmented" role="group" aria-label="表示">
      {#each viewModes as m (m.id)}
        <button class:on={prefs.reportView === m.id} onclick={() => setViewMode(m.id)}>{m.label}</button>
      {/each}
    </div>
    {#if edited}
      <button class="ghost" onclick={() => ((text = generated), (edited = false))}>作り直す</button>
    {/if}
    <button onclick={copy}>コピー</button>
    <button class="primary" onclick={save}>保存</button>
  </div>
  <div class="editor" class:split={prefs.reportView === "split"}>
    {#if prefs.reportView !== "preview"}
      <textarea bind:value={text} oninput={() => (edited = true)} spellcheck="false" aria-label="日報の本文"></textarea>
    {/if}
    {#if prefs.reportView !== "edit"}
      <div class="preview panel" aria-label="プレビュー">
        <Markdown source={text} />
      </div>
    {/if}
  </div>
</div>

<style>
  .wrap {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }

  .bar {
    padding: 10px 12px;
    border-bottom: 1px solid var(--line);
  }

  .spacer {
    flex: 1;
  }

  .editor {
    flex: 1;
    min-height: 0;
    padding: 12px 16px 16px;
    display: flex;
    gap: 12px;
  }

  textarea {
    flex: 1;
    min-width: 0;
    resize: none;
    font-family: var(--mono);
    font-size: 13px;
    line-height: 1.6;
    padding: 12px 14px;
    max-width: 980px;
  }

  .preview {
    flex: 1;
    min-width: 0;
    max-width: 980px;
    overflow-y: auto;
    padding: 14px 20px;
  }

  /* 並べて表示では左右を同じ幅に */
  .split textarea,
  .split .preview {
    flex: 1 1 0;
  }
</style>
