<script lang="ts">
  // Claude のセッションの会話の全文。履歴タブ・詳細パネルの Claude タブから開く
  import type { Session, TranscriptEntry } from "$lib/types";
  import * as api from "$lib/api";
  import { dayKey, formatDateTime, formatDayLabel, formatTime, toMs } from "$lib/format";
  import { errorText, resumeSession } from "$lib/store.svelte";
  import Markdown from "./Markdown.svelte";

  let { session, onclose }: { session: Session; onclose: () => void } = $props();

  let entries = $state<TranscriptEntry[] | null>(null);
  let error = $state("");
  let query = $state("");

  $effect(() => {
    const id = session.id;
    entries = null;
    error = "";
    let cancelled = false;
    api
      .sessionTranscript(id)
      .then((t) => !cancelled && (entries = t))
      .catch((e) => !cancelled && (error = errorText(e)));
    return () => (cancelled = true);
  });

  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!entries || !q) return entries ?? [];
    return entries.filter((e) => e.text.toLowerCase().includes(q) || e.tools.some((t) => t.toLowerCase().includes(q)));
  });

  function span(): string {
    const a = toMs(session.startedAt);
    const b = toMs(session.endedAt);
    if (a == null || b == null) return "";
    return dayKey(a) === dayKey(b)
      ? `${formatDayLabel(dayKey(a))} ${formatTime(a)}〜${formatTime(b)}`
      : `${formatDateTime(a)} 〜 ${formatDateTime(b)}`;
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="dialog panel" role="dialog" aria-modal="true" aria-labelledby="tx-title">
    <header>
      <div class="title-row">
        <h2 id="tx-title">{session.title ?? "(無題)"}</h2>
        <button class="ghost" onclick={onclose}>閉じる</button>
      </div>
      <div class="meta muted small">
        {span()} / プロンプト {session.promptCount} 回
        {#if session.gitBranch}/ <span class="mono">{session.gitBranch}</span>{/if}
        {#if session.cwd}/ <span class="mono">{session.cwd}</span>{/if}
      </div>
      <div class="tools-row">
        {#if session.interactive && session.cwd}
          <button class="primary" onclick={() => resumeSession(session, false)} title="端末で claude -r を実行して、この会話を続ける"
            >再開</button
          >
          <button onclick={() => resumeSession(session, true)} title="元の会話は残して、別の会話として続ける (--fork-session)"
            >分岐して再開</button
          >
        {/if}
        <input type="search" placeholder="この会話を検索" bind:value={query} aria-label="この会話を検索" />
        {#if entries}<span class="muted small num">{shown.length} / {entries.length} 件</span>{/if}
      </div>
    </header>

    <div class="body">
      {#if error}
        <p class="err">{error}</p>
      {:else if !entries}
        <p class="muted">読み込んでいます…</p>
      {:else if !shown.length}
        <p class="muted">{query ? "見つかりません" : "会話がありません"}</p>
      {:else}
        {#each shown as e, i (i)}
          <article class="entry {e.role}">
            <div class="who">
              <span class="role">{e.role === "user" ? "自分" : "Claude"}</span>
              {#if e.at}<span class="muted small num">{formatDateTime(toMs(e.at) ?? 0)}</span>{/if}
            </div>
            {#if e.role === "user"}
              <div class="text plain">{e.text}</div>
            {:else}
              {#if e.text}<Markdown source={e.text} />{/if}
              {#if e.tools.length}
                <div class="used muted small">使ったツール: {e.tools.join(" → ")}</div>
              {/if}
            {/if}
          </article>
        {/each}
      {/if}
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
    z-index: 60;
  }

  .dialog {
    width: min(960px, calc(100vw - 48px));
    height: calc(100vh - 64px);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  header {
    padding: 14px 18px 10px;
    border-bottom: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .title-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .title-row h2 {
    margin: 0;
  }

  .tools-row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .tools-row input {
    width: 240px;
    margin-left: auto;
  }

  .small {
    font-size: 12px;
  }

  .body {
    overflow-y: auto;
    padding: 12px 18px 24px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .entry {
    border-radius: 8px;
    padding: 10px 14px;
    border: 1px solid var(--line);
  }

  /* 自分の発言は薄い青、Claude は面の色。色だけに頼らず「自分 / Claude」も書く */
  .entry.user {
    background: var(--accent-wash);
    border-color: color-mix(in srgb, var(--accent) 30%, var(--line));
  }

  .entry.assistant {
    background: var(--surface);
  }

  .who {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin-bottom: 4px;
  }

  .role {
    font-weight: 600;
    font-size: 12px;
  }

  .plain {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .used {
    margin-top: 6px;
    font-family: var(--mono);
  }

  .err {
    color: var(--st-high);
  }
</style>
