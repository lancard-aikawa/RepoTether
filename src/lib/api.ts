// バックエンド (Tauri コマンド) の呼び出し口。
// ブラウザで `pnpm dev` だけを開いたとき (Tauri の外) は、static/dev-snapshot.json を読む表示確認用のモードになる。
// dev-snapshot.json は `cargo run --example dump -- ../static/dev-snapshot.json` で作る (git には入れない)。

import type { Commit, Config, Snapshot, TranscriptEntry } from "./types";

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export const isMac = typeof navigator !== "undefined" && /Mac/.test(navigator.userAgent);
export const isWindows = typeof navigator !== "undefined" && /Windows/.test(navigator.userAgent);
/** パスの区切り */
export const sep = isWindows ? "\\" : "/";
/** OS の資格情報の保管庫の呼び名 */
export const secretStoreName = isMac ? "キーチェーン" : isWindows ? "資格情報マネージャー" : "資格情報の保管庫";
export const secretStoreApp = isMac ? "キーチェーンアクセス" : secretStoreName;

/** gh でログインしているアカウント名。ログインしていなければ理由を投げる */
export async function checkGh(baseUrl: string): Promise<string> {
  if (!inTauri) {
    const user = hooks().__mockGhUser;
    if (user) return user;
    throw new Error("ブラウザ表示では確認できません");
  }
  return call("check_gh", { baseUrl });
}

export async function openSecretStore(): Promise<void> {
  if (!inTauri) throw new Error("ブラウザ表示では開けません");
  return call("open_credential_manager");
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

const mockConfig: Config = {
  roots: [isWindows ? "C:\\Repos" : "~/Repos"],
  scanDepth: 3,
  includeSessionFolders: true,
  claudeDir: null,
  historyDays: 365,
  authorEmails: [],
  accounts: [],
  cloneRoot: null,
  hidden: [],
  tags: {},
  tagDefs: [],
  starred: [],
};

/** ブラウザ表示の確認用に、外から差し込める値 (tools/manual-shots.mjs がマニュアルの画面を撮るときに使う) */
type MockHooks = {
  __mockConfig?: Config;
  __mockReadme?: Record<string, string>;
  __noReadme?: string[];
  __mockGhUser?: string;
  __mockTranscript?: TranscriptEntry[];
};
const hooks = () => (typeof window === "undefined" ? {} : (window as unknown as MockHooks));

export async function getConfig(): Promise<Config> {
  if (!inTauri) {
    const injected = hooks().__mockConfig;
    if (injected) Object.assign(mockConfig, structuredClone(injected));
    return structuredClone(mockConfig);
  }
  return call("get_config");
}

/** 保存後の設定 (トークンは抜いたもの) を返す */
export async function saveConfig(config: Config): Promise<Config> {
  if (!inTauri) {
    // Svelte の $state (Proxy) は structuredClone できないので JSON で写す (本番の invoke も JSON で送る)
    const next: Config = JSON.parse(JSON.stringify(config));
    for (const a of next.accounts) {
      if (a.clearToken) a.hasToken = false;
      else if (a.token.trim()) a.hasToken = true;
      a.token = "";
      a.clearToken = false;
    }
    Object.assign(mockConfig, next);
    return structuredClone(next);
  }
  return call("save_config", { config });
}

export async function loadSnapshot(): Promise<Snapshot | null> {
  if (!inTauri) {
    const r = await fetch("/dev-snapshot.json");
    return r.ok ? r.json() : null;
  }
  return call("load_snapshot");
}

export async function refresh(includeRemote: boolean, fetch = false): Promise<Snapshot> {
  if (!inTauri) {
    const s = await loadSnapshot();
    if (!s) throw new Error("static/dev-snapshot.json がありません");
    return s;
  }
  return call("refresh", { includeRemote, fetch });
}

export async function onProgress(f: (msg: string) => void): Promise<() => void> {
  if (!inTauri) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return listen<string>("refresh-progress", (e) => f(e.payload));
}

export type OpenTarget = "vscode" | "terminal" | "explorer";

/** terminal は端末の種類 (空なら自動)。target が terminal のときだけ使う */
export async function openIn(target: OpenTarget, path: string, terminal = ""): Promise<void> {
  if (!inTauri) throw new Error("ブラウザ表示では開けません");
  return call("open_in", { target, path, terminal });
}

/** 期間に関係なく、新しい方から skip 件を飛ばして limit 件のコミット (詳細パネルの「次の 10 件」) */
export async function repoLog(path: string, skip: number, limit: number, mineOnly: boolean): Promise<Commit[]> {
  if (!inTauri) {
    // ブラウザ表示ではリポジトリを読めないので、画面の確認用に古い日付の見本を返す (60 件で終わり)
    const total = 60;
    return Array.from({ length: Math.max(0, Math.min(limit, total - skip)) }, (_, i) => {
      const n = skip + i;
      return {
        repoId: path,
        hash: `sample${n}`,
        at: new Date(Date.UTC(2021, 0, 1) - n * 86400000 * 7).toISOString(),
        authorName: mineOnly ? "自分" : n % 2 ? "自分" : "ほかの人",
        authorEmail: "sample@example.com",
        subject: `見本のコミット ${n + 1}`,
        isMerge: false,
      };
    });
  }
  return call("repo_log", { path, skip, limit, mineOnly });
}

/** そのフォルダで claude を起動する (選んだ端末で) */
export async function claudeOpen(path: string, terminal = ""): Promise<void> {
  if (!inTauri) throw new Error("ブラウザ表示では開けません");
  return call("claude_open", { path, terminal });
}

/** セッションを再開する (claude -r)。fork なら元の会話を残して別の会話として続ける */
export async function claudeResume(cwd: string, sessionId: string, fork: boolean, terminal = ""): Promise<void> {
  if (!inTauri) throw new Error("ブラウザ表示では再開できません");
  return call("claude_resume", { cwd, sessionId, fork, terminal });
}

/** セッションの会話の全文 */
export async function sessionTranscript(sessionId: string): Promise<TranscriptEntry[]> {
  if (!inTauri) {
    const given = hooks().__mockTranscript;
    if (given) return given;
    return [
      { role: "user", at: new Date().toISOString(), text: "ブラウザ表示では、会話の全文の代わりに見本を出します", tools: [] },
      { role: "assistant", at: new Date().toISOString(), text: "**見本**の返答です。\n\n- 箇条書き\n- `code`", tools: ["Read", "Edit"] },
    ];
  }
  return call("session_transcript", { sessionId });
}

export interface TerminalChoice {
  id: string;
  label: string;
}

/** この PC で見つかった端末 */
export async function listTerminals(): Promise<TerminalChoice[]> {
  if (!inTauri) return [{ id: "wt", label: "Windows Terminal" }, { id: "pwsh", label: "PowerShell 7" }];
  return call("list_terminals");
}

export async function openUrl(url: string): Promise<void> {
  if (!inTauri) {
    window.open(url, "_blank");
    return;
  }
  const { openUrl } = await import("@tauri-apps/plugin-opener");
  return openUrl(url);
}

export async function cloneRepo(url: string, dest: string): Promise<string> {
  if (!inTauri) throw new Error("ブラウザ表示ではクローンできません");
  return call("clone_repo", { url, dest });
}

export async function trustRepo(path: string): Promise<void> {
  if (!inTauri) throw new Error("ブラウザ表示では使えません");
  return call("trust_repo", { path });
}

/** ウィンドウのタイトルバーの明暗。null なら OS に合わせる */
export async function setWindowTheme(theme: "light" | "dark" | null): Promise<void> {
  if (!inTauri) return;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  await getCurrentWindow().setTheme(theme);
}

/** 保存ダイアログを出して書く。キャンセルなら null */
export async function saveTextWithDialog(defaultName: string, content: string): Promise<string | null> {
  if (!inTauri) {
    const a = document.createElement("a");
    a.href = URL.createObjectURL(new Blob([content], { type: "text/markdown" }));
    a.download = defaultName;
    a.click();
    return defaultName;
  }
  return call("save_text_with_dialog", { defaultName, content });
}

export interface Readme {
  name: string;
  content: string;
  markdown: boolean;
  truncated: boolean;
}

/** リポジトリ直下の README。無ければ null */
export async function readReadme(path: string): Promise<Readme | null> {
  if (!inTauri) {
    // ブラウザ表示ではファイルを読めないので、無害化の確認を兼ねた見本を返す。
    // 画面の確認用に window.__noReadme に入れたパスは「README なし」、__mockReadme にあればその中身にする
    if (hooks().__noReadme?.includes(path)) return null;
    const given = hooks().__mockReadme;
    if (given) return given[path] ? { name: "README.md", content: given[path], markdown: true, truncated: false } : null;
    return {
      name: "README.md (ブラウザ表示の見本)",
      markdown: true,
      truncated: false,
      content: [
        `# 見本の README`,
        ``,
        `${path} の README は、アプリで開くと表示されます。`,
        ``,
        `## 表`,
        ``,
        `| 項目 | 値 |`,
        `|---|---|`,
        `| a | \`code\` |`,
        ``,
        "```sh\npnpm install\n```",
        ``,
        `- [外部リンク](https://github.com/)`,
        `- [相対リンク](docs/setup.md)`,
        ``,
        `<script>document.title = "XSS"</script>`,
        `<img src="x" onerror="document.title='XSS'">`,
        `<a href="javascript:document.title='XSS'">javascript リンク</a>`,
      ].join("\n"),
    };
  }
  return call("read_readme", { path });
}

/** フォルダ選択ダイアログ。キャンセルなら null */
export async function pickFolder(title: string, defaultPath?: string): Promise<string | null> {
  if (!inTauri) return window.prompt(title, defaultPath ?? "") || null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const r = await open({ directory: true, multiple: false, title, defaultPath });
  return typeof r === "string" ? r : null;
}
