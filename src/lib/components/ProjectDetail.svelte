<script lang="ts">
  import type { Project } from "$lib/derive";
  import { formatDateTime, formatTime, relative, toMs, dayKey, formatDayLabel } from "$lib/format";
  import * as api from "$lib/api";
  import {
    allTags,
    isAncestorTag,
    errorText,
    normalizeTag,
    prefs,
    refresh,
    savePrefs,
    setHidden,
    setTags,
    toast,
    type DetailTab,
  } from "$lib/store.svelte";
  import ProjectActions from "./ProjectActions.svelte";
  import ReadmeView from "./ReadmeView.svelte";
  import RemoteBadges from "./RemoteBadges.svelte";

  let { project, now, onclose }: { project: Project; now: number; onclose: () => void } = $props();

  let showAllCommits = $state(false);
  let newTag = $state("");
  let tagError = $state("");
  const tagChoices = $derived(allTags().filter((t) => !project.tags.includes(t)));

  async function addTag() {
    const t = normalizeTag(newTag);
    if (!t) {
      tagError = "「仕事/客先/案件」のように、/ 区切りで 3 階層までにしてください";
      return;
    }
    tagError = "";
    if (project.tags.includes(t)) {
      tagError = `「${t}」はもう付いています`;
      return;
    }
    const child = project.tags.find((x) => isAncestorTag(t, x));
    if (child) {
      tagError = `「${t}」は「${child}」に含まれるので追加しません`;
      return;
    }
    const replaced = project.tags.filter((x) => isAncestorTag(x, t));
    try {
      await setTags(project.prefKey, [...project.tags, t]);
      newTag = "";
      if (replaced.length) toast(`「${replaced.join("」「")}」を「${t}」に置き換えました`);
    } catch (e) {
      toast(errorText(e));
    }
  }

  async function removeTag(t: string) {
    try {
      await setTags(
        project.prefKey,
        project.tags.filter((x) => x !== t),
      );
    } catch (e) {
      toast(errorText(e));
    }
  }

  const r = $derived(project.local);

  // ---- README (有無でタブを出し分けるので、選んだ時点で読む) ----
  type ReadmeState =
    | { state: "loading" }
    | { state: "found"; readme: api.Readme }
    | { state: "none" }
    | { state: "error"; message: string };
  let readme = $state<ReadmeState>({ state: "none" });

  $effect(() => {
    const path = project.path;
    if (!path) {
      readme = { state: "none" };
      return;
    }
    readme = { state: "loading" };
    let cancelled = false;
    api
      .readReadme(path)
      .then((r) => {
        if (!cancelled) readme = r ? { state: "found", readme: r } : { state: "none" };
      })
      .catch((e) => {
        if (!cancelled) readme = { state: "error", message: errorText(e) };
      });
    return () => (cancelled = true);
  });

  // ---- タブ ----
  const tabs = $derived(
    (
      [
        { id: "summary", label: "概要", show: true },
        { id: "git", label: "git", show: project.kind !== "folder" },
        { id: "commits", label: "コミット", count: project.myCommits.length, show: project.commits.length > 0 },
        { id: "claude", label: "Claude", count: project.sessions.length, show: project.sessions.length > 0 },
        // 読み込み中は出しておき、無いと分かったら消す (タブがちらつかないように)
        { id: "readme", label: "README", show: readme.state === "loading" || readme.state === "found" },
      ] as { id: DetailTab; label: string; count?: number; show: boolean }[]
    ).filter((t) => t.show),
  );
  // 最後に開いたタブを覚える。そのプロジェクトに無いタブなら概要
  const tab = $derived(tabs.some((t) => t.id === prefs.detailTab) ? prefs.detailTab : "summary");

  function selectTab(t: DetailTab) {
    prefs.detailTab = t;
    savePrefs();
  }

  // 一覧は多いときだけ「さらに表示」
  let sessionLimit = $state(30);
  let commitLimit = $state(50);
  // 別のプロジェクトを選んだら元に戻す
  $effect(() => {
    void project.key;
    sessionLimit = 30;
    commitLimit = 50;
  });
  const sessions = $derived(project.sessions.slice(0, sessionLimit));
  const commitSource = $derived(showAllCommits ? project.commits : project.myCommits);
  const commits = $derived(commitSource.slice(0, commitLimit));
  const webLink = $derived(project.links[0] ?? null);


  async function trust() {
    if (!project.path) return;
    if (
      !confirm(
        `${project.path} を git の safe.directory に追加します。\n` +
          `(ユーザーのグローバル設定 ~/.gitconfig に書き込みます。自分のフォルダであることを確認してから進めてください)`,
      )
    )
      return;
    try {
      await api.trustRepo(project.path);
      toast("safe.directory に追加しました");
      await refresh(false);
    } catch (e) {
      toast(errorText(e));
    }
  }

  function when(s: string | null): string {
    const t = toMs(s);
    return t == null ? "-" : relative(t, now);
  }

  function sessionSpan(start: string | null, end: string | null): string {
    const a = toMs(start);
    const b = toMs(end);
    if (a == null || b == null) return "";
    const sameDay = dayKey(a) === dayKey(b);
    return sameDay
      ? `${formatDayLabel(dayKey(a))} ${formatTime(a)}〜${formatTime(b)}`
      : `${formatDateTime(a)} 〜 ${formatDateTime(b)}`;
  }
</script>

<aside class="detail">
  <div class="head">
    <div class="title">
      <h2>{project.name}</h2>
      <button class="ghost" onclick={onclose} aria-label="閉じる">閉じる</button>
    </div>
    {#if project.path}<div class="mono muted path">{project.path}</div>{/if}
    <ProjectActions {project} />
  </div>

  <div class="tabs" role="tablist">
    {#each tabs as t (t.id)}
      <button role="tab" class="tab" class:on={tab === t.id} aria-selected={tab === t.id} onclick={() => selectTab(t.id)}>
        {t.label}{#if t.count != null}<span class="count num">{t.count}</span>{/if}
      </button>
    {/each}
  </div>

  <div class="body">
    {#if tab === "summary"}
    <section>
      <h3>タグ</h3>
      {#if project.tags.length}
        <div class="tags">
          {#each project.tags as t (t)}
            <span class="tag-chip">
              {t}
              <button class="x" onclick={() => removeTag(t)} aria-label="タグ {t} を外す">×</button>
            </span>
          {/each}
        </div>
      {/if}
      <div class="tag-add">
        <input
          type="text"
          list="tag-choices"
          placeholder="仕事/客先/案件 (3 階層まで)"
          bind:value={newTag}
          oninput={() => (tagError = "")}
          onkeydown={(e) => e.key === "Enter" && !e.isComposing && addTag()}
        />
        <button onclick={addTag} disabled={!newTag.trim()}>追加</button>
        <datalist id="tag-choices">
          {#each tagChoices as t (t)}<option value={t}></option>{/each}
        </datalist>
      </div>
      {#if tagError}<p class="small err">{tagError}</p>{/if}
    </section>

    {#if project.leftovers.length}
      <section>
        <h3>取り残し</h3>
        <ul class="plain">
          {#each project.leftovers as l (l.kind)}
            <li>
              <span class="badge {l.severity}">{l.label}</span>
              {#if l.detail}<span class="muted small">{l.detail}</span>{/if}
            </li>
          {/each}
        </ul>
        {#if project.leftovers.some((l) => l.kind === "dubious")}
          <p class="small">
            git がフォルダの所有者を確認できないため読めません。自分のフォルダなら、safe.directory に追加すると読めるようになります。
          </p>
          <button onclick={trust}>safe.directory に追加</button>
        {/if}
        {#if r?.error && !project.leftovers.some((l) => l.kind === "dubious")}
          <pre class="small">{r.error}</pre>
        {/if}
      </section>
    {/if}

    <section>
      {#if project.hidden}
        <button onclick={() => setHidden(project.prefKey, false)}>表示に戻す</button>
      {:else}
        <button onclick={() => setHidden(project.prefKey, true)} title="状態タブの「非表示」で見られます。履歴・グラフ・日報には出なくなります">非表示にする</button>
      {/if}
    </section>
    {/if}

    {#if tab === "git"}
    {#if r && !r.error}
      <section>
        <h3>ブランチ</h3>
        <dl>
          <dt>現在</dt>
          <dd class="mono">{r.branch ?? "(detached)"}{r.head ? ` @ ${r.head.slice(0, 8)}` : ""}</dd>
          <dt>追跡先</dt>
          <dd class="mono">
            {r.upstream ?? "なし"}
            {#if r.upstream}<span class="muted"> (未 push {r.ahead} / 未取り込み {r.behind})</span>{/if}
          </dd>
          <dt>既定</dt>
          <dd class="mono">{r.defaultBranch ?? "-"}</dd>
          <dt>最後の fetch</dt>
          <dd>{when(r.lastFetchAt)}</dd>
          <dt>最新コミット</dt>
          <dd>{when(r.lastCommitAt)}</dd>
        </dl>
        {#if r.branches.length}
          <table>
            <thead><tr><th>ブランチ</th><th>状態</th><th>最新</th></tr></thead>
            <tbody>
              {#each r.branches as b (b.name)}
                <tr>
                  <td class="mono">{b.name}</td>
                  <td class="small">
                    {[
                      b.merged ? "マージ済み" : "未マージ",
                      b.gone ? "リモートで削除済み" : b.upstream ? null : "upstream なし",
                      b.ahead ? `未 push ${b.ahead}` : null,
                    ]
                      .filter(Boolean)
                      .join(" / ")}
                  </td>
                  <td class="small muted">{when(b.lastCommitAt)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      </section>
    {/if}

    {#if r?.error}<p class="muted small">git の状態を読めません。概要タブを見てください。</p>{/if}
    {#if project.kind !== "folder"}
      <section>
        <h3>リモート</h3>
        <div class="remotes">
          <RemoteBadges links={project.links} showUrl />
        </div>
        {#each project.remotes as rr (rr.key)}
          <p class="small muted">
            {rr.fullName}
            {#if rr.private}(非公開){/if}
            {#if rr.archived}(アーカイブ){/if}
            {#if rr.pushedAt || rr.updatedAt}/ 最終 push {when(rr.pushedAt ?? rr.updatedAt)}{/if}
            {#if rr.description}<br />{rr.description}{/if}
          </p>
        {/each}
      </section>
    {/if}

    {/if}

    {#if tab === "claude"}
      <section>
        <h3>Claude のセッション <span class="muted small">{project.sessions.length} 件 (新しい順)</span></h3>
        <ul class="sessions">
          {#each sessions as s (s.id)}
            <li class:auto={!s.interactive}>
              <div class="s-head">
                <strong>{s.title ?? "(無題)"}</strong>
                {#if !s.interactive}<span class="host">自動</span>{/if}
              </div>
              <div class="muted small">
                {sessionSpan(s.startedAt, s.endedAt)} / プロンプト {s.promptCount} 回
                {#if s.gitBranch}/ <span class="mono">{s.gitBranch}</span>{/if}
              </div>
              {#if s.firstPrompt}<div class="small"><span class="muted">最初:</span> {s.firstPrompt}</div>{/if}
              {#if s.lastPrompt && s.lastPrompt !== s.firstPrompt}
                <div class="small"><span class="muted">最後:</span> {s.lastPrompt}</div>
              {/if}
              {#if s.lastReply}
                <div class="reply small">{s.lastReply}</div>
              {/if}
            </li>
          {/each}
        </ul>
        {#if project.sessions.length > sessions.length}
          <button class="more" onclick={() => (sessionLimit += 30)}>さらに表示 (残り {project.sessions.length - sessions.length} 件)</button>
        {/if}
      </section>
    {/if}

    {#if tab === "commits"}
      <section>
        <h3>
          最近のコミット
          <label class="check small muted">
            <input type="checkbox" bind:checked={showAllCommits} /> 他の人の分も
          </label>
        </h3>
        <ul class="plain commits">
          {#each commits as c (c.hash)}
            <li>
              <span class="muted small num">{formatDateTime(toMs(c.at) ?? 0)}</span>
              <span class="subject">{c.subject}</span>
              {#if showAllCommits}<span class="muted small">{c.authorName}</span>{/if}
            </li>
          {:else}
            <li class="muted small">自分のコミットはありません</li>
          {/each}
        </ul>
        {#if commitSource.length > commits.length}
          <button class="more" onclick={() => (commitLimit += 50)}>さらに表示 (残り {commitSource.length - commits.length} 件)</button>
        {/if}
      </section>
    {/if}

    {#if tab === "readme"}
      {#if readme.state === "found"}
        <ReadmeView readme={readme.readme} link={webLink} />
      {:else if readme.state === "loading"}
        <p class="muted">読み込んでいます…</p>
      {/if}
    {/if}
  </div>
</aside>

<style>
  .detail {
    width: 480px;
    flex: none;
    border-left: 1px solid var(--line);
    background: var(--surface);
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .head {
    padding: 12px 16px;
    border-bottom: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .title {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .title h2 {
    margin: 0;
  }

  .path {
    font-size: 11.5px;
    word-break: break-all;
  }

  /* タブ: 選んでいるものは下線と太字で、はっきり分かるように */
  .tabs {
    display: flex;
    gap: 2px;
    padding: 0 10px;
    border-bottom: 1px solid var(--line);
    flex: none;
  }

  .tab {
    border: none;
    border-radius: 0;
    background: transparent;
    padding: 7px 10px 6px;
    color: var(--muted);
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
    display: inline-flex;
    align-items: center;
    gap: 5px;
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

  .count {
    font-size: 11px;
    font-weight: 400;
    color: var(--ink-2);
    background: var(--surface-2);
    border-radius: 999px;
    padding: 0 6px;
  }

  .more {
    margin-top: 8px;
  }

  .body {
    overflow-y: auto;
    padding: 12px 16px 24px;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }

  h3 {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }

  .small {
    font-size: 12px;
  }

  .plain {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .plain li {
    display: flex;
    gap: 6px;
    align-items: baseline;
    flex-wrap: wrap;
  }

  dl {
    display: grid;
    grid-template-columns: 90px 1fr;
    gap: 2px 8px;
    margin: 0 0 8px;
  }

  dt {
    color: var(--muted);
  }

  dd {
    margin: 0;
  }

  table {
    width: 100%;
    border-collapse: collapse;
  }

  th,
  td {
    text-align: left;
    padding: 3px 6px 3px 0;
    border-bottom: 1px solid var(--line);
    vertical-align: top;
  }

  th {
    font-weight: 500;
    color: var(--muted);
    font-size: 12px;
  }

  .sessions {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .sessions li {
    border-left: 2px solid var(--accent);
    padding-left: 10px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .sessions li.auto {
    border-left-color: var(--line-strong);
  }

  .s-head {
    display: flex;
    gap: 6px;
    align-items: baseline;
  }

  .host {
    font-size: 11px;
    color: var(--ink-2);
    border: 1px solid var(--line-strong);
    border-radius: 4px;
    padding: 0 5px;
  }

  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-bottom: 6px;
  }

  .tag-chip {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    font-size: 12px;
    padding: 1px 2px 1px 8px;
    border-radius: 4px;
    border: 1px solid var(--line-strong);
    background: var(--surface-2);
  }

  .tag-chip .x {
    border: none;
    background: transparent;
    padding: 0 5px;
    line-height: 1.2;
    color: var(--ink-2);
  }

  .tag-add {
    display: flex;
    gap: 6px;
  }

  .tag-add input {
    flex: 1;
  }

  .err {
    color: var(--st-high);
  }

  .remotes {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    min-width: 0;
  }

  .remotes :global(.remote) {
    max-width: 100%;
  }

  .reply {
    color: var(--ink-2);
    background: var(--surface-2);
    border-radius: 4px;
    padding: 4px 8px;
    white-space: pre-wrap;
    max-height: 7.5em;
    overflow: hidden;
  }

  .commits .subject {
    flex: 1;
    min-width: 0;
  }

  pre {
    white-space: pre-wrap;
    margin: 4px 0 0;
    font-family: var(--mono);
  }

  p {
    margin: 6px 0;
  }
</style>
