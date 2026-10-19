<script lang="ts">
  // Markdown を HTML にして表示する (README・日報のプレビュー)。
  // 中身は他人の README やコミットのメッセージなど、信用できない文字列を含む。
  // 無害化しないと、埋め込まれたスクリプトからアプリの機能を呼べてしまうので、必ず DOMPurify を通す。
  import { marked } from "marked";
  import DOMPurify from "dompurify";
  import * as api from "$lib/api";
  import { imagePlan } from "$lib/mdimages";
  import { errorText, toast } from "$lib/store.svelte";

  let {
    source,
    resolve = () => null,
  }: {
    source: string;
    /** 相対パスのリンク・画像の行き先。解決できなければ null (リンクは開けず、画像は代替文字にする) */
    resolve?: (rel: string, raw: boolean) => string | null;
  } = $props();

  const isHttp = (u: string) => /^https?:\/\//i.test(u);

  // 外の画像は、押すまで読み込まない (理由は mdimages.ts)。押したら、この文書の画像をまとめて出す。
  // 別の文書に変わったら、また押すまで読み込まない
  let imagesShownFor = $state<string | null>(null);
  const imagesAllowed = $derived(imagesShownFor === source);

  const html = $derived.by(() => {
    const raw = marked.parse(source, { async: false, gfm: true }) as string;
    const clean = DOMPurify.sanitize(raw, {
      FORBID_TAGS: ["style", "form", "input", "button", "textarea", "select", "iframe", "object", "embed"],
      FORBID_ATTR: ["style"],
    });
    // 画像とリンクの行き先を整える (http(s) 以外は落とす)
    const doc = new DOMParser().parseFromString(`<div>${clean}</div>`, "text/html");
    for (const img of doc.querySelectorAll("img")) {
      const plan = imagePlan(img.getAttribute("src") ?? "", (rel) => resolve(rel, true), imagesAllowed);
      const alt = img.getAttribute("alt") ?? "";
      if (plan.kind === "show") {
        img.setAttribute("src", plan.url);
        // 幅を変えて何度も取りに行かせない
        img.removeAttribute("srcset");
      } else if (plan.kind === "ask") {
        // 無害化のあとで自分で作る要素なので、button でよい (中身は文字として入れる)
        const b = doc.createElement("button");
        b.setAttribute("type", "button");
        b.setAttribute("class", "img-ask");
        b.setAttribute("data-img-ask", "");
        b.setAttribute("title", `${plan.url}\n押すと、この文書の画像を読み込みます`);
        b.textContent = `画像を表示: ${alt ? `${alt} ` : ""}(${plan.host})`;
        img.replaceWith(b);
      } else {
        img.replaceWith(doc.createTextNode(alt));
      }
    }
    // <picture> の <source> など、img のほかに画像を取りに行くものは落とす
    for (const s of doc.querySelectorAll("source, video, audio, track")) s.remove();
    for (const a of doc.querySelectorAll("a")) {
      const href = a.getAttribute("href") ?? "";
      const url = href.startsWith("#") ? href : isHttp(href) ? href : resolve(href, false);
      if (url) a.setAttribute("data-href", url);
      a.removeAttribute("href");
      a.setAttribute("title", url ?? (href ? `${href} (開けません)` : "開けないリンクです"));
    }
    return doc.body.firstElementChild?.innerHTML ?? "";
  });

  /** リンクはアプリ内では開かず、ブラウザへ渡す */
  async function onClick(e: MouseEvent) {
    if ((e.target as HTMLElement).closest("[data-img-ask]")) {
      imagesShownFor = source;
      return;
    }
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

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="md" onclick={onClick}>{@html html}</div>

<style>
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

  /* 押すまで読み込まない画像の代わり。押せるものなので、枠と塗り・乗せたときの変化・指のカーソルを付ける */
  .md :global(.img-ask) {
    font: inherit;
    font-size: 12px;
    color: var(--ink-2);
    background: var(--surface-2);
    border: 1px solid var(--line-strong);
    border-radius: 4px;
    padding: 2px 8px;
    margin: 1px 0;
    cursor: pointer;
    max-width: 100%;
    overflow-wrap: anywhere;
    text-align: left;
  }

  .md :global(.img-ask:hover) {
    background: var(--accent-wash);
    border-color: var(--accent);
    color: var(--accent);
  }

  .md :global(.img-ask:active) {
    background: var(--accent);
    color: var(--accent-ink);
  }

  .md :global(.img-ask:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .md :global(hr) {
    border: none;
    border-top: 1px solid var(--line);
  }
</style>
