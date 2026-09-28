// リモートの見せ方: どのサービスか (GitHub / Gogs / ...) と、ブラウザで開く URL。

import type { Config, LocalRepo, RemoteRepo } from "./types";

export type RemoteKind = "github" | "gogs" | "gitea" | "gitlab" | "backlog" | "other";

export interface RemoteLink {
  kind: RemoteKind;
  /** GitHub / Gogs / ... 。other はホスト名 */
  label: string;
  host: string;
  /** git の remote 名 (origin など)。リモート一覧だけのものは null */
  remoteName: string | null;
  /** git の URL (clone / fetch に使うもの) */
  url: string;
  /** 「所有者/リポジトリ名」。URL が長いので、一覧ではこれを出す (大文字小文字は URL のまま) */
  path: string;
  /** ブラウザで開く URL。作れなければ null */
  webUrl: string | null;
}

const KIND_LABEL: Record<Exclude<RemoteKind, "other">, string> = {
  github: "GitHub",
  gogs: "Gogs",
  gitea: "Gitea",
  gitlab: "GitLab",
  backlog: "Backlog",
};

function hostOf(url: string): string | null {
  try {
    return new URL(url).hostname.toLowerCase();
  } catch {
    return null;
  }
}

/** ホスト名からサービスを決める。設定のアカウントに登録したサーバーを優先する */
export function kindOf(host: string, cfg: Config | null): { kind: RemoteKind; label: string } {
  for (const a of cfg?.accounts ?? []) {
    const h = a.kind === "github" ? (a.baseUrl ? hostOf(a.baseUrl)?.replace(/^api\./, "") : "github.com") : hostOf(a.baseUrl);
    if (h && h === host) return { kind: a.kind, label: KIND_LABEL[a.kind] };
  }
  if (host === "github.com") return { kind: "github", label: KIND_LABEL.github };
  if (host === "gitlab.com") return { kind: "gitlab", label: KIND_LABEL.gitlab };
  if (/(^|\.)backlog\.(jp|com)$/.test(host)) return { kind: "backlog", label: KIND_LABEL.backlog };
  if (host.includes("gogs")) return { kind: "gogs", label: KIND_LABEL.gogs };
  if (host.includes("gitea")) return { kind: "gitea", label: KIND_LABEL.gitea };
  return { kind: "other", label: host };
}

/**
 * git の URL から「所有者/リポジトリ名」を取り出す。大文字小文字は元のまま。
 * https / ssh:// / scp 形式 (git@host:owner/name.git) に対応し、末尾 2 階層を使う
 */
export function repoPathOf(url: string): string {
  let path = url.trim();
  const scheme = path.match(/^[a-z][a-z0-9+.-]*:\/\/[^/]+\/(.*)$/i);
  if (scheme) path = scheme[1];
  else {
    // user@host:owner/name。C:\repos\x のようなドライブ文字 (1 文字 + :) は除く
    const scp = path.match(/^[^/\\:]+:(.+)$/);
    if (scp && !/^[a-z]:/i.test(path)) path = scp[1];
  }
  const segs = path
    .replace(/\.git\/?$/, "")
    .split(/[\\/]/)
    .filter(Boolean);
  return segs.length >= 2 ? segs.slice(-2).join("/") : segs.join("/") || url;
}

/** git の URL からブラウザ用の URL を作る */
export function webUrlOf(url: string, key: string | null): string | null {
  // Backlog の git は <space>.git.backlog.jp、Web は <space>.backlog.jp/git/<project>/<repo>
  const bl = key?.match(/^([^.]+)\.git\.(backlog\.(?:jp|com))\/([^/]+)\/([^/]+)$/);
  if (bl) return `https://${bl[1]}.${bl[2]}/git/${bl[3]}/${bl[4]}`;
  if (/^https?:\/\//.test(url)) {
    // 認証情報と末尾の .git を落とす
    return url.replace(/\/\/[^@/]+@/, "//").replace(/\.git$/, "").replace(/\/$/, "");
  }
  return key ? `https://${key}` : null;
}

/** ローカルのリポジトリの remote から。同じリポジトリを指す remote は 1 つにまとめる */
export function linksOfLocal(repo: LocalRepo, matched: RemoteRepo[], cfg: Config | null): RemoteLink[] {
  const out: RemoteLink[] = [];
  const seen = new Set<string>();
  // origin を先頭に
  const remotes = [...repo.remotes].sort((a, b) => Number(b.name === "origin") - Number(a.name === "origin"));
  for (const r of remotes) {
    const id = r.key ?? r.url;
    if (seen.has(id)) continue;
    seen.add(id);
    const host = r.key?.split("/")[0] ?? hostOf(r.url) ?? r.url;
    const known = matched.find((m) => m.key === r.key);
    out.push({
      ...kindOf(host, cfg),
      host,
      remoteName: r.name,
      url: r.url,
      path: known?.fullName || repoPathOf(r.url),
      webUrl: known?.htmlUrl ?? webUrlOf(r.url, r.key),
    });
  }
  return out;
}

/** リモート一覧にだけあるもの (未クローン) */
export function linkOfRemote(r: RemoteRepo, cfg: Config | null): RemoteLink {
  const host = r.key.split("/")[0];
  const account = cfg?.accounts.find((a) => a.id === r.accountId);
  const k = account ? { kind: account.kind as RemoteKind, label: KIND_LABEL[account.kind] } : kindOf(host, cfg);
  return {
    ...k,
    host,
    remoteName: null,
    url: r.cloneUrl ?? r.sshUrl ?? `https://${r.key}`,
    path: r.fullName || repoPathOf(r.cloneUrl ?? r.key),
    webUrl: r.htmlUrl ?? `https://${r.key}`,
  };
}
