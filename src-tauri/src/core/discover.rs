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
