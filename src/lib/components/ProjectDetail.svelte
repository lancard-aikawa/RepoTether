<script lang="ts">
  import type { Project } from "$lib/derive";
  import type { Commit, SessionFinding } from "$lib/types";
  import { formatDateTime, formatTime, relative, toMs, dayKey, formatDayLabel } from "$lib/format";
  import * as api from "$lib/api";
  import {
    allTags,
    app,
    isAncestorTag,
    errorText,
    normalizeTag,
    prefs,
    refresh,
    savePrefs,
    setHidden,
    setTags,
    toast,
    openTranscript,
    resumeSession,
    scanVulns,
    type DetailTab,
  } from "$lib/store.svelte";
  import * as vulnsLib from "$lib/vulns";
  import ProjectActions from "./ProjectActions.svelte";
  import ReadmeView from "./ReadmeView.svelte";
  import RemoteBadges from "./RemoteBadges.svelte";
  import RepoIcon from "./RepoIcon.svelte";
  import StarButton from "./StarButton.svelte";

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

  // ---- 脆弱性 (LockWatch の結果) ----
  const vulns = $derived(project.local ? app.vulns?.byRepo[project.local.id] ?? null : null);
  const vulnAll = $derived(vulns?.result?.findings ?? []);
  const vulnFindings = $derived(vulnsLib.visible(vulnAll, prefs.vulnHide));
  const vulnHiddenCount = $derived(vulnAll.length - vulnFindings.length);
  const vulnChoices = $derived(vulnsLib.hideChoices(vulnAll));
  const isNewVuln = (pkg: string, id: string) => !!vulns?.new.some(([p, i]) => p === pkg && i === id);
  /** checking = 事前チェック中、scanning = 照合中 */
  let scanning = $state<"" | "checking" | "scanning">("");
  let scanError = $state("");
  /** 事前チェックで「前回から変わっていない」と分かったときの、その結果の時刻 (照合し直すかを選ばせる) */
  let unchangedSince = $state<string | null>(null);
  $effect(() => {
    void project.key;
    scanError = "";
    unchangedSince = null;
  });

  /** 設定の「脆弱性」タブを開く (osv-scanner が無い・定期実行が未登録のときの案内から) */
  function openLockwatchSettings() {
    prefs.settingsTab = "vulns";
    prefs.tab = "settings";
    savePrefs();
  }

  function toggleHide(k: string) {
    prefs.vulnHide = prefs.vulnHide.includes(k) ? prefs.vulnHide.filter((x) => x !== k) : [...prefs.vulnHide, k];
    savePrefs();
  }

  /**
   * 「今すぐ調べる」: 先に事前チェックをし、lock ファイルも脆弱性 DB も前回から変わっていなければ
   * 照合せずに知らせて、照合し直すかを選ばせる (照合し直しても時刻が変わらず、何もしていないように見えるため)
   */
  async function runScan() {
    if (!project.local) return;
    const key = project.key;
    scanning = "checking";
    scanError = "";
    unchangedSince = null;
    try {
      const c = await api.lockwatchCheck(project.local.id);
      if (project.key !== key) return;
      if (c.cached) {
        unchangedSince = c.scannedAt;
        return;
      }
      scanning = "scanning";
      await scanVulns(project.local.id);
    } catch (e) {
      if (project.key === key) scanError = errorText(e);
    } finally {
      scanning = "";
    }
  }

  /** 前回の結果を使わずに照合し直す */
  async function rescan() {
    if (!project.local) return;
    const key = project.key;
    scanning = "scanning";
    scanError = "";
    unchangedSince = null;
    try {
      await scanVulns(project.local.id, true);
    } catch (e) {
      if (project.key === key) scanError = errorText(e);
    } finally {
      scanning = "";
    }
  }

  const MODE_LABEL: Record<string, string> = {
    online: "オンライン (api.osv.dev)",
    offline: "手元の脆弱性 DB",
  };
  const VISIBILITY_NOTE: Record<string, string> = {
    public: "公開リポジトリなので、オンライン (api.osv.dev) で照合します。",
    private: "非公開リポジトリなので、パッケージ名を外に出さず、手元の脆弱性 DB で照合します。",
    unknown: "公開か分からない (リモートが無い・リモートの一覧に無い) ので、手元の脆弱性 DB で照合します。",
  };

  // ---- タブ ----
  const tabs = $derived(
    (
      [
        { id: "summary", label: "概要", show: true },
        { id: "git", label: "リポジトリ", show: project.kind !== "folder" },
        {
          id: "commits",
          label: "コミット",
          count: project.myCommits.length || undefined,
          show: project.commits.length > 0 || (project.kind === "local" && !project.local?.error),
        },
        { id: "claude", label: "Claude", count: project.sessions.length, show: project.sessions.length > 0 },
        // LockWatch を使う設定で、手元にあるリポジトリだけ
        {
          id: "vulns",
          label: "脆弱性",
          count: vulnFindings.length || undefined,
          show: !!app.vulns?.configured && !!project.local,
        },
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

  // ---- セッションのログの検査 (SessionVault)。10 秒近くかかるので、押したときだけ ----
  type CheckState =
    | { state: "idle" }
    | { state: "running" }
    | { state: "done"; findings: SessionFinding[]; at: number }
    | { state: "error"; message: string };
  let check = $state<CheckState>({ state: "idle" });
  // サブフォルダで始めた会話は別のフォルダに入るので、このプロジェクトのセッションのフォルダをすべて渡す
  const projectDirs = $derived([...new Set(project.sessions.map((s) => s.projectDir).filter((d): d is string => !!d))]);
  $effect(() => {
    void project.key;
    check = { state: "idle" };
  });

  async function runCheck() {
    const key = project.key;
    check = { state: "running" };
    try {
      const findings = await api.sessionvaultVerify(projectDirs);
      if (project.key === key) check = { state: "done", findings, at: Date.now() };
    } catch (e) {
      if (project.key === key) check = { state: "error", message: errorText(e) };
    }
  }

  const CHECK_LABEL: Record<string, string> = {
    "bad-json": "読めない行",
    "truncated-tail": "最後の行が途中まで",
    "no-newline": "末尾に改行が無い",
    "dangling-parent": "会話のつながりが切れている",
    "duplicate-uuid": "同じ記録が 2 回",
    diverged: "保管庫の版と食い違う",
    "src-missing": "Claude Code から消えた (保管庫にはある)",
    unreadable: "開けない",
  };
  const SEVERITY_BADGE: Record<string, string> = { error: "high", warning: "mid", info: "info" };
  const SEVERITY_LABEL: Record<string, string> = { error: "エラー", warning: "警告", info: "情報" };
  const sessionTitle = (id: string | null) =>
    (id && project.sessions.find((s) => s.id === id)?.title) || (id ? id.slice(0, 8) + "…" : "(セッション外)");
  const findingCounts = $derived.by(() => {
    if (check.state !== "done") return [];
    const found = check.findings;
    return (["error", "warning", "info"] as const)
      .map((sev) => [sev, found.filter((f) => f.severity === sev).length] as const)
      .filter(([, n]) => n > 0);
  });
  const commitSource = $derived(showAllCommits ? project.commits : project.myCommits);
  const commits = $derived(commitSource.slice(0, commitLimit));

  // ---- 期間より前のコミット (git から 10 件ずつ) ----
  const PAGE = 10;
  let older = $state<Commit[]>([]);
  let olderDone = $state(false);
  let olderLoading = $state(false);
  let olderError = $state("");

  // 別のプロジェクト・「他の人の分も」を切り替えたら読み直す
  $effect(() => {
    void project.key;
    void showAllCommits;
    older = [];
    olderDone = false;
    olderError = "";
  });

  /** 期間内のコミットを出し切ったら、git から続きを読む */
  const windowDone = $derived(commits.length >= commitSource.length);

  async function loadOlder() {
    if (!project.path) return;
    olderLoading = true;
    olderError = "";
    try {
      // 取り込みと同じ並びなので、表示済みの件数だけ飛ばせば続きになる
      const got = await api.repoLog(project.path, commitSource.length + older.length, PAGE, !showAllCommits);
      const seen = new Set([...commitSource, ...older].map((c) => c.hash));
      older = [...older, ...got.filter((c) => !seen.has(c.hash))];
      if (got.length < PAGE) olderDone = true;
    } catch (e) {
      olderError = errorText(e);
    } finally {
      olderLoading = false;
    }
  }
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
      <h2><StarButton {project} big /> <RepoIcon name={project.name} path={project.path} size={26} /> {project.name}</h2>
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
          <RemoteBadges links={project.links} />
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
        <h3>
          ログの検査
          <button class="small-btn" onclick={runCheck} disabled={check.state === "running" || projectDirs.length === 0}>
            {check.state === "running" ? "検査中…" : check.state === "done" ? "もう一度" : "検査する"}
          </button>
        </h3>
        {#if check.state === "idle"}
          <p class="muted small">
            SessionVault で、このプロジェクトのセッションのログが壊れていないか (読めない行・途中で切れた行・会話のつながり)、
            保管庫の版と食い違っていないかを調べます。ログは書き換えません。
          </p>
        {:else if check.state === "error"}
          <p class="err small">{check.message}</p>
        {:else if check.state === "done"}
          {#if check.findings.length === 0}
            <p class="small"><span class="badge good">問題なし</span> <span class="muted">{formatTime(check.at)}</span></p>
          {:else}
            <p class="small">
              {#each findingCounts as [sev, n]}<span class="badge {SEVERITY_BADGE[sev]}">{SEVERITY_LABEL[sev]} {n}</span> {/each}
              <span class="muted">{formatTime(check.at)}</span>
            </p>
            <ul class="findings">
              {#each check.findings as f, i (i)}
                <li>
                  <span class="badge {SEVERITY_BADGE[f.severity]}">{CHECK_LABEL[f.check] ?? f.check}</span>
                  <span class="small">{sessionTitle(f.session)}</span>
                  <div class="muted small mono">{f.path}{f.line ? `:${f.line}` : ""} {f.detail}</div>
                </li>
              {/each}
            </ul>
            {#if check.findings.some((f) => f.severity === "error")}
              <p class="muted small">直すときは SessionVault の repair / restore を使います (Claude Code で開いていないときに)。</p>
            {/if}
          {/if}
        {/if}
      </section>

      <section>
        <h3>Claude のセッション <span class="muted small">{project.sessions.length} 件 (新しい順)</span></h3>
        <ul class="sessions">
          {#each sessions as s (s.id)}
            <li class:auto={!s.interactive}>
              <div class="s-head">
                <button class="s-title" onclick={() => openTranscript(s)} title="会話の全文を見る">{s.title ?? "(無題)"}</button>
                {#if !s.interactive}<span class="host">自動</span>{/if}
                <span class="s-actions">
                  <button class="small-btn" onclick={() => openTranscript(s)}>全文</button>
                  {#if s.interactive && s.cwd}
                    <button class="small-btn" onclick={() => resumeSession(s, false)} title="端末で claude -r を実行して続ける">再開</button>
                  {/if}
                </span>
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
          {#each [...commits, ...(windowDone ? older : [])] as c (c.hash)}
            <li>
              <span class="muted small num">{formatDateTime(toMs(c.at) ?? 0)}</span>
              <span class="subject">{c.subject}</span>
              {#if showAllCommits}<span class="muted small">{c.authorName}</span>{/if}
            </li>
          {:else}
            {#if olderDone}
              <li class="muted small">{showAllCommits ? "コミットはありません" : "自分のコミットはありません"}</li>
            {:else}
              <li class="muted small">
                過去 {app.config?.historyDays ?? 365} 日の{showAllCommits ? "" : "自分の"}コミットはありません。下のボタンで、それより前を読めます
              </li>
            {/if}
          {/each}
        </ul>
        {#if !windowDone}
          <button class="more" onclick={() => (commitLimit += 50)}>さらに表示 (残り {commitSource.length - commits.length} 件)</button>
        {:else if olderError}
          <p class="small err">{olderError}</p>
        {:else if !olderDone}
          <button class="more" onclick={loadOlder} disabled={olderLoading}>
            {olderLoading ? "読んでいます…" : `次の ${PAGE} 件を読む${older.length ? "" : " (それより前のコミット)"}`}
          </button>
        {:else if commits.length || older.length}
          <p class="muted small">これより前のコミットはありません</p>
        {/if}
      </section>
    {/if}

    {#if tab === "vulns"}
      <section>
        <h3>
          脆弱性
          <button class="small-btn" onclick={runScan} disabled={!!scanning || !vulns} title="LockWatch で、このリポジトリだけ照合し直す">
            {scanning === "checking" ? "確かめています…" : scanning === "scanning" ? "調べています…" : "今すぐ調べる"}
          </button>
        </h3>
        {#if unchangedSince}
          <div class="notice small">
            <p>
              前回の照合 ({when(unchangedSince)}) から、lock ファイルも脆弱性 DB も変わっていません。下の結果がそのまま最新です。
              そのあとに公表された脆弱性も拾うなら、照合し直してください。
            </p>
            <button class="small-btn" onclick={() => (unchangedSince = null)}>この結果のまま</button>
            <button class="small-btn" onclick={rescan}>もう一度照合する</button>
          </div>
        {/if}
        {#if app.vulns?.error}
          <p class="err small">{app.vulns.error}</p>
        {:else if !vulns}
          <p class="muted small">このリポジトリは LockWatch の対象にありません (非表示にしたものは対象外です)。</p>
        {:else}
          <p class="muted small">{VISIBILITY_NOTE[vulns.visibility]}</p>
          {@const lws = app.lockwatchStatus}
          {#if lws?.osvScanner.error}
            <p class="small"><span class="badge high">osv-scanner が使えません</span> {lws.osvScanner.error}</p>
          {/if}
          {#if lws?.task.registered === false}
            <p class="small muted">
              LockWatch の定期実行が登録されていないので、全体の照合は「今すぐ調べる」を押したものだけです。
            </p>
          {/if}
          {#if lws?.osvScanner.error || lws?.task.registered === false}
            <button class="small-btn" onclick={openLockwatchSettings}>設定の「脆弱性」を開く</button>
          {/if}
          {#if scanError}<p class="err small">{scanError}</p>{/if}
          {@const res = vulns.result}
          {#if !res}
            <p class="small">まだ照合していません。「今すぐ調べる」か、LockWatch の定期実行を待ってください。</p>
          {:else if res.status === "error"}
            <p class="small"><span class="badge high">照合できません</span></p>
            <pre class="small">{res.error}</pre>
          {:else if res.status === "no-lockfile"}
            <p class="small"><span class="badge low">lock ファイルなし</span></p>
            <p class="muted small">
              git が管理している lock ファイル (package-lock.json・pnpm-lock.yaml・uv.lock・Cargo.lock など) がありません。
              lock ファイルの無いプロジェクトは、LockWatch では調べられません。
            </p>
          {:else}
            <p class="small">
              {#if vulnAll.length === 0}
                <span class="badge good">見つかりません</span>
              {:else}
                {#each vulnsLib.counts(vulnFindings) as [sev, n] (sev)}
                  <span class="badge {vulnsLib.SEVERITY_BADGE[sev]}">{vulnsLib.SEVERITY_LABEL[sev]} {n}</span>
                {/each}
              {/if}
              <span class="muted">{when(res.scannedAt)} / {MODE_LABEL[res.mode] ?? res.mode}</span>
            </p>
            <p class="muted small mono">{res.lockfiles.join(", ")}</p>
            {#if vulnChoices.length}
              <div class="hide-row small">
                <span class="muted">隠す:</span>
                {#each vulnChoices as k (k)}
                  <label class="check">
                    <input type="checkbox" checked={prefs.vulnHide.includes(k)} onchange={() => toggleHide(k)} />
                    {vulnsLib.choiceLabel(k)}
                  </label>
                {/each}
              </div>
            {/if}
            <ul class="findings">
              {#each vulnFindings as f (f.lockfile + f.package + f.version + f.id)}
                <li>
                  <span class="badge {vulnsLib.SEVERITY_BADGE[f.severity]}">{vulnsLib.SEVERITY_LABEL[f.severity]}</span>
                  <span class="mono">{f.package} {f.version}</span>
                  {#if f.informational}<span class="host">{vulnsLib.choiceLabel(f.informational)}</span>{/if}
                  {#if isNewVuln(f.package, f.id)}<span class="host new">新しく出た</span>{/if}
                  <div class="small">
                    <button class="link mono" onclick={() => api.openUrl(`https://osv.dev/vulnerability/${encodeURIComponent(f.id)}`)} title="osv.dev で詳しく見る">{f.id}</button>
                    {#if f.fixed.length}<span class="muted">直る版</span> <span class="mono">{f.fixed.join(", ")}</span>{:else}<span class="muted">直る版なし</span>{/if}
                  </div>
                  {#if f.summary}<div class="small">{f.summary}</div>{/if}
                  <div class="muted small mono">{f.lockfile}</div>
                </li>
              {/each}
            </ul>
            {#if vulnHiddenCount}<p class="muted small">{vulnHiddenCount} 件を隠しています</p>{/if}
          {/if}
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
    display: flex;
    align-items: center;
    gap: 6px;
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

  /* タイトルを押すと全文。見た目はリンク風の太字 */
  .s-title {
    border: none;
    background: transparent;
    padding: 0;
    font-weight: 600;
    text-align: left;
    white-space: normal;
  }

  .s-title:hover:not(:disabled) {
    background: transparent;
    color: var(--accent);
    text-decoration: underline;
  }

  .s-actions {
    margin-left: auto;
    display: inline-flex;
    gap: 4px;
    flex: none;
  }

  .small-btn {
    font-size: 12px;
    padding: 1px 8px;
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

  .findings {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .findings .mono {
    word-break: break-all;
  }

  /* 「今すぐ調べる」で前回から変わっていなかったときの知らせ */
  .notice {
    border: 1px solid var(--line-strong);
    background: var(--surface-2);
    border-radius: 6px;
    padding: 6px 10px 8px;
    margin-bottom: 8px;
  }

  .notice p {
    margin: 0 0 6px;
  }

  .hide-row {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 10px;
    align-items: center;
    margin-bottom: 8px;
  }

  .host.new {
    color: var(--st-high);
    border-color: var(--st-high);
  }

  /* 脆弱性の ID: 押すと osv.dev を開く */
  .link {
    border: none;
    background: transparent;
    padding: 0;
    color: var(--accent);
    text-decoration: underline;
  }

  .link:hover:not(:disabled) {
    background: transparent;
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
