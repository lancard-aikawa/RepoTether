<script lang="ts">
  // 「影響を調べる」: 脆弱性がビルド・リリースにどう効くかを、LockWatch (assess) から Claude Code に調べさせて HTML で開く。
  // 詳細パネルの「脆弱性」タブと、脆弱性タブで広げた行の両方に置く。動いているかどうかは store に置き、画面を移っても続く
  import { relative, toMs } from "$lib/format";
  import { app, loadAssessed, runAssess } from "$lib/store.svelte";
  import type { RepoVulns } from "$lib/types";
  import { ASSESS_SINCE, hasAssess } from "$lib/vulns";

  let { repoId, visibility, now }: { repoId: string; visibility: RepoVulns["visibility"]; now: number } = $props();

  const running = $derived(app.assessing.includes(repoId));
  const last = $derived(app.assessed[repoId]);
  const error = $derived(app.assessError[repoId]);
  /** LockWatch の版が分かっていて、assess の無い古いもの。分からなければ押させてみる (古ければ LockWatch がそう答える) */
  const lwVersion = $derived(app.lockwatchStatus?.lockwatch ?? null);
  const tooOld = $derived(lwVersion != null && !hasAssess(lwVersion));

  $effect(() => {
    if (!tooOld && !(repoId in app.assessed)) void loadAssessed(repoId);
  });
</script>

<div class="assess">
  <div class="row">
    <strong>ビルド・リリースへの影響</strong>
    <button
      class:primary={!last?.exists || !!last?.changed}
      onclick={() => runAssess(repoId, true)}
      disabled={running || tooOld}
      title={tooOld
        ? `LockWatch ${lwVersion} にはありません (${ASSESS_SINCE} から)。LockWatch を更新してください (git pull と uv sync)`
        : "Claude Code にリポジトリの中を読ませ、脆弱性のあるパッケージがリリースに入るか、ビルド・開発のときだけかを調べて HTML にする"}
    >
      {running ? "Claude が調べています…" : last?.exists ? "もう一度調べる" : "Claude に影響を調べさせる"}
    </button>
    {#if last?.exists}
      <button onclick={() => runAssess(repoId, false)} title="前回調べた HTML をブラウザで開く (調べ直さない)">
        前回の結果を開く <span class="muted">({relative(toMs(last.madeAt), now)})</span>
      </button>
      {#if last.changed}
        <span class="stale" title="前回調べたあとに照合し直して、脆弱性か言語の版の中身が変わりました">前回のあとに脆弱性が変わった</span>
      {/if}
    {/if}
  </div>
  {#if tooOld}
    <p class="note">
      LockWatch {lwVersion} にはこの機能がありません ({ASSESS_SINCE} から)。LockWatch のフォルダで <code>git pull</code> と <code>uv sync</code> をしてください。
    </p>
  {:else if running}
    <p class="note">数分 (100 パッケージを超えると 10 分ほど) かかります。ほかの操作をしていてかまいません。終わるとブラウザで開きます。</p>
  {:else}
    {#if last?.changed}
      <p class="note">前回の結果は、そのときの脆弱性についての判断です。今の中身で見るなら「もう一度調べる」を押してください。</p>
    {/if}
    <p class="note">
      リリースに入るものだけが急ぎです。Claude Code (claude.exe) にリポジトリの中を<strong>読むことだけ</strong>させて、
      パッケージごとに「リリースに入る・ビルドのときだけ・開発の手元だけ」を根拠のファイルつきで調べます。リポジトリには何も書きません。
      料金は Claude Code の使い方に従います (5 パッケージで約 $0.5、120 パッケージで約 $2)。
      {#if visibility !== "public"}<strong>非公開のリポジトリの中身も Claude (Anthropic) に送ります。</strong>{/if}
    </p>
  {/if}
  {#if error}<p class="err">{error}</p>{/if}
</div>

<style>
  .assess {
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 8px 10px;
    margin: 6px 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
    background: var(--surface);
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 8px;
  }

  .row strong {
    font-size: 12px;
    margin-right: 4px;
  }

  .row button {
    font-size: 12px;
    padding: 2px 8px;
  }

  /* 押せない札 (状態)。ボタンとは形で分ける: 角の丸い平らな札で、枠は点線 */
  .stale {
    font-size: 11px;
    color: var(--st-mid);
    border: 1px dashed var(--st-mid);
    border-radius: 999px;
    padding: 0 8px;
  }

  .note {
    font-size: 12px;
    color: var(--ink-2);
    margin: 0;
  }

  .err {
    font-size: 12px;
    color: var(--st-high);
    margin: 0;
    white-space: pre-wrap;
  }

  code {
    font-family: var(--mono);
    font-size: 12px;
    background: var(--surface-2);
    padding: 0 4px;
    border-radius: 3px;
  }
</style>
