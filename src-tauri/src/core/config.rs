use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::{gh, secrets, util};

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
    /// プロジェクトに付けたタグ。キーは hidden と同じ。タグは "仕事/客先/案件" のように / で 3 階層まで
    pub tags: BTreeMap<String, Vec<String>>,
    /// 作ったタグの一覧。プロジェクトが付いていなくても、削除するまで残す
    pub tag_defs: Vec<String>,
    /// スター (お気に入り) を付けたプロジェクト。キーは hidden と同じ
    pub starred: Vec<String>,
    /// SessionVault の場所。sessionvault.exe か、Claude History Viewer のフォルダ (同梱の SessionVault を Python で呼ぶ)。
    /// None なら PATH の sessionvault.exe
    pub sessionvault_path: Option<String>,
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
    /// 認証の方法。"token" (資格情報マネージャーのトークン) / "gh" (GitHub CLI のログインを借りる。GitHub のみ)
    pub auth: String,
    /// 画面から受け取る新しいトークン (書き込み専用)。保存時に資格情報マネージャーへ移し、
    /// 設定ファイルには書かない。画面にも返さない
    #[serde(skip_serializing_if = "String::is_empty")]
    pub token: String,
    /// 資格情報マネージャーにトークンがあるか
    pub has_token: bool,
    /// 画面からの「トークンを消す」指示 (保存時に処理して false に戻す)
    #[serde(skip_serializing_if = "is_false")]
    pub clear_token: bool,
    pub enabled: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Account {
    /// API を呼ぶときのトークン。資格情報マネージャーから読む。
    /// 資格情報マネージャーに手で登録したものも使えるよう、has_token に関係なく探す
    pub fn resolve_token(&self) -> Result<String, String> {
        if self.kind == "github" && self.auth == "gh" {
            return gh::token(&gh::host_of(&self.base_url));
        }
        if !self.token.trim().is_empty() {
            return Ok(self.token.trim().to_string());
        }
        match secrets::get(&secrets::target(&self.id))? {
            Some(t) => Ok(t),
            None if self.has_token => {
                Err("資格情報マネージャーにトークンがありません。設定で入れ直してください".into())
            }
            None => Ok(String::new()),
        }
    }
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
            tags: BTreeMap::new(),
            tag_defs: vec![],
            starred: vec![],
            sessionvault_path: None,
        }
    }
}

impl Config {
    /// 無ければ既定値。読めないときは既定値で起動するが、次の保存で上書きして
    /// トークンなどを失わないよう、元のファイルを config.broken.json に退避する。
    pub fn load(path: &Path) -> Config {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Config::default();
        };
        match serde_json::from_str(text.trim_start_matches('\u{feff}')) {
            Ok(c) => c,
            Err(e) => {
                let backup = path.with_file_name("config.broken.json");
                let _ = std::fs::copy(path, &backup);
                eprintln!("{} を読めません ({e})。{} に退避しました", path.display(), backup.display());
                Config::default()
            }
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    /// 画面から来たトークンの変更を資格情報マネージャーに反映し、設定からはトークンを消す。
    /// prev にあって self に無いアカウントのトークンも消す。
    pub fn store_secrets(&mut self, prev: &Config) -> Result<(), String> {
        for a in &mut self.accounts {
            let target = secrets::target(&a.id);
            if a.clear_token {
                secrets::delete(&target)?;
                a.has_token = false;
            } else if !a.token.trim().is_empty() {
                let user = if a.user.is_empty() { &a.label } else { &a.user };
                secrets::set(&target, user, a.token.trim())?;
                a.has_token = true;
            }
            a.token.clear();
            a.clear_token = false;
        }
        for old in &prev.accounts {
            if !self.accounts.iter().any(|a| a.id == old.id) {
                secrets::delete(&secrets::target(&old.id))?;
            }
        }
        Ok(())
    }

    /// 以前の版で平文のトークンが入っていれば、資格情報マネージャーへ移す。移したら true
    pub fn migrate_plaintext_tokens(&mut self) -> Result<bool, String> {
        if !self.accounts.iter().any(|a| !a.token.trim().is_empty()) {
            return Ok(false);
        }
        let prev = self.clone();
        self.store_secrets(&prev)?;
        Ok(true)
    }

    /// 画面に渡す形。トークンは含めない。has_token は資格情報マネージャーに実際にあるかで決める
    /// (手で登録した・手で消した場合も画面の表示が合うように)
    pub fn for_view(&self) -> Config {
        let mut c = self.clone();
        for a in &mut c.accounts {
            a.token.clear();
            a.clear_token = false;
            // 認証の方法を選べるようになる前のアカウントはトークン
            if a.auth.is_empty() {
                a.auth = "token".into();
            }
            if let Ok(found) = secrets::get(&secrets::target(&a.id)) {
                a.has_token = found.is_some();
            }
        }
        c
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
        // macOS でよく使われる場所
        if cfg!(target_os = "macos") {
            cands.push(h.join("Developer"));
            cands.push(h.join("Projects"));
            cands.push(h.join("code"));
        }
    }
    if cfg!(windows) {
        for d in ['C', 'D', 'E', 'F'] {
            cands.push(PathBuf::from(format!("{d}:\\Repos")));
        }
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

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    fn account(id: &str, token: &str) -> Account {
        Account { id: id.into(), kind: "github".into(), token: token.into(), enabled: true, ..Default::default() }
    }

    #[test]
    fn plaintext_token_moves_to_secret_store_and_leaves_file() {
        let id = "test-migrate";
        let target = secrets::target(id);
        let mut cfg = Config { accounts: vec![account(id, "ghp_plain")], ..Default::default() };

        assert!(cfg.migrate_plaintext_tokens().unwrap());
        assert!(cfg.accounts[0].has_token);
        assert_eq!(cfg.accounts[0].token, "");
        assert_eq!(secrets::get(&target).unwrap().as_deref(), Some("ghp_plain"));
        assert_eq!(cfg.accounts[0].resolve_token().unwrap(), "ghp_plain");

        // 設定ファイルにはトークンも clearToken も書かれない
        let dir = std::env::temp_dir().join("repotether-config-test");
        let path = dir.join("config.json");
        cfg.save(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("ghp_plain"));
        assert!(!text.contains("\"token\""));
        assert!(!text.contains("clearToken"));
        assert!(Config::load(&path).accounts[0].has_token);

        // 消す指示
        let prev = cfg.clone();
        cfg.accounts[0].clear_token = true;
        cfg.store_secrets(&prev).unwrap();
        assert!(!cfg.accounts[0].has_token);
        assert_eq!(secrets::get(&target).unwrap(), None);

        // アカウントを消したらトークンも消える
        let mut with = Config { accounts: vec![account(id, "ghp_again")], ..Default::default() };
        with.store_secrets(&Config::default()).unwrap();
        let mut without = Config { accounts: vec![], ..Default::default() };
        without.store_secrets(&with).unwrap();
        assert_eq!(secrets::get(&target).unwrap(), None);

        let _ = std::fs::remove_dir_all(dir);
    }
}
