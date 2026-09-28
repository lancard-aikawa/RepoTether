<script lang="ts">
  import type { Project } from "$lib/derive";
  import { formatDateTime, formatTime, relative, toMs, dayKey, formatDayLabel } from "$lib/format";
  import * as api from "$lib/api";
  import { errorText, refresh, setHidden, toast } from "$lib/store.svelte";
  import ProjectActions from "./ProjectActions.svelte";
  import RemoteBadges from "./RemoteBadges.svelte";

  let { project, now, onclose }: { project: Project; now: number; onclose: () => void } = $props();

  let showAllCommits = $state(false);

  const r = $derived(project.local);
  const sessions = $derived(project.sessions.slice(0, 8));
  const commits = $derived((showAllCommits ? project.commits : project.myCommits).slice(0, 15));
  const hideKey = $derived(project.kind === "remote" ? project.key.replace(/^remote:/, "") : project.key);

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

  <div class="body">
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

    {#if sessions.length}
      <section>
        <h3>Claude のセッション <span class="muted small">{project.sessions.length} 件</span></h3>
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
      </section>
    {/if}

    {#if project.commits.length}
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
      </section>
    {/if}

    <section>
      {#if project.hidden}
        <button onclick={() => setHidden(hideKey, false)}>一覧に戻す</button>
      {:else}
        <button onclick={() => setHidden(hideKey, true)} title="設定の「非表示」から戻せます">一覧から外す</button>
      {/if}
    </section>
  </div>
</aside>

<style>
  .detail {
    width: 440px;
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
