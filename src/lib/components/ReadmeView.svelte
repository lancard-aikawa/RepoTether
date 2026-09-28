<script lang="ts">
  import { marked } from "marked";
  import DOMPurify from "dompurify";
  import * as api from "$lib/api";
  import { errorText, toast } from "$lib/store.svelte";
  import type { RemoteLink } from "$lib/remotes";

  let { path, link }: { path: string; link: RemoteLink | null } = $props();

  let readme = $state<api.Readme | null>(null);
  let loading = $state(true);
  let error = $state("");

  $effect(() => {
    const p = path;
    loading = true;
    error = "";
    readme = null;
    let cancelled = false;
    api
      .readReadme(p)
      .then((r) => !cancelled && (readme = r))
      .catch((e) => !cancelled && (error = errorText(e)))
      .finally(() => !cancelled && (loading = false));
    return () => (cancelled = true);
  });

  /** GitHub なら相対パスを github.com の URL にする (ほかのサービスはブランチ名の形が違うので解決しない) */
  function resolve(rel: string, raw: boolean): string | null {
    if (link?.kind !== "github" || !link.webUrl) return null;
    const clean = rel.replace(/^\.?\//, "");
    return `${link.webUrl}/${raw ? "raw" : "blob"}/HEAD/${clean}`;
  }

  const isHttp = (u: string) => /^https?:\/\//i.test(u);

  // README は他人が書いたもの。ここで無害化しないと、埋め込まれたスクリプトからアプリの機能を呼べてしまう
  const html = $derived.by(() => {
    if (!readme?.markdown) return "";
    const raw = marked.parse(readme.content, { async: false, gfm: true }) as string;
    const clean = DOMPurify.sanitize(raw, {
      FORBID_TAGS: ["style", "form", "input", "button", "textarea", "select", "iframe", "object", "embed"],
      FORBID_ATTR: ["style"],
    });
    // 画像とリンクの行き先を整える (http(s) 以外は落とす)
    const doc = new DOMParser().parseFromString(`<div>${clean}</div>`, "text/html");
    for (const img of doc.querySelectorAll("img")) {
      const src = img.getAttribute("src") ?? "";
      const url = isHttp(src) ? src : resolve(src, true);
      if (url) img.setAttribute("src", url);
      else img.replaceWith(doc.createTextNode(img.getAttribute("alt") ?? ""));
    }
    for (const a of doc.querySelectorAll("a")) {
      const href = a.getAttribute("href") ?? "";
      const url = href.startsWith("#") ? href : isHttp(href) ? href : resolve(href, false);
      if (url) a.setAttribute("data-href", url);
      a.removeAttribute("href");
      a.setAttribute("title", url ?? `${href} (開けません)`);
    }
    return doc.body.firstElementChild?.innerHTML ?? "";
  });

  /** リンクはアプリ内では開かず、ブラウザへ渡す */
  async function onClick(e: MouseEvent) {
    const a = (e.target as HTMLElement).closest("a");
    if (!a) return;
    e.preventDefault();
    const url = a.getAttribute("data-href");
    if (!url) return;
    if (url.startsWith("#")) {
      const id = decodeURIComponent(url.slice(1));
      (e.currentTarget as HTMLElement).querySelector(`[id="${CSS.escape(id)}"]`)?.scrollIntoView();
      return;
    }
    try {
      await api.openUrl(url);
    } catch (err) {
      toast(errorText(err));
    }
  }
</script>

{#if loading}
  <p class="muted">読み込んでいます…</p>
{:else if error}
  <p class="muted">{error}</p>
{:else if !readme}
  <p class="muted">README はありません。</p>
{:else}
  <div class="meta muted">
    {readme.name}
    {#if readme.truncated}<span> (長いので先頭の 1MB だけ)</span>{/if}
  </div>
  {#if readme.markdown}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="md" onclick={onClick}>{@html html}</div>
  {:else}
    <pre class="plain">{readme.content}</pre>
  {/if}
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

  .md {
    font-size: 13px;
    line-height: 1.65;
    overflow-wrap: anywhere;
  }

  .md :global(h1),
  .md :global(h2),
  .md :global(h3),
  .md :global(h4) {
    margin: 1.1em 0 0.4em;
    line-height: 1.3;
  }

  .md :global(h1) {
    font-size: 18px;
    padding-bottom: 4px;
    border-bottom: 1px solid var(--line);
  }

  .md :global(h2) {
    font-size: 15px;
    padding-bottom: 3px;
    border-bottom: 1px solid var(--line);
  }

  .md :global(h3) {
    font-size: 13.5px;
  }

  .md :global(> :first-child) {
    margin-top: 0;
  }

  .md :global(p),
  .md :global(ul),
  .md :global(ol),
  .md :global(pre),
  .md :global(table),
  .md :global(blockquote) {
    margin: 0 0 0.8em;
  }

  .md :global(ul),
  .md :global(ol) {
    padding-left: 1.4em;
  }

  .md :global(a) {
    color: var(--accent);
    cursor: pointer;
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .md :global(code) {
    font-family: var(--mono);
    font-size: 12px;
    background: var(--surface-2);
    padding: 0.1em 0.35em;
    border-radius: 3px;
  }

  .md :global(pre) {
    background: var(--surface-2);
    padding: 8px 10px;
    border-radius: 4px;
    overflow-x: auto;
  }

  .md :global(pre code) {
    background: none;
    padding: 0;
  }

  .md :global(table) {
    border-collapse: collapse;
    display: block;
    overflow-x: auto;
  }

  .md :global(th),
  .md :global(td) {
    border: 1px solid var(--line-strong);
    padding: 3px 8px;
  }

  .md :global(th) {
    background: var(--surface-2);
  }

  .md :global(blockquote) {
    border-left: 3px solid var(--line-strong);
    padding-left: 10px;
    color: var(--ink-2);
  }

  .md :global(img) {
    max-width: 100%;
  }

  .md :global(hr) {
    border: none;
    border-top: 1px solid var(--line);
  }
</style>
