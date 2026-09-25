//! データの取り込み。Tauri に依存しないので examples/dump.rs からも呼べる。

pub mod config;
pub mod discover;
pub mod git;
pub mod model;
pub mod remote;
pub mod sessions;
pub mod util;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use config::Config;
use model::{Commit, LocalRepo, RemoteRepo, Session, Snapshot, SourceError};

/// ローカル側 (リポジトリ・コミット・セッション) を読み直す。リモート一覧は含めない。
/// progress には「何をしているか」の短い文を渡す。
pub fn build_local(cfg: &Config, cache_dir: &Path, progress: &(dyn Fn(String) + Sync)) -> Snapshot {
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

    let paths: Vec<PathBuf> = paths.into_values().collect();
    let total = paths.len();
    progress(format!("git の状態を読んでいます (0/{total})"));
    let results = inspect_parallel(&paths, cfg.history_days, &|done| {
        progress(format!("git の状態を読んでいます ({done}/{total})"));
    });

    let mut repos: Vec<LocalRepo> = vec![];
    let mut commits: Vec<Commit> = vec![];
    for (repo, cs) in results {
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
            .find(|id| key == **id || key.starts_with(&format!("{id}\\")))
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

fn inspect_parallel(
    paths: &[PathBuf],
    history_days: u32,
    on_done: &(dyn Fn(usize) + Sync),
) -> Vec<(LocalRepo, Vec<Commit>)> {
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let out: Mutex<Vec<(LocalRepo, Vec<Commit>)>> = Mutex::new(Vec::with_capacity(paths.len()));
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 8);
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
        match remote::fetch(a).await {
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

