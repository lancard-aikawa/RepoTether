<script lang="ts">
  import { untrack } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import type { Project } from "$lib/derive";
  import { emailCandidates } from "$lib/derive";
  import type { Account, AccountKind, Config, ExternalTool } from "$lib/types";
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

  const configDirty = $derived(JSON.stringify(draft) !== JSON.stringify(app.config));

  // 画面の好み (この PC だけ) の下書き。設定と同じく、保存するまで反映しない
  const PREF_KEYS = [
    "theme",
    "terminal",
    "density",
    "autoLocalMin",
    "autoRemoteMin",
    "autoActiveDays",
    "fetchOnRemote",
    "includeAutomated",
  ] as const;
  type PrefDraft = Pick<typeof prefs, (typeof PREF_KEYS)[number]>;
  const pickPrefs = () => Object.fromEntries(PREF_KEYS.map((k) => [k, prefs[k]])) as PrefDraft;
  let prefDraft = $state(pickPrefs());
  const prefsDirty = $derived(PREF_KEYS.some((k) => prefDraft[k] !== prefs[k]));

  /** 設定ファイル以外 (画面の好み・Claude Code の保存期間) に、保存していない変更があるか */
  function extraDirty() {
    return prefsDirty || retentionChanged;
  }
  const dirty = $derived(configDirty || extraDirty());
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

  async function pickCloneRoot() {
    const p = await api.pickFolder("クローン先の親フォルダ", draft.cloneRoot ?? undefined);
    if (p) draft.cloneRoot = p;
  }

  function addTool() {
    const ids = new Set(draft.externalTools.map((t) => t.id));
    let i = 1;
    while (ids.has(`tool${i}`)) i++;
    draft.externalTools.push({ id: `tool${i}`, label: "", command: "", args: "" });
  }

  async function pickTool(t: ExternalTool) {
    const p = await api.pickFile("起動するプログラム", t.command || undefined, api.isWindows ? ["exe", "cmd", "bat"] : undefined);
    if (!p) return;
    t.command = p;
    // 名前が空なら、ファイル名 (拡張子なし) を入れておく
    if (!t.label.trim()) t.label = p.split(/[\\/]/).pop()!.replace(/\.[^.]+$/, "");
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
    if (retentionChanged && chosenDays != null && (!Number.isInteger(chosenDays) || chosenDays < 1 || chosenDays > 36500)) {
      toast("保存期間は 1〜36500 日にしてください");
      return;
    }
    saving = true;
    try {
      // 読み直すのは設定ファイルの中身を変えたときだけ (テーマなどを変えただけなら読み直さない)
      const changed = configDirty;
      const accountsChanged = JSON.stringify(draft.accounts) !== JSON.stringify(app.config?.accounts);
      if (changed) await updateConfig(structuredClone($state.snapshot(draft)));
      if (retentionChanged) retention = await api.setClaudeRetention(chosenDays);
      if (prefsDirty) {
        Object.assign(prefs, $state.snapshot(prefDraft));
        savePrefs();
        applyTheme();
      }
      // 入力したトークンは保存後に消えるので、保存後の設定から下書きを作り直す
      revert();
      toast("保存しました");
      if (changed) await refresh(accountsChanged);
    } catch (e) {
      toast(errorText(e));
    } finally {
      saving = false;
    }
  }

  function revert() {
    draft = structuredClone($state.snapshot(app.config!));
    replacing.clear();
    prefDraft = pickPrefs();
    if (retention) syncChoice(retention);
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
          label: "表示・その他",
          dirty:
            extraDirty() ||
            !same(
              [draft.cloneRoot, draft.claudeDir, draft.sessionvaultPath, draft.externalTools],
              [saved.cloneRoot, saved.claudeDir, saved.sessionvaultPath, saved.externalTools],
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

  // ---- Claude Code の保存期間 (~/.claude/settings.json の cleanupPeriodDays) ----
  // 選んだ値は下書き。保存のときに Claude Code の設定ファイルへ書く
  let retention = $state<api.ClaudeRetention | null>(null);
  let retentionError = $state("");
  let retentionChoice = $state("default");
  let retentionCustom = $state(365);

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

  // この PC で見つかった端末
  let terminals = $state<api.TerminalChoice[]>([]);
  $effect(() => {
    api.listTerminals().then((t) => (terminals = t)).catch(() => {});
  });
</script>

<div class="wrap">
  <div class="toolbar bar">
    <span class="muted">{dirty ? "保存していない変更があります" : "設定"}</span>
    <span class="spacer"></span>
    <button onclick={revert} disabled={!dirty || saving}>元に戻す</button>
    <button class="primary" onclick={save} disabled={!dirty || saving}>
      {saving ? "保存しています…" : configDirty || !dirty ? "保存して更新" : "保存"}
    </button>
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
      <h2>外部ツール</h2>
      <p class="note">
        プロジェクトのフォルダを渡して起動するプログラムです。VS Code / 端末 / フォルダ / Claude のボタンの横に、ここの名前のボタンが並びます
        (保存してから)。引数の <code>{"{path}"}</code> がフォルダのパスになり、引数が空ならフォルダのパスだけを渡します。
      </p>
      {#each draft.externalTools as t, i (t.id)}
        <div class="account">
          <div class="grid">
            <label for="tool-label-{t.id}">名前</label>
            <div class="inline">
              <input id="tool-label-{t.id}" type="text" class="grow" placeholder="ボタンに出す名前" bind:value={t.label} />
              <button class="ghost" onclick={() => draft.externalTools.splice(i, 1)}>削除</button>
            </div>
            <label for="tool-command-{t.id}">プログラム</label>
            <div class="inline">
              <input id="tool-command-{t.id}" type="text" class="mono grow" placeholder="exe の場所 (PATH にあれば名前だけでもよい)" bind:value={t.command} />
              <button onclick={() => pickTool(t)}>参照</button>
            </div>
            <label for="tool-args-{t.id}">引数</label>
            <input id="tool-args-{t.id}" type="text" class="mono" placeholder={"空ならフォルダのパスだけ。例: --dir={path}"} bind:value={t.args} />
          </div>
        </div>
      {/each}
      <button onclick={addTool}>ツールを追加</button>
    </section>

    <section class="panel card">
      <h2>Claude Code のログの保存期間</h2>
      <p class="note">
        Claude Code は、保存期間 (既定 {retention?.defaultDays ?? 30} 日) より古いセッションのログを起動時に消します。
        消えたセッションは、RepoTether の履歴・グラフ・日報・会話の全文からも消えます。
        ここで変えて保存すると Claude Code の設定ファイルの <code>cleanupPeriodDays</code> だけを書き換え、ほかの設定には触れません。
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
        </div>
        <p class="note mono">{retention.path}</p>
      {:else}
        <p class="muted">読み込んでいます…</p>
      {/if}
    </section>

    <section class="panel card">
      <h2>表示 <span class="muted small">(この PC だけ)</span></h2>
      <div class="inline">
        <span class="muted">テーマ</span>
        <div class="segmented" role="group" aria-label="テーマ">
          {#each themes as t (t.id)}
            <button class:on={prefDraft.theme === t.id} onclick={() => (prefDraft.theme = t.id)}>{t.label}</button>
          {/each}
        </div>
        <label class="field">
          <span class="muted">端末</span>
          <select bind:value={prefDraft.terminal}>
            <option value="">自動 (Windows Terminal、無ければ PowerShell)</option>
            {#each terminals as t (t.id)}<option value={t.id}>{t.label}</option>{/each}
          </select>
        </label>
        <span class="muted">一覧の密度</span>
        <div class="segmented" role="group" aria-label="一覧の密度">
          <button class:on={prefDraft.density === "normal"} onclick={() => (prefDraft.density = "normal")}>標準</button>
          <button class:on={prefDraft.density === "compact"} onclick={() => (prefDraft.density = "compact")}>コンパクト</button>
        </div>
      </div>
      <div class="auto">
        <span class="muted">自動更新</span>
        <label class="field">
          ローカル
          <select bind:value={prefDraft.autoLocalMin}>
            {#each [0, 1, 5, 15, 30, 60] as m (m)}<option value={m}>{m ? `${m} 分ごと` : "しない"}</option>{/each}
          </select>
        </label>
        <label class="field">
          リモート
          <select bind:value={prefDraft.autoRemoteMin}>
            {#each [0, 15, 30, 60, 180] as m (m)}<option value={m}>{m ? `${m} 分ごと` : "しない"}</option>{/each}
          </select>
        </label>
        <label class="field">
          読み直す範囲
          <select bind:value={prefDraft.autoActiveDays}>
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
        <input type="checkbox" bind:checked={prefDraft.fetchOnRemote} />
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
          <input type="checkbox" bind:checked={prefDraft.includeAutomated} />
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
