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
  /** 置いてあるファイルから見当を付けた言語 (GitHub の表記)。主なものが先。null は未調査 */
  languages?: string[] | null;
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
  /** リモートが判定した主な言語 (Gogs は返さない) */
  language?: string | null;
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
  /** ToolKeeper のリポジトリのフォルダ。null なら使わない (道具のタブには使い始め方だけを出す) */
  toolkeeperPath: string | null;
  /** 外部ツール。VS Code などのボタンの横に並べ、プロジェクトのフォルダを渡して起動する */
  externalTools: ExternalTool[];
  /** プロジェクトの設定キー (Project.prefKey) -> 関連ページ (サービスの管理画面など) */
  links: Record<string, Link[]>;
  /** 社員向けの脆弱性の一覧に載せる場所。ホスト (gogs.example.com) かホスト/所有者 (github.com/example) */
  shareLocations: string[];
  /** 廃止の印を付けたプロジェクト (LocalRepo.id)。LockWatch で照合しない */
  retired: string[];
  /** 確定した言語の版。LocalRepo.id -> 言語 (php / node) -> 版 */
  runtimes: Record<string, Record<string, string>>;
}

export interface Link {
  title: string;
  url: string;
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
  /** 脆弱性の発行日 (OSV の published)。古い LockWatch の結果では null */
  published: string | null;
}

export interface VulnRepoResult {
  status: "ok" | "no-lockfile" | "error";
  mode: "online" | "offline";
  scannedAt: string | null;
  lockfiles: string[];
  findings: VulnFinding[];
  /** lock ファイルの健全性の注意 (脆弱性とは別)。古い LockWatch の結果では空 */
  notices: VulnNotice[];
  /** 言語そのもの (PHP・Node.js) の版。古い LockWatch の結果では空 */
  runtimes: VulnRuntime[];
  error: string | null;
}

/** 言語そのものの版の結果 (LockWatch の design.md §3.6) */
export interface VulnRuntime {
  /** php / node */
  name: string;
  version: string;
  /** どこから (ファイルの名前か「RepoTether で確定」) */
  source: string;
  confirmed: boolean;
  /** composer の require.php などの下限から推したもの */
  lowerBound: boolean;
  cycle: string | null;
  eol: string | null;
  status: "ok" | "soon" | "ended" | "unknown";
  latest: string | null;
  exact: boolean;
  /** 同じ系列で、この版より新しいセキュリティリリース */
  security: { version: string; date: string | null }[];
  upgradeTo: string | null;
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
  /** 社員向けの一覧での見出し。載せないなら null */
  share: string | null;
  /** 廃止 (照合しない)。廃止のものは result が null */
  retired: boolean;
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

/** ToolKeeper が使える状態か (toolkeeper status --json。ToolKeeper の design.md §9.2) */
export interface ToolkeeperStatus {
  /** ToolKeeper の版 */
  toolkeeper: string;
  /** ToolKeeper が書く check.json の形 */
  schema: number;
  dataDir: string;
  /** 最後に調べた時刻。まだなら null */
  lastCheck: string | null;
  task: { registered: boolean };
  sources: { winget: string | null; choco: string | null; cygwin: string | null; ghostscript: boolean };
}

/** 道具 1 つ (ToolKeeper の check.json の items。design.md §9.1) */
export interface ToolItem {
  /** winget:<ID>・choco:<名前>・php・ghostscript・cygwin */
  key: string;
  /** A 専用の処理・B winget の開発の道具・C winget の残り・D choco・E 大きい版の乗り換え */
  tier: "A" | "B" | "C" | "D" | "E";
  name: string;
  installed: string | null;
  latest: string | null;
  /** latest の公開日 (YYYY-MM-DD。ToolKeeper 0.2.0 から)。分からなければ null */
  latestDate: string | null;
  updatable: boolean;
  eol: { product: string; cycle: string; date: string | null; state: "ended" | "soon" | "ok" } | null;
  /** 前回調べたときに無かった更新 */
  new: boolean;
  /** twin:<key>・pinned・pinned:<件数>・running:<名前>・manual・old:<版>・same */
  notes: string[];
  packages: { installed: number; outdated: number; names: string[] } | null;
}

export interface ToolCheck {
  schema: number;
  toolkeeper: string;
  checkedAt: string;
  /** env は環境変数と PATH で「切れている」と注意の数 (ToolKeeper 0.2.0 から) */
  summary: { updatable: number; eol: number; new: number; failed: number; env: number };
  items: ToolItem[];
  failed: { source: string; reason: string }[];
}

export interface ToolsReport {
  /** 設定で ToolKeeper の場所が決まっているか */
  configured: boolean;
  /** ToolKeeper を呼べない・結果を読めないときの理由 */
  error: string | null;
  /** まだ調べていなければ null */
  check: ToolCheck | null;
}

/** 前回の「影響を調べる」の HTML (LockWatch の assess --last の答え) */
export interface Assessed {
  path: string;
  exists: boolean;
  /** ファイルの更新時刻。無ければ null */
  madeAt: string | null;
  /** そのとき渡した脆弱性と今の結果の中身が違うか (照合の時刻だけの違いは false)。比べられなければ null */
  changed: boolean | null;
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
