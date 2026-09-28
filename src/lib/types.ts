// src-tauri/src/core/model.rs と config.rs の形をそのまま写したもの。

export interface Snapshot {
  generatedAt: string;
  remoteFetchedAt: string | null;
  repos: LocalRepo[];
  commits: Commit[];
  sessions: Session[];
  remoteRepos: RemoteRepo[];
  errors: SourceError[];
}

export interface LocalRepo {
  id: string;
  path: string;
  name: string;
  remotes: Remote[];
  branch: string | null;
  head: string | null;
  upstream: string | null;
  ahead: number;
  behind: number;
  defaultBranch: string | null;
  branches: BranchInfo[];
  staged: number;
  modified: number;
  untracked: number;
  conflicted: number;
  stashes: number;
  dirtyModifiedAt: string | null;
  lastCommitAt: string | null;
  lastFetchAt: string | null;
  error: string | null;
}

export interface Remote {
  name: string;
  url: string;
  key: string | null;
}

export interface BranchInfo {
  name: string;
  upstream: string | null;
  ahead: number;
  behind: number;
  gone: boolean;
  merged: boolean;
  lastCommitAt: string | null;
}

export interface Commit {
  repoId: string;
  hash: string;
  at: string;
  authorName: string;
  authorEmail: string;
  subject: string;
  isMerge: boolean;
}

export interface Session {
  id: string;
  cwd: string | null;
  repoId: string | null;
  entrypoint: string | null;
  interactive: boolean;
  startedAt: string | null;
  endedAt: string | null;
  title: string | null;
  firstPrompt: string | null;
  lastPrompt: string | null;
  lastReply: string | null;
  promptCount: number;
  promptTimes: string[];
  gitBranch: string | null;
}

export interface RemoteRepo {
  accountId: string;
  key: string;
  fullName: string;
  name: string;
  owner: string;
  description: string | null;
  htmlUrl: string | null;
  cloneUrl: string | null;
  sshUrl: string | null;
  private: boolean;
  fork: boolean;
  archived: boolean;
  defaultBranch: string | null;
  updatedAt: string | null;
  pushedAt: string | null;
}

export interface SourceError {
  source: string;
  message: string;
}

export interface Config {
  roots: string[];
  scanDepth: number;
  includeSessionFolders: boolean;
  claudeDir: string | null;
  historyDays: number;
  authorEmails: string[];
  accounts: Account[];
  cloneRoot: string | null;
  hidden: string[];
  /** プロジェクトの設定キー (Project.prefKey) -> タグ ("仕事/客先/案件" のように / で 3 階層まで) */
  tags: Record<string, string[]>;
  /** 作ったタグの一覧 (プロジェクトが無くても削除するまで残す) */
  tagDefs: string[];
  /** スター (お気に入り) を付けたプロジェクトの設定キー */
  starred: string[];
}

export type AccountKind = "github" | "gogs" | "gitea";

export interface Account {
  id: string;
  kind: AccountKind;
  label: string;
  baseUrl: string;
  user: string;
  /** "token": 資格情報マネージャーのトークン / "gh": GitHub CLI のログインを借りる (GitHub のみ) */
  auth: "token" | "gh";
  /** 新しく入れたトークン (書き込み専用)。バックエンドは返さない */
  token: string;
  /** OS の資格情報の保管庫にトークンがあるか */
  hasToken: boolean;
  /** 保存時にトークンを消す */
  clearToken: boolean;
  enabled: boolean;
}
