//! GitHub CLI (gh) にログイン済みなら、そのトークンを借りる。
//! トークンの保管と更新は gh に任せ、RepoTether には保存しない。

use std::process::Command;

use super::util;

/// アカウントの API の URL から、gh に渡すホスト名を決める。
/// 空・api.github.com なら github.com、GitHub Enterprise (https://ghe.example.com/api/v3) ならそのホスト
pub fn host_of(base_url: &str) -> String {
    let b = base_url.trim();
    if b.is_empty() {
        return "github.com".into();
    }
    let rest = b.split_once("://").map(|(_, r)| r).unwrap_or(b);
    let host = rest.split('/').next().unwrap_or(rest).split(':').next().unwrap_or(rest);
    if host.eq_ignore_ascii_case("api.github.com") {
        "github.com".into()
    } else {
        host.to_lowercase()
    }
}

fn gh(args: &[&str]) -> Result<std::process::Output, String> {
    let mut cmd = Command::new("gh");
    cmd.args(args);
    util::hide_window(&mut cmd);
    cmd.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "gh コマンドが見つかりません。GitHub CLI を入れて `gh auth login --web` を実行してください".to_string()
        } else {
            format!("gh を起動できません: {e}")
        }
    })
}

fn not_logged_in(host: &str, detail: &str) -> String {
    let d = util::truncate_chars(detail.trim(), 200);
    format!("gh で {host} にログインしていません。ターミナルで `gh auth login --web` を実行してください ({d})")
}

/// `gh auth token --hostname <host>`。トークンは呼び出し元で使うだけで、保存しない
pub fn token(host: &str) -> Result<String, String> {
    let out = gh(&["auth", "token", "--hostname", host])?;
    let t = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !out.status.success() || t.is_empty() {
        return Err(not_logged_in(host, &String::from_utf8_lossy(&out.stderr)));
    }
    Ok(t)
}

/// ログインしているアカウント名 (設定画面の確認用)。トークンは返さない
pub fn user(host: &str) -> Result<String, String> {
    let out = gh(&["auth", "status", "--hostname", host])?;
    // 版によって stdout / stderr のどちらに出るかが違うので両方を見る
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if !out.status.success() {
        return Err(not_logged_in(host, &text));
    }
    // "Logged in to github.com account NAME (keyring)" / 古い版は "Logged in to github.com as NAME"
    for line in text.lines() {
        let l = line.trim().trim_start_matches(['✓', '-', ' ']);
        if let Some(rest) = l.strip_prefix(&format!("Logged in to {host} ")) {
            let name = rest
                .trim_start_matches("account ")
                .trim_start_matches("as ")
                .split_whitespace()
                .next()
                .unwrap_or("");
            if !name.is_empty() {
                return Ok(name.to_string());
            }
        }
    }
    Err(not_logged_in(host, &text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_from_base_url() {
        assert_eq!(host_of(""), "github.com");
        assert_eq!(host_of("https://api.github.com"), "github.com");
        assert_eq!(host_of("https://GHE.example.com/api/v3/"), "ghe.example.com");
        assert_eq!(host_of("https://ghe.example.com:8443/api/v3"), "ghe.example.com");
    }
}
