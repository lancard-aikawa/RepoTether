//! 画面のための読み書き: 検索・1 件の詳しい情報・印の数・人の上書き・git の資料の一覧。

use crate::classify::Class;
use crate::link::CONTENT_KINDS;
use crate::store::Store;
use rusqlite::{params, params_from_iter, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::UNIX_EPOCH;

/// 検索の条件。空の欄は絞らない
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Query {
    /// パスに含む文字 (大文字小文字は区別しない)
    pub text: String,
    /// 区分 (generated / secret / tooldata / personal / doc)
    pub classes: Vec<String>,
    /// 状態 (present / missing / moved / dismissed)
    pub states: Vec<String>,
    pub repo: String,
    /// "" / "any" (会話に出た) / "content" (中身が送られた) / "none" (出ていない)
    pub mention: String,
    /// 拡張子 (小文字、点なし)
    pub ext: String,
    /// 会話での出方 (read / attached / grep / edit / bash / tool / write / result / text)。どれかで出たもの
    pub kinds: Vec<String>,
    pub limit: usize,
}

/// 一覧の 1 行
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub repo: String,
    pub rel_path: String,
    /// 区分。git の資料は "doc"
    pub class: String,
    pub state: String,
    pub is_dir: bool,
    pub size: i64,
    /// 更新時刻 (UNIX 秒)
    pub modified: Option<i64>,
    pub folded: Option<String>,
    pub moved_to: Option<String>,
    /// 状態になった時刻 (UNIX 秒)
    pub state_since: Option<i64>,
    pub mentions: i64,
    pub content_mentions: i64,
    pub last_mention: Option<String>,
}

/// 印の数
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Alerts {
    pub missing: i64,
    pub moved: i64,
    pub secrets_sent: i64,
    pub repos_gone: i64,
    pub last_scan: Option<i64>,
    pub repos: i64,
    pub files: i64,
    pub docs: i64,
}

fn kinds_sql() -> String {
    CONTENT_KINDS.iter().map(|k| format!("'{k}'")).collect::<Vec<_>>().join(",")
}

/// 記録 (files) と git の資料 (docs) を同じ形に並べた表
fn union_sql() -> String {
    let kinds = kinds_sql();
    format!(
        "WITH m AS (
           SELECT repo, rel_path, COUNT(*) AS n, SUM(kind IN ({kinds})) AS c, MAX(ts) AS last
           FROM mentions GROUP BY repo, rel_path
         ),
         u AS (
           SELECT repo, rel_path, class, state, is_dir, size, mtime_ns, folded,
                  CASE WHEN moved_to_path IS NULL THEN NULL ELSE moved_to_repo || '\\' || moved_to_path END AS moved_to,
                  state_since
           FROM files
           UNION ALL
           SELECT repo, rel_path, 'doc', 'present', 0, size, mtime_ns, NULL, NULL, NULL FROM docs
         )
         SELECT u.repo, u.rel_path, u.class, u.state, u.is_dir, u.size, u.mtime_ns, u.folded, u.moved_to,
                u.state_since, COALESCE(m.n, 0), COALESCE(m.c, 0), m.last
         FROM u LEFT JOIN m ON m.repo = u.repo AND m.rel_path = u.rel_path"
    )
}

fn row(r: &rusqlite::Row) -> rusqlite::Result<Row> {
    let mt: Option<i64> = r.get(6)?;
    Ok(Row {
        repo: r.get(0)?,
        rel_path: r.get(1)?,
        class: r.get(2)?,
        state: r.get(3)?,
        is_dir: r.get(4)?,
        size: r.get(5)?,
        modified: mt.map(|n| n / 1_000_000_000),
        folded: r.get(7)?,
        moved_to: r.get(8)?,
        state_since: r.get(9)?,
        mentions: r.get(10)?,
        content_mentions: r.get(11)?,
        last_mention: r.get(12)?,
    })
}

impl Store {
    /// 検索する。並びは 印のあるもの → 会話に出た新しいもの → パス
    pub fn search(&self, q: &Query) -> Result<Vec<Row>, String> {
        let mut wh: Vec<String> = Vec::new();
        let mut args: Vec<String> = Vec::new();
        if !q.text.trim().is_empty() {
            for word in q.text.split_whitespace() {
                wh.push("(u.rel_path LIKE ? ESCAPE '\\' OR u.repo LIKE ? ESCAPE '\\')".into());
                let pat = format!("%{}%", word.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
                args.push(pat.clone());
                args.push(pat);
            }
        }
        let list = |col: &str, vals: &[String], wh: &mut Vec<String>, args: &mut Vec<String>| {
            if !vals.is_empty() {
                wh.push(format!("{col} IN ({})", vec!["?"; vals.len()].join(",")));
                args.extend(vals.iter().cloned());
            }
        };
        list("u.class", &q.classes, &mut wh, &mut args);
        list("u.state", &q.states, &mut wh, &mut args);
        if !q.repo.is_empty() {
            wh.push("u.repo = ?".into());
            args.push(q.repo.clone());
        }
        match q.mention.as_str() {
            "any" => wh.push("COALESCE(m.n, 0) > 0".into()),
            "content" => wh.push("COALESCE(m.c, 0) > 0".into()),
            "none" => wh.push("COALESCE(m.n, 0) = 0".into()),
            _ => {}
        }
        if !q.kinds.is_empty() {
            wh.push(format!(
                "EXISTS (SELECT 1 FROM mentions x WHERE x.repo = u.repo AND x.rel_path = u.rel_path AND x.kind IN ({}))",
                vec!["?"; q.kinds.len()].join(",")
            ));
            args.extend(q.kinds.iter().cloned());
        }
        if !q.ext.is_empty() {
            wh.push("lower(u.rel_path) LIKE ?".into());
            args.push(format!("%.{}", q.ext.to_lowercase()));
        }
        let limit = if q.limit == 0 { 500 } else { q.limit.min(5000) };
        let sql = format!(
            "{} {} ORDER BY (u.state IN ('missing','moved')) DESC,
                       (u.class = 'secret' AND COALESCE(m.c, 0) > 0) DESC,
                       m.last DESC, u.repo, u.rel_path LIMIT {limit}",
            union_sql(),
            if wh.is_empty() { String::new() } else { format!("WHERE {}", wh.join(" AND ")) }
        );
        let mut st = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = st.query_map(params_from_iter(args.iter()), row).map_err(|e| e.to_string())?;
        rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
    }

    /// 1 件
    pub fn row_of(&self, repo: &str, rel_path: &str) -> Result<Option<Row>, String> {
        let sql = format!("{} WHERE u.repo = ?1 AND u.rel_path = ?2", union_sql());
        self.conn
            .query_row(&sql, params![repo, rel_path], row)
            .optional()
            .map_err(|e| e.to_string())
    }

    /// 印の数
    pub fn alerts(&self) -> Result<Alerts, String> {
        let one = |sql: &str| -> Result<i64, String> {
            self.conn.query_row(sql, [], |r| r.get(0)).map_err(|e| e.to_string())
        };
        Ok(Alerts {
            missing: one("SELECT COUNT(*) FROM files WHERE state = 'missing'")?,
            moved: one("SELECT COUNT(*) FROM files WHERE state = 'moved'")?,
            secrets_sent: self.secrets_sent()?.len() as i64,
            repos_gone: one("SELECT COUNT(*) FROM repos WHERE gone_since IS NOT NULL")?,
            last_scan: self
                .conn
                .query_row("SELECT MAX(last_scan) FROM repos", [], |r| r.get(0))
                .map_err(|e| e.to_string())?,
            repos: one("SELECT COUNT(*) FROM repos")?,
            files: one("SELECT COUNT(*) FROM files WHERE class != 'generated'")?,
            docs: one("SELECT COUNT(*) FROM docs")?,
        })
    }

    /// 記録にあるリポジトリ (フォルダごと無いものも)
    pub fn repos(&self) -> Result<Vec<(String, bool)>, String> {
        let mut st = self
            .conn
            .prepare("SELECT repo, gone_since IS NOT NULL FROM repos ORDER BY repo")
            .map_err(|e| e.to_string())?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map_err(|e| e.to_string())?;
        rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
    }

    /// そのリポジトリの人の上書き
    pub fn overrides_for(&self, repo: &str) -> Result<Vec<(String, Class)>, String> {
        let mut st = self
            .conn
            .prepare("SELECT rel_prefix, class FROM overrides WHERE repo = ?1")
            .map_err(|e| e.to_string())?;
        let rows = st
            .query_map(params![repo], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            let (p, c) = r.map_err(|e| e.to_string())?;
            if let Some(c) = Class::from_label(&c) {
                out.push((p, c));
            }
        }
        Ok(out)
    }

    /// 区分を付け替える。記録の区分もすぐ直す (生成物にしたフォルダの中は、次の取り込みで記録から外れる)
    pub fn set_override(&self, repo: &str, rel_prefix: &str, class: Class) -> Result<(), String> {
        let p = rel_prefix.trim_end_matches('/');
        self.conn
            .execute(
                "INSERT INTO overrides (repo, rel_prefix, class) VALUES (?1, ?2, ?3)
                 ON CONFLICT(repo, rel_prefix) DO UPDATE SET class = excluded.class",
                params![repo, p, class.label()],
            )
            .map_err(|e| e.to_string())?;
        self.conn
            .execute(
                "UPDATE files SET class = ?3 WHERE repo = ?1 AND (rel_path = ?2 OR substr(rel_path, 1, length(?2) + 1) = ?2 || '/')",
                params![repo, p, class.label()],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 付け替えを外す (区分は次の取り込みで規則に戻る)
    pub fn clear_override(&self, repo: &str, rel_prefix: &str) -> Result<bool, String> {
        let n = self
            .conn
            .execute(
                "DELETE FROM overrides WHERE repo = ?1 AND rel_prefix = ?2",
                params![repo, rel_prefix.trim_end_matches('/')],
            )
            .map_err(|e| e.to_string())?;
        Ok(n > 0)
    }

    /// そのパスに当たっている付け替え (いちばん長いもの)
    pub fn override_of(&self, repo: &str, rel_path: &str) -> Result<Option<(String, String)>, String> {
        self.conn
            .query_row(
                "SELECT rel_prefix, class FROM overrides
                 WHERE repo = ?1 AND (?2 = rel_prefix OR substr(?2, 1, length(rel_prefix) + 1) = rel_prefix || '/')
                 ORDER BY length(rel_prefix) DESC LIMIT 1",
                params![repo, rel_path],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|e| e.to_string())
    }

    /// git の資料の一覧をリポジトリごと入れ替える
    pub fn set_docs(&mut self, repo: &Path, rels: &[String]) -> Result<(), String> {
        let repo_s = repo.to_string_lossy().into_owned();
        let tx = self.conn.transaction().map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM docs WHERE repo = ?1", params![repo_s]).map_err(|e| e.to_string())?;
        for rel in rels {
            let (size, mt) = match std::fs::metadata(repo.join(rel)) {
                Ok(m) => (
                    m.len() as i64,
                    m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_nanos() as i64),
                ),
                Err(_) => (0, None),
            };
            tx.execute(
                "INSERT OR REPLACE INTO docs (repo, rel_path, size, mtime_ns) VALUES (?1, ?2, ?3, ?4)",
                params![repo_s, rel, size, mt],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::{scan_repo, scan_repo_with};
    use std::fs;
    use std::process::Command;

    #[test]
    fn search_filters_overrides_and_docs() {
        let t = std::env::temp_dir().join(format!("offstage-query-{}", std::process::id()));
        let _ = fs::remove_dir_all(&t);
        let repo = t.join("proj");
        fs::create_dir_all(repo.join("sandbox/_shots")).unwrap();
        assert!(Command::new("git").arg("-C").arg(&repo).args(["init", "-q"]).status().unwrap().success());
        fs::write(repo.join(".gitignore"), "sandbox/\n").unwrap();
        fs::write(repo.join("sandbox/memo.md"), "m").unwrap();
        fs::write(repo.join("sandbox/demo_token.json"), "{}").unwrap();
        fs::write(repo.join("sandbox/_shots/a.png"), "p").unwrap();
        fs::write(repo.join("README.md"), "r").unwrap();
        let mut s = Store::open(&t.join("idx/index.sqlite")).unwrap();
        s.apply_run(&[scan_repo(&repo).unwrap()]).unwrap();
        s.set_docs(&repo, &["README.md".to_string()]).unwrap();
        let rp = repo.to_string_lossy().into_owned();

        let all = s.search(&Query::default()).unwrap();
        assert_eq!(all.len(), 4);
        let secret = s.search(&Query { classes: vec!["secret".into()], ..Default::default() }).unwrap();
        assert_eq!(secret.iter().map(|r| r.rel_path.as_str()).collect::<Vec<_>>(), vec!["sandbox/demo_token.json"]);
        let docs = s.search(&Query { classes: vec!["doc".into()], ..Default::default() }).unwrap();
        assert_eq!(docs[0].rel_path, "README.md");
        let md = s.search(&Query { ext: "md".into(), ..Default::default() }).unwrap();
        assert_eq!(md.len(), 2);
        let txt = s.search(&Query { text: "SHOTS".into(), ..Default::default() }).unwrap();
        assert_eq!(txt.len(), 1);
        // LIKE の % や _ は文字として扱う
        assert_eq!(s.search(&Query { text: "_shots".into(), ..Default::default() }).unwrap().len(), 1);
        assert_eq!(s.search(&Query { text: "%".into(), ..Default::default() }).unwrap().len(), 0);

        // 付け替え: すぐ記録に効き、次の取り込みでも保たれる
        s.set_override(&rp, "sandbox/_shots", Class::ToolData).unwrap();
        assert_eq!(s.row_of(&rp, "sandbox/_shots/a.png").unwrap().unwrap().class, "tooldata");
        assert_eq!(s.override_of(&rp, "sandbox/_shots/a.png").unwrap().unwrap().0, "sandbox/_shots");
        let ov = s.overrides_for(&rp).unwrap();
        s.apply_run(&[scan_repo_with(&repo, &ov).unwrap()]).unwrap();
        assert_eq!(s.row_of(&rp, "sandbox/_shots/a.png").unwrap().unwrap().class, "tooldata");

        // 消えたものが先に並ぶ
        fs::remove_file(repo.join("sandbox/memo.md")).unwrap();
        let c = s.apply_run(&[scan_repo_with(&repo, &ov).unwrap()]).unwrap();
        assert_eq!(c.missing.len(), 1);
        let all = s.search(&Query::default()).unwrap();
        assert_eq!((all[0].rel_path.as_str(), all[0].state.as_str()), ("sandbox/memo.md", "missing"));
        let a = s.alerts().unwrap();
        assert_eq!((a.missing, a.files, a.docs, a.repos), (1, 3, 1, 1));
        drop(s);
        let _ = fs::remove_dir_all(&t);
    }

    #[test]
    fn filter_by_how_it_appeared_in_conversations() {
        let t = std::env::temp_dir().join(format!("offstage-kinds-{}", std::process::id()));
        let _ = fs::remove_dir_all(&t);
        let s = Store::open(&t.join("index.sqlite")).unwrap();
        for (p, k) in [("a.md", "read"), ("b.md", "bash"), ("b.md", "text"), ("c.md", "text")] {
            s.conn
                .execute(
                    "INSERT OR IGNORE INTO docs (repo, rel_path, size, mtime_ns) VALUES ('R', ?1, 0, NULL)",
                    params![p],
                )
                .unwrap();
            s.conn
                .execute(
                    "INSERT INTO mentions (log_path, line_no, session_id, ts, kind, tool, repo, rel_path, origin)
                     VALUES ('L', 1, 'S', NULL, ?2, NULL, 'R', ?1, 'doc')",
                    params![p, k],
                )
                .unwrap();
        }
        let names = |kinds: &[&str]| -> Vec<String> {
            let mut v: Vec<String> = s
                .search(&Query { kinds: kinds.iter().map(|k| k.to_string()).collect(), ..Default::default() })
                .unwrap()
                .into_iter()
                .map(|r| r.rel_path)
                .collect();
            v.sort();
            v
        };
        assert_eq!(names(&["bash"]), vec!["b.md"]);
        assert_eq!(names(&["text"]), vec!["b.md", "c.md"]);
        assert_eq!(names(&["read", "bash"]), vec!["a.md", "b.md"]);
        assert_eq!(names(&[]).len(), 3);
        drop(s);
        let _ = fs::remove_dir_all(&t);
    }
}
