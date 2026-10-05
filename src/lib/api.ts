// バックエンド (Tauri コマンド) の呼び出し口。
// ブラウザで `pnpm dev` だけを開いたとき (Tauri の外) は、static/dev-snapshot.json を読む表示確認用のモードになる。
// dev-snapshot.json は `cargo run --example dump -- ../static/dev-snapshot.json` で作る (git には入れない)。

import type {
  Commit,
  Config,
  LockwatchStatus,
  SessionFinding,
  Snapshot,
  TranscriptEntry,
  VulnCheck,
  VulnReport,
} from "./types";

/** LockWatch の入手先 (設定と詳細パネルの案内に出す) */
export const LOCKWATCH_URL = "https://github.com/lancard-aikawa/LockWatch";
/** osv-scanner を入れるコマンド (LockWatch の README と同じ。版を固定する) */
export const OSV_SCANNER_INSTALL = "winget install --id Google.OSVScanner --version 2.6.0 --exact";
/** LockWatch の定期実行を登録するコマンド (LockWatch のフォルダで)。pwsh は入っていない環境があるので、Windows に最初からある powershell で書く */
export const LOCKWATCH_REGISTER = "powershell -NoProfile -ExecutionPolicy Bypass -File scripts\\register-task.ps1";

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
  sessionvaultPath: null,
  lockwatchPath: null,
};

/** ブラウザ表示の確認用に、外から差し込める値 (tools/manual-shots.mjs がマニュアルの画面を撮るときに使う) */
type MockHooks = {
  __mockConfig?: Config;
  __mockReadme?: Record<string, string>;
  __noReadme?: string[];
  __mockGhUser?: string;
  __mockTranscript?: TranscriptEntry[];
  __mockFindings?: SessionFinding[];
  __mockVulns?: VulnReport;
  __mockVulnCheck?: VulnCheck;
  __mockLockwatchStatus?: LockwatchStatus;
  /** パス → アイコンの URL (data: など) */
  __mockIcons?: Record<string, string>;
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

/**
 * 更新で git から読み直すリポジトリ。読み直さないものは前回の結果を引き継ぐ。
 * all = すべて、active = 最近 days 日に作業したものと .git の中が変わったもの、under = path の下のもの
 */
export type RefreshScope = { kind: "all" } | { kind: "active"; days: number } | { kind: "under"; path: string };

export async function refresh(
  includeRemote: boolean,
  fetch = false,
  scope: RefreshScope = { kind: "all" },
): Promise<Snapshot> {
  if (!inTauri) {
    const s = await loadSnapshot();
    if (!s) throw new Error("static/dev-snapshot.json がありません");
    return s;
  }
  return call("refresh", { includeRemote, fetch, scope });
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

/** SessionVault でセッションのログを検査する (読むだけ)。projectDirs はログのあるフォルダの名前 */
export async function sessionvaultVerify(projectDirs: string[]): Promise<SessionFinding[]> {
  if (!inTauri) return hooks().__mockFindings ?? [];
  return call("sessionvault_verify", { projectDirs });
}

const noVulns: VulnReport = {
  configured: false,
  error: null,
  locations: null,
  scannedAt: null,
  osvScanner: null,
  dbDownloadedAt: null,
  byRepo: {},
};

/** LockWatch の結果 (手元のリポジトリごと)。LockWatch を使わない設定なら configured: false */
export async function lockwatchResults(): Promise<VulnReport> {
  if (!inTauri) return hooks().__mockVulns ?? noVulns;
  return call("lockwatch_results");
}

/** LockWatch が使える状態か。path を渡せばその場所 (保存前の入力)、省けば設定の場所 */
export async function lockwatchStatus(path?: string | null): Promise<LockwatchStatus> {
  if (!inTauri) {
    const given = hooks().__mockLockwatchStatus;
    if (given) return given;
    throw new Error("ブラウザ表示では確かめられません");
  }
  return call("lockwatch_status", { path: path ?? null });
}

/** LockWatch の画面 (状態・結果・設定) を開く。path を渡せばその場所、省けば設定の場所 */
export async function lockwatchOpenGui(path?: string | null): Promise<void> {
  if (!inTauri) throw new Error("ブラウザ表示では開けません");
  return call("lockwatch_open_gui", { path: path ?? null });
}

/** 「今すぐ調べる」の事前チェック: 今照合したら前回の結果がそのまま返るか (照合はしない) */
export async function lockwatchCheck(repoId: string): Promise<VulnCheck> {
  if (!inTauri) {
    const given = hooks().__mockVulnCheck;
    if (given) return given;
    throw new Error("ブラウザ表示では調べられません");
  }
  return call("lockwatch_check", { repoId });
}

/** そのリポジトリだけ LockWatch で照合し直す。repoId は LocalRepo.id。fresh ならキャッシュを使わない */
export async function lockwatchScan(repoId: string, fresh = false): Promise<VulnReport> {
  if (!inTauri) throw new Error("ブラウザ表示では調べられません");
  return call("lockwatch_scan", { repoId, fresh });
}

/** ファイル選択ダイアログ。キャンセルなら null */
export async function pickFile(title: string, defaultPath?: string, extensions?: string[]): Promise<string | null> {
  if (!inTauri) return window.prompt(title, defaultPath ?? "") || null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const filters = extensions ? [{ name: extensions.join(", "), extensions }] : undefined;
  const r = await open({ directory: false, multiple: false, title, defaultPath, filters });
  return typeof r === "string" ? r : null;
}

export interface TerminalChoice {
  id: string;
  label: string;
}

export interface ClaudeRetention {
  /** ~/.claude/settings.json の場所 */
  path: string;
  /** 書かれている保存期間 (日)。null なら既定 */
  days: number | null;
  defaultDays: number;
}

// ブラウザ表示では Claude Code の設定を読めないので、画面の確認用に覚えておくだけ
let mockRetention: ClaudeRetention = { path: "%USERPROFILE%\\.claude\\settings.json", days: null, defaultDays: 30 };

/** Claude Code のセッションの保存期間 (cleanupPeriodDays) */
export async function getClaudeRetention(): Promise<ClaudeRetention> {
  if (!inTauri) return { ...mockRetention };
  return call("get_claude_retention");
}

/** 保存期間を書く。null なら既定に戻す (項目を消す) */
export async function setClaudeRetention(days: number | null): Promise<ClaudeRetention> {
  if (!inTauri) {
    mockRetention = { ...mockRetention, days };
    return { ...mockRetention };
  }
  return call("set_claude_retention", { days });
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

/** 画像の中身から種類を見分ける (SVG は種類を付けないと img で出ない) */
function imageType(b: Uint8Array): string | null {
  const at = (i: number, ...xs: number[]) => xs.every((x, k) => b[i + k] === x);
  if (at(0, 0x89, 0x50, 0x4e, 0x47)) return "image/png";
  if (at(0, 0xff, 0xd8, 0xff)) return "image/jpeg";
  if (at(0, 0x47, 0x49, 0x46)) return "image/gif";
  if (at(0, 0x52, 0x49, 0x46, 0x46) && at(8, 0x57, 0x45, 0x42, 0x50)) return "image/webp";
  if (at(0, 0x00, 0x00, 0x01, 0x00)) return "image/x-icon";
  // SVG は先頭に <?xml や コメントが付くことがあるので、最初の部分に <svg があるかで見る
  const head = new TextDecoder().decode(b.subarray(0, 512)).toLowerCase();
  return head.includes("<svg") ? "image/svg+xml" : null;
}

const iconCache = new Map<string, Promise<string | null>>();

/**
 * プロジェクトのフォルダの中のアプリのアイコン (表示用の URL)。無ければ null。
 * アイコンはめったに変わらないので、一度読んだものはアプリを閉じるまで覚えておく
 */
export function repoIcon(path: string): Promise<string | null> {
  let p = iconCache.get(path);
  if (!p) {
    // 読めなかったとき (ドライブが一時的に見えないなど) は覚えず、次に表示したときにもう一度読む
    p = loadIcon(path).catch(() => {
      iconCache.delete(path);
      return null;
    });
    iconCache.set(path, p);
  }
  return p;
}

async function loadIcon(path: string): Promise<string | null> {
  if (!inTauri) return hooks().__mockIcons?.[path] ?? null;
  const buf = await call<ArrayBuffer>("repo_icon", { path });
  const bytes = new Uint8Array(buf);
  const type = bytes.length ? imageType(bytes) : null;
  return type ? URL.createObjectURL(new Blob([bytes], { type })) : null;
}

/** フォルダ選択ダイアログ。キャンセルなら null */
export async function pickFolder(title: string, defaultPath?: string): Promise<string | null> {
  if (!inTauri) return window.prompt(title, defaultPath ?? "") || null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const r = await open({ directory: true, multiple: false, title, defaultPath });
  return typeof r === "string" ? r : null;
}
