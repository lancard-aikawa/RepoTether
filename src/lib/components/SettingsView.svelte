<script lang="ts">
  import { untrack } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import type { Project } from "$lib/derive";
  import { emailCandidates } from "$lib/derive";
  import type { Account, AccountKind, Config, LockwatchStatus } from "$lib/types";
  import * as api from "$lib/api";
  import {
    app,
    applyTheme,
    errorText,
    prefs,
    refresh,
    savePrefs,
    toast,
    updateConfig,
    type Theme,
  } from "$lib/store.svelte";

  let { projects }: { projects: Project[] } = $props();

  // 編集中の下書き。保存するまで app.config には反映しない
  let draft = $state<Config>(structuredClone($state.snapshot(app.config!)));
  let newEmail = $state("");
  let saving = $state(false);
  /** 保存済みのトークンを入れ直そうとしているアカウント */
  const replacing = new SvelteSet<string>();

  // gh のログイン状態 (アカウントごと)
  let ghState = $state<Record<string, { state: "checking" | "ok" | "error"; text: string }>>({});

  async function checkGh(a: Account) {
    ghState[a.id] = { state: "checking", text: "" };
    try {
      ghState[a.id] = { state: "ok", text: await api.checkGh(a.baseUrl) };
    } catch (e) {
      ghState[a.id] = { state: "error", text: errorText(e) };
    }
  }

  // アカウントのタブを開いたら、gh を使うアカウントの状態を自動で確かめる
  $effect(() => {
    if (tab !== "accounts") return;
    for (const a of untrack(() => draft.accounts)) {
      if (a.kind === "github" && a.auth === "gh" && !untrack(() => ghState[a.id])) checkGh(a);
    }
  });

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

  async function pickSessionvault() {
    const p = await api.pickFile("sessionvault.exe の場所", draft.sessionvaultPath ?? undefined, ["exe"]);
    if (p) draft.sessionvaultPath = p;
  }

  async function pickViewerFolder() {
    const p = await api.pickFolder("Claude History Viewer のフォルダ", draft.sessionvaultPath ?? undefined);
    if (p) draft.sessionvaultPath = p;
  }

  async function pickLockwatch() {
    const p = await api.pickFolder("LockWatch のリポジトリのフォルダ", draft.lockwatchPath ?? undefined);
    if (p) {
      draft.lockwatchPath = p;
      checkLockwatch();
    }
  }

  // ---- LockWatch (脆弱性タブ) ----
  // 入力中の場所で確かめる (保存前でも)。保存済みの場所と同じなら、詳細パネルの案内にも使う
  let lw = $state<
    | { state: "idle" }
    | { state: "checking" }
    | { state: "ok"; status: LockwatchStatus; path: string }
    | { state: "error"; text: string }
  >({ state: "idle" });

  async function checkLockwatch() {
    const path = draft.lockwatchPath?.trim();
    if (!path) {
      lw = { state: "idle" };
      return;
    }
    lw = { state: "checking" };
    try {
      const status = await api.lockwatchStatus(path);
      lw = { state: "ok", status, path };
      if (path === app.config?.lockwatchPath?.trim()) app.lockwatchStatus = status;
    } catch (e) {
      lw = { state: "error", text: errorText(e) };
    }
  }

  // 脆弱性のタブを開いたら、場所が決まっていれば自動で確かめる
  $effect(() => {
    if (tab !== "vulns") return;
    if (untrack(() => lw.state === "idle" && !!draft.lockwatchPath?.trim())) checkLockwatch();
  });

  let guiOpened = $state(false);

  /** LockWatch の画面を開く (確かめて使えると分かった場所で) */
  async function openLockwatchGui() {
    try {
      await api.lockwatchOpenGui(draft.lockwatchPath);
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
      // GitHub は gh のログインを借りるのを既定にする (トークンの管理が要らない)
      auth: kind === "github" ? "gh" : "token",
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
  type SettingsTab = "roots" | "authors" | "accounts" | "vulns" | "other" | "hidden" | "errors";
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
        { id: "vulns", label: "脆弱性", dirty: !same(draft.lockwatchPath ?? "", saved.lockwatchPath ?? "") },
        {
          id: "other",
          label: "表示・その他",
          dirty: !same(
            [draft.cloneRoot, draft.claudeDir, draft.sessionvaultPath],
            [saved.cloneRoot, saved.claudeDir, saved.sessionvaultPath],
          ),
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

  const themes: { id: Theme; label: string }[] = [
    { id: "system", label: "OS に合わせる" },
    { id: "light", label: "ライト" },
    { id: "dark", label: "ダーク" },
  ];

  function setFetch(v: boolean) {
    prefs.fetchOnRemote = v;
    savePrefs();
  }

  function setAuto(k: "autoLocalMin" | "autoRemoteMin" | "autoActiveDays", v: number) {
    prefs[k] = v;
    savePrefs();
  }

  // ---- Claude Code の保存期間 (~/.claude/settings.json の cleanupPeriodDays) ----
  // 設定の下書きとは別に、「変更」を押したときだけ Claude Code の設定ファイルへ書く
  let retention = $state<api.ClaudeRetention | null>(null);
  let retentionError = $state("");
  let retentionChoice = $state("default");
  let retentionCustom = $state(365);
  let retentionSaving = $state(false);

  function syncChoice(r: api.ClaudeRetention) {
    if (r.days == null) retentionChoice = "default";
    else if ([90, 180, 365, 3650].includes(r.days)) retentionChoice = String(r.days);
    else {
      retentionChoice = "custom";
      retentionCustom = r.days;
    }
  }

  $effect(() => {
    api
      .getClaudeRetention()
      .then((r) => {
        retention = r;
        syncChoice(r);
      })
      .catch((e) => (retentionError = errorText(e)));
  });

  const chosenDays = $derived(
    retentionChoice === "default" ? null : retentionChoice === "custom" ? Number(retentionCustom) : Number(retentionChoice),
  );
  const retentionChanged = $derived(!!retention && chosenDays !== retention.days);

  function daysLabel(d: number): string {
    if (d % 365 === 0) return `${d} 日 (約 ${d / 365} 年)`;
    return `${d} 日`;
  }

  async function saveRetention() {
    if (chosenDays != null && (!Number.isInteger(chosenDays) || chosenDays < 1 || chosenDays > 36500)) {
      toast("保存期間は 1〜36500 日にしてください");
      return;
    }
    retentionSaving = true;
    try {
      retention = await api.setClaudeRetention(chosenDays);
      syncChoice(retention);
      toast(chosenDays == null ? "Claude Code の保存期間を既定に戻しました" : `Claude Code の保存期間を ${chosenDays} 日にしました`);
    } catch (e) {
      toast(errorText(e));
    } finally {
      retentionSaving = false;
    }
  }

  // この PC で見つかった端末
  let terminals = $state<api.TerminalChoice[]>([]);
  $effect(() => {
    api.listTerminals().then((t) => (terminals = t)).catch(() => {});
  });

  function setTerminal(id: string) {
    prefs.terminal = id;
    savePrefs();
  }

  function setDensity(d: "normal" | "compact") {
    prefs.density = d;
    savePrefs();
    applyTheme();
  }

  function setTheme(t: Theme) {
    prefs.theme = t;
    savePrefs();
    applyTheme();
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
        GitHub は gh コマンドのログインを借りるのがおすすめです (RepoTether にトークンを置かずに済みます)。
        トークンを使う場合は、トークンがあれば見られる全リポジトリ、無ければユーザー名の公開リポジトリを取ります (Gogs はトークンが必須)。
        トークンは設定ファイルではなく {api.secretStoreName} に保存され、この画面にも表示されません。読み取り権限だけのトークンを使ってください。
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
            {#if a.kind === "github"}
              <span class="muted">認証</span>
              <div class="inline">
                <label class="check"><input type="radio" bind:group={a.auth} value="gh" /> gh コマンドのログインを使う (おすすめ)</label>
                <label class="check"><input type="radio" bind:group={a.auth} value="token" /> トークン</label>
              </div>
            {/if}
            {#if a.kind === "github" && a.auth === "gh"}
              <span class="muted">gh の状態</span>
              <div class="inline gh-status">
                {#if ghState[a.id]?.state === "ok"}
                  <span class="badge good">{ghState[a.id].text} でログイン中</span>
                {:else if ghState[a.id]?.state === "error"}
                  <span class="gh-error">{ghState[a.id].text}</span>
                {:else if ghState[a.id]?.state === "checking"}
                  <span class="muted">確かめています…</span>
                {/if}
                <button onclick={() => checkGh(a)}>確認</button>
              </div>
              <span></span>
              <p class="note">
                一覧を取るたびに <code>gh auth token</code> でトークンを受け取ります。RepoTether には保存しません。
                ログインしていなければ、ターミナルで <code>gh auth login --web</code> を実行してください (ブラウザでログインします)。
              </p>
            {:else}
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
            {/if}
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

    {#if tab === "vulns"}
    <section class="panel card">
      <h2>脆弱性 (LockWatch)</h2>
      <p class="note">
        LockWatch は、リポジトリの lock ファイル (package-lock.json・pnpm-lock.yaml・uv.lock・Cargo.lock など) を osv-scanner にかけて、
        使っているパッケージの脆弱性を調べる別の道具です (入れなくても RepoTether は使えます)。場所を指定すると、更新のたびに手元のリポジトリの一覧
        (公開・非公開の別つき) を LockWatch に渡し、結果を詳細パネルの「脆弱性」タブと一覧の印に出します。
        <strong>非公開のリポジトリと公開か分からないものは、LockWatch がパッケージ名を外に出さず、手元の脆弱性 DB で照合します。</strong>
        公開とみなすのは github.com で公開のものだけで、Gogs・Gitea などの自前のサーバーのものは非公開として渡します。
      </p>
      <div class="grid">
        <label for="lockwatch-path">LockWatch の場所</label>
        <div class="inline">
          <input id="lockwatch-path" type="text" class="mono grow" bind:value={draft.lockwatchPath} placeholder="LockWatch のフォルダ。空なら使わない" />
          <button onclick={pickLockwatch}>参照</button>
          <button onclick={checkLockwatch} disabled={!draft.lockwatchPath?.trim() || lw.state === "checking"}>
            {lw.state === "checking" ? "確かめています…" : "確かめる"}
          </button>
          <button
            onclick={openLockwatchGui}
            disabled={lw.state !== "ok"}
            title="LockWatch の画面 (状態・結果・設定) を開く。LockWatch の設定はそこで変える"
          >
            LockWatch を開く
          </button>
        </div>
      </div>
      {#if guiOpened}
        <p class="note">
          LockWatch の画面を開きました。そこで設定 (データの場所など) を変えたら、戻ってきて「確かめる」を押してください。
        </p>
      {/if}

      {#if !draft.lockwatchPath?.trim()}
        <h3>使い始めるには</h3>
        <ol class="note steps">
          <li>
            osv-scanner を入れる:
            <code>{api.OSV_SCANNER_INSTALL}</code>
            <button class="small" onclick={() => copyText(api.OSV_SCANNER_INSTALL)}>コピー</button>
          </li>
          <li>
            LockWatch を入れる (Python 3.10 以上と uv が要ります。手順は
            <button class="link" onclick={() => api.openUrl(api.LOCKWATCH_URL + "#入れ方")}>LockWatch の README</button>)
          </li>
          <li>上の「LockWatch の場所」に LockWatch のフォルダを指定し、「確かめる」で使えるかを見てから保存する</li>
          <li>LockWatch の定期実行を登録する (<code>{api.LOCKWATCH_REGISTER}</code>)</li>
        </ol>
      {:else if lw.state === "error"}
        <p class="gh-error">{lw.text}</p>
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
        {#if lw.path !== (saved.lockwatchPath ?? "").trim()}
          <p class="note">この場所はまだ保存していません。下の「保存」で使い始めます。</p>
        {/if}
      {/if}
      <p class="note">
        <button class="link" onclick={() => api.openUrl(api.LOCKWATCH_URL)}>LockWatch (GitHub)</button>
      </p>
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
        <label for="sessionvault-path">SessionVault の場所</label>
        <div class="inline">
          <input id="sessionvault-path" type="text" class="mono grow" bind:value={draft.sessionvaultPath} placeholder="空なら PATH の sessionvault.exe" />
          <button onclick={pickViewerFolder} title="Claude History Viewer のフォルダ (同梱の SessionVault を Python で動かす)">フォルダ</button>
          <button onclick={pickSessionvault} title="sessionvault.exe">exe</button>
        </div>
      </div>
      <p class="note">
        SessionVault は Claude Code のログを残し・検査する道具です。詳細パネルの Claude タブの「ログの検査」で使います (読むだけ)。
        Claude History Viewer を使っているなら、そのフォルダを指定すると、Viewer に同梱の SessionVault で、Viewer と同じ保管庫を見て検査します
        (Python 3.10 以上が要ります)。
      </p>
    </section>

    <section class="panel card">
      <h2>Claude Code のログの保存期間</h2>
      <p class="note">
        Claude Code は、保存期間 (既定 {retention?.defaultDays ?? 30} 日) より古いセッションのログを起動時に消します。
        消えたセッションは、RepoTether の履歴・グラフ・日報・会話の全文からも消えます。
        ここで変えると Claude Code の設定ファイルの <code>cleanupPeriodDays</code> だけを書き換え、ほかの設定には触れません。
        効くのは次に起動する Claude Code からで、すでに消えたログは戻りません。
      </p>
      {#if retentionError}
        <p class="gh-error">{retentionError}</p>
      {:else if retention}
        <div class="inline">
          <span class="muted">今の設定</span>
          <strong>{retention.days == null ? `既定 (${retention.defaultDays} 日)` : daysLabel(retention.days)}</strong>
          {#if (retention.days ?? retention.defaultDays) <= 30}
            <span class="badge mid">1 か月より前の会話は消えていきます</span>
          {/if}
        </div>
        <div class="inline">
          <select bind:value={retentionChoice} aria-label="保存期間">
            <option value="default">既定 ({retention.defaultDays} 日)</option>
            {#each [90, 180, 365, 3650] as d (d)}<option value={String(d)}>{daysLabel(d)}</option>{/each}
            <option value="custom">日数を指定</option>
          </select>
          {#if retentionChoice === "custom"}
            <input type="number" min="1" max="36500" bind:value={retentionCustom} aria-label="保存期間 (日)" /> <span class="muted">日</span>
          {/if}
          <button onclick={saveRetention} disabled={!retentionChanged || retentionSaving}>
            {retentionSaving ? "書き込んでいます…" : "変更"}
          </button>
        </div>
        <p class="note mono">{retention.path}</p>
      {:else}
        <p class="muted">読み込んでいます…</p>
      {/if}
    </section>

    <section class="panel card">
      <h2>表示 <span class="muted small">(すぐに反映・この PC だけ)</span></h2>
      <div class="inline">
        <span class="muted">テーマ</span>
        <div class="segmented" role="group" aria-label="テーマ">
          {#each themes as t (t.id)}
            <button class:on={prefs.theme === t.id} onclick={() => setTheme(t.id)}>{t.label}</button>
          {/each}
        </div>
        <label class="field">
          <span class="muted">端末</span>
          <select value={prefs.terminal} onchange={(e) => setTerminal(e.currentTarget.value)}>
            <option value="">自動 (Windows Terminal、無ければ PowerShell)</option>
            {#each terminals as t (t.id)}<option value={t.id}>{t.label}</option>{/each}
          </select>
        </label>
        <span class="muted">一覧の密度</span>
        <div class="segmented" role="group" aria-label="一覧の密度">
          <button class:on={prefs.density === "normal"} onclick={() => setDensity("normal")}>標準</button>
          <button class:on={prefs.density === "compact"} onclick={() => setDensity("compact")}>コンパクト</button>
        </div>
      </div>
      <div class="auto">
        <span class="muted">自動更新</span>
        <label class="field">
          ローカル
          <select value={prefs.autoLocalMin} onchange={(e) => setAuto("autoLocalMin", Number(e.currentTarget.value))}>
            {#each [0, 1, 5, 15, 30, 60] as m (m)}<option value={m}>{m ? `${m} 分ごと` : "しない"}</option>{/each}
          </select>
        </label>
        <label class="field">
          リモート
          <select
            value={prefs.autoRemoteMin}
            onchange={(e) => setAuto("autoRemoteMin", Number(e.currentTarget.value))}
          >
            {#each [0, 15, 30, 60, 180] as m (m)}<option value={m}>{m ? `${m} 分ごと` : "しない"}</option>{/each}
          </select>
        </label>
        <label class="field">
          読み直す範囲
          <select
            value={prefs.autoActiveDays}
            onchange={(e) => setAuto("autoActiveDays", Number(e.currentTarget.value))}
          >
            {#each [30, 90, 180, 365] as d (d)}<option value={d}>最近 {d} 日に作業したもの</option>{/each}
            <option value={0}>すべて</option>
          </select>
        </label>
      </div>
      <p class="note">
        自動更新と起動時は、最近作業したものと、git の操作 (コミット・切り替え・pull・push など) があったものだけを git から読み直し、
        ほかは前回の結果を使います。git を使わずにファイルだけを書き換えた古いリポジトリは、上の「更新」(フォルダを選んでいるときは「すべて更新」) を押すまで反映されません
        (どちらもすべてを読み直します)。
      </p>
      <label class="check">
        <input type="checkbox" checked={prefs.fetchOnRemote} onchange={(e) => setFetch(e.currentTarget.checked)} />
        リモートを更新するとき、各リポジトリで <code>git fetch</code> もする
      </label>
      <p class="note">
        fetch すると「取り込み待ち」の数と、別の PC で push した分が最新になります。リモートの追跡ブランチを書き換え、
        ネットワークにもつなぐので既定はしません。認証の画面は出さず、1 つ 20 秒で打ち切り、失敗は「問題」タブに出します。
      </p>
      <p class="note">
        ローカルはリポジトリの状態・コミット・Claude のセッション、リモートは GitHub / Gogs の一覧 (アカウントを使う設定のときだけ) を読み直します。
        前回の更新から間隔が過ぎたら裏で更新し、ウィンドウが見えていないあいだは止めます。
      </p>
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

  .auto {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 16px;
  }

  .field {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  /* 脆弱性 (LockWatch) の状態 */
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

  .gh-error {
    font-size: 12px;
    color: var(--st-high);
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
