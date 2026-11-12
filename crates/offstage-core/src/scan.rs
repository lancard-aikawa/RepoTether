//! リポジトリの中で gitignore されているファイルを洗い出す。
//!
//! git には「無視されていて手元にあるもの」を `--directory` で畳んで答えさせ、
//! 生成物のフォルダには入らない (node_modules などを 1 件ずつ並べない)。
//! 個人のフォルダだけ自分で降りてファイルを並べる。中身は読まない。

use crate::classify::{classify, Class};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// 子プロセスのコンソール窓を出さない (CREATE_NO_WINDOW)。
/// Offstage は窓のアプリなので、付けないと git を呼ぶたびに黒い窓が開く (2026-11-12、リポジトリの数 × 2 個開いて操作できなくなった)
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 窓を出さない git。取り込みで git を呼ぶときは必ずこれを使う
pub fn git_command() -> Command {
    let mut cmd = Command::new("git");
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// 見つかったファイル 1 件
#[derive(Debug, Clone)]
pub struct Entry {
    /// リポジトリの根からの相対パス (区切りは `/`)
    pub rel_path: String,
    pub class: Class,
    /// 生成物のフォルダは降りないので、件数も大きさも数えない
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    /// フォルダを 1 件に畳んだ理由 (例 `profile:chromium`)。畳んでいなければ None
    pub folded: Option<&'static str>,
}

impl Entry {
    /// 区分を決めた規則 (機密の疑いと畳んだフォルダだけ。画面の「なぜこの区分か」に出す)
    pub fn rule(&self) -> Option<String> {
        match self.folded {
            Some(f) => Some(f.to_string()),
            None if self.class == Class::Secret => crate::classify::secret_rule_of(&self.rel_path),
            None => None,
        }
    }
}

/// Chromium 系 (Chrome・Edge・WebView2・Electron の user-data-dir) の利用者データのフォルダか。
/// 作り直せるが Cookie やログインの情報を持つので、生成物にも個人の 1 件ずつにもせず、機密の疑いの 1 件に畳む
fn is_browser_profile(dir: &Path) -> bool {
    dir.join("Local State").is_file()
}

/// リポジトリ 1 つの結果
#[derive(Debug, Clone)]
pub struct RepoScan {
    pub root: PathBuf,
    pub entries: Vec<Entry>,
}

impl RepoScan {
    pub fn count(&self, class: Class) -> usize {
        self.entries.iter().filter(|e| e.class == class).count()
    }
}

/// git に、無視されていて手元にあるパスを聞く (フォルダは畳まれ、末尾が `/`)
pub fn ignored_paths(repo: &Path) -> Result<Vec<String>, String> {
    let out = git_command()
        .arg("-C")
        .arg(repo)
        // リポジトリの設定から外のプログラムを動かさない (RepoTether の git.rs と同じ考え)
        .args(["-c", "core.fsmonitor=false", "-c", "core.untrackedCache=false"])
        .args(["ls-files", "--others", "--ignored", "--exclude-standard", "--directory", "-z"])
        .output()
        .map_err(|e| format!("git を起動できません: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(out
        .stdout
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect())
}

/// 人の上書き: (リポジトリの中のパス, 区分)。パスはフォルダでもファイルでもよく、いちばん長く当たったものを使う
pub type Overrides = [(String, Class)];

/// 規則で分けたあと、人の上書きを当てる
pub fn class_for(rel: &str, is_dir: bool, ov: &Overrides) -> Class {
    ov.iter()
        .filter(|(p, _)| rel == p || (rel.len() > p.len() && rel.starts_with(p.as_str()) && rel.as_bytes()[p.len()] == b'/'))
        .max_by_key(|(p, _)| p.len())
        .map(|(_, c)| *c)
        .unwrap_or_else(|| classify(rel, is_dir))
}

/// リポジトリ 1 つを洗い出す
pub fn scan_repo(repo: &Path) -> Result<RepoScan, String> {
    scan_repo_with(repo, &[])
}

/// 人の上書きを当てて洗い出す
pub fn scan_repo_with(repo: &Path, ov: &Overrides) -> Result<RepoScan, String> {
    let mut entries = Vec::new();
    for p in without_nested(ignored_paths(repo)?) {
        let is_dir = p.ends_with('/');
        let rel = p.trim_end_matches('/').to_string();
        let class = class_for(&rel, is_dir, ov);
        if is_dir && class != Class::Generated {
            walk(repo, &rel, ov, &mut entries);
        } else {
            push_entry(repo, rel, class, is_dir, &mut entries);
        }
    }
    entries.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    entries.dedup_by(|a, b| a.rel_path == b.rel_path);
    Ok(RepoScan { root: repo.to_path_buf(), entries })
}

/// git は、無視されたフォルダの中に自分用の .gitignore を持つフォルダがあると、
/// 親のフォルダ (`a/`) と中のパス (`a/b/c`) を両方返す。親は自分で降りるので、中のほうを外す
fn without_nested(mut paths: Vec<String>) -> Vec<String> {
    paths.sort();
    let dirs: Vec<String> = paths.iter().filter(|p| p.ends_with('/')).cloned().collect();
    paths.retain(|p| !dirs.iter().any(|d| p != d && p.starts_with(d.as_str())));
    paths
}

fn push_entry(repo: &Path, rel: String, class: Class, is_dir: bool, out: &mut Vec<Entry>) {
    let (size, modified) = if is_dir {
        (0, None)
    } else {
        match std::fs::symlink_metadata(repo.join(&rel)) {
            Ok(m) => (m.len(), m.modified().ok()),
            Err(_) => (0, None),
        }
    };
    out.push(Entry { rel_path: rel, class, is_dir, size, modified, folded: None });
}

/// 個人のフォルダを降りる。下に生成物のフォルダがあれば畳んで 1 件にする。
/// シンボリックリンク・ジャンクションは辿らない (別の場所を二重に数えない)
fn walk(repo: &Path, rel_dir: &str, ov: &Overrides, out: &mut Vec<Entry>) {
    let dir = repo.join(rel_dir);
    if is_browser_profile(&dir) {
        out.push(Entry {
            rel_path: rel_dir.to_string(),
            class: Class::Secret,
            is_dir: true,
            size: 0,
            modified: None,
            folded: Some("profile:chromium"),
        });
        return;
    }
    let Ok(read) = std::fs::read_dir(&dir) else {
        return;
    };
    for item in read.flatten() {
        let Ok(ft) = item.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        let name = item.file_name().to_string_lossy().into_owned();
        let rel = format!("{rel_dir}/{name}");
        let class = class_for(&rel, ft.is_dir(), ov);
        if ft.is_dir() {
            if class == Class::Generated {
                push_entry(repo, rel, class, true, out);
            } else {
                walk(repo, &rel, ov, out);
            }
        } else {
            push_entry(repo, rel, class, false, out);
        }
    }
}

/// git が管理しているファイルのうち、資料の種類 ([`crate::link::is_doc`]) のもの (1-2 の対象)
pub fn tracked_docs(repo: &Path) -> Result<Vec<String>, String> {
    let out = git_command()
        .arg("-C")
        .arg(repo)
        .args(["-c", "core.fsmonitor=false", "-c", "core.quotePath=false", "ls-files", "-z"])
        .output()
        .map_err(|e| format!("git を起動できません: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(out
        .stdout
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .filter(|p| crate::link::is_doc(p))
        .collect())
}

/// `root` の直下で `.git` を持つフォルダ (RepoTether の一覧につなぐまでの仮の探し方)
pub fn child_repos(root: &Path) -> Vec<PathBuf> {
    let mut repos: Vec<PathBuf> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join(".git").exists())
        .collect();
    repos.sort();
    repos
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git").arg("-C").arg(dir).args(args).status().unwrap().success();
        assert!(ok, "git {args:?}");
    }

    #[test]
    fn overrides_take_the_longest_match() {
        let ov = vec![
            ("tools/_check".to_string(), Class::ToolData),
            ("tools/_check/keep.md".to_string(), Class::Personal),
            ("old".to_string(), Class::Generated),
        ];
        assert_eq!(class_for("tools/_check/shot.png", false, &ov), Class::ToolData);
        assert_eq!(class_for("tools/_check/keep.md", false, &ov), Class::Personal);
        assert_eq!(class_for("tools/_checker/a.md", false, &ov), Class::Personal); // 前方一致は区切りで
        assert_eq!(class_for("old", true, &ov), Class::Generated);
        assert_eq!(class_for("sandbox/x.md", false, &ov), Class::Personal);
    }

    #[test]
    fn nested_answers_under_a_reported_folder_are_dropped() {
        let got = without_nested(vec![
            ".tool/sub/x.md".into(),
            ".tool/".into(),
            ".tool/sub/".into(),
            ".toolbox/y.md".into(),
            "z.md".into(),
        ]);
        assert_eq!(got, vec![".tool/", ".toolbox/y.md", "z.md"]);
    }

    /// 架空のリポジトリを作って洗い出す (手元の実際のファイル名は使わない)
    #[test]
    fn scans_personal_folders_and_folds_generated_ones() {
        let dir = std::env::temp_dir().join(format!("offstage-scan-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q"]);
        fs::write(dir.join(".gitignore"), "sandbox/\nnode_modules/\n.work/\n*.vsix\n").unwrap();
        fs::write(dir.join("README.md"), "tracked\n").unwrap();
        fs::create_dir_all(dir.join("sandbox/sub/.venv/lib")).unwrap();
        fs::write(dir.join("sandbox/memo.md"), "x").unwrap();
        fs::write(dir.join("sandbox/sub/note.txt"), "xy").unwrap();
        fs::write(dir.join("sandbox/sub/.venv/lib/a.py"), "z").unwrap();
        fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
        fs::write(dir.join("node_modules/pkg/index.js"), "z").unwrap();
        fs::create_dir_all(dir.join(".work")).unwrap();
        fs::write(dir.join(".work/demo_token.json"), "{}").unwrap();
        fs::write(dir.join("app-0.1.0.vsix"), "z").unwrap();
        // ブラウザの利用者データは中を並べず 1 件に畳む
        fs::create_dir_all(dir.join("sandbox/webview/Default/Network")).unwrap();
        fs::write(dir.join("sandbox/webview/Local State"), "{}").unwrap();
        fs::write(dir.join("sandbox/webview/Default/Network/Trust Tokens"), "z").unwrap();

        let scan = scan_repo(&dir).unwrap();
        let got: Vec<(String, Class)> =
            scan.entries.iter().map(|e| (e.rel_path.clone(), e.class)).collect();
        assert_eq!(
            got,
            vec![
                (".work/demo_token.json".into(), Class::Secret),
                ("app-0.1.0.vsix".into(), Class::Generated),
                ("node_modules".into(), Class::Generated),
                ("sandbox/memo.md".into(), Class::Personal),
                ("sandbox/sub/.venv".into(), Class::Generated),
                ("sandbox/sub/note.txt".into(), Class::Personal),
                ("sandbox/webview".into(), Class::Secret),
            ]
        );
        let profile = scan.entries.iter().find(|e| e.rel_path == "sandbox/webview").unwrap();
        assert_eq!(profile.rule().as_deref(), Some("profile:chromium"));
        let token = scan.entries.iter().find(|e| e.class == Class::Secret && !e.is_dir).unwrap();
        assert_eq!(token.rule().as_deref(), Some("word:token"));
        let note = scan.entries.iter().find(|e| e.rel_path == "sandbox/sub/note.txt").unwrap();
        assert_eq!(note.size, 2);
        assert!(note.modified.is_some());
        let _ = fs::remove_dir_all(&dir);
    }
}
