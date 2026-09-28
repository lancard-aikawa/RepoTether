<script lang="ts">
  import { SvelteSet } from "svelte/reactivity";
  import type { Project } from "$lib/derive";
  import { emailCandidates } from "$lib/derive";
  import type { Account, AccountKind, Config } from "$lib/types";
  import * as api from "$lib/api";
  import { app, errorText, prefs, refresh, savePrefs, toast, updateConfig } from "$lib/store.svelte";

  let { projects }: { projects: Project[] } = $props();

  // 編集中の下書き。保存するまで app.config には反映しない
  let draft = $state<Config>(structuredClone($state.snapshot(app.config!)));
  let newEmail = $state("");
  let saving = $state(false);
  /** 保存済みのトークンを入れ直そうとしているアカウント */
  const replacing = new SvelteSet<string>();

  async function copy(text: string) {
    try {
      await navigator.clipboard.writeText(text);
      toast("コピーしました");
    } catch (e) {
      toast(errorText(e));
    }
  }

  async function openStore() {
    try {
      await api.openSecretStore();
    } catch (e) {
      toast(errorText(e));
    }
  }

  const dirty = $derived(JSON.stringify(draft) !== JSON.stringify(app.config));
  const candidates = $derived(
    app.snapshot
      ? emailCandidates(app.snapshot)
          .filter((c) => !draft.authorEmails.some((e) => e.toLowerCase() === c.email.toLowerCase()))
          .slice(0, 8)
      : [],
  );

  const hiddenRows = $derived(
    draft.hidden.map((key) => {
      const p = projects.find((x) => x.key === key || x.key === `remote:${key}`);
      return { key, name: p?.name ?? key, path: p?.path ?? null };
    }),
  );

  async function addRoot() {
    const p = await api.pickFolder("リポジトリを探すフォルダ");
    if (p && !draft.roots.some((r) => r.toLowerCase() === p.toLowerCase())) draft.roots.push(p);
  }

  async function pickCloneRoot() {
    const p = await api.pickFolder("クローン先の親フォルダ", draft.cloneRoot ?? undefined);
    if (p) draft.cloneRoot = p;
  }

  function addEmail(e: string) {
    const v = e.trim();
    if (v && !draft.authorEmails.some((x) => x.toLowerCase() === v.toLowerCase())) draft.authorEmails.push(v);
    newEmail = "";
  }

  function addAccount(kind: AccountKind) {
    const ids = new Set(draft.accounts.map((a) => a.id));
    let i = 1;
    while (ids.has(`acc${i}`)) i++;
    const a: Account = {
      id: `acc${i}`,
      kind,
      label: kind === "github" ? "GitHub" : kind === "gogs" ? "Gogs" : "Gitea",
      baseUrl: "",
      user: "",
      token: "",
      hasToken: false,
      clearToken: false,
      enabled: true,
    };
    draft.accounts.push(a);
  }

  async function save() {
    saving = true;
    try {
      const accountsChanged = JSON.stringify(draft.accounts) !== JSON.stringify(app.config?.accounts);
      await updateConfig(structuredClone($state.snapshot(draft)));
      // 入力したトークンは保存後に消えるので、保存後の設定から下書きを作り直す
      revert();
      toast("保存しました");
      await refresh(accountsChanged);
    } catch (e) {
      toast(errorText(e));
    } finally {
      saving = false;
    }
  }

  function revert() {
    draft = structuredClone($state.snapshot(app.config!));
    replacing.clear();
  }

  // ---- タブ ----
  type SettingsTab = "roots" | "authors" | "accounts" | "other" | "hidden" | "errors";
  const saved = $derived(app.config!);
  const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
  const tabs = $derived(
    (
      [
        {
          id: "roots",
          label: "探す場所",
          dirty: !same(
            [draft.roots, draft.scanDepth, draft.historyDays, draft.includeSessionFolders],
            [saved.roots, saved.scanDepth, saved.historyDays, saved.includeSessionFolders],
          ),
        },
        { id: "authors", label: "自分のコミット", dirty: !same(draft.authorEmails, saved.authorEmails) },
        { id: "accounts", label: "アカウント", dirty: !same(draft.accounts, saved.accounts) },
        {
          id: "other",
          label: "その他",
          dirty: !same([draft.cloneRoot, draft.claudeDir], [saved.cloneRoot, saved.claudeDir]),
        },
        { id: "hidden", label: "非表示", count: draft.hidden.length, dirty: !same(draft.hidden, saved.hidden) },
        { id: "errors", label: "問題", count: app.snapshot?.errors.length ?? 0, dirty: false },
      ] as { id: SettingsTab; label: string; count?: number; dirty: boolean }[]
    ).filter((t) => t.id !== "errors" || t.count),
  );
  const tab = $derived(tabs.some((t) => t.id === prefs.settingsTab) ? prefs.settingsTab : "roots");

  function selectTab(t: SettingsTab) {
    prefs.settingsTab = t;
    savePrefs();
  }

  function setPref(k: "includeAutomated", v: boolean) {
    prefs[k] = v;
    savePrefs();
  }
</script>

<div class="wrap">
  <div class="toolbar bar">
    <span class="muted">{dirty ? "保存していない変更があります" : "設定"}</span>
    <span class="spacer"></span>
    <button onclick={revert} disabled={!dirty || saving}>元に戻す</button>
    <button class="primary" onclick={save} disabled={!dirty || saving}>{saving ? "保存しています…" : "保存して更新"}</button>
  </div>

  <div class="tabs" role="tablist">
    {#each tabs as t (t.id)}
      <button role="tab" class="tab" class:on={tab === t.id} aria-selected={tab === t.id} onclick={() => selectTab(t.id)}>
        {t.label}
        {#if t.count}<span class="count num">{t.count}</span>{/if}
        {#if t.dirty}<span class="dirty" title="保存していない変更があります"></span>{/if}
      </button>
    {/each}
  </div>

  <div class="scroll">
    {#if tab === "roots"}
    <section class="panel card">
      <h2>リポジトリを探す場所</h2>
      <ul class="rows">
        {#each draft.roots as r, i (r)}
          <li>
            <span class="mono grow">{r}</span>
            <button class="ghost" onclick={() => draft.roots.splice(i, 1)}>外す</button>
          </li>
        {:else}
          <li class="muted">まだありません</li>
        {/each}
      </ul>
      <button onclick={addRoot}>フォルダを追加</button>
      <div class="grid">
        <label for="depth">探す深さ</label>
        <div><input id="depth" type="number" min="1" max="6" bind:value={draft.scanDepth} /> <span class="muted">階層</span></div>
        <label for="days">コミットを読む期間</label>
        <div><input id="days" type="number" min="7" max="3650" bind:value={draft.historyDays} /> <span class="muted">日</span></div>
        <span></span>
        <label class="check">
          <input type="checkbox" bind:checked={draft.includeSessionFolders} />
          Claude のセッションで使ったフォルダも、git リポジトリなら対象にする (探す場所の外でも)
        </label>
      </div>
    </section>
    {/if}

    {#if tab === "authors"}
    <section class="panel card">
      <h2>自分のコミット</h2>
      <p class="muted">履歴・グラフ・日報は、ここにあるメールアドレスのコミットだけを数えます。空なら全員分を数えます。</p>
      <ul class="rows">
        {#each draft.authorEmails as e, i (e)}
          <li>
            <span class="mono grow">{e}</span>
            <button class="ghost" onclick={() => draft.authorEmails.splice(i, 1)}>外す</button>
          </li>
        {/each}
      </ul>
      <div class="inline">
        <input type="text" placeholder="メールアドレス" bind:value={newEmail} onkeydown={(e) => e.key === "Enter" && addEmail(newEmail)} />
        <button onclick={() => addEmail(newEmail)} disabled={!newEmail.trim()}>追加</button>
      </div>
      {#if candidates.length}
        <h3 class="sub">コミットに出てくるアドレス</h3>
        <ul class="rows">
          {#each candidates as c (c.email)}
            <li>
              <span class="mono grow">{c.email}</span>
              <span class="muted">{c.name} / {c.count} 件</span>
              <button onclick={() => addEmail(c.email)}>追加</button>
            </li>
          {/each}
        </ul>
      {/if}
    </section>
    {/if}

    {#if tab === "accounts"}
    <section class="panel card">
      <h2>リモートのアカウント</h2>
      <p class="muted">
        トークンがあれば見られる全リポジトリ、無ければユーザー名の公開リポジトリを取ります (Gogs はトークンが必須)。
        トークンは設定ファイルではなく {api.secretStoreName} に保存され、この画面にも表示されません。
        読み取り権限だけのトークンを使ってください。
      </p>
      <button onclick={openStore}>{api.secretStoreApp}を開く</button>
      <details class="howto">
        <summary>{api.secretStoreName}に手で登録するには</summary>
        {#if api.isMac}
          <p>
            キーチェーンアクセスで「新規パスワード項目」を作り、キーチェーン項目名に <code>RepoTether</code>、アカウント名に
            下の「{api.secretStoreName}での名前」(例: <code>RepoTether:acc1</code>)、パスワードにトークンを入れます。
          </p>
        {:else}
          <p>
            先にこの画面でアカウントを追加して保存しておき、資格情報マネージャーの「Windows 資格情報」→「汎用資格情報の追加」で
            次のように入れます。
          </p>
          <ul>
            <li>インターネットまたはネットワークのアドレス: 下の「{api.secretStoreName}での名前」(例: <code>RepoTether:acc1</code>)</li>
            <li>ユーザー名: 何でもよい (Gogs のユーザー名など)</li>
            <li>パスワード: トークン</li>
          </ul>
        {/if}
        <p>登録したら、この画面を開き直すと「保存済み」になります。普通はこの画面でトークンを入れて保存するだけで十分です。</p>
      </details>
      {#each draft.accounts as a, i (a.id)}
        <div class="account">
          <div class="acc-head">
            <strong>{a.label || a.kind}</strong>
            <span class="muted">{a.kind}</span>
            <span class="spacer"></span>
            <label class="check"><input type="checkbox" bind:checked={a.enabled} /> 使う</label>
            <button class="ghost" onclick={() => draft.accounts.splice(i, 1)}>削除</button>
          </div>
          <div class="grid">
            <label for="label-{a.id}">表示名</label>
            <input id="label-{a.id}" type="text" bind:value={a.label} />
            <label for="url-{a.id}">{a.kind === "github" ? "API の URL" : "サーバーの URL"}</label>
            <input
              id="url-{a.id}"
              type="text"
              class="mono"
              placeholder={a.kind === "github" ? "空なら https://api.github.com" : "https://git.example.com"}
              bind:value={a.baseUrl}
            />
            <label for="user-{a.id}">ユーザー名</label>
            <input id="user-{a.id}" type="text" placeholder="トークンなしのときに使う" bind:value={a.user} />
            <label for="token-{a.id}">トークン</label>
            {#if a.hasToken && !a.clearToken && !replacing.has(a.id)}
              <div class="inline">
                <span class="badge good">{api.secretStoreName}に保存済み</span>
                <button onclick={() => replacing.add(a.id)}>入れ直す</button>
                <button class="ghost" onclick={() => (a.clearToken = true)}>消す</button>
              </div>
            {:else if a.clearToken}
              <div class="inline">
                <span class="badge mid">保存すると消します</span>
                <button class="ghost" onclick={() => (a.clearToken = false)}>やめる</button>
              </div>
            {:else}
              <div class="inline">
                <input
                  id="token-{a.id}"
                  type="password"
                  autocomplete="off"
                  class="grow"
                  placeholder={a.hasToken ? "新しいトークン" : ""}
                  bind:value={a.token}
                />
                {#if replacing.has(a.id)}
                  <button class="ghost" onclick={() => (replacing.delete(a.id), (a.token = ""))}>やめる</button>
                {/if}
              </div>
            {/if}
            <span class="muted">{api.secretStoreName}での名前</span>
            <div class="inline">
              <code class="mono">RepoTether:{a.id}</code>
              <button class="ghost" onclick={() => copy(`RepoTether:${a.id}`)}>コピー</button>
            </div>
          </div>
        </div>
      {/each}
      <div class="inline">
        <button onclick={() => addAccount("github")}>GitHub を追加</button>
        <button onclick={() => addAccount("gogs")}>Gogs を追加</button>
        <button onclick={() => addAccount("gitea")}>Gitea を追加</button>
      </div>
      {#if app.snapshot?.remoteFetchedAt}
        <p class="muted">最後に取得: {new Date(app.snapshot.remoteFetchedAt).toLocaleString()} / {app.snapshot.remoteRepos.length} 件</p>
      {/if}
    </section>
    {/if}

    {#if tab === "other"}
    <section class="panel card">
      <h2>その他</h2>
      <div class="grid">
        <label for="clone-root">クローン先の親フォルダ</label>
        <div class="inline">
          <input id="clone-root" type="text" class="mono grow" bind:value={draft.cloneRoot} placeholder="空なら探す場所の 1 つ目" />
          <button onclick={pickCloneRoot}>参照</button>
        </div>
        <label for="claude-dir">Claude のログの場所</label>
        <input id="claude-dir" type="text" class="mono" bind:value={draft.claudeDir} placeholder="空なら %USERPROFILE%\.claude\projects" />
      </div>
    </section>

    <section class="panel card">
      <h2>表示 <span class="muted small">(すぐに反映・この PC だけ)</span></h2>
      <div class="col">
        <label class="check">
          <input type="checkbox" checked={prefs.includeAutomated} onchange={(e) => setPref("includeAutomated", e.currentTarget.checked)} />
          SDK などからの自動実行のセッションも活動に数える
        </label>
      </div>
    </section>

    {/if}

    {#if tab === "hidden"}
      <section class="panel card">
        <h2>非表示にしたもの</h2>
        {#if !hiddenRows.length}
          <p class="muted">ありません。状態タブで行にカーソルを乗せると「非表示」ボタンが出ます。</p>
        {/if}
        <ul class="rows">
          {#each hiddenRows as h, i (h.key)}
            <li>
              <span class="grow">{h.name} <span class="mono muted small">{h.path ?? h.key}</span></span>
              <button class="ghost" onclick={() => draft.hidden.splice(i, 1)}>戻す</button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if tab === "errors" && app.snapshot?.errors.length}
      <section class="panel card">
        <h2>読み込みの問題</h2>
        <ul class="rows">
          {#each app.snapshot.errors as e, i (i)}
            <li><span class="muted mono">{e.source}</span> <span class="grow">{e.message}</span></li>
          {/each}
        </ul>
      </section>
    {/if}
  </div>
</div>

<style>
  .wrap {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }

  .bar {
    padding: 10px 12px;
    border-bottom: 1px solid var(--line);
  }

  .spacer {
    flex: 1;
  }

  /* タブ: 選んでいるものは下線と太字で、はっきり分かるように */
  .tabs {
    display: flex;
    gap: 2px;
    padding: 0 12px;
    border-bottom: 1px solid var(--line);
    background: var(--surface);
    flex: none;
  }

  .tab {
    border: none;
    border-radius: 0;
    background: transparent;
    padding: 8px 12px 7px;
    color: var(--muted);
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
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

  /* 保存していない変更がある印 */
  .dirty {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--accent);
  }

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
  }

  .card > * {
    max-width: 100%;
  }

  .card h2 {
    margin: 0;
  }

  .sub {
    margin: 8px 0 0;
    color: var(--ink-2);
  }

  p {
    margin: 0;
  }

  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
    width: 100%;
  }

  .rows li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 0;
    border-bottom: 1px solid var(--line);
  }

  .grow {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .grid {
    display: grid;
    grid-template-columns: 160px 1fr;
    gap: 6px 12px;
    align-items: center;
    width: 100%;
  }

  .grid label:not(.check) {
    color: var(--ink-2);
  }

  input[type="number"] {
    width: 80px;
  }

  .inline {
    display: flex;
    gap: 6px;
    align-items: center;
    flex-wrap: wrap;
  }

  .col {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .howto {
    font-size: 12px;
    color: var(--ink-2);
    width: 100%;
  }

  .howto summary {
    cursor: pointer;
    color: var(--ink);
  }

  .howto p,
  .howto ul {
    margin: 6px 0;
  }

  code {
    font-family: var(--mono);
    font-size: 12px;
    background: var(--surface-2);
    padding: 0 4px;
    border-radius: 3px;
  }

  .account {
    width: 100%;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .acc-head {
    display: flex;
    gap: 8px;
    align-items: baseline;
  }

  .small {
    font-size: 12px;
  }
</style>
