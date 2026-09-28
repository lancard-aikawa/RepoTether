<script lang="ts">
  import type { RemoteLink } from "$lib/remotes";
  import * as api from "$lib/api";
  import { errorText, toast } from "$lib/store.svelte";

  let {
    links,
    showUrl = false,
    showNone = true,
  }: {
    links: RemoteLink[];
    /** URL を横に並べる (詳細パネル用)。false なら URL はマウスを乗せたときに出す */
    showUrl?: boolean;
    /** リモートが無いとき「なし」を出す */
    showNone?: boolean;
  } = $props();

  async function open(e: MouseEvent, url: string) {
    // 行の選択 (クリック) まで伝えない
    e.stopPropagation();
    try {
      await api.openUrl(url);
    } catch (err) {
      toast(errorText(err));
    }
  }

  function tip(l: RemoteLink): string {
    return [l.remoteName ? `${l.remoteName}: ${l.url}` : l.url, l.webUrl && l.webUrl !== l.url ? l.webUrl : null]
      .filter(Boolean)
      .join("\n");
  }
</script>

{#snippet icon(kind: RemoteLink["kind"] | "none")}
  <svg class="icon" viewBox="0 0 16 16" aria-hidden="true">
    {#if kind === "github"}
      <path
        class="fill"
        d="M8 0c4.42 0 8 3.58 8 8a8.013 8.013 0 0 1-5.45 7.59c-.4.08-.55-.17-.55-.38 0-.27.01-1.13.01-2.2 0-.75-.25-1.23-.54-1.48 1.78-.2 3.65-.88 3.65-3.95 0-.88-.31-1.59-.82-2.15.08-.2.36-1.02-.08-2.12 0 0-.67-.22-2.2.82-.64-.18-1.32-.27-2-.27-.68 0-1.36.09-2 .27-1.53-1.03-2.2-.82-2.2-.82-.44 1.1-.16 1.92-.08 2.12-.51.56-.82 1.28-.82 2.15 0 3.06 1.86 3.75 3.64 3.95-.23.2-.44.55-.51 1.07-.46.21-1.61.55-2.33-.66-.15-.24-.6-.83-1.23-.82-.67.01-.27.38.01.53.34.19.73.9.82 1.13.16.45.68 1.31 2.69.94 0 .67.01 1.3.01 1.49 0 .21-.15.45-.55.38A7.995 7.995 0 0 1 0 8c0-4.42 3.58-8 8-8Z"
      />
    {:else if kind === "gogs" || kind === "gitea"}
      <!-- 自前のサーバー -->
      <rect class="line" x="2" y="2.5" width="12" height="4.5" rx="1" />
      <rect class="line" x="2" y="9" width="12" height="4.5" rx="1" />
      <circle class="fill" cx="4.75" cy="4.75" r="0.9" />
      <circle class="fill" cx="4.75" cy="11.25" r="0.9" />
    {:else if kind === "gitlab" || kind === "backlog"}
      <!-- クラウドのサービス -->
      <path class="line" d="M4.5 13h7.25a3 3 0 0 0 .35-5.98A4.25 4.25 0 0 0 3.9 6.3 3.4 3.4 0 0 0 4.5 13Z" />
    {:else if kind === "other"}
      <circle class="line" cx="8" cy="8" r="6" />
      <path class="line" d="M2 8h12M8 2c1.8 1.7 2.7 3.7 2.7 6S9.8 12.3 8 14c-1.8-1.7-2.7-3.7-2.7-6S6.2 3.7 8 2Z" />
    {:else}
      <circle class="line" cx="8" cy="8" r="6" />
      <path class="line" d="M3.8 12.2 12.2 3.8" />
    {/if}
  </svg>
{/snippet}

{#if links.length}
  {#each links as l (l.url)}
    <span class="remote">
      <span class="chip k-{l.kind}" title={tip(l)}>
        {@render icon(l.kind)}
        <span>{l.label}</span>
        <!-- 複数あるときは origin 以外に remote 名を添えて見分ける -->
        {#if links.length > 1 && l.remoteName && l.remoteName !== "origin"}
          <span class="rname">{l.remoteName}</span>
        {/if}
      </span>
      {#if showUrl}<span class="url mono" title={l.url}>{l.url}</span>{/if}
      {#if l.webUrl}
        <button
          class="open"
          title="ブラウザで開く: {l.webUrl}"
          aria-label="{l.label} をブラウザで開く"
          onclick={(e) => open(e, l.webUrl!)}
        >
          <svg class="icon" viewBox="0 0 16 16" aria-hidden="true">
            <path class="line" d="M9.5 2.5h4v4M13.5 2.5 7.5 8.5M12 9.5V13a.5.5 0 0 1-.5.5h-8A.5.5 0 0 1 3 13V5a.5.5 0 0 1 .5-.5H7" />
          </svg>
        </button>
      {/if}
    </span>
  {/each}
{:else if showNone}
  <span class="remote">
    <span class="chip k-none" title="リモートが設定されていません (push 先がありません)">
      {@render icon("none")}
      <span>なし</span>
    </span>
  </span>
{/if}

<style>
  .remote {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    min-width: 0;
  }

  /* 種類ごとに色を変える。色はアイコンと地の薄い色だけで、文字は本文の色のまま */
  .chip {
    --k: var(--muted);
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 0 7px 0 5px;
    height: 20px;
    border-radius: 4px;
    font-size: 11.5px;
    line-height: 1;
    white-space: nowrap;
    color: var(--ink);
    background: color-mix(in srgb, var(--k) 13%, var(--surface));
    border: 1px solid color-mix(in srgb, var(--k) 35%, var(--surface));
  }

  .k-github {
    --k: var(--rk-github);
  }
  .k-gogs {
    --k: var(--rk-gogs);
  }
  .k-gitea {
    --k: var(--rk-gitea);
  }
  .k-gitlab {
    --k: var(--rk-gitlab);
  }
  .k-backlog {
    --k: var(--rk-backlog);
  }
  .k-other {
    --k: var(--muted);
  }
  .k-none {
    --k: var(--muted);
    background: transparent;
    border-style: dashed;
    color: var(--ink-2);
  }

  .rname {
    color: var(--ink-2);
    border-left: 1px solid color-mix(in srgb, var(--k) 35%, var(--surface));
    padding-left: 4px;
  }

  .icon {
    width: 13px;
    height: 13px;
    flex: none;
  }

  .icon .fill {
    fill: var(--k, currentColor);
  }

  .icon .line {
    fill: none;
    stroke: var(--k, currentColor);
    stroke-width: 1.4;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .url {
    font-size: 11.5px;
    color: var(--ink-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }

  .open {
    --k: var(--ink-2);
    display: inline-grid;
    place-items: center;
    width: 22px;
    height: 20px;
    padding: 0;
    border: 1px solid transparent;
    background: transparent;
    border-radius: 4px;
  }

  .open:hover {
    --k: var(--accent);
    background: var(--hover);
    border-color: var(--line-strong);
  }
</style>
