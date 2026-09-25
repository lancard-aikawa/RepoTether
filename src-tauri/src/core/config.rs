use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::util;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    /// リポジトリを探すフォルダ
    pub roots: Vec<String>,
    /// roots から何階層下まで探すか
    pub scan_depth: u32,
    /// Claude のセッションで使ったフォルダが git リポジトリなら、roots の外でも対象にする
    pub include_session_folders: bool,
    /// Claude Code のログの場所。None なら ~/.claude/projects
    pub claude_dir: Option<String>,
    /// コミットを何日前まで読むか
    pub history_days: u32,
    /// 「自分のコミット」とみなすメールアドレス
    pub author_emails: Vec<String>,
    pub accounts: Vec<Account>,
    /// クローン先の既定の親フォルダ
    pub clone_root: Option<String>,
    /// 一覧から外すリポジトリ (LocalRepo.id または RemoteRepo.key)
    pub hidden: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Account {
    pub id: String,
    /// "github" / "gogs" / "gitea"
    pub kind: String,
    pub label: String,
    /// GitHub は空なら https://api.github.com。Gogs / Gitea はサーバーの URL (例: https://git.example.com)
    pub base_url: String,
    /// トークンなしで公開リポジトリだけ取るときのユーザー名
    pub user: String,
    /// アクセストークン。設定ファイルに平文で保存される
    pub token: String,
    pub enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            roots: default_roots(),
            scan_depth: 3,
            include_session_folders: true,
            claude_dir: None,
            history_days: 365,
            author_emails: global_git_email().into_iter().collect(),
            accounts: vec![],
            clone_root: None,
            hidden: vec![],
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Config {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    pub fn claude_projects_dir(&self) -> Option<PathBuf> {
        match &self.claude_dir {
            Some(d) if !d.trim().is_empty() => Some(PathBuf::from(d)),
            _ => util::home_dir().map(|h| h.join(".claude").join("projects")),
        }
    }
}

/// よくある置き場所のうち、実在するものを既定の探索先にする。
fn default_roots() -> Vec<String> {
    let mut cands: Vec<PathBuf> = vec![];
    if let Some(h) = util::home_dir() {
        cands.push(h.join("Repos"));
        cands.push(h.join("source").join("repos"));
        cands.push(h.join("src"));
    }
    for d in ['C', 'D', 'E', 'F'] {
        cands.push(PathBuf::from(format!("{d}:\\Repos")));
    }
    let mut out: Vec<String> = vec![];
    for c in cands {
        if c.is_dir() {
            let s = util::display_path(&c.to_string_lossy());
            if !out.iter().any(|o| util::path_key(o) == util::path_key(&s)) {
                out.push(s);
            }
        }
    }
    out
}

fn global_git_email() -> Option<String> {
    let mut cmd = std::process::Command::new("git");
    cmd.args(["config", "--global", "user.email"]);
    util::hide_window(&mut cmd);
    let out = cmd.output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}
