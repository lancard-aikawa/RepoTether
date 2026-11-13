//! RepoTether に渡す印の数 (`summary.json`)。記録 (index.sqlite) の隣に、取り込むたびに書く。
//!
//! RepoTether は記録の DB には触らず、このファイルだけを読む (LockWatch の結果ファイルと同じつなぎ方)。
//! 中身はリポジトリのパスと件数だけで、ファイル名は書かない。
//!
//! ```json
//! { "version": 1, "generatedAt": 1794475381, "repos": { "C:\\Repos\\x": { "missing": 1, "moved": 0, "secretsSent": 2, "gone": false } } }
//! ```
//! 件数がすべて 0 で、フォルダもあるリポジトリは書かない。

use crate::store::Store;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const SUMMARY_FILE: &str = "summary.json";
pub const SUMMARY_VERSION: u32 = 1;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RepoMarks {
    pub missing: i64,
    pub moved: i64,
    pub secrets_sent: i64,
    /// リポジトリのフォルダごと見当たらない
    pub gone: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub version: u32,
    /// 書いた時刻 (UNIX 秒)
    pub generated_at: i64,
    pub repos: BTreeMap<String, RepoMarks>,
}

impl Store {
    /// 記録の隣の summary.json の場所
    pub fn summary_path(&self) -> PathBuf {
        self.path.parent().unwrap_or(Path::new(".")).join(SUMMARY_FILE)
    }

    /// 今の記録から印の数を数える
    pub fn summary(&self) -> Result<Summary, String> {
        let mut repos: BTreeMap<String, RepoMarks> = BTreeMap::new();
        let mut st = self
            .conn
            .prepare(
                "SELECT repo, SUM(state = 'missing'), SUM(state = 'moved') FROM files
                 GROUP BY repo HAVING SUM(state = 'missing') + SUM(state = 'moved') > 0",
            )
            .map_err(|e| e.to_string())?;
        let rows = st
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)))
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (repo, missing, moved) = row.map_err(|e| e.to_string())?;
            let m = repos.entry(repo).or_default();
            m.missing = missing;
            m.moved = moved;
        }
        for (repo, _, _) in self.secrets_sent()? {
            repos.entry(repo).or_default().secrets_sent += 1;
        }
        for (repo, gone) in self.repos()? {
            if gone {
                repos.entry(repo).or_default().gone = true;
            }
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        Ok(Summary { version: SUMMARY_VERSION, generated_at: now, repos })
    }

    /// summary.json を書き直す。読む側が書きかけを読まないよう、一時ファイルに書いてから置き換える
    pub fn write_summary(&self) -> Result<PathBuf, String> {
        let s = self.summary()?;
        let path = self.summary_path();
        let tmp = path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(&s).map_err(|e| e.to_string())?;
        std::fs::write(&tmp, json).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(path)
    }
}

/// summary.json を読む (RepoTether から)。無い・壊れている・版が違うときは None
pub fn read_summary(path: &Path) -> Option<Summary> {
    let text = std::fs::read_to_string(path).ok()?;
    let s: Summary = serde_json::from_str(&text).ok()?;
    (s.version == SUMMARY_VERSION).then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::scan_repo;
    use std::fs;
    use std::process::Command;

    #[test]
    fn summary_counts_marks_per_repo_without_file_names() {
        let t = std::env::temp_dir().join(format!("offstage-summary-{}", std::process::id()));
        let _ = fs::remove_dir_all(&t);
        let (a, b) = (t.join("a"), t.join("b"));
        for r in [&a, &b] {
            fs::create_dir_all(r.join("sandbox")).unwrap();
            assert!(Command::new("git").arg("-C").arg(r).args(["init", "-q"]).status().unwrap().success());
            fs::write(r.join(".gitignore"), "sandbox/\n").unwrap();
            fs::write(r.join("sandbox/memo.md"), "m").unwrap();
        }
        let mut s = Store::open(&t.join("idx/index.sqlite")).unwrap();
        s.apply_run(&[scan_repo(&a).unwrap(), scan_repo(&b).unwrap()]).unwrap();
        assert!(s.summary().unwrap().repos.is_empty(), "印が無ければ何も書かない");

        fs::remove_file(a.join("sandbox/memo.md")).unwrap();
        s.apply_run(&[scan_repo(&a).unwrap(), scan_repo(&b).unwrap()]).unwrap();
        let path = s.write_summary().unwrap();
        assert_eq!(path, t.join("idx").join(SUMMARY_FILE));
        let got = read_summary(&path).unwrap();
        let ak = a.to_string_lossy().into_owned();
        assert_eq!(got.repos.keys().collect::<Vec<_>>(), vec![&ak]);
        assert_eq!(got.repos[&ak], RepoMarks { missing: 1, moved: 0, secrets_sent: 0, gone: false });
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains("memo.md"), "ファイル名は書かない");
        assert!(!t.join("idx").join("summary.json.tmp").exists());
        drop(s);
        let _ = fs::remove_dir_all(&t);
    }
}
