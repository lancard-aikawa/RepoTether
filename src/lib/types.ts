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
  /** ログのあるフォルダの名前 (~/.claude/projects/<これ>/)。SessionVault の検査に渡す */
  projectDir: string | null;
}

/** SessionVault の verify が返す 1 件 */
export interface SessionFinding {
  session: string | null;
  /** ログのあるフォルダの名前 */
  project: string;
  path: string;
  /** bad-json / truncated-tail / no-newline / dangling-parent / duplicate-uuid / diverged / src-missing / unreadable */
  check: string;
  severity: "error" | "warning" | "info";
  line: number | null;
  detail: string;
}

/** セッションの会話の 1 件 (全文を見る) */
export interface TranscriptEntry {
  /** summary は文脈が長くなったときに Claude Code が書いた要約 (人の発言ではない) */
  role: "user" | "assistant" | "summary";
  at: string | null;
  text: string;
  /** Claude が使ったツールの名前 */
  tools: string[];
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
  /** sessionvault.exe か Claude History Viewer のフォルダ。null なら PATH の sessionvault.exe */
  sessionvaultPath: string | null;
  /** LockWatch のリポジトリのフォルダ。null なら使わない (脆弱性のタブを出さない) */
  lockwatchPath: string | null;
  /** 外部ツール。VS Code などのボタンの横に並べ、プロジェクトのフォルダを渡して起動する */
  externalTools: ExternalTool[];
}

export interface ExternalTool {
  id: string;
  /** ボタンに出す名前 */
  label: string;
  /** プログラムの場所 (PATH にあれば名前だけでもよい) */
  command: string;
  /** 引数。{path} がフォルダのパスになる。{path} が無ければ最後にフォルダのパスを足す */
  args: string;
}

// ---- LockWatch (src-tauri/src/core/lockwatch.rs) ----

/** critical / high / medium / low / unknown */
export type VulnSeverity = "critical" | "high" | "medium" | "low" | "unknown";

/** LockWatch の結果の脆弱性 1 件 */
export interface VulnFinding {
  lockfile: string;
  ecosystem: string;
  package: string;
  version: string;
  id: string;
  aliases: string[];
  severity: VulnSeverity;
  score: number | null;
  fixed: string[];
  /** 脆弱性ではない知らせの種類 (unmaintained / unsound / notice)。知らせでなければ null */
  informational: string | null;
  /** 悪意あるコードの記録 (OSV の MAL-) か */
  malicious: boolean;
  summary: string;
}

export interface VulnRepoResult {
  status: "ok" | "no-lockfile" | "error";
  mode: "online" | "offline";
  scannedAt: string | null;
  lockfiles: string[];
  findings: VulnFinding[];
  /** lock ファイルの健全性の注意 (脆弱性とは別)。古い LockWatch の結果では空 */
  notices: VulnNotice[];
  error: string | null;
}

/** LockWatch が lock ファイルそのものを読んで分かったこと */
export interface VulnNotice {
  lockfile: string;
  /** unpinned / not-registry / no-integrity / recent */
  kind: string;
  package: string;
  version: string;
  /** kind ごとの中身 (書かれている版の指定、取得元、件数、公開の時刻) */
  detail: string;
  /** 前回から新しく出たものか */
  fresh: boolean;
}

export interface RepoVulns {
  /** LockWatch での id */
  targetId: string;
  visibility: "public" | "private" | "unknown";
  /** まだ照合していなければ null */
  result: VulnRepoResult | null;
  /** 前回から新しく出たもの [package, id] */
  new: [string, string][];
}

/** LockWatch が使える状態か (lockwatch status --json) */
export interface LockwatchStatus {
  /** LockWatch の版 */
  lockwatch: string;
  osvScanner: { path: string | null; version: string | null; error: string | null };
  dataDir: string;
  targets: string;
  targetsCount: number | null;
  targetsError: string | null;
  /** 最後の照合。まだなら null */
  latest: { scannedAt: string | null; repos: number; errors: number } | null;
  /** 生態系 → 手元の脆弱性 DB を取った時刻 */
  db: Record<string, string>;
  /** registered は Windows 以外・調べられないときは null */
  task: { name: string; registered: boolean | null };
}

/** 「今すぐ調べる」の事前チェック (LockWatch の scan --check) の答え */
export interface VulnCheck {
  id: string;
  mode: "online" | "offline";
  /** 今照合したら前回の結果がそのまま返る */
  cached: boolean;
  /** cached のとき、その結果を作った時刻 */
  scannedAt: string | null;
  lockfiles: string[];
  reason: "cached" | "no-cache" | "db-update" | "no-lockfile" | "error";
  error: string | null;
}

export interface VulnReport {
  /** 設定で LockWatch の場所が決まっているか */
  configured: boolean;
  /** LockWatch を呼べない・結果を読めないときの理由 */
  error: string | null;
  locations: { dataDir: string; targets: string } | null;
  scannedAt: string | null;
  osvScanner: string | null;
  dbDownloadedAt: string | null;
  /** LocalRepo.id → そのリポジトリの結果 */
  byRepo: Record<string, RepoVulns>;
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
