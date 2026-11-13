//! 取り込んだ結果を手元の SQLite に置き、前回と比べて「消えた」「移動した」を出す。
//!
//! 中身は残さない。残すのはパス・区分・大きさ・更新時刻・ハッシュ (個人と機密の疑いのファイルだけ) と、
//! 最初と最後に見た時刻。DB 自体にファイル名が載るので、クラウド同期のフォルダには置かない ([`check_not_synced`])。
//!
//! 「消えた」の決め方: 前回あって今回の洗い出しに無く、**ディスクにも無い**もの。
//! ディスクにあるのに洗い出しに無いもの (gitignore を外して git の管理に入った・規則が変わって生成物になった) は、
//! 消えたのではないので記録から外す。洗い出しに失敗したリポジトリは比べない。
//! リポジトリのフォルダごと無いときは、ファイルごとに「消えた」を出さず、リポジトリ 1 件の印にする。

use crate::classify::Class;
use crate::scan::{Entry, RepoScan};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// 記録の状態
pub const PRESENT: &str = "present";
pub const MISSING: &str = "missing";
pub const MOVED: &str = "moved";
/// 人が「消してよかった」と付けたもの
pub const DISMISSED: &str = "dismissed";

/// 1 回の取り込みで変わったこと
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Changes {
    pub added: Vec<(String, String)>,
    pub changed: Vec<(String, String)>,
    pub missing: Vec<(String, String)>,
    /// (リポジトリ, 前のパス, 新しいリポジトリ, 新しいパス)
    pub moved: Vec<(String, String, String, String)>,
    /// 前は見ていたが、今は洗い出しの対象でなくなったもの (ディスクにはある)
    pub dropped: Vec<(String, String)>,
    /// フォルダごと見当たらないリポジトリ
    pub repos_gone: Vec<String>,
    /// 前に消えたものがまた現れた
    pub returned: Vec<(String, String)>,
}

pub struct Store {
    pub(crate) conn: Connection,
    /// 記録のファイルの場所 (隣に summary.json を書く)
    pub(crate) path: PathBuf,
}

/// 既定の置き場所 `%USERPROFILE%\.offstage\index.sqlite`
pub fn default_store_path() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(home).join(".offstage").join("index.sqlite"))
}

/// OneDrive など、クラウドへ写されるフォルダの下なら Err。
/// 見るのは OneDrive が設定する環境変数 (OneDrive・OneDriveConsumer・OneDriveCommercial)
pub fn check_not_synced(path: &Path) -> Result<(), String> {
    let target = normalize(path);
    for var in ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"] {
        if let Some(root) = std::env::var_os(var) {
            let root = normalize(Path::new(&root));
            if !root.is_empty() && (target == root || target.starts_with(&(root.clone() + "\\"))) {
                return Err(format!(
                    "索引の置き場所が {var} の同期フォルダの下です。ファイル名がクラウドに写るので使えません"
                ));
            }
        }
    }
    Ok(())
}

fn normalize(p: &Path) -> String {
    p.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

fn unix_now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn mtime_ns(e: &Entry) -> Option<i64> {
    e.modified
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as i64)
}

/// ハッシュを取る区分 (消えた・移動を見分けたいもの)
fn wants_hash(e: &Entry) -> bool {
    !e.is_dir && matches!(e.class, Class::Personal | Class::Secret)
}

/// ファイルの SHA-256 (16 進)。読めなければ None
pub fn sha256_file(path: &Path) -> Option<String> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Some(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// 前回の記録 1 件
struct Prev {
    size: i64,
    mtime_ns: Option<i64>,
    sha256: Option<String>,
    state: String,
}

impl Store {
    /// 開く (無ければ作る)。クラウド同期のフォルダの下なら開かない
    pub fn open(path: &Path) -> Result<Store, String> {
        check_not_synced(path)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let conn = Connection::open(path).map_err(|e| e.to_string())?;
        // 画面と裏の取り込み (RepoTether から --refresh) が同時に書いても、すぐには失敗させず待つ
        conn.busy_timeout(std::time::Duration::from_secs(60)).map_err(|e| e.to_string())?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS files (
               repo TEXT NOT NULL,
               rel_path TEXT NOT NULL,
               class TEXT NOT NULL,
               is_dir INTEGER NOT NULL,
               size INTEGER NOT NULL,
               mtime_ns INTEGER,
               sha256 TEXT,
               folded TEXT,
               first_seen INTEGER NOT NULL,
               last_seen INTEGER NOT NULL,
               state TEXT NOT NULL,
               state_since INTEGER NOT NULL,
               moved_to_repo TEXT,
               moved_to_path TEXT,
               PRIMARY KEY (repo, rel_path)
             );
             CREATE INDEX IF NOT EXISTS files_sha ON files(sha256);
             CREATE INDEX IF NOT EXISTS files_state ON files(state);
             CREATE TABLE IF NOT EXISTS repos (
               repo TEXT PRIMARY KEY,
               last_scan INTEGER NOT NULL,
               gone_since INTEGER
             );
             -- 会話に出た場面 (link.rs)
             CREATE TABLE IF NOT EXISTS mentions (
               log_path TEXT NOT NULL,
               line_no INTEGER NOT NULL,
               session_id TEXT NOT NULL,
               ts TEXT,
               kind TEXT NOT NULL,
               tool TEXT,
               repo TEXT NOT NULL,
               rel_path TEXT NOT NULL,
               origin TEXT NOT NULL,
               PRIMARY KEY (log_path, line_no, repo, rel_path, kind)
             );
             CREATE INDEX IF NOT EXISTS mentions_file ON mentions(repo, rel_path);
             -- 人の上書き (区分の付け替え)。rel_prefix はフォルダでもファイルでもよい
             CREATE TABLE IF NOT EXISTS overrides (
               repo TEXT NOT NULL,
               rel_prefix TEXT NOT NULL,
               class TEXT NOT NULL,
               PRIMARY KEY (repo, rel_prefix)
             );
             -- git が管理する資料 (1-2)。取り込むたびにリポジトリごと入れ替える
             CREATE TABLE IF NOT EXISTS docs (
               repo TEXT NOT NULL,
               rel_path TEXT NOT NULL,
               size INTEGER NOT NULL,
               mtime_ns INTEGER,
               PRIMARY KEY (repo, rel_path)
             );
             -- 会話の記録をどこまで読んだか
             CREATE TABLE IF NOT EXISTS session_logs (
               path TEXT PRIMARY KEY,
               offset INTEGER NOT NULL,
               lines INTEGER NOT NULL,
               cwd TEXT NOT NULL
             );",
        )
        .map_err(|e| e.to_string())?;
        Ok(Store { conn, path: path.to_path_buf() })
    }

    /// 記録にある全部のパス (消えた・移動したものも)。(リポジトリ, パス, 中を並べず 1 件に畳んだフォルダか)
    pub fn all_paths(&self) -> Result<Vec<(String, String, bool)>, String> {
        let mut st = self
            .conn
            .prepare("SELECT repo, rel_path, folded IS NOT NULL FROM files")
            .map_err(|e| e.to_string())?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).map_err(|e| e.to_string())?;
        rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
    }

    /// 1 回分の洗い出しを記録し、前回との違いを返す。
    /// `scans` に無いリポジトリは比べない (フォルダごと無いものだけ `repos_gone` にする)
    pub fn apply_run(&mut self, scans: &[RepoScan]) -> Result<Changes, String> {
        self.apply_run_at(scans, unix_now())
    }

    pub fn apply_run_at(&mut self, scans: &[RepoScan], now: i64) -> Result<Changes, String> {
        let tx = self.conn.transaction().map_err(|e| e.to_string())?;
        let mut ch = Changes::default();
        // 今回新しく現れた、ハッシュのあるファイル (移動先の候補)
        let mut new_by_hash: HashMap<String, Vec<(String, String)>> = HashMap::new();
        // 今回消えた候補 (リポジトリ, パス, ハッシュ)
        let mut vanished: Vec<(String, String, Option<String>)> = Vec::new();

        for scan in scans {
            let repo = scan.root.to_string_lossy().into_owned();
            let mut prev: HashMap<String, Prev> = HashMap::new();
            {
                let mut st = tx
                    .prepare("SELECT rel_path, size, mtime_ns, sha256, state FROM files WHERE repo = ?1")
                    .map_err(|e| e.to_string())?;
                let rows = st
                    .query_map(params![repo], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            Prev { size: r.get(1)?, mtime_ns: r.get(2)?, sha256: r.get(3)?, state: r.get(4)? },
                        ))
                    })
                    .map_err(|e| e.to_string())?;
                for row in rows {
                    let (k, v) = row.map_err(|e| e.to_string())?;
                    prev.insert(k, v);
                }
            }

            for e in &scan.entries {
                let mt = mtime_ns(e);
                let old = prev.remove(&e.rel_path);
                let same = old.as_ref().is_some_and(|p| p.size == e.size as i64 && p.mtime_ns == mt);
                let sha = if !wants_hash(e) {
                    None
                } else if same && old.as_ref().is_some_and(|p| p.sha256.is_some()) {
                    old.as_ref().and_then(|p| p.sha256.clone())
                } else {
                    sha256_file(&scan.root.join(&e.rel_path))
                };
                match &old {
                    None => {
                        ch.added.push((repo.clone(), e.rel_path.clone()));
                        if let Some(h) = &sha {
                            new_by_hash.entry(h.clone()).or_default().push((repo.clone(), e.rel_path.clone()));
                        }
                    }
                    Some(p) if p.state != PRESENT => ch.returned.push((repo.clone(), e.rel_path.clone())),
                    Some(_) if !same => ch.changed.push((repo.clone(), e.rel_path.clone())),
                    Some(_) => {}
                }
                tx.execute(
                    "INSERT INTO files (repo, rel_path, class, is_dir, size, mtime_ns, sha256, folded,
                                        first_seen, last_seen, state, state_since)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, 'present', ?9)
                     ON CONFLICT(repo, rel_path) DO UPDATE SET
                       class = excluded.class, is_dir = excluded.is_dir, size = excluded.size,
                       mtime_ns = excluded.mtime_ns, sha256 = excluded.sha256, folded = excluded.folded,
                       last_seen = excluded.last_seen,
                       state_since = CASE WHEN files.state = 'present' THEN files.state_since ELSE excluded.last_seen END,
                       state = 'present', moved_to_repo = NULL, moved_to_path = NULL",
                    params![repo, e.rel_path, e.class.label(), e.is_dir, e.size as i64, mt, sha, e.folded, now],
                )
                .map_err(|e| e.to_string())?;
            }

            // 前回あって今回無いもの
            for (rel, p) in prev {
                if p.state != PRESENT {
                    continue; // もう消えた・移動・消してよかった として記録済み
                }
                if scan.root.join(&rel).symlink_metadata().is_ok() {
                    // ディスクにはある。洗い出しの対象でなくなっただけ
                    tx.execute("DELETE FROM files WHERE repo = ?1 AND rel_path = ?2", params![repo, rel])
                        .map_err(|e| e.to_string())?;
                    ch.dropped.push((repo.clone(), rel));
                } else {
                    vanished.push((repo.clone(), rel, p.sha256));
                }
            }
            tx.execute(
                "INSERT INTO repos (repo, last_scan, gone_since) VALUES (?1, ?2, NULL)
                 ON CONFLICT(repo) DO UPDATE SET last_scan = excluded.last_scan, gone_since = NULL",
                params![repo, now],
            )
            .map_err(|e| e.to_string())?;
        }

        // 消えた候補を、同じハッシュの新しいファイルと突き合わせる (リポジトリをまたいでもよい)
        for (repo, rel, sha) in vanished {
            let dest = sha.as_ref().and_then(|h| new_by_hash.get_mut(h)).and_then(|v| v.pop());
            match dest {
                Some((to_repo, to_path)) => {
                    tx.execute(
                        "UPDATE files SET state = 'moved', state_since = ?3, moved_to_repo = ?4, moved_to_path = ?5
                         WHERE repo = ?1 AND rel_path = ?2",
                        params![repo, rel, now, to_repo, to_path],
                    )
                    .map_err(|e| e.to_string())?;
                    ch.added.retain(|a| !(a.0 == to_repo && a.1 == to_path));
                    ch.moved.push((repo, rel, to_repo, to_path));
                }
                None => {
                    tx.execute(
                        "UPDATE files SET state = 'missing', state_since = ?3 WHERE repo = ?1 AND rel_path = ?2",
                        params![repo, rel, now],
                    )
                    .map_err(|e| e.to_string())?;
                    ch.missing.push((repo, rel));
                }
            }
        }

        // 記録にあって今回洗い出していないリポジトリ: フォルダごと無ければ 1 件の印にする
        let scanned: Vec<String> = scans.iter().map(|s| s.root.to_string_lossy().into_owned()).collect();
        let known: Vec<(String, Option<i64>)> = {
            let mut st = tx.prepare("SELECT repo, gone_since FROM repos").map_err(|e| e.to_string())?;
            let rows = st
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(|e| e.to_string())?;
            rows.collect::<Result<_, _>>().map_err(|e| e.to_string())?
        };
        for (repo, gone_since) in known {
            if scanned.contains(&repo) || Path::new(&repo).exists() {
                continue;
            }
            if gone_since.is_none() {
                tx.execute("UPDATE repos SET gone_since = ?2 WHERE repo = ?1", params![repo, now])
                    .map_err(|e| e.to_string())?;
            }
            ch.repos_gone.push(repo);
        }

        tx.commit().map_err(|e| e.to_string())?;
        Ok(ch)
    }

    /// 人が「消してよかった」と付ける
    pub fn dismiss(&self, repo: &str, rel_path: &str) -> Result<bool, String> {
        let n = self
            .conn
            .execute(
                "UPDATE files SET state = 'dismissed', state_since = ?3
                 WHERE repo = ?1 AND rel_path = ?2 AND state IN ('missing', 'moved')",
                params![repo, rel_path, unix_now()],
            )
            .map_err(|e| e.to_string())?;
        Ok(n > 0)
    }

    /// 記録の状態 (試験と画面用)
    pub fn state_of(&self, repo: &str, rel_path: &str) -> Result<Option<String>, String> {
        self.conn
            .query_row(
                "SELECT state FROM files WHERE repo = ?1 AND rel_path = ?2",
                params![repo, rel_path],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())
    }

    /// 状態ごとの件数 (区分つき)
    pub fn counts(&self) -> Result<Vec<(String, String, i64)>, String> {
        let mut st = self
            .conn
            .prepare("SELECT state, class, COUNT(*) FROM files GROUP BY state, class ORDER BY state, class")
            .map_err(|e| e.to_string())?;
        let rows = st
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::scan_repo;
    use std::fs;
    use std::process::Command;

    struct Tmp(PathBuf);
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn tmp(name: &str) -> Tmp {
        let d = std::env::temp_dir().join(format!("offstage-store-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        Tmp(d)
    }

    fn repo(dir: &Path) {
        assert!(Command::new("git").arg("-C").arg(dir).args(["init", "-q"]).status().unwrap().success());
        fs::write(dir.join(".gitignore"), "sandbox/\n").unwrap();
        fs::create_dir_all(dir.join("sandbox")).unwrap();
    }

    fn run(store: &mut Store, repos: &[&Path], now: i64) -> Changes {
        let scans: Vec<RepoScan> = repos.iter().map(|r| scan_repo(r).unwrap()).collect();
        store.apply_run_at(&scans, now).unwrap()
    }

    fn paths(v: &[(String, String)]) -> Vec<&str> {
        v.iter().map(|(_, p)| p.as_str()).collect()
    }

    #[test]
    fn notices_missing_moved_returned_and_dropped() {
        let t = tmp("a");
        let (a, b) = (t.0.join("a"), t.0.join("b"));
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        repo(&a);
        repo(&b);
        fs::write(a.join("sandbox/keep.md"), "keep").unwrap();
        fs::write(a.join("sandbox/gone.md"), "gone").unwrap();
        fs::write(a.join("sandbox/move.md"), "move me").unwrap();
        fs::write(a.join("sandbox/to-other.md"), "cross repo").unwrap();
        fs::write(a.join("sandbox/tracked.md"), "will be tracked").unwrap();
        let mut s = Store::open(&t.0.join("db/index.sqlite")).unwrap();

        let c1 = run(&mut s, &[&a, &b], 100);
        assert_eq!(c1.added.len(), 5);

        // 消す・同じリポジトリの中で移す・別のリポジトリへ移す・gitignore から外す
        fs::remove_file(a.join("sandbox/gone.md")).unwrap();
        fs::create_dir_all(a.join("sandbox/sub")).unwrap();
        fs::rename(a.join("sandbox/move.md"), a.join("sandbox/sub/moved.md")).unwrap();
        fs::rename(a.join("sandbox/to-other.md"), b.join("sandbox/arrived.md")).unwrap();
        fs::write(a.join(".gitignore"), "sandbox/*\n!sandbox/tracked.md\n").unwrap();
        fs::write(a.join("sandbox/keep.md"), "keep, edited").unwrap();

        let c2 = run(&mut s, &[&a, &b], 200);
        assert_eq!(paths(&c2.missing), vec!["sandbox/gone.md"]);
        let mut moved: Vec<(&str, &str)> = c2.moved.iter().map(|m| (m.1.as_str(), m.3.as_str())).collect();
        moved.sort();
        assert_eq!(moved, vec![("sandbox/move.md", "sandbox/sub/moved.md"), ("sandbox/to-other.md", "sandbox/arrived.md")]);
        assert!(c2.added.is_empty(), "移動先は added に出さない: {:?}", c2.added);
        assert_eq!(paths(&c2.dropped), vec!["sandbox/tracked.md"]);
        assert_eq!(paths(&c2.changed), vec!["sandbox/keep.md"]);
        let ar = a.to_string_lossy();
        assert_eq!(s.state_of(&ar, "sandbox/gone.md").unwrap().as_deref(), Some(MISSING));
        assert_eq!(s.state_of(&ar, "sandbox/tracked.md").unwrap(), None);

        // 何も変えなければ何も出ない (消えたものを毎回出し直さない)
        let c3 = run(&mut s, &[&a, &b], 300);
        assert_eq!(c3, Changes::default());

        // 戻ってきた
        fs::write(a.join("sandbox/gone.md"), "gone").unwrap();
        let c4 = run(&mut s, &[&a, &b], 400);
        assert_eq!(paths(&c4.returned), vec!["sandbox/gone.md"]);
        assert_eq!(s.state_of(&ar, "sandbox/gone.md").unwrap().as_deref(), Some(PRESENT));

        // 消してよかった
        fs::remove_file(a.join("sandbox/gone.md")).unwrap();
        run(&mut s, &[&a, &b], 500);
        assert!(s.dismiss(&ar, "sandbox/gone.md").unwrap());
        assert_eq!(s.state_of(&ar, "sandbox/gone.md").unwrap().as_deref(), Some(DISMISSED));
    }

    #[test]
    fn whole_repo_gone_is_one_mark_and_unscanned_repos_are_left_alone() {
        let t = tmp("b");
        let (a, b) = (t.0.join("a"), t.0.join("b"));
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        repo(&a);
        repo(&b);
        fs::write(a.join("sandbox/x.md"), "x").unwrap();
        fs::write(b.join("sandbox/y.md"), "y").unwrap();
        let mut s = Store::open(&t.0.join("index.sqlite")).unwrap();
        run(&mut s, &[&a, &b], 100);

        // b を洗い出さなかっただけなら何も出ない
        let c = run(&mut s, &[&a], 200);
        assert_eq!(c, Changes::default());

        // b のフォルダごと無くなった
        fs::remove_dir_all(&b).unwrap();
        let c = run(&mut s, &[&a], 300);
        assert!(c.missing.is_empty());
        assert_eq!(c.repos_gone, vec![b.to_string_lossy().into_owned()]);
        assert_eq!(s.state_of(&b.to_string_lossy(), "sandbox/y.md").unwrap().as_deref(), Some(PRESENT));
    }

    #[test]
    fn hashes_only_personal_and_secret_files() {
        let t = tmp("c");
        repo(&t.0);
        fs::create_dir_all(t.0.join("sandbox/logs")).unwrap();
        fs::write(t.0.join("sandbox/a.md"), "abc").unwrap();
        fs::write(t.0.join("sandbox/logs/run.txt"), "abc").unwrap();
        let mut s = Store::open(&t.0.join("idx/index.sqlite")).unwrap();
        run(&mut s, &[&t.0], 100);
        let hashes: Vec<(String, Option<String>)> = {
            let mut st = s.conn.prepare("SELECT rel_path, sha256 FROM files ORDER BY rel_path").unwrap();
            st.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(|r| r.unwrap()).collect()
        };
        assert_eq!(
            hashes,
            vec![
                ("sandbox/a.md".into(), Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into())),
                ("sandbox/logs/run.txt".into(), None),
            ]
        );
    }

    #[test]
    fn refuses_onedrive_folder() {
        // 環境変数を書き換えずに、判定だけを確かめる
        let od = std::env::var_os("OneDrive");
        if let Some(root) = od {
            let p = PathBuf::from(root).join("x").join("index.sqlite");
            assert!(check_not_synced(&p).is_err());
        }
        assert!(check_not_synced(&std::env::temp_dir().join("index.sqlite")).is_ok());
    }
}
