//! 1 つのリポジトリから状態とコミットを読む。git CLI を呼ぶ (libgit2 は使わない)。

use std::path::Path;

use super::discover;
use super::model::{BranchInfo, Commit, LocalRepo, Remote};
use super::util::{self, git};

/// 状態のファイル更新時刻を調べる上限 (未追跡が大量にあるリポジトリ対策)
const MAX_STAT_FILES: usize = 300;

pub fn inspect(path: &Path, history_days: u32) -> (LocalRepo, Vec<Commit>) {
    let display = util::display_path(&path.to_string_lossy());
    let mut repo = LocalRepo {
        id: util::path_key(&display),
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| display.clone()),
        path: display,
        ..Default::default()
    };

    if let Err(e) = read_status(path, &mut repo) {
        repo.error = Some(e);
        return (repo, vec![]);
    }
    repo.remotes = read_remotes(path);
    read_branches(path, &mut repo);
    repo.last_fetch_at = discover::git_dir(path)
        .and_then(|d| util::mtime(&d.join("FETCH_HEAD")))
        .map(util::system_time_to_rfc3339);

    let commits = read_log(path, &repo.id, history_days).unwrap_or_default();
    (repo, commits)
}

/// `git status --porcelain=v2 --branch --show-stash`
fn read_status(path: &Path, repo: &mut LocalRepo) -> Result<(), String> {
    let out = git(
        path,
        &["status", "--porcelain=v2", "--branch", "--show-stash", "--untracked-files=normal"],
    )?;
    let mut changed: Vec<String> = vec![];
    for line in out.lines() {
        if let Some(h) = line.strip_prefix("# ") {
            let (k, v) = h.split_once(' ').unwrap_or((h, ""));
            match k {
                "branch.oid" if v != "(initial)" => repo.head = Some(v.to_string()),
                "branch.head" if v != "(detached)" => repo.branch = Some(v.to_string()),
                "branch.upstream" => repo.upstream = Some(v.to_string()),
                "branch.ab" => {
                    for part in v.split_whitespace() {
                        if let Some(n) = part.strip_prefix('+') {
                            repo.ahead = n.parse().unwrap_or(0);
                        } else if let Some(n) = part.strip_prefix('-') {
                            repo.behind = n.parse().unwrap_or(0);
                        }
                    }
                }
                "stash" => repo.stashes = v.parse().unwrap_or(0),
                _ => {}
            }
            continue;
        }
        let mut it = line.splitn(2, ' ');
        let kind = it.next().unwrap_or("");
        let rest = it.next().unwrap_or("");
        match kind {
            "1" | "2" => {
                let xy: Vec<char> = rest.chars().take(2).collect();
                if xy.first().is_some_and(|c| *c != '.') {
                    repo.staged += 1;
                }
                if xy.get(1).is_some_and(|c| *c != '.') {
                    repo.modified += 1;
                }
                // 1: XY sub mH mI mW hH hI path / 2: ... Xscore path<TAB>orig
                let fields = if kind == "1" { 8 } else { 9 };
                if let Some(p) = rest.splitn(fields, ' ').nth(fields - 1) {
                    changed.push(p.split('\t').next().unwrap_or(p).to_string());
                }
            }
            "u" => {
                repo.conflicted += 1;
                if let Some(p) = rest.splitn(10, ' ').nth(9) {
                    changed.push(p.to_string());
                }
            }
            "?" => {
                repo.untracked += 1;
                changed.push(rest.to_string());
            }
            _ => {}
        }
    }
    let newest = changed
        .iter()
        .take(MAX_STAT_FILES)
        .filter_map(|p| util::mtime(&path.join(p.trim_end_matches('/'))))
        .max();
    repo.dirty_modified_at = newest.map(util::system_time_to_rfc3339);
    Ok(())
}

fn read_remotes(path: &Path) -> Vec<Remote> {
    let Ok(out) = git(path, &["remote", "-v"]) else {
        return vec![];
    };
    let mut remotes: Vec<Remote> = vec![];
    for line in out.lines() {
        // origin\thttps://... (fetch)
        let Some((name, rest)) = line.split_once('\t') else { continue };
        if !rest.ends_with("(fetch)") {
            continue;
        }
        let url = rest.trim_end_matches("(fetch)").trim().to_string();
        remotes.push(Remote {
            name: name.to_string(),
            key: util::remote_key(&url),
            url,
        });
    }
    remotes
}

fn read_branches(path: &Path, repo: &mut LocalRepo) {
    let fmt = "%(refname)%09%(upstream:short)%09%(upstream:track)%09%(committerdate:iso-strict)%09%(symref)";
    let Ok(out) = git(path, &["for-each-ref", &format!("--format={fmt}"), "refs/heads", "refs/remotes"])
    else {
        return;
    };

    let mut locals: Vec<BranchInfo> = vec![];
    let mut origin_head: Option<String> = None;
    let mut remote_names: Vec<String> = vec![];
    for line in out.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 5 {
            continue;
        }
        let (refname, upstream, track, date, symref) = (f[0], f[1], f[2], f[3], f[4]);
        if let Some(name) = refname.strip_prefix("refs/heads/") {
            let (ahead, behind, gone) = parse_track(track);
            locals.push(BranchInfo {
                name: name.to_string(),
                upstream: (!upstream.is_empty()).then(|| upstream.to_string()),
                ahead,
                behind,
                gone,
                merged: false,
                last_commit_at: (!date.is_empty()).then(|| date.to_string()),
            });
        } else if let Some(name) = refname.strip_prefix("refs/remotes/") {
            if name.ends_with("/HEAD") {
                if name.starts_with("origin/") {
                    // refs/remotes/origin/main -> main
                    origin_head = symref
                        .strip_prefix("refs/remotes/origin/")
                        .map(|s| s.to_string());
                }
            } else {
                remote_names.push(name.to_string());
            }
        }
    }

    repo.last_commit_at = locals
        .iter()
        .filter_map(|b| b.last_commit_at.as_deref())
        .max_by(|a, b| compare_rfc3339(a, b))
        .map(|s| s.to_string());

    let has_local = |n: &str| locals.iter().any(|b| b.name == n);
    let default = origin_head
        .filter(|d| has_local(d) || remote_names.iter().any(|r| r == &format!("origin/{d}")))
        .or_else(|| ["main", "master", "develop"].iter().find(|n| has_local(n)).map(|s| s.to_string()))
        .or_else(|| repo.branch.clone());
    repo.default_branch = default.clone();

    // 既定ブランチに取り込み済みのブランチ。ローカルに既定ブランチが無ければ origin 側と比べる
    if let Some(d) = &default {
        let base = if has_local(d) { d.clone() } else { format!("origin/{d}") };
        if let Ok(merged) = git(
            path,
            &["for-each-ref", &format!("--merged={base}"), "--format=%(refname:short)", "refs/heads"],
        ) {
            let merged: Vec<&str> = merged.lines().collect();
            for b in &mut locals {
                b.merged = merged.contains(&b.name.as_str());
            }
        }
    }

    // 注意が要るものだけ残す: 既定ブランチ以外で、未マージ・未 push・upstream なし・upstream 消失
    repo.branches = locals
        .into_iter()
        .filter(|b| Some(&b.name) != default.as_ref())
        .filter(|b| !b.merged || b.ahead > 0 || b.gone)
        .collect();
}

/// "[ahead 1, behind 2]" / "[gone]" / ""
fn parse_track(track: &str) -> (u32, u32, bool) {
    let t = track.trim_matches(|c| c == '[' || c == ']');
    let mut ahead = 0;
    let mut behind = 0;
    let mut gone = false;
    for part in t.split(',') {
        let part = part.trim();
        if part == "gone" {
            gone = true;
        } else if let Some(n) = part.strip_prefix("ahead ") {
            ahead = n.parse().unwrap_or(0);
        } else if let Some(n) = part.strip_prefix("behind ") {
            behind = n.parse().unwrap_or(0);
        }
    }
    (ahead, behind, gone)
}

fn compare_rfc3339(a: &str, b: &str) -> std::cmp::Ordering {
    let pa = chrono::DateTime::parse_from_rfc3339(a).ok();
    let pb = chrono::DateTime::parse_from_rfc3339(b).ok();
    pa.cmp(&pb)
}

/// ブランチ・リモート追跡・タグから届くコミット。stash は含めない。
fn read_log(path: &Path, repo_id: &str, history_days: u32) -> Result<Vec<Commit>, String> {
    let since = format!("--since={history_days}.days");
    let out = git(
        path,
        &[
            "log",
            "--branches",
            "--remotes",
            "--tags",
            &since,
            "--format=%H%x1f%aI%x1f%an%x1f%ae%x1f%P%x1f%s%x1e",
        ],
    )?;
    let mut commits = vec![];
    for rec in out.split('\x1e') {
        let rec = rec.trim_matches(['\n', '\r']);
        let f: Vec<&str> = rec.split('\x1f').collect();
        if f.len() < 6 {
            continue;
        }
        commits.push(Commit {
            repo_id: repo_id.to_string(),
            hash: f[0].to_string(),
            at: f[1].to_string(),
            author_name: f[2].to_string(),
            author_email: f[3].to_string(),
            is_merge: f[4].split_whitespace().count() > 1,
            subject: f[5].to_string(),
        });
    }
    Ok(commits)
}

/// fetch 1 回の上限。接続できない古いサーバーを待ち続けない
const FETCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// `git fetch --all --prune`。裏で動くので、認証の画面やパスワードの入力は一切出さない
/// (出せないときは失敗にする)。FETCH_TIMEOUT を過ぎたら打ち切る。
pub fn fetch(path: &Path) -> Result<(), String> {
    let mut cmd = std::process::Command::new("git");
    cmd.arg("-C")
        .arg(path)
        .args(["-c", "credential.interactive=false", "fetch", "--all", "--prune", "--quiet"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never")
        .env("GIT_ASKPASS", "")
        .env("SSH_ASKPASS", "")
        .env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes -o ConnectTimeout=10")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());
    util::hide_window(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| format!("git を起動できません: {e}"))?;
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    return Ok(());
                }
                let mut err = String::new();
                if let Some(mut e) = child.stderr.take() {
                    use std::io::Read;
                    let _ = e.read_to_string(&mut err);
                }
                let first = err.lines().find(|l| !l.trim().is_empty()).unwrap_or("fetch に失敗しました");
                return Err(util::truncate_chars(first.trim(), 200));
            }
            Ok(None) if started.elapsed() > FETCH_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{} 秒で応答がないので打ち切りました", FETCH_TIMEOUT.as_secs()));
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// `git clone <url> <dest>`。認証は git の資格情報マネージャーに任せる。
pub fn clone(url: &str, dest: &Path) -> Result<(), String> {
    let ok_scheme = ["https://", "http://", "ssh://", "git://"].iter().any(|s| url.starts_with(s))
        || (url.contains('@') && util::remote_key(url).is_some() && !url.contains("://"));
    if url.starts_with('-') || !ok_scheme {
        return Err(format!("クローンできない URL です: {url}"));
    }
    if !dest.is_absolute() {
        return Err("クローン先は絶対パスで指定してください".into());
    }
    if dest.exists() {
        return Err(format!("{} は既にあります", dest.display()));
    }
    let parent = dest.parent().ok_or("クローン先が不正です")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut cmd = std::process::Command::new("git");
    cmd.arg("clone").arg("--").arg(url).arg(dest).env("GIT_TERMINAL_PROMPT", "0");
    util::hide_window(&mut cmd);
    let out = cmd.output().map_err(|e| format!("git を起動できません: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_parsing() {
        assert_eq!(parse_track("[ahead 2, behind 3]"), (2, 3, false));
        assert_eq!(parse_track("[gone]"), (0, 0, true));
        assert_eq!(parse_track(""), (0, 0, false));
    }
}
