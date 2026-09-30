<script lang="ts">
  // プロジェクトのアイコン。フォルダの中にアプリのアイコンがあればそれを、無ければ名前の頭文字と色で出す。
  import * as api from "$lib/api";

  let { name, path, size = 20 }: { name: string; path: string | null; size?: number } = $props();

  let url = $state<string | null>(null);
  let broken = $state(false);

  $effect(() => {
    const p = path;
    url = null;
    broken = false;
    if (!p) return;
    let alive = true;
    api.repoIcon(p).then((u) => alive && (url = u));
    return () => {
      alive = false;
    };
  });

  /** 頭文字: 最初の文字か数字 (英字は大文字に) */
  const letter = $derived((name.match(/[\p{L}\p{N}]/u)?.[0] ?? "?").toUpperCase());

  /** 名前から決まる色相。同じ名前なら毎回同じ色 */
  const hue = $derived.by(() => {
    let h = 0;
    for (const c of name.toLowerCase()) h = (h * 31 + c.codePointAt(0)!) >>> 0;
    return h % 360;
  });
</script>

{#if url && !broken}
  <img class="icon" src={url} alt="" width={size} height={size} style="--s: {size}px" onerror={() => (broken = true)} />
{:else}
  <span class="icon letter" style="--s: {size}px; --h: {hue}" aria-hidden="true">{letter}</span>
{/if}

<style>
  .icon {
    width: var(--s);
    height: var(--s);
    flex: none;
    border-radius: calc(var(--s) * 0.22);
    object-fit: contain;
    vertical-align: middle;
  }

  .letter {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: calc(var(--s) * 0.58);
    font-weight: 700;
    line-height: 1;
    color: #fff;
    background: oklch(0.6 0.11 var(--h));
    user-select: none;
  }
</style>
