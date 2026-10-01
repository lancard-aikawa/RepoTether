//! データの取り込み。Tauri に依存しないので examples/dump.rs からも呼べる。

pub mod claude_settings;
pub mod config;
pub mod discover;
pub mod gh;
pub mod git;
pub mod icon;
pub mod model;
pub mod remote;
pub mod secrets;
pub mod sessions;
pub mod sessionvault;
pub mod util;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use config::Config;
use model::{Commit, LocalRepo, RemoteRepo, Scope, Session, Snapshot, SourceError};

/// ローカル側 (リポジトリ・コミット・セッション) を読み直す。リモート一覧は含めない。
/// progress には「何をしているか」の短い文を渡す。
/// fetch が真なら、状態を読む前に各リポジトリで git fetch する (ahead / behind を最新にする)。
/// scope で git から読み直すリポジトリを絞れる。読み直さないものは prev (前回の結果) から引き継ぐ。
/// セッションとリポジトリの一覧は毎回すべて読む (どちらも軽い)。
pub fn build_local(
    cfg: &Config,
    cache_dir: &Path,
    fetch: bool,
    scope: &Scope,
    prev: Option<&Snapshot>,
    progress: &(dyn Fn(String) + Sync),
) -> Snapshot {
    let mut errors: Vec<SourceError> = vec![];

    progress("Claude のセッションを読んでいます".into());
    let mut sessions: Vec<Session> = match cfg.claude_projects_dir() {
        Some(dir) if dir.is_dir() => sessions::load_all(&dir, &cache_dir.join("sessions-cache.json"))
            .unwrap_or_else(|e| {
                errors.push(SourceError { source: "sessions".into(), message: e });
                vec![]
            }),
        Some(dir) => {
            errors.push(SourceError {
                source: "sessions".into(),
                message: format!("{} がありません", dir.display()),
            });
            vec![]
        }
        None => vec![],
    };

    progress("リポジトリを探しています".into());
    let roots: Vec<PathBuf> = cfg.roots.iter().map(PathBuf::from).collect();
    for r in &roots {
        if !r.is_dir() {
            errors.push(SourceError {
                source: "scan".into(),
                message: format!("探索先 {} がありません", r.display()),
            });
        }
    }
    let mut paths: BTreeMap<String, PathBuf> = BTreeMap::new();
    for p in discover::find_repos(&roots, cfg.scan_depth) {
        paths.insert(util::path_key(&p.to_string_lossy()), p);
    }
    if cfg.include_session_folders {
        for s in &sessions {
            if let Some(repo) = s.cwd.as_deref().and_then(|c| discover::enclosing_repo(Path::new(c))) {
                paths.entry(util::path_key(&repo.to_string_lossy())).or_insert(repo);
            }
        }
    }

    let all: Vec<PathBuf> = paths.into_values().collect();
    // fetch は読み直すかどうかに関係なくすべてに対して先にする。fetch で FETCH_HEAD が変われば、
    // 下の振り分けで .git の変化として拾われて読み直しになる
    if fetch {
        let n = all.len();
        progress(format!("git fetch しています (0/{n})"));
        let failed = fetch_parallel(&all, &|done| progress(format!("git fetch しています ({done}/{n})")));
        for (p, e) in failed {
            errors.push(SourceError {
                source: "fetch".into(),
                message: format!("{}: {e}", util::display_path(&p.to_string_lossy())),
            });
        }
    }
    let (paths, kept) = split_by_scope(&all, scope, prev, &sessions, cfg.history_days);
    let total = paths.len();

    // 読み直さないものがあれば、その件数も出す
    let rest = if kept.is_empty() { String::new() } else { format!("。{} 件は前回のまま", kept.len()) };
    progress(format!("git の状態を読んでいます (0/{total}{rest})"));
    let results = inspect_parallel(&paths, cfg.history_days, &|done| {
        progress(format!("git の状態を読んでいます ({done}/{total}{rest})"));
    });

    let mut repos: Vec<LocalRepo> = vec![];
    let mut commits: Vec<Commit> = vec![];
    for (repo, cs) in results.into_iter().chain(kept) {
        repos.push(repo);
        commits.extend(cs);
    }
    repos.sort_by_key(|r| r.name.to_lowercase());

    // セッションの cwd を、それを含む一番深いリポジトリに割り当てる
    let mut ids: Vec<&str> = repos.iter().map(|r| r.id.as_str()).collect();
    ids.sort_by_key(|id| std::cmp::Reverse(id.len()));
    for s in &mut sessions {
        let Some(cwd) = s.cwd.as_deref() else { continue };
        let key = util::path_key(cwd);
        s.repo_id = ids
            .iter()
            .find(|id| util::is_under(&key, id))
            .map(|id| id.to_string());
    }
    sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    // git の日時は作者のオフセット付き (+09:00 / -05:00 など) なので、文字列ではなく時刻で比べる
    commits.sort_by_cached_key(|c| std::cmp::Reverse(chrono::DateTime::parse_from_rfc3339(&c.at).ok()));

    Snapshot {
        generated_at: chrono::Local::now().to_rfc3339(),
        remote_fetched_at: None,
        repos,
        commits,
        sessions,
        remote_repos: vec![],
        errors,
    }
}

/// scope に従って、git から読み直すパスと、前回の結果を引き継ぐもの (リポジトリとコミット) に分ける。
/// 前回の結果に無いもの (新しく見つかったもの) は必ず読む。
/// 引き継ぐコミットは、読み直したときと同じく履歴の期間 (history_days) で絞る
fn split_by_scope(
    paths: &[PathBuf],
    scope: &Scope,
    prev: Option<&Snapshot>,
    sessions: &[Session],
    history_days: u32,
) -> (Vec<PathBuf>, Vec<(LocalRepo, Vec<Commit>)>) {
    let Some(prev) = prev else { return (paths.to_vec(), vec![]) };
    // 画面のフォルダの ID は小文字なので、大文字・小文字を区別する OS でも比べられるようにそろえる
    let under = match scope {
        Scope::Under { path } => Some(util::path_key(path).to_lowercase()),
        _ => None,
    };
    let repos: HashMap<&str, &LocalRepo> = prev.repos.iter().map(|r| (r.id.as_str(), r)).collect();
    let mut commits: HashMap<&str, Vec<&Commit>> = HashMap::new();
    let history_from = chrono::Utc::now() - chrono::Duration::days(i64::from(history_days));
    for c in &prev.commits {
        let recent = chrono::DateTime::parse_from_rfc3339(&c.at).is_ok_and(|t| t.with_timezone(&chrono::Utc) >= history_from);
        if recent {
            commits.entry(c.repo_id.as_str()).or_default().push(c);
        }
    }
    let cutoff = match scope {
        Scope::Active { days } => Some(chrono::Utc::now() - chrono::Duration::days(i64::from(*days))),
        _ => None,
    };
    let mut read = vec![];
    let mut kept = vec![];
    for p in paths {
        let key = util::path_key(&p.to_string_lossy());
        let Some(&old) = repos.get(key.as_str()) else {
            read.push(p.clone());
            continue;
        };
        let keep = match scope {
            Scope::All => false,
            Scope::Under { .. } => !util::is_under(&key.to_lowercase(), under.as_deref().unwrap_or_default()),
            Scope::Active { .. } => {
                let cutoff = cutoff.expect("Active には cutoff がある");
                old.error.is_none()
                    && old.inspected_at.is_some()
                    && !worked_since(old, sessions, cutoff)
                    && git::stamp(p) == old.git_stamp
            }
        };
        if keep {
            let cs = commits.get(key.as_str()).map(|cs| cs.iter().map(|c| (*c).clone()).collect()).unwrap_or_default();
            kept.push((old.clone(), cs));
        } else {
            read.push(p.clone());
        }
    }
    (read, kept)
}

/// cutoff より後に作業したか (ローカルの最新コミット・未コミットのファイルの更新・そのフォルダでの Claude のセッション)
fn worked_since(repo: &LocalRepo, sessions: &[Session], cutoff: chrono::DateTime<chrono::Utc>) -> bool {
    let after = |s: Option<&str>| {
        s.and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .is_some_and(|t| t.with_timezone(&chrono::Utc) >= cutoff)
    };
    after(repo.last_commit_at.as_deref())
        || after(repo.dirty_modified_at.as_deref())
        || sessions.iter().any(|s| {
            s.cwd.as_deref().is_some_and(|c| util::is_under(&util::path_key(c), &repo.id))
                && (after(s.ended_at.as_deref()) || after(s.started_at.as_deref()))
        })
}

/// remote のあるリポジトリで並列に fetch する。失敗したものを返す
fn fetch_parallel(paths: &[PathBuf], on_done: &(dyn Fn(usize) + Sync)) -> Vec<(PathBuf, String)> {
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let failed: Mutex<Vec<(PathBuf, String)>> = Mutex::new(vec![]);
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 8);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                let Some(p) = paths.get(i) else { break };
                let has_remote = util::git(p, &["remote"]).map(|o| !o.trim().is_empty()).unwrap_or(false);
                if has_remote {
                    if let Err(e) = git::fetch(p) {
                        failed.lock().unwrap().push((p.clone(), e));
                    }
                }
                on_done(done.fetch_add(1, Ordering::SeqCst) + 1);
            });
        }
    });
    let mut out = failed.into_inner().unwrap();
    out.sort();
    out
}

fn inspect_parallel(
    paths: &[PathBuf],
    history_days: u32,
    on_done: &(dyn Fn(usize) + Sync),
) -> Vec<(LocalRepo, Vec<Commit>)> {
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let out: Mutex<Vec<(LocalRepo, Vec<Commit>)>> = Mutex::new(Vec::with_capacity(paths.len()));
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 16);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                let Some(p) = paths.get(i) else { break };
                let r = git::inspect(p, history_days);
                out.lock().unwrap().push(r);
                on_done(done.fetch_add(1, Ordering::SeqCst) + 1);
            });
        }
    });
    out.into_inner().unwrap()
}

/// 有効なアカウントすべてからリモート一覧を取る。失敗したアカウントは errors に入れて続ける。
pub async fn fetch_remotes(cfg: &Config) -> (Vec<RemoteRepo>, Vec<SourceError>) {
    let mut repos = vec![];
    let mut errors = vec![];
    for a in cfg.accounts.iter().filter(|a| a.enabled) {
        // トークンは資格情報マネージャーから、呼ぶ直前に読む
        let result = match a.resolve_token() {
            Ok(token) => remote::fetch(&config::Account { token, ..a.clone() }).await,
            Err(e) => Err(e),
        };
        match result {
            Ok(rs) => repos.extend(rs),
            Err(e) => errors.push(SourceError {
                source: format!("remote:{}", a.id),
                message: format!("{}: {e}", if a.label.is_empty() { &a.kind } else { &a.label }),
            }),
        }
    }
    (repos, errors)
}

/// ローカルの読み直し結果に、前回のリモート一覧を引き継ぐ。
pub fn carry_remote(mut snap: Snapshot, prev: Option<&Snapshot>) -> Snapshot {
    if let Some(p) = prev {
        snap.remote_repos = p.remote_repos.clone();
        snap.remote_fetched_at = p.remote_fetched_at.clone();
        snap.errors.extend(p.errors.iter().filter(|e| e.source.starts_with("remote:")).cloned());
    }
    snap
}

pub fn load_snapshot(path: &Path) -> Option<Snapshot> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

pub fn save_snapshot(path: &Path, snap: &Snapshot) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, serde_json::to_string(snap).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}


#[cfg(test)]
mod tests {
    use super::*;

    /// .git/HEAD だけある、見かけ上のリポジトリを作る
    fn fake_repo(root: &Path, name: &str) -> PathBuf {
        let p = root.join(name);
        std::fs::create_dir_all(p.join(".git")).unwrap();
        std::fs::write(p.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        p
    }

    fn old_repo(p: &Path, last_commit: &str) -> LocalRepo {
        LocalRepo {
            id: util::path_key(&p.to_string_lossy()),
            path: p.to_string_lossy().into_owned(),
            last_commit_at: Some(last_commit.into()),
            inspected_at: Some("2026-01-01T00:00:00+09:00".into()),
            git_stamp: git::stamp(p),
            ..Default::default()
        }
    }

    fn keys(ps: &[PathBuf]) -> Vec<String> {
        ps.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn scope_decides_what_to_read() {
        let root = std::env::temp_dir().join("repotether-scope-test");
        let _ = std::fs::remove_dir_all(&root);
        let dormant = fake_repo(&root, "dormant");
        let recent = fake_repo(&root.join("sub"), "recent");
        let touched = fake_repo(&root, "touched");
        let chatted = fake_repo(&root, "chatted");
        let fresh = fake_repo(&root, "fresh");
        let long_ago = "2020-01-01T00:00:00+09:00";
        let now = chrono::Utc::now().to_rfc3339();
        let prev = Snapshot {
            repos: vec![
                old_repo(&dormant, long_ago),
                old_repo(&recent, &now),
                old_repo(&touched, long_ago),
                old_repo(&chatted, long_ago),
            ],
            commits: vec![
                Commit { repo_id: util::path_key(&dormant.to_string_lossy()), at: now.clone(), ..Default::default() },
                // 履歴の期間 (365 日) より古いものは引き継がない
                Commit { repo_id: util::path_key(&dormant.to_string_lossy()), at: long_ago.into(), ..Default::default() },
            ],
            ..Default::default()
        };
        // 前回読んだあとに git の操作があった
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(touched.join(".git/HEAD"), "ref: refs/heads/other\n").unwrap();
        // 最近 Claude と作業した (サブフォルダで)
        let sessions = vec![Session {
            cwd: Some(chatted.join("src").to_string_lossy().into_owned()),
            started_at: Some(now.clone()),
            ..Default::default()
        }];
        let paths = vec![dormant.clone(), recent.clone(), touched.clone(), chatted.clone(), fresh.clone()];

        let (read, kept) = split_by_scope(&paths, &Scope::Active { days: 90 }, Some(&prev), &sessions, 365);
        assert_eq!(keys(&read), ["recent", "touched", "chatted", "fresh"]);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].1.len(), 1, "引き継ぐリポジトリのコミットも、履歴の期間の分だけ引き継ぐ");

        let (read, kept) = split_by_scope(&paths, &Scope::All, Some(&prev), &sessions, 365);
        assert_eq!(read.len(), 5);
        assert!(kept.is_empty());

        // sub の下だけ。前回に無いもの (fresh) は範囲の外でも読む。画面から来るパスは小文字
        let under = Scope::Under { path: root.join("sub").to_string_lossy().to_lowercase() };
        let (read, kept) = split_by_scope(&paths, &under, Some(&prev), &sessions, 365);
        assert_eq!(keys(&read), ["recent", "fresh"]);
        assert_eq!(kept.len(), 3);

        // 前回の結果が無ければすべて読む
        let (read, _) = split_by_scope(&paths, &Scope::Active { days: 90 }, None, &sessions, 365);
        assert_eq!(read.len(), 5);
    }
}
