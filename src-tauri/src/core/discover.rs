//! ローカルの git リポジトリを探す。

use std::path::{Path, PathBuf};

/// 中を探さないフォルダ。依存やビルド成果物で、リポジトリが置かれることはまず無い。
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    "out",
    "bin",
    "obj",
    "vendor",
    "venv",
    "__pycache__",
    "Pods",
    "DerivedData",
];

/// roots の下を max_depth 階層まで探し、`.git` を持つフォルダを返す。
/// リポジトリを見つけたらその中は探さない (サブモジュールや入れ子は対象外)。
pub fn find_repos(roots: &[PathBuf], max_depth: u32) -> Vec<PathBuf> {
    let mut found = vec![];
    for root in roots {
        walk(root, 0, max_depth, &mut found);
    }
    found
}

fn walk(dir: &Path, depth: u32, max_depth: u32, found: &mut Vec<PathBuf>) {
    if is_repo(dir) {
        found.push(dir.to_path_buf());
        return;
    }
    if depth >= max_depth {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut subdirs: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            // シンボリックリンクとジャンクションはたどらない (ループと二重計上を避ける)
            e.file_type().map(|t| t.is_dir() && !t.is_symlink()).unwrap_or(false)
        })
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            !name.starts_with('.') && !SKIP_DIRS.iter().any(|s| s.eq_ignore_ascii_case(&name))
        })
        .map(|e| e.path())
        .collect();
    subdirs.sort();
    for d in subdirs {
        walk(&d, depth + 1, max_depth, found);
    }
}

pub fn is_repo(dir: &Path) -> bool {
    dir.join(".git").exists()
}

/// path 自身か、その親をたどって最初に見つかるリポジトリ。
pub fn enclosing_repo(path: &Path) -> Option<PathBuf> {
    let mut cur = Some(path);
    while let Some(p) = cur {
        if is_repo(p) {
            return Some(p.to_path_buf());
        }
        cur = p.parent();
    }
    None
}

/// つながらないドライブや共有を待つ上限。つながる場所なら、ふつうは一瞬で返る
const REACH_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1500);

/// フォルダの一覧 (セッションのフォルダ) から、それぞれを含むリポジトリを探す。同じリポジトリは 1 つにまとめる。
/// つながらないネットワークの場所 (\\server\share\...) は、OS が数十秒待ってから「無い」と答えるので、
/// ドライブや共有ごとに先に時間を切って確かめ、届かなければその下は調べない
pub fn enclosing_repos<'a>(dirs: impl IntoIterator<Item = &'a str>) -> Vec<PathBuf> {
    let dirs: std::collections::BTreeSet<&str> = dirs.into_iter().collect();
    let mut reach: std::collections::HashMap<PathBuf, bool> = Default::default();
    let mut found = std::collections::BTreeSet::new();
    for d in dirs {
        let dir = Path::new(d);
        // ancestors の最後が、ドライブや共有の根 (C:\ / \\server\share\)
        let Some(root) = dir.ancestors().last().filter(|r| !r.as_os_str().is_empty()) else { continue };
        if !*reach.entry(root.to_path_buf()).or_insert_with(|| reachable(root)) {
            continue;
        }
        if let Some(repo) = enclosing_repo(dir) {
            found.insert(repo);
        }
    }
    found.into_iter().collect()
}

/// 届かなかった場所を、もう一度確かめるまでの時間 (更新のたびに REACH_TIMEOUT を待たないように)
const REACH_RETRY: std::time::Duration = std::time::Duration::from_secs(600);

/// REACH_TIMEOUT のうちに root が読めるか。待ちきれなかったときのスレッドは、OS が答えるまで残して捨てる。
/// 届かなかった場所は REACH_RETRY のあいだ覚えておき、確かめずに「届かない」と答える
fn reachable(root: &Path) -> bool {
    static UNREACHABLE: std::sync::Mutex<Vec<(PathBuf, std::time::Instant)>> = std::sync::Mutex::new(Vec::new());
    let mut dead = UNREACHABLE.lock().unwrap();
    dead.retain(|(_, at)| at.elapsed() < REACH_RETRY);
    if dead.iter().any(|(p, _)| p == root) {
        return false;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let probe = root.to_path_buf();
    std::thread::spawn(move || {
        let _ = tx.send(std::fs::metadata(&probe).is_ok());
    });
    let ok = rx.recv_timeout(REACH_TIMEOUT).unwrap_or(false);
    if !ok {
        dead.push((root.to_path_buf(), std::time::Instant::now()));
    }
    ok
}

/// `.git` がファイル (worktree / サブモジュール) のときは、指している git ディレクトリを返す。
pub fn git_dir(repo: &Path) -> Option<PathBuf> {
    let dot = repo.join(".git");
    if dot.is_dir() {
        return Some(dot);
    }
    let text = std::fs::read_to_string(&dot).ok()?;
    let rel = text.lines().find_map(|l| l.strip_prefix("gitdir:"))?.trim();
    let p = PathBuf::from(rel);
    Some(if p.is_absolute() { p } else { repo.join(p) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enclosing_repos_dedupes_and_skips_unreachable() {
        let root = std::env::temp_dir().join("repotether-enclosing-test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("a/.git")).unwrap();
        std::fs::create_dir_all(root.join("a/src/deep")).unwrap();
        std::fs::create_dir_all(root.join("plain")).unwrap();
        let s = |p: &str| root.join(p).to_string_lossy().into_owned();
        let dirs = [s("a"), s("a/src"), s("a/src/deep"), s("plain"), s("gone")];
        // 192.0.2.0/24 は文書用のアドレスで、どこにもつながらない
        let dead = r"\\192.0.2.1\share\proj\src";
        let started = std::time::Instant::now();
        let found = enclosing_repos(dirs.iter().map(|d| d.as_str()).chain([dead, dead]));
        assert_eq!(found, [root.join("a")]);
        assert!(started.elapsed() < std::time::Duration::from_secs(5), "つながらない場所を待ち続けた: {:?}", started.elapsed());
    }
}
