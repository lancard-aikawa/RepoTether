<script lang="ts">
  // 道具タブ。上に ToolKeeper の結果 (PC に入れた道具の版) を並べ、下に ToolKeeper の場所と状態を出す。
  // RepoTether は見るだけ。道具の更新 (UAC が出る) は「ToolKeeper を開く」で ToolKeeper の画面でする。
  // 約束は ToolKeeper の docs/design.md §9
  import { untrack } from "svelte";
  import type { ToolkeeperStatus } from "$lib/types";
  import * as api from "$lib/api";
  import { relative, toMs } from "$lib/format";
  import { app, errorText, loadTools, toast, updateConfig } from "$lib/store.svelte";
  import * as toolsLib from "$lib/tools";

  let { now }: { now: number } = $props();

  // ---- 結果 ----
  let showAll = $state(false);
  let reloading = $state(false);
  const check = $derived(app.tools?.check ?? null);
  const items = $derived(toolsLib.visibleItems(check, showAll));
  const stale = $derived(toolsLib.isStale(check?.checkedAt, now));

  async function reload() {
    reloading = true;
    try {
      await loadTools();
    } finally {
      reloading = false;
    }
  }

  // ---- ToolKeeper の場所と状態 ----
  // 場所はこのタブの「保存」で設定に書く (設定タブの保存とは別。脆弱性タブの LockWatch と同じ)
  const savedPath = $derived((app.config?.toolkeeperPath ?? "").trim());
  let path = $state(app.config?.toolkeeperPath ?? "");
  const pathDirty = $derived(path.trim() !== savedPath);
  let saving = $state(false);

  let tk = $state<
    | { state: "idle" }
    | { state: "checking" }
    | { state: "ok"; status: ToolkeeperStatus; path: string }
    | { state: "error"; text: string }
  >({ state: "idle" });

  async function checkToolkeeper() {
    const p = path.trim();
    if (!p) {
      tk = { state: "idle" };
      return;
    }
    tk = { state: "checking" };
    try {
      tk = { state: "ok", status: await api.toolkeeperStatus(p), path: p };
    } catch (e) {
      tk = { state: "error", text: errorText(e) };
    }
  }

  // タブを開いたら、場所が決まっていれば自動で確かめる
  $effect(() => {
    if (untrack(() => tk.state === "idle" && !!path.trim())) checkToolkeeper();
  });

  async function pickToolkeeper() {
    const p = await api.pickFolder("ToolKeeper のリポジトリのフォルダ", path || undefined);
    if (p) {
      path = p;
      checkToolkeeper();
    }
  }

  async function savePath() {
    if (!app.config) return;
    saving = true;
    try {
      const next = path.trim();
      await updateConfig({ ...$state.snapshot(app.config), toolkeeperPath: next || null });
      await loadTools();
      toast(next ? "保存しました" : "ToolKeeper を使わない設定にしました");
    } catch (e) {
      toast(errorText(e));
    } finally {
      saving = false;
    }
  }

  /** 入力中の場所が、確かめて使えると分かった場所のままか */
  const checkedPath = $derived(tk.state === "ok" && tk.path === path.trim() ? tk.path : null);
  /** 開く場所: 確かめた入力か、保存済みの場所 */
  const openPath = $derived(checkedPath ?? (savedPath && !pathDirty ? savedPath : null));
  let guiOpened = $state(false);

  async function openToolkeeperGui() {
    if (!openPath) return;
    try {
      await api.toolkeeperOpenGui(openPath);
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
  {#if app.tools?.configured}
    <section class="panel card wide">
      <div class="head">
        <h2>道具</h2>
        {#if check}
          <span class="badge info" title="ToolKeeper の画面で選んで上げられる数 (区分 A・B)">更新できる {check.summary.updatable}</span>
          {#if check.summary.eol}<span class="badge high" title="系列のサポートが終わった・90 日以内に終わる">サポート終了 {check.summary.eol}</span>{/if}
          {#if check.summary.new}<span class="badge mid" title="前回調べたときに無かった更新">新しい {check.summary.new}</span>{/if}
        {/if}
        <span class="spacer"></span>
        {#if check}
          <span class="muted small" title={new Date(check.checkedAt).toLocaleString()}>最後に調べた {relative(toMs(check.checkedAt), now)}</span>
        {/if}
        <button onclick={reload} disabled={reloading} title="ToolKeeper の結果を読み直す (調べ直しはしない)">
          {reloading ? "読み直しています…" : "結果を読み直す"}
        </button>
        <button class="primary" onclick={openToolkeeperGui} disabled={!openPath} title="ToolKeeper の画面を開く。道具を選んで上げるのはそこで (UAC の確認が出ます)">
          ToolKeeper を開く
        </button>
      </div>
      <p class="note">道具の更新は ToolKeeper の画面でします (このタブは見るだけです)。上げ終わったら「結果を読み直す」を押してください。</p>
      {#if guiOpened}
        <p class="note">ToolKeeper の画面を開きました。</p>
      {/if}

      {#if app.tools.error}
        <p class="err">{app.tools.error}</p>
      {:else if !check}
        <p class="muted">まだ調べていません。ToolKeeper の画面の「調べ直す」か、定期実行 (週 1 回) で調べます。</p>
      {:else}
        {#if stale}
          <p class="err">
            最後に調べてから {toolsLib.STALE_DAYS} 日より経っています。定期実行が止まっているかもしれません (下の「定期実行」を見てください)。
          </p>
        {/if}
        {#each check.failed as f (f.source)}
          <p class="err">調べられなかった: {f.source} ({f.reason})</p>
        {/each}
        <div class="filters">
          <span class="spacer"></span>
          <label class="check" title="winget の残り (C) と choco (D) も並べる。サポート終了の物はいつも出す">
            <input type="checkbox" bind:checked={showAll} />
            C・D も出す <span class="muted num">({toolsLib.hiddenCount(check)})</span>
          </label>
        </div>
        {#if !items.length}
          <p class="muted">古くなっている道具はありません。</p>
        {:else}
          <table class="tools">
            <thead>
              <tr><th>区分</th><th>名前</th><th>入っている版</th><th>最新</th><th>注意</th></tr>
            </thead>
            <tbody>
              {#each items as it (it.key)}
                <tr class:dim={!toolsLib.canUpdate(it) && it.tier !== "E"}>
                  <td class="c-tier"><span class="tier" title={toolsLib.TIER_HINT[it.tier]}>{toolsLib.TIER_LABEL[it.tier]}</span></td>
                  <td class="c-name">
                    {it.name}
                    {#if it.new}<span class="badge mid">新しい</span>{/if}
                  </td>
                  <td class="mono">{it.installed ?? "-"}</td>
                  <td class="mono">{it.latest ?? "-"}</td>
                  <td class="c-note">
                    {#if it.eol && it.eol.state !== "ok"}
                      <span class="badge {it.eol.state === 'ended' ? 'high' : 'mid'}">{toolsLib.noteParts(it)[0]}</span>
                      {toolsLib.noteParts(it).slice(1).join("、")}
                    {:else}
                      {toolsLib.noteParts(it).join("、")}
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
          <p class="note">灰色の行は、ToolKeeper の画面でも上げない物です (最新・見るだけの区分・手で入れた物)。区分の意味は札にマウスを乗せると出ます。</p>
        {/if}
      {/if}
    </section>
  {/if}

  <section class="panel card">
    <h2>ToolKeeper</h2>
    <p class="note">
      ToolKeeper は、この PC に入れた道具 (PHP・Ghostscript・Cygwin と、winget・choco で入れたもの) が古くなっていないかを週に 1 回調べ、
      選んだものを最新に上げる別の道具です (入れなくても RepoTether は使えます)。場所を指定すると、その結果をこのタブに出します。
      RepoTether は結果を読むだけで、何も入れません。
    </p>
    <div class="grid">
      <label for="toolkeeper-path">ToolKeeper の場所</label>
      <div class="inline">
        <input id="toolkeeper-path" type="text" class="mono grow" bind:value={path} placeholder="ToolKeeper のフォルダ。空なら使わない" />
        <button onclick={pickToolkeeper}>参照</button>
        <button onclick={checkToolkeeper} disabled={!path.trim() || tk.state === "checking"}>
          {tk.state === "checking" ? "確かめています…" : "確かめる"}
        </button>
        <button class="primary" onclick={savePath} disabled={!pathDirty || saving}>{saving ? "保存しています…" : "保存"}</button>
      </div>
    </div>
    {#if pathDirty}
      <p class="note">
        {path.trim() ? "この場所はまだ保存していません。「保存」で使い始めます。" : "「保存」で ToolKeeper を使わない設定にします。"}
      </p>
    {/if}

    {#if !path.trim()}
      <h3>使い始めるには</h3>
      <ol class="note steps">
        <li>ToolKeeper を置く (Python 3.11 以上が要ります。依存は標準ライブラリだけで、.venv は要りません)</li>
        <li>上の「ToolKeeper の場所」に ToolKeeper のフォルダを指定し、「確かめる」で使えるかを見てから保存する</li>
        <li>ToolKeeper の定期実行を登録する (<code>{api.TOOLKEEPER_REGISTER}</code>)</li>
      </ol>
    {:else if tk.state === "error"}
      <p class="err">{tk.text}</p>
    {:else if tk.state === "ok"}
      {@const s = tk.status}
      <table class="tk-status">
        <tbody>
          <tr>
            <th>ToolKeeper</th>
            <td>
              <span class="badge good">{s.toolkeeper}</span>
              {#if s.schema !== 1}<span class="badge high">結果の形 {s.schema} は読めません</span>{/if}
            </td>
          </tr>
          <tr><th>データの場所</th><td><span class="mono">{s.dataDir}</span></td></tr>
          <tr>
            <th>最後に調べた</th>
            <td>{#if s.lastCheck}{new Date(s.lastCheck).toLocaleString()}{:else}<span class="muted">まだありません</span>{/if}</td>
          </tr>
          <tr>
            <th>定期実行</th>
            <td>
              {#if s.task.registered}
                <span class="badge good">登録済み</span> <span class="muted">(タスク「ToolKeeper check」、週 1 回、調べるだけ)</span>
              {:else}
                <span class="badge mid">未登録</span> ToolKeeper のフォルダで次を実行してください
                <div class="cmd">
                  <code>{api.TOOLKEEPER_REGISTER}</code>
                  <button class="small" onclick={() => copyText(api.TOOLKEEPER_REGISTER)}>コピー</button>
                </div>
              {/if}
            </td>
          </tr>
          <tr>
            <th>調べる先</th>
            <td>
              winget {s.sources.winget ?? "なし"}、choco {s.sources.choco ?? "なし"}、Cygwin {s.sources.cygwin ? "あり" : "なし"}、Ghostscript
              {s.sources.ghostscript ? "あり" : "なし"}
            </td>
          </tr>
        </tbody>
      </table>
    {/if}
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

  /* 道具の表。行は押せないので、押せる見た目 (指のカーソル・乗せたときの色) は付けない */
  .tools {
    width: 100%;
    border-collapse: collapse;
  }

  .tools th {
    text-align: left;
    font-size: 12px;
    font-weight: 600;
    color: var(--ink-2);
    padding: 4px 8px;
    border-bottom: 1px solid var(--line);
    white-space: nowrap;
  }

  .tools td {
    padding: 5px 8px;
    border-bottom: 1px solid var(--line);
    vertical-align: middle;
  }

  .tools tr.dim td {
    color: var(--ink-2);
  }

  /* 区分は押せない札。枠も塗りも付けず、小さな文字にする (ボタンと形で分ける) */
  .tier {
    font-size: 11px;
    color: var(--ink-2);
    white-space: nowrap;
    cursor: help;
  }

  .c-name {
    font-weight: 600;
  }

  .c-name .badge {
    margin-left: 6px;
    font-weight: normal;
  }

  .c-note {
    font-size: 12px;
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

  .tk-status {
    width: 100%;
    border-collapse: collapse;
    margin: 10px 0 4px;
  }

  .tk-status th,
  .tk-status td {
    text-align: left;
    vertical-align: top;
    padding: 4px 8px 4px 0;
    border-bottom: 1px solid var(--line);
  }

  .tk-status th {
    font-weight: 500;
    color: var(--muted);
    white-space: nowrap;
    width: 9em;
  }

  .tk-status .mono {
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
