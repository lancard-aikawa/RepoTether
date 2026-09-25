//! GitHub / Gogs / Gitea からリポジトリ一覧を取る。
//!
//! トークンがあれば自分が見られる全リポジトリ (`/user/repos`)、
//! 無ければ user の公開リポジトリ (`/users/{user}/repos`) を取る。
//! Gogs の API は Gitea と同じ形 (`/api/v1/...`) で、`Authorization: token <t>` を受け付ける。

use serde_json::Value;

use super::config::Account;
use super::model::RemoteRepo;
use super::util;

const PAGE_LIMIT: usize = 50;
const MAX_PAGES: usize = 40;

pub async fn fetch(account: &Account) -> Result<Vec<RemoteRepo>, String> {
    let client = reqwest::Client::builder()
        .user_agent("RepoTether")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let token = account.token.trim();
    let user = account.user.trim();
    if token.is_empty() && user.is_empty() {
        return Err("トークンかユーザー名のどちらかが必要です".into());
    }

    let (base, kind) = match account.kind.as_str() {
        "github" => {
            let b = account.base_url.trim().trim_end_matches('/');
            (if b.is_empty() { "https://api.github.com".to_string() } else { b.to_string() }, Kind::GitHub)
        }
        "gogs" | "gitea" => {
            let b = account.base_url.trim().trim_end_matches('/');
            if b.is_empty() {
                return Err("サーバーの URL が必要です".into());
            }
            (format!("{b}/api/v1"), Kind::Gitea)
        }
        k => return Err(format!("未対応の種類です: {k}")),
    };

    let list_path = if token.is_empty() {
        format!("{base}/users/{user}/repos")
    } else {
        format!("{base}/user/repos")
    };

    let mut repos: Vec<RemoteRepo> = vec![];
    let mut seen = std::collections::HashSet::new();
    for page in 1..=MAX_PAGES {
        let mut req = client.get(&list_path);
        req = match kind {
            Kind::GitHub => {
                let mut q = vec![("per_page", "100".to_string()), ("page", page.to_string())];
                if !token.is_empty() {
                    q.push(("affiliation", "owner,collaborator,organization_member".into()));
                }
                req.query(&q).header("Accept", "application/vnd.github+json")
            }
            Kind::Gitea => req.query(&[("page", page.to_string()), ("limit", PAGE_LIMIT.to_string())]),
        };
        if !token.is_empty() {
            req = match kind {
                Kind::GitHub => req.bearer_auth(token),
                Kind::Gitea => req.header("Authorization", format!("token {token}")),
            };
        }
        let resp = req.send().await.map_err(|e| format!("接続できません: {e}"))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            let hint = match status.as_u16() {
                401 | 403 if token.is_empty() => " (このサーバーはトークンが必要です)",
                401 => " (トークンが無効か期限切れです)",
                404 if token.is_empty() => " (ユーザー名か URL を確認してください)",
                _ => "",
            };
            return Err(format!("{status}{hint}: {}", util::truncate_chars(body.trim(), 200)));
        }
        let items: Vec<Value> = resp.json().await.map_err(|e| format!("応答を読めません: {e}"))?;
        let n = items.len();
        let mut added = 0;
        for v in items {
            if let Some(r) = to_repo(&account.id, &v) {
                if seen.insert(r.key.clone()) {
                    repos.push(r);
                    added += 1;
                }
            }
        }
        // Gogs はページ指定を無視して毎回全件を返すので、新しいものが無ければ終わる
        let page_size = if matches!(kind, Kind::GitHub) { 100 } else { PAGE_LIMIT };
        if n < page_size || added == 0 {
            break;
        }
    }
    Ok(repos)
}

#[derive(Clone, Copy)]
enum Kind {
    GitHub,
    Gitea,
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(|x| x.as_str()).filter(|x| !x.is_empty()).map(|x| x.to_string())
}

fn b(v: &Value, k: &str) -> bool {
    v.get(k).and_then(|x| x.as_bool()).unwrap_or(false)
}

fn to_repo(account_id: &str, v: &Value) -> Option<RemoteRepo> {
    let clone_url = s(v, "clone_url");
    let html_url = s(v, "html_url");
    let key = clone_url
        .as_deref()
        .and_then(util::remote_key)
        .or_else(|| html_url.as_deref().and_then(util::remote_key))?;
    let full_name = s(v, "full_name").unwrap_or_default();
    let owner = v
        .get("owner")
        .and_then(|o| s(o, "login").or_else(|| s(o, "username")))
        .or_else(|| full_name.split('/').next().map(|x| x.to_string()))
        .unwrap_or_default();
    Some(RemoteRepo {
        account_id: account_id.to_string(),
        key,
        name: s(v, "name").unwrap_or_default(),
        full_name,
        owner,
        description: s(v, "description"),
        html_url,
        clone_url,
        ssh_url: s(v, "ssh_url"),
        private: b(v, "private"),
        fork: b(v, "fork"),
        archived: b(v, "archived"),
        default_branch: s(v, "default_branch"),
        updated_at: s(v, "updated_at").or_else(|| s(v, "updated")),
        pushed_at: s(v, "pushed_at"),
    })
}
