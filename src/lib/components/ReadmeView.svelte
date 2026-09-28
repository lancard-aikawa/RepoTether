<script lang="ts">
  import type * as api from "$lib/api";
  import type { RemoteLink } from "$lib/remotes";
  import Markdown from "./Markdown.svelte";

  // 読み込みは詳細パネル側 (README の有無でタブを出し分けるため)
  let { readme, link }: { readme: api.Readme; link: RemoteLink | null } = $props();

  /** GitHub なら相対パスを github.com の URL にする (ほかのサービスはブランチ名の形が違うので解決しない) */
  function resolve(rel: string, raw: boolean): string | null {
    if (link?.kind !== "github" || !link.webUrl) return null;
    const clean = rel.replace(/^\.?\//, "");
    return `${link.webUrl}/${raw ? "raw" : "blob"}/HEAD/${clean}`;
  }
</script>

<div class="meta muted">
  {readme.name}
  {#if readme.truncated}<span> (長いので先頭の 1MB だけ)</span>{/if}
</div>
{#if readme.markdown}
  <Markdown source={readme.content} {resolve} />
{:else}
  <pre class="plain">{readme.content}</pre>
{/if}

<style>
  .meta {
    font-size: 12px;
    margin-bottom: 8px;
  }

  .plain {
    white-space: pre-wrap;
    font-family: var(--mono);
    font-size: 12px;
    margin: 0;
  }
</style>
