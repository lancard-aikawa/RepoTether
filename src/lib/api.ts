// バックエンド (Tauri コマンド) の呼び出し口。
// ブラウザで `pnpm dev` だけを開いたとき (Tauri の外) は、static/dev-snapshot.json を読む表示確認用のモードになる。
// dev-snapshot.json は `cargo run --example dump -- ../static/dev-snapshot.json` で作る (git には入れない)。

import type { Config, Snapshot } from "./types";

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export const isMac = typeof navigator !== "undefined" && /Mac/.test(navigator.userAgent);
export const isWindows = typeof navigator !== "undefined" && /Windows/.test(navigator.userAgent);
/** パスの区切り */
export const sep = isWindows ? "\\" : "/";
/** OS の資格情報の保管庫の呼び名 */
export const secretStoreName = isMac ? "キーチェーン" : isWindows ? "資格情報マネージャー" : "資格情報の保管庫";
export const secretStoreApp = isMac ? "キーチェーンアクセス" : secretStoreName;

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
};

export async function getConfig(): Promise<Config> {
  if (!inTauri) return structuredClone(mockConfig);
  return call("get_config");
}

/** 保存後の設定 (トークンは抜いたもの) を返す */
export async function saveConfig(config: Config): Promise<Config> {
  if (!inTauri) {
    const next = structuredClone(config);
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

export async function refresh(includeRemote: boolean): Promise<Snapshot> {
  if (!inTauri) {
    const s = await loadSnapshot();
    if (!s) throw new Error("static/dev-snapshot.json がありません");
    return s;
  }
  return call("refresh", { includeRemote });
}

export async function onProgress(f: (msg: string) => void): Promise<() => void> {
  if (!inTauri) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return listen<string>("refresh-progress", (e) => f(e.payload));
}

export type OpenTarget = "vscode" | "terminal" | "explorer";

export async function openIn(target: OpenTarget, path: string): Promise<void> {
  if (!inTauri) throw new Error("ブラウザ表示では開けません");
  return call("open_in", { target, path });
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

/** フォルダ選択ダイアログ。キャンセルなら null */
export async function pickFolder(title: string, defaultPath?: string): Promise<string | null> {
  if (!inTauri) return window.prompt(title, defaultPath ?? "") || null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const r = await open({ directory: true, multiple: false, title, defaultPath });
  return typeof r === "string" ? r : null;
}
