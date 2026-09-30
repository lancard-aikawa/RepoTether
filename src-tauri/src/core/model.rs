//! フロントエンドに渡すデータの形。JSON は camelCase。
//! 時刻はすべて RFC 3339 文字列 (git はオフセット付き、Claude のログは UTC)。
//! 日付への丸めや集計はフロントエンドでローカル時刻に直してから行う。

use serde::{Deserialize, Serialize};

/// 1 回の取り込み結果。アプリ起動時はキャッシュから読み、更新で作り直す。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub generated_at: String,
    /// リモート一覧を最後に取得した時刻。未取得なら None
    pub remote_fetched_at: Option<String>,
    pub repos: Vec<LocalRepo>,
    pub commits: Vec<Commit>,
    pub sessions: Vec<Session>,
    pub remote_repos: Vec<RemoteRepo>,
    pub errors: Vec<SourceError>,
}

/// ローカルにある git リポジトリ 1 つ。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalRepo {
    /// 正規化したパス (小文字・バックスラッシュ)。照合用のキー
    pub id: String,
    /// 表示用のパス
    pub path: String,
    pub name: String,
    pub remotes: Vec<Remote>,
    /// 現在のブランチ。detached のときは None
    pub branch: Option<String>,
    pub head: Option<String>,
    pub upstream: Option<String>,
    /// upstream に対して未 push のコミット数
    pub ahead: u32,
    /// upstream に対して未取り込みのコミット数 (最後の fetch 時点)
    pub behind: u32,
    pub default_branch: Option<String>,
    /// 既定ブランチ以外で、注意が要るローカルブランチ
    /// (未マージ・未 push・upstream なし・upstream 消失)
    pub branches: Vec<BranchInfo>,
    pub staged: u32,
    pub modified: u32,
    pub untracked: u32,
    pub conflicted: u32,
    pub stashes: u32,
    /// 変更中のファイルで一番新しい更新時刻
    pub dirty_modified_at: Option<String>,
    /// ローカルブランチの最新コミット時刻
    pub last_commit_at: Option<String>,
    /// .git/FETCH_HEAD の更新時刻 (ahead/behind の鮮度の目安)
    pub last_fetch_at: Option<String>,
    /// 読み取りに失敗したときの理由 (dubious ownership など)
    pub error: Option<String>,
    /// この状態を git から読んだ時刻。自動更新で読み直さなかったものは前回の時刻のまま
    pub inspected_at: Option<String>,
    /// 読んだときの .git の中の主なファイルの更新時刻。変わっていれば git の操作があったとみなして読み直す
    pub git_stamp: Option<String>,
}

/// 更新でどのリポジトリを git から読み直すか。読み直さないものは前回の結果を引き継ぐ
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Scope {
    /// すべて (手動の更新)
    All,
    /// 最近 days 日に作業したものと、.git の中が変わったものだけ (自動更新・起動時)
    Active { days: u32 },
    /// path の下にあるものだけ (フォルダ表示で階層を選んでの更新)
    Under { path: String },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Remote {
    pub name: String,
    pub url: String,
    /// host/owner/name (小文字)。リモート一覧との照合に使う
    pub key: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchInfo {
    pub name: String,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// upstream が設定されているが、リモートでは消えている
    pub gone: bool,
    /// 既定ブランチに取り込み済みか
    pub merged: bool,
    pub last_commit_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    pub repo_id: String,
    pub hash: String,
    /// author date
    pub at: String,
    pub author_name: String,
    pub author_email: String,
    pub subject: String,
    pub is_merge: bool,
}

/// Claude Code のセッション 1 つ (~/.claude/projects/*/<id>.jsonl)。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    /// セッションを始めたフォルダ
    pub cwd: Option<String>,
    /// cwd を含むリポジトリ。リポジトリ外なら None
    pub repo_id: Option<String>,
    /// claude-vscode / cli / sdk-py など
    pub entrypoint: Option<String>,
    /// 人が対話したセッションか (SDK からの自動実行は false)
    pub interactive: bool,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub title: Option<String>,
    pub first_prompt: Option<String>,
    pub last_prompt: Option<String>,
    /// 最後の Claude の返答 (先頭の数百文字)
    pub last_reply: Option<String>,
    pub prompt_count: u32,
    /// 人が入力したプロンプトの時刻。日ごとの集計に使う
    pub prompt_times: Vec<String>,
    pub git_branch: Option<String>,
}

/// セッションの会話の 1 件 (詳細の「全文を見る」)。Claude の返答は、続けて届いた記録を 1 件にまとめる
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptEntry {
    /// "user" / "assistant"
    pub role: String,
    pub at: Option<String>,
    pub text: String,
    /// Claude が使ったツールの名前 (Bash / Edit など)。同じ名前が続けば 1 つにまとめる
    pub tools: Vec<String>,
}

/// GitHub / Gogs / Gitea のリモートにあるリポジトリ。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteRepo {
    pub account_id: String,
    /// host/owner/name (小文字)
    pub key: String,
    pub full_name: String,
    pub name: String,
    pub owner: String,
    pub description: Option<String>,
    pub html_url: Option<String>,
    pub clone_url: Option<String>,
    pub ssh_url: Option<String>,
    pub private: bool,
    pub fork: bool,
    pub archived: bool,
    pub default_branch: Option<String>,
    pub updated_at: Option<String>,
    pub pushed_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceError {
    /// "remote:<account>" / "sessions" / "scan" など
    pub source: String,
    pub message: String,
}
