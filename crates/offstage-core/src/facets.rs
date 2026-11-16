//! 絞り込みの選択肢ごとの件数と、種類のまとまり (画像・文章など)。
//!
//! 件数は「その選択肢を選んだら何件になるか」: ほかの欄の条件は効かせたまま、その欄の条件だけ外して数える。
//! 記録は 1 回だけ読み、欄ごとの当否を手元で判定して数える。

use crate::query::{Query, Row};
use crate::store::Store;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 種類のまとまり: (鍵, 拡張子)。名前は画面が持つ
pub const KIND_GROUPS: &[(&str, &[&str])] = &[
    ("image", &["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico", "tif", "tiff", "heic", "psd"]),
    ("video", &["mp4", "mov", "webm", "mkv", "avi", "m4v"]),
    ("audio", &["mp3", "wav", "ogg", "flac", "m4a", "aac", "mid", "midi"]),
    ("text", &["md", "txt", "rst", "org", "adoc", "log"]),
    ("office", &["pdf", "docx", "doc", "xlsx", "xls", "pptx", "ppt", "odt", "ods", "html", "htm"]),
    ("data", &["csv", "tsv", "json", "jsonl", "ndjson", "yaml", "yml", "toml", "xml", "sqlite", "db", "parquet"]),
    (
        "code",
        &["py", "js", "mjs", "cjs", "ts", "tsx", "jsx", "rs", "go", "java", "kt", "cs", "cpp", "c", "h", "gd", "ps1", "sh", "bat", "cmd", "css", "sql"],
    ),
    ("archive", &["zip", "7z", "tar", "gz", "rar"]),
    ("model", &["blend", "fbx", "obj", "glb", "gltf", "stl"]),
];

/// 拡張子 (小文字、点なし)。無ければ空
pub fn ext_of(rel: &str) -> String {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    match name.rfind('.') {
        Some(i) if i > 0 => name[i + 1..].to_lowercase(),
        _ => String::new(),
    }
}

/// そのまとまりの拡張子 (知らない鍵なら None)
pub fn kind_exts(kind: &str) -> Option<&'static [&'static str]> {
    KIND_GROUPS.iter().find(|(k, _)| *k == kind).map(|(_, e)| *e)
}

/// ファイルのまとまり (どれにも当たらない・フォルダなら None)
pub fn kind_of(r: &Row) -> Option<&'static str> {
    if r.is_dir {
        return None;
    }
    let e = ext_of(&r.rel_path);
    KIND_GROUPS.iter().find(|(_, exts)| exts.contains(&e.as_str())).map(|(k, _)| *k)
}

/// 期間の選択肢 (画面の手元の時刻で決めた範囲 [since, until))
#[derive(Debug, Clone, Deserialize)]
pub struct PeriodOpt {
    pub key: String,
    pub since: Option<i64>,
    pub until: Option<i64>,
}

/// 欄ごとの、選択肢の値 → 件数。"" は「すべて」(区分では生成以外)
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Facets {
    pub period: HashMap<String, i64>,
    pub kind: HashMap<String, i64>,
    pub repo: HashMap<String, i64>,
    pub class: HashMap<String, i64>,
    pub mention: HashMap<String, i64>,
    pub state: HashMap<String, i64>,
}

fn in_range(m: Option<i64>, since: Option<i64>, until: Option<i64>) -> bool {
    if since.is_none() && until.is_none() {
        return true;
    }
    let Some(m) = m else { return false };
    since.is_none_or(|s| m >= s) && until.is_none_or(|u| m < u)
}

fn kind_ok(q: &Query, r: &Row) -> bool {
    let by_group = q.kind.is_empty() || kind_of(r) == Some(q.kind.as_str());
    let by_ext = q.ext.is_empty() || r.rel_path.to_lowercase().ends_with(&format!(".{}", q.ext.to_lowercase()));
    by_group && by_ext
}

fn mention_ok(mention: &str, r: &Row) -> bool {
    match mention {
        "any" => r.mentions > 0,
        "content" => r.content_mentions > 0,
        "none" => r.mentions == 0,
        _ => true,
    }
}

/// 件数を数える (rows は欄の条件を外して取ったもの)
pub fn count(rows: &[Row], q: &Query, periods: &[PeriodOpt]) -> Facets {
    let mut f = Facets::default();
    let add = |m: &mut HashMap<String, i64>, k: &str| *m.entry(k.to_string()).or_insert(0) += 1;
    for r in rows {
        // 欄ごとの当否: 期間・種類・リポジトリ・区分・会話・状態
        let ok = [
            in_range(r.modified, q.since, q.until),
            kind_ok(q, r),
            q.repo.is_empty() || r.repo == q.repo,
            q.classes.is_empty() || q.classes.contains(&r.class),
            mention_ok(&q.mention, r),
            q.states.is_empty() || q.states.contains(&r.state),
        ];
        let others = |d: usize| ok.iter().enumerate().all(|(i, v)| i == d || *v);
        if others(0) {
            add(&mut f.period, "");
            for p in periods {
                if in_range(r.modified, p.since, p.until) {
                    add(&mut f.period, &p.key);
                }
            }
        }
        if others(1) {
            add(&mut f.kind, "");
            if let Some(k) = kind_of(r) {
                add(&mut f.kind, k);
            }
        }
        if others(2) {
            add(&mut f.repo, "");
            add(&mut f.repo, &r.repo);
        }
        if others(3) {
            if r.class != "generated" {
                add(&mut f.class, "");
            }
            add(&mut f.class, &r.class);
        }
        if others(4) {
            add(&mut f.mention, "");
            for m in ["any", "content", "none"] {
                if mention_ok(m, r) {
                    add(&mut f.mention, m);
                }
            }
        }
        if others(5) {
            add(&mut f.state, "");
            add(&mut f.state, &r.state);
        }
    }
    f
}

impl Store {
    /// 絞り込みの選択肢ごとの件数。検索の文字・フォルダ・会話の出方・不明のリポジトリの条件はそのまま効かせる
    pub fn facets(&self, q: &Query, periods: &[PeriodOpt]) -> Result<Facets, String> {
        let base = Query {
            classes: vec![],
            states: vec![],
            repo: String::new(),
            mention: String::new(),
            kind: String::new(),
            ext: String::new(),
            since: None,
            until: None,
            ..q.clone()
        };
        Ok(count(&self.matching(&base)?, q, periods))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(repo: &str, rel: &str, class: &str, modified: Option<i64>, mentions: i64) -> Row {
        Row {
            repo: repo.into(),
            rel_path: rel.into(),
            class: class.into(),
            state: "present".into(),
            is_dir: false,
            size: 1,
            modified,
            folded: None,
            moved_to: None,
            state_since: None,
            mentions,
            content_mentions: 0,
            last_mention: None,
            repo_gone: false,
        }
    }

    fn rows() -> Vec<Row> {
        vec![
            row("A", "shots/a.png", "tooldata", Some(100), 1),
            row("A", "notes/plan.md", "personal", Some(50), 0),
            row("B", "notes/todo.MD", "personal", Some(100), 2),
            row("B", "build/out.bin", "generated", None, 0),
        ]
    }

    fn periods() -> Vec<PeriodOpt> {
        vec![PeriodOpt { key: "new".into(), since: Some(80), until: None }, PeriodOpt { key: "old".into(), since: None, until: Some(80) }]
    }

    #[test]
    fn kinds_by_extension() {
        assert_eq!(ext_of("a/b.TXT"), "txt");
        assert_eq!(ext_of(".gitignore"), "");
        assert_eq!(ext_of("a/noext"), "");
        assert_eq!(kind_of(&rows()[0]), Some("image"));
        assert_eq!(kind_of(&rows()[2]), Some("text")); // 大文字の拡張子も
        assert_eq!(kind_of(&rows()[3]), None);
        assert!(kind_exts("image").unwrap().contains(&"png"));
        assert_eq!(kind_exts("nope"), None);
        // まとまりどうしで拡張子が重ならない
        let mut all: Vec<&str> = KIND_GROUPS.iter().flat_map(|(_, e)| e.iter().copied()).collect();
        let n = all.len();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), n);
    }

    #[test]
    fn counts_each_field_without_its_own_condition() {
        let none = Query::default();
        let f = count(&rows(), &none, &periods());
        assert_eq!(f.class[""], 3); // 「すべて」は生成以外
        assert_eq!(f.class["generated"], 1);
        assert_eq!(f.kind[""], 4);
        assert_eq!((f.kind["image"], f.kind["text"]), (1, 2));
        assert_eq!((f.period["new"], f.period["old"]), (2, 1)); // 更新日時の無いものはどちらにも入らない
        assert_eq!((f.mention["any"], f.mention["none"]), (2, 2));
        assert_eq!((f.repo["A"], f.repo["B"]), (2, 2));

        // 種類を「文章」にすると、ほかの欄はその中で数え、種類の欄は種類の条件を外して数える
        let q = Query { kind: "text".into(), ..Default::default() };
        let f = count(&rows(), &q, &periods());
        assert_eq!((f.repo["A"], f.repo["B"]), (1, 1));
        assert_eq!(f.period.get("old"), Some(&1));
        assert_eq!(f.kind["image"], 1);
        assert_eq!(f.kind[""], 4);

        // 2 つの欄: リポジトリ B かつ 新しい → 種類の欄は B の新しいものだけで数える
        let q = Query { repo: "B".into(), since: Some(80), ..Default::default() };
        let f = count(&rows(), &q, &periods());
        assert_eq!(f.kind[""], 1);
        assert_eq!(f.kind.get("image"), None);
        assert_eq!(f.period["new"], 1);
        assert_eq!(f.period[""], 2); // 期間の欄は期間を外す (B の 2 件)
        assert_eq!(f.repo["A"], 1); // リポジトリの欄はリポジトリを外す (新しいものは A に 1 件)
    }

    #[test]
    fn counts_match_search() {
        // 選択肢の件数は、それを選んで検索したときの件数と同じ
        let t = std::env::temp_dir().join(format!("offstage-facets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        let s = Store::open(&t.join("index.sqlite")).unwrap();
        for (p, c, mt) in [("a.png", "personal", 100i64), ("b.md", "personal", 50), ("c.md", "data", 100), ("d.csv", "data", 100)] {
            s.conn
                .execute(
                    "INSERT INTO files (repo, rel_path, class, is_dir, size, mtime_ns, sha256, folded, first_seen, last_seen, state, state_since)
                     VALUES ('R', ?1, ?2, 0, 1, ?3, NULL, NULL, 1, 1, 'present', 1)",
                    rusqlite::params![p, c, mt * 1_000_000_000],
                )
                .unwrap();
        }
        let q = Query { classes: vec!["data".into()], ..Default::default() };
        let f = s.facets(&q, &periods()).unwrap();
        for (kind, n) in [("text", 1), ("data", 1), ("image", 0)] {
            let hits = s.search(&Query { kind: kind.into(), ..q.clone() }).unwrap().len() as i64;
            assert_eq!((kind, f.kind.get(kind).copied().unwrap_or(0)), (kind, hits));
            assert_eq!(hits, n);
        }
        let hits = s.search(&Query { classes: vec!["personal".into()], since: Some(80), ..Default::default() }).unwrap().len() as i64;
        let f2 = s.facets(&Query { since: Some(80), ..Default::default() }, &periods()).unwrap();
        assert_eq!(f2.class["personal"], hits);
        drop(s);
        let _ = std::fs::remove_dir_all(&t);
    }
}
