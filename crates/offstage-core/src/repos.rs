//! 対象のリポジトリを探す。探すフォルダは RepoTether の設定から借りる (roots と scanDepth だけを読む。トークンなどの欄は読まない)。

use serde_json::Value;
use std::path::{Path, PathBuf};

/// 探すときに降りないフォルダ (RepoTether の discover.rs と同じ考え)
const SKIP_DIRS: &[&str] = &[
    "node_modules", "target", "dist", "build", "out", "bin", "obj", "vendor", "venv",
    "__pycache__", "Pods", "DerivedData",
];

/// RepoTether の設定 `%APPDATA%\com.repotether.app\config.json` の場所
pub fn repotether_config_path() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(appdata).join("com.repotether.app").join("config.json"))
}

/// 設定ファイルから (探すフォルダ, 階層) を読む。読めなければ None
pub fn roots_from_config(path: &Path) -> Option<(Vec<PathBuf>, u32)> {
    let text = std::fs::read_to_string(path).ok()?;
    let v: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    let roots: Vec<PathBuf> = v
        .get("roots")?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .map(PathBuf::from)
        .collect();
    let depth = v
        .get("scanDepth")
        .or_else(|| v.get("scan_depth"))
        .and_then(Value::as_u64)
        .unwrap_or(3) as u32;
    Some((roots, depth))
}

/// roots の下を depth 階層まで探し、`.git` を持つフォルダを返す。リポジトリの中は探さない
pub fn find_repos(roots: &[PathBuf], depth: u32) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for r in roots {
        walk(r, 0, depth, &mut found);
    }
    found.sort();
    found.dedup();
    found
}

fn walk(dir: &Path, depth: u32, max: u32, found: &mut Vec<PathBuf>) {
    if dir.join(".git").exists() {
        found.push(dir.to_path_buf());
        return;
    }
    if depth >= max {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        if !ft.is_dir() || ft.is_symlink() {
            continue;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || SKIP_DIRS.iter().any(|s| s.eq_ignore_ascii_case(&name)) {
            continue;
        }
        walk(&e.path(), depth + 1, max, found);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn reads_only_roots_and_depth_and_finds_repos() {
        let t = std::env::temp_dir().join(format!("offstage-repos-{}", std::process::id()));
        let _ = fs::remove_dir_all(&t);
        for d in ["a/.git", "g/b/.git", "g/b/inner/.git", "node_modules/x/.git", "deep/1/2/3/.git"] {
            fs::create_dir_all(t.join(d)).unwrap();
        }
        let cfg = t.join("config.json");
        let root = t.to_string_lossy().replace('\\', "\\\\");
        fs::write(&cfg, format!("\u{feff}{{\"roots\":[\"{root}\"],\"scanDepth\":2,\"accounts\":[{{\"token\":\"x\"}}]}}")).unwrap();
        let (roots, depth) = roots_from_config(&cfg).unwrap();
        assert_eq!(depth, 2);
        let found: Vec<String> = find_repos(&roots, depth)
            .iter()
            .map(|p| p.strip_prefix(&t).unwrap().to_string_lossy().replace('\\', "/"))
            .collect();
        assert_eq!(found, vec!["a", "g/b"]);
        let _ = fs::remove_dir_all(&t);
    }
}
