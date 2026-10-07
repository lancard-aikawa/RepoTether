<script lang="ts">
  // 脆弱性タブ。上に LockWatch の結果をプロジェクトごとに並べ、下に LockWatch の場所と状態を出す。
  // 1 件ずつの中身は、行を押して開く詳細パネルの「脆弱性」タブで見る。
  import { untrack } from "svelte";
  import type { Project } from "$lib/derive";
  import type { LockwatchStatus, VulnFinding } from "$lib/types";
  import * as api from "$lib/api";
  import { relative, toMs } from "$lib/format";
  import { app, errorText, loadVulns, openProject, prefs, savePrefs, toast, updateConfig } from "$lib/store.svelte";
  import * as vulnsLib from "$lib/vulns";
  import RepoIcon from "./RepoIcon.svelte";

  let { projects, now }: { projects: Project[]; now: number } = $props();

  // ---- 結果 (プロジェクトごと) ----
  type Row = {
    project: Project;
    status: "ok" | "no-lockfile" | "error" | "none";
    /** 「隠す」を除いたもの (重い順) */
    findings: VulnFinding[];
    heavy: number;
    /** 前回から新しく出た数 */
    fresh: number;
    notices: number;
    scannedAt: number | null;
    error: string | null;
  };

  const rows = $derived.by(() => {
    const by = app.vulns?.byRepo ?? {};
    const out: Row[] = [];
    for (const p of projects) {
      const v = p.local ? by[p.local.id] : undefined;
      if (!v) continue;
      const res = v.result;
      const findings = vulnsLib.visible(vulnsLib.findingsOf(app.vulns, p.local?.id), prefs.vulnHide);
      out.push({
        project: p,
        status: res?.status ?? "none",
        findings,
        heavy: vulnsLib.heavyCount(findings, []),
        fresh: findings.filter((f) => v.new.some(([pkg, id]) => pkg === f.package && id === f.id)).length,
        notices: res?.notices.length ?? 0,
        scannedAt: toMs(res?.scannedAt),
        error: res?.status === "error" ? (res.error ?? "照合できませんでした") : null,
      });
    }
    return out.sort(
      (a, b) =>
        b.heavy - a.heavy ||
        b.findings.length - a.findings.length ||
        Number(!!b.error) - Number(!!a.error) ||
        b.notices - a.notices ||
        a.project.name.localeCompare(b.project.name, "ja"),
    );
  });

  /** 見るべきもの: 脆弱性・注意があるか、照合できなかったもの */
  const flagged = $derived(rows.filter((r) => r.findings.length || r.notices || r.error));
  let showAll = $state(false);
  const shown = $derived(showAll ? rows : flagged);

  const rawFindings = $derived(projects.flatMap((p) => vulnsLib.findingsOf(app.vulns, p.local?.id)));
  const totals = $derived(vulnsLib.counts(rows.flatMap((r) => r.findings)));
  const hideChoices = $derived(vulnsLib.hideChoices(rawFindings));
  const hiddenCount = $derived(rawFindings.length - rows.reduce((n, r) => n + r.findings.length, 0));

  function toggleHide(k: string) {
    prefs.vulnHide = prefs.vulnHide.includes(k) ? prefs.vulnHide.filter((x) => x !== k) : [...prefs.vulnHide, k];
    savePrefs();
  }

  let reloading = $state(false);
  async function reload() {
    reloading = true;
    await loadVulns();
    reloading = false;
  }

  function rowKey(e: KeyboardEvent, run: () => void) {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      run();
    }
  }

  // ---- LockWatch の場所と状態 ----
  // 場所はこのタブの「保存」で設定に書く (設定タブの保存とは別)
  const savedPath = $derived((app.config?.lockwatchPath ?? "").trim());
  let path = $state(app.config?.lockwatchPath ?? "");
  const pathDirty = $derived(path.trim() !== savedPath);
  let saving = $state(false);

  // 入力中の場所で確かめる (保存前でも)。保存済みの場所と同じなら、詳細パネルの案内にも使う
  let lw = $state<
    | { state: "idle" }
    | { state: "checking" }
    | { state: "ok"; status: LockwatchStatus; path: string }
    | { state: "error"; text: string }
  >({ state: "idle" });

  async function checkLockwatch() {
    const p = path.trim();
    if (!p) {
      lw = { state: "idle" };
      return;
    }
    lw = { state: "checking" };
    try {
      const status = await api.lockwatchStatus(p);
      lw = { state: "ok", status, path: p };
      if (p === savedPath) app.lockwatchStatus = status;
    } catch (e) {
      lw = { state: "error", text: errorText(e) };
    }
  }

  // タブを開いたら、場所が決まっていれば自動で確かめる
  $effect(() => {
    if (untrack(() => lw.state === "idle" && !!path.trim())) checkLockwatch();
  });

  async function pickLockwatch() {
    const p = await api.pickFolder("LockWatch のリポジトリのフォルダ", path || undefined);
    if (p) {
      path = p;
      checkLockwatch();
    }
  }

  async function savePath() {
    if (!app.config) return;
    saving = true;
    try {
      const next = path.trim();
      await updateConfig({ ...$state.snapshot(app.config), lockwatchPath: next || null });
      // 一覧を LockWatch に渡し、結果を読み直す
      await loadVulns();
      toast(next ? "保存しました" : "LockWatch を使わない設定にしました");
    } catch (e) {
      toast(errorText(e));
    } finally {
      saving = false;
    }
  }

  let guiOpened = $state(false);

  /** LockWatch の画面 (lockwatch gui) は 0.2.0 から。古い LockWatch では押せないようにする */
  const LOCKWATCH_GUI_SINCE = [0, 2];
  const hasGui = (version: string) => {
    const [major, minor] = version.split(".").map((x) => Number.parseInt(x, 10) || 0);
    return major > LOCKWATCH_GUI_SINCE[0] || (major === LOCKWATCH_GUI_SINCE[0] && minor >= LOCKWATCH_GUI_SINCE[1]);
  };

  /** 入力中の場所が、確かめて使えると分かった場所のままか (確かめたあとに書き換えたら、もう一度確かめるまで開かせない) */
  const checkedPath = $derived(lw.state === "ok" && lw.path === path.trim() ? lw.path : null);

  /** LockWatch の画面を開く (確かめて使えると分かった場所で) */
  async function openLockwatchGui() {
    if (!checkedPath) return;
    try {
      await api.lockwatchOpenGui(checkedPath);
      guiOpened = true;
    } catch (e) {
      toast(errorText(e));
    }
  }

  async function copyText(text: string) {
    try {
      await navigator.clipboard.writeText(text);
      toast("コピーしました");
    } catch (e) {
      toast(errorText(e));
    }
  }
</script>

<div class="scroll">
  {#if app.vulns?.configured}
    <section class="panel card wide">
      <div class="head">
        <h2>脆弱性</h2>
        {#each totals as [sev, n] (sev)}
          <span class="badge {vulnsLib.SEVERITY_BADGE[sev]}">{vulnsLib.SEVERITY_LABEL[sev]} {n}</span>
        {/each}
        <span class="spacer"></span>
        {#if app.vulns.scannedAt}
          <span class="muted small" title={new Date(app.vulns.scannedAt).toLocaleString()}
            >最後の照合 {relative(toMs(app.vulns.scannedAt), now)}</span
          >
        {/if}
        <button onclick={reload} disabled={reloading} title="LockWatch の結果を読み直す (照合はしない)">
          {reloading ? "読み直しています…" : "結果を読み直す"}
        </button>
      </div>

      {#if app.vulns.error}
        <p class="err">{app.vulns.error}</p>
      {:else}
        <div class="filters">
          {#if hideChoices.length}
            <span class="muted">隠す</span>
            {#each hideChoices as k (k)}
              <label class="check">
                <input type="checkbox" checked={prefs.vulnHide.includes(k)} onchange={() => toggleHide(k)} />
                {vulnsLib.choiceLabel(k)}
              </label>
            {/each}
            {#if hiddenCount}<span class="muted small">({hiddenCount} 件を隠しています)</span>{/if}
          {/if}
          <span class="spacer"></span>
          <label class="check" title="脆弱性も注意も無いリポジトリ、まだ照合していないリポジトリも並べる">
            <input type="checkbox" bind:checked={showAll} />
            問題の無いものも出す <span class="muted num">({rows.length - flagged.length})</span>
          </label>
        </div>

        {#if !rows.length}
          <p class="muted">LockWatch の対象になっているリポジトリがありません。上の「更新」で一覧を渡します。</p>
        {:else if !shown.length}
          <p><span class="badge good">脆弱性の見つかったリポジトリはありません</span> <span class="muted">({rows.length} 件を見ています)</span></p>
        {:else}
          <table class="results">
            <thead>
              <tr>
                <th>プロジェクト</th>
                <th>脆弱性</th>
                <th>注意</th>
                <th>照合</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {#each shown as r (r.project.key)}
                {@const open = () => openProject(r.project.key, "vulns")}
                <tr tabindex="0" onclick={open} onkeydown={(e) => rowKey(e, open)}>
                  <td class="c-name">
                    <RepoIcon name={r.project.name} path={r.project.path} />
                    <span class="name">{r.project.name}</span>
                    {#if r.fresh}<span class="host new">新しく出た {r.fresh}</span>{/if}
                  </td>
                  <td class="c-sev">
                    {#each vulnsLib.counts(r.findings) as [sev, n] (sev)}
                      <span class="badge {vulnsLib.SEVERITY_BADGE[sev]}">{vulnsLib.SEVERITY_LABEL[sev]} {n}</span>
                    {:else}
                      {#if r.error}
                        <span class="badge high" title={r.error}>照合できません</span>
                      {:else if r.status === "none"}
                        <span class="muted">まだ照合していません</span>
                      {:else if r.status === "no-lockfile"}
                        <span class="muted">lock ファイルなし</span>
                      {:else}
                        <span class="muted">なし</span>
                      {/if}
                    {/each}
                  </td>
                  <td class="c-notice">{#if r.notices}<span class="badge mid">注意 {r.notices}</span>{/if}</td>
                  <td class="c-when muted" title={r.scannedAt ? new Date(r.scannedAt).toLocaleString() : ""}>
                    {relative(r.scannedAt, now)}
                  </td>
                  <!-- 行のどこを押しても開くが、押せると分かるようにボタンも置く -->
                  <td class="c-act"><button tabindex="-1">詳細を開く</button></td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      {/if}
    </section>
  {/if}

  <section class="panel card">
    <h2>LockWatch</h2>
    <p class="note">
      LockWatch は、リポジトリの lock ファイル (package-lock.json・pnpm-lock.yaml・uv.lock・Cargo.lock など) を osv-scanner にかけて、
      使っているパッケージの脆弱性を調べる別の道具です (入れなくても RepoTether は使えます)。場所を指定すると、更新のたびに手元のリポジトリの一覧
      (公開・非公開の別つき) を LockWatch に渡し、結果をこのタブと、詳細パネルの「脆弱性」タブ、一覧の印に出します。
      <strong>非公開のリポジトリと公開か分からないものは、LockWatch がパッケージ名を外に出さず、手元の脆弱性 DB で照合します。</strong>
      公開とみなすのは github.com で公開のものだけで、Gogs・Gitea などの自前のサーバーのものは非公開として渡します。
    </p>
    <div class="grid">
      <label for="lockwatch-path">LockWatch の場所</label>
      <div class="inline">
        <input id="lockwatch-path" type="text" class="mono grow" bind:value={path} placeholder="LockWatch のフォルダ。空なら使わない" />
        <button onclick={pickLockwatch}>参照</button>
        <button onclick={checkLockwatch} disabled={!path.trim() || lw.state === "checking"}>
          {lw.state === "checking" ? "確かめています…" : "確かめる"}
        </button>
        <button class="primary" onclick={savePath} disabled={!pathDirty || saving}>{saving ? "保存しています…" : "保存"}</button>
        <button
          onclick={openLockwatchGui}
          disabled={lw.state !== "ok" || !checkedPath || !hasGui(lw.status.lockwatch)}
          title={lw.state === "ok" && !hasGui(lw.status.lockwatch)
            ? "LockWatch の画面は 0.2.0 からです。LockWatch を更新してください (git pull と uv sync)"
            : lw.state === "ok" && !checkedPath
              ? "場所を書き換えたので、先に「確かめる」を押してください"
              : "LockWatch の画面 (状態・結果・設定) を開く。LockWatch の設定はそこで変える"}
        >
          LockWatch を開く
        </button>
      </div>
    </div>
    {#if pathDirty}
      <p class="note">
        {path.trim() ? "この場所はまだ保存していません。「保存」で使い始めます。" : "「保存」で LockWatch を使わない設定にします。"}
      </p>
    {/if}
    {#if lw.state === "ok" && !hasGui(lw.status.lockwatch)}
      <p class="note">
        LockWatch {lw.status.lockwatch} には画面がありません (0.2.0 から)。LockWatch のフォルダで <code>git pull</code> と
        <code>uv sync</code> をすると「LockWatch を開く」が使えます。
      </p>
    {/if}
    {#if guiOpened}
      <p class="note">
        LockWatch の画面を開きました。そこで設定 (データの場所など) を変えたら、戻ってきて「確かめる」を押してください。
      </p>
    {/if}

    {#if !path.trim()}
      <h3>使い始めるには</h3>
      <ol class="note steps">
        <li>
          osv-scanner を入れる:
          <code>{api.OSV_SCANNER_INSTALL}</code>
          <button class="small" onclick={() => copyText(api.OSV_SCANNER_INSTALL)}>コピー</button>
        </li>
        <li>
          LockWatch を入れる (Python 3.11 以上と uv が要ります。手順は
          <button class="link" onclick={() => api.openUrl(api.LOCKWATCH_URL + "#入れ方")}>LockWatch の README</button>)
        </li>
        <li>上の「LockWatch の場所」に LockWatch のフォルダを指定し、「確かめる」で使えるかを見てから保存する</li>
        <li>LockWatch の定期実行を登録する (<code>{api.LOCKWATCH_REGISTER}</code>)</li>
      </ol>
    {:else if lw.state === "error"}
      <p class="err">{lw.text}</p>
    {:else if lw.state === "ok"}
      {@const s = lw.status}
      <table class="lw-status">
        <tbody>
          <tr><th>LockWatch</th><td><span class="badge good">{s.lockwatch}</span></td></tr>
          <tr>
            <th>osv-scanner</th>
            <td>
              {#if s.osvScanner.version}
                <span class="badge good">{s.osvScanner.version}</span> <span class="mono muted">{s.osvScanner.path}</span>
              {:else}
                <span class="badge high">使えません</span> {s.osvScanner.error}
                <div class="cmd">
                  <code>{api.OSV_SCANNER_INSTALL}</code>
                  <button class="small" onclick={() => copyText(api.OSV_SCANNER_INSTALL)}>コピー</button>
                </div>
              {/if}
            </td>
          </tr>
          <tr>
            <th>受け渡し</th>
            <td>
              <span class="mono">{s.targets}</span>
              {#if s.targetsError == null}<span class="muted"> ({s.targetsCount} 件)</span>
              {:else}<span class="muted"> (まだありません。次の更新で書きます)</span>{/if}
            </td>
          </tr>
          <tr>
            <th>最後の照合</th>
            <td>
              {#if s.latest?.scannedAt}
                {new Date(s.latest.scannedAt).toLocaleString()} <span class="muted">({s.latest.repos} 件{s.latest.errors ? `、照合できなかったもの ${s.latest.errors} 件` : ""})</span>
              {:else}
                <span class="muted">まだありません</span>
              {/if}
            </td>
          </tr>
          <tr>
            <th>手元の脆弱性 DB</th>
            <td>
              {#if Object.keys(s.db).length}
                {Object.keys(s.db).join(", ")} <span class="muted">(最後に取った時刻: {new Date(Object.values(s.db).sort()[0]).toLocaleString()})</span>
              {:else}
                <span class="muted">まだ取っていません (最初の照合で取ります。npm だけで約 200 MB)</span>
              {/if}
            </td>
          </tr>
          <tr>
            <th>定期実行</th>
            <td>
              {#if s.task.registered}
                <span class="badge good">登録済み</span> <span class="muted">(タスク「{s.task.name}」)</span>
              {:else if s.task.registered === false}
                <span class="badge mid">未登録</span> 全体の照合は定期実行で行います。LockWatch のフォルダで次を実行してください
                <div class="cmd">
                  <code>{api.LOCKWATCH_REGISTER}</code>
                  <button class="small" onclick={() => copyText(api.LOCKWATCH_REGISTER)}>コピー</button>
                </div>
              {:else}
                <span class="muted">分かりません</span>
              {/if}
            </td>
          </tr>
        </tbody>
      </table>
    {/if}
    <p class="note">
      <button class="link" onclick={() => api.openUrl(api.LOCKWATCH_URL)}>LockWatch (GitHub)</button>
    </p>
  </section>
</div>

<style>
  .scroll {
    overflow-y: auto;
    flex: 1;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .card {
    padding: 14px 16px;
    max-width: 860px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    align-items: flex-start;
    flex: none;
  }

  .card.wide {
    max-width: 1100px;
    align-items: stretch;
  }

  .card > * {
    max-width: 100%;
  }

  .card h2 {
    margin: 0;
  }

  p {
    margin: 0;
  }

  .head,
  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 10px;
  }

  .filters {
    font-size: 12px;
  }

  .spacer {
    flex: 1;
  }

  /* プロジェクトごとの結果。行を押すと状態タブの詳細パネルを開く */
  .results {
    width: 100%;
    border-collapse: collapse;
  }

  .results th {
    text-align: left;
    font-size: 12px;
    font-weight: 600;
    color: var(--ink-2);
    padding: 4px 8px;
    border-bottom: 1px solid var(--line);
    white-space: nowrap;
  }

  .results td {
    padding: 5px 8px;
    border-bottom: 1px solid var(--line);
    vertical-align: middle;
  }

  .results tbody tr {
    cursor: pointer;
  }

  .results tbody tr:hover {
    background: var(--hover);
  }

  .results tbody tr:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }

  .c-name {
    white-space: nowrap;
  }

  .c-name :global(.icon) {
    margin-right: 6px;
  }

  .c-name .name {
    font-weight: 600;
  }

  .c-sev .badge {
    margin: 1px 4px 1px 0;
  }

  .c-when {
    font-size: 12px;
    white-space: nowrap;
  }

  .c-act {
    width: 1%;
    text-align: right;
  }

  .c-act button {
    font-size: 12px;
    padding: 2px 8px;
  }

  .host {
    font-size: 11px;
    color: var(--ink-2);
    border: 1px solid var(--line);
    border-radius: 4px;
    padding: 0 5px;
    margin-left: 4px;
  }

  .host.new {
    border-color: var(--st-mid);
  }

  .grow {
    flex: 1;
    min-width: 0;
  }

  .grid {
    display: grid;
    grid-template-columns: 160px 1fr;
    gap: 6px 12px;
    align-items: center;
    width: 100%;
  }

  .grid label {
    color: var(--ink-2);
  }

  .inline {
    display: flex;
    gap: 6px;
    align-items: center;
    flex-wrap: wrap;
  }

  /* LockWatch の状態 */
  .lw-status {
    width: 100%;
    border-collapse: collapse;
    margin: 10px 0 4px;
  }

  .lw-status th,
  .lw-status td {
    text-align: left;
    vertical-align: top;
    padding: 4px 8px 4px 0;
    border-bottom: 1px solid var(--line);
  }

  .lw-status th {
    font-weight: 500;
    color: var(--muted);
    white-space: nowrap;
    width: 9em;
  }

  .lw-status .mono {
    word-break: break-all;
  }

  .cmd {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 4px;
  }

  .steps li {
    margin: 4px 0;
  }

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

  .note {
    font-size: 12px;
    color: var(--ink-2);
    margin: 0;
  }

  .err {
    font-size: 12px;
    color: var(--st-high);
  }

  code {
    font-family: var(--mono);
    font-size: 12px;
    background: var(--surface-2);
    padding: 0 4px;
    border-radius: 3px;
  }

  .small {
    font-size: 12px;
  }
</style>
