//! Claude の会話の記録 (`~/.claude/projects/**/*.jsonl`) から、ファイルが会話に出た場面を拾う。
//!
//! 拾ったパスは、Offstage の記録 (消えたものも含む) か、git が管理する資料 ([`is_doc`]) に当たるものだけ残す。
//! ディスクにあるかは見ない (消えたファイルが前に会話に出ていたかを知りたいので)。
//!
//! 出方 (`kind`) は、中身が送られたかで分ける:
//! - 中身が送られた: `read` (Read)・`attached` (@ で渡した・外で直した・CLAUDE.md や memory・エディタで選んだ範囲)・
//!   `grep` (Grep の対象)・`edit` (Edit の前後の文)・`bash` (コマンドに出た。出力に中身が出たかもしれない)・`tool` (その他の道具に渡した)
//! - Claude が書いた: `write`
//! - 名前だけ: `result` (道具の結果に出た)・`text` (本文に出た)
//!
//! 2 回目からは、前回読んだところから先 (増えた分) だけ読む。

use crate::store::Store;
use regex::Regex;
use rusqlite::params;
use serde_json::Value;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// 1-2 の資料として結び付ける種類 (スクリプトは入れない)
pub const DOC_EXTS: &[&str] = &[
    "md", "markdown", "html", "htm", "pdf", "png", "jpg", "jpeg", "gif", "webp", "svg", "bmp",
    "mp4", "mov", "webm", "mkv", "avi", "mp3", "wav", "ogg", "flac", "m4a", "docx", "doc",
    "xlsx", "xls", "pptx", "ppt", "odt", "ods", "odp",
];

/// 中身が送られた出方
pub const CONTENT_KINDS: &[&str] = &["read", "attached", "grep", "edit", "bash", "tool"];

pub fn is_doc(rel_path: &str) -> bool {
    let name = rel_path.rsplit(['/', '\\']).next().unwrap_or("").to_ascii_lowercase();
    name.rsplit_once('.').is_some_and(|(_, e)| DOC_EXTS.contains(&e))
}

/// 結び付ける先
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub repo: String,
    pub rel_path: String,
    /// `offstage` (git の外の記録) か `doc` (git が管理する資料)
    pub origin: &'static str,
}

/// 結び付けてよいパスの一覧。キーは [`norm_key`] の形
pub struct Known {
    map: HashMap<String, Target>,
    /// 1 件に畳んだフォルダ (生成物のフォルダは入れない。中を読んでも結び付ける意味が無い)
    folded: HashMap<String, Target>,
    /// リポジトリの根のキー (候補を早く捨てるため)
    roots: Vec<String>,
}

impl Known {
    /// Offstage の記録の全部 (消えたもの・移動したものも) と、git が管理する資料から作る
    pub fn build(store: &Store, docs: &[(PathBuf, Vec<String>)]) -> Result<Known, String> {
        let mut map = HashMap::new();
        let mut roots: Vec<String> = Vec::new();
        let mut folded = HashMap::new();
        for (repo, rel, is_folded) in store.all_paths()? {
            let key = norm_key(&format!("{repo}\\{rel}"));
            let root = norm_key(&repo);
            if !roots.contains(&root) {
                roots.push(root);
            }
            let t = Target { repo, rel_path: rel, origin: "offstage" };
            if is_folded {
                folded.insert(key.clone(), t.clone());
            }
            map.insert(key, t);
        }
        for (repo, rels) in docs {
            let repo_s = repo.to_string_lossy().into_owned();
            let root = norm_key(&repo_s);
            if !roots.contains(&root) {
                roots.push(root);
            }
            for rel in rels {
                let key = norm_key(&format!("{repo_s}\\{rel}"));
                map.entry(key).or_insert(Target { repo: repo_s.clone(), rel_path: rel.clone(), origin: "doc" });
            }
        }
        Ok(Known { map, folded, roots })
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// 候補の文字列を当てる。後ろに日本語や句読点が続いていても、区切りをまたがない範囲で縮めて探す
    fn lookup(&self, raw: &str, base: &str) -> Option<&Target> {
        let s = clean(raw);
        if s.len() < 3 {
            return None;
        }
        let abs = |x: &str| -> String {
            if is_absolute(x) {
                x.to_string()
            } else {
                format!("{base}\\{x}")
            }
        };
        let full = norm_key(&abs(s));
        if !self.roots.iter().any(|r| full.starts_with(r.as_str())) {
            return None;
        }
        if let Some(t) = self.map.get(&full) {
            return Some(t);
        }
        let mut ends: Vec<usize> = s.char_indices().map(|(i, _)| i).skip(1).collect();
        ends.reverse();
        for end in ends {
            let head = &s[..end];
            if head.ends_with(['/', '\\']) || s[end..].starts_with(['/', '\\']) {
                break; // 区切りまで戻ったら、それはフォルダなのでやめる (2026-11-12、`out\` まで縮めて畳んだフォルダに当てていた)
            }
            if let Some(t) = self.map.get(&norm_key(&abs(head))) {
                return Some(t);
            }
        }
        // 1 件に畳んだフォルダ (ブラウザの利用者データなど) の中のファイルは、そのフォルダに結び付ける
        let mut key = full.as_str();
        while let Some(i) = key.rfind('\\') {
            key = &key[..i];
            if let Some(t) = self.folded.get(key) {
                return Some(t);
            }
        }
        None
    }
}

fn is_absolute(p: &str) -> bool {
    let b = p.as_bytes();
    (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/'))
        || p.starts_with("\\\\")
        || git_bash_drive(p).is_some()
}

/// `/c/Repos/x` → `C:\Repos\x`
fn git_bash_drive(p: &str) -> Option<String> {
    let b = p.as_bytes();
    if b.len() >= 3 && b[0] == b'/' && b[1].is_ascii_alphabetic() && b[2] == b'/' {
        Some(format!("{}:\\{}", (b[1] as char).to_ascii_uppercase(), &p[3..]))
    } else {
        None
    }
}

/// 比べるための形: 区切りを `\`、`.` と `..` を畳み、小文字にする
pub fn norm_key(p: &str) -> String {
    let p = git_bash_drive(p).unwrap_or_else(|| p.to_string());
    let p = p.replace('/', "\\");
    let mut out: Vec<&str> = Vec::new();
    for part in p.split('\\') {
        match part {
            "" | "." => {}
            ".." => {
                if out.len() > 1 {
                    out.pop();
                }
            }
            x => out.push(x),
        }
    }
    out.join("\\").to_lowercase()
}

/// 前後の飾りを外す: `file:///`・行の指定 (`#L10`・`:10:5`)・後ろの句読点
fn clean(raw: &str) -> &str {
    let mut s = raw.trim();
    for pre in ["file:///", "file://"] {
        if s.get(..pre.len()).is_some_and(|h| h.eq_ignore_ascii_case(pre)) && s.len() > pre.len() {
            s = &s[pre.len()..];
        }
    }
    if let Some(i) = s.find("#L") {
        s = &s[..i];
    }
    loop {
        let t = s.trim_end_matches(['.', ',', ';', ')', ']', '}', '\'', '"', '`', '>', '。', '、', '）', '」', '』']);
        // 行・列の指定 (:12 や :12:3)。ドライブの `C:` は残す
        let t = match t.rfind(':') {
            Some(i) if i > 1 && !t[i + 1..].is_empty() && t[i + 1..].bytes().all(|b| b.is_ascii_digit()) => &t[..i],
            _ => t,
        };
        if t.len() == s.len() {
            return s;
        }
        s = t;
    }
}

struct Patterns {
    win: Regex,
    gitbash: Regex,
    home: Regex,
    mdlink: Regex,
    backtick: Regex,
    reminder: Regex,
    selection: Regex,
    opened: Regex,
}

fn patterns() -> &'static Patterns {
    static P: OnceLock<Patterns> = OnceLock::new();
    P.get_or_init(|| Patterns {
        win: Regex::new(r#"(?:^|[^A-Za-z0-9])([A-Za-z]:[\\/][^\s`'"<>|*?()\[\]{}]*)"#).unwrap(),
        gitbash: Regex::new(r#"(?:^|[\s"'=(])(/[a-zA-Z]/[^\s`'"<>|*?()\[\]{}]*)"#).unwrap(),
        home: Regex::new(r#"(?i)(?:%USERPROFILE%|~)[\\/][^\s`'"<>|*?()\[\]{}]*"#).unwrap(),
        mdlink: Regex::new(r"\]\(([^)\s]+)\)").unwrap(),
        backtick: Regex::new(r"`([^`\n\s]+)`").unwrap(),
        reminder: Regex::new(r"(?s)<system-reminder>.*?</system-reminder>").unwrap(),
        selection: Regex::new(r"(?s)<ide_selection>(.*?)</ide_selection>").unwrap(),
        opened: Regex::new(r"(?s)<ide_opened_file>(.*?)</ide_opened_file>").unwrap(),
    })
}

/// 1 行から拾った 1 件
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub kind: &'static str,
    pub tool: Option<String>,
    pub target: Target,
}

/// 会話の記録を 1 行ずつ読む。`cwd` は行をまたいで引き継ぐ
pub struct LineReader<'a> {
    known: &'a Known,
    pub cwd: String,
    home: String,
}

impl<'a> LineReader<'a> {
    pub fn new(known: &'a Known, cwd: &str) -> Self {
        let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).unwrap_or_default();
        LineReader { known, cwd: cwd.to_string(), home }
    }

    pub fn line(&mut self, raw: &str) -> (Option<Value>, Vec<Hit>) {
        let Ok(o) = serde_json::from_str::<Value>(raw) else {
            return (None, Vec::new());
        };
        if let Some(c) = o.get("cwd").and_then(Value::as_str) {
            self.cwd = c.to_string();
        }
        let mut hits = Vec::new();
        let ty = o.get("type").and_then(Value::as_str).unwrap_or("");
        let content = o.pointer("/message/content");
        match ty {
            "assistant" => {
                for b in content.and_then(Value::as_array).into_iter().flatten() {
                    match b.get("type").and_then(Value::as_str) {
                        Some("text") => self.text(b.get("text"), "text", None, &mut hits),
                        Some("tool_use") => self.tool_use(b, &mut hits),
                        _ => {} // thinking は Claude が書いたもので、名前も前に出ている
                    }
                }
            }
            "user" => match content {
                Some(Value::String(s)) => self.prompt(s, &mut hits),
                Some(Value::Array(blocks)) => {
                    for b in blocks {
                        match b.get("type").and_then(Value::as_str) {
                            Some("text") => {
                                if let Some(s) = b.get("text").and_then(Value::as_str) {
                                    self.prompt(s, &mut hits);
                                }
                            }
                            Some("tool_result") => match b.get("content") {
                                Some(Value::String(s)) => self.scan(s, "result", None, &mut hits),
                                Some(Value::Array(parts)) => {
                                    for p in parts {
                                        self.text(p.get("text"), "result", None, &mut hits);
                                    }
                                }
                                _ => {}
                            },
                            _ => {}
                        }
                    }
                }
                _ => {}
            },
            "attachment" => {
                let a = o.get("attachment");
                let at = a.and_then(|a| a.get("type")).and_then(Value::as_str).unwrap_or("");
                match at {
                    "file" | "edited_text_file" => {
                        self.one(a.and_then(|a| a.get("filename")), "attached", Some(at), &mut hits)
                    }
                    "compact_file_reference" => {
                        self.one(a.and_then(|a| a.get("filename")), "text", Some(at), &mut hits)
                    }
                    "instructions" => {
                        for f in a.and_then(|a| a.get("files")).and_then(Value::as_array).into_iter().flatten() {
                            self.one(f.get("path"), "attached", Some(at), &mut hits);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        // 同じ行で同じファイル・同じ出方は 1 件にする
        let mut seen = Vec::new();
        hits.retain(|h| {
            let k = (h.kind, h.target.repo.clone(), h.target.rel_path.clone());
            if seen.contains(&k) {
                false
            } else {
                seen.push(k);
                true
            }
        });
        (Some(o), hits)
    }

    fn tool_use(&mut self, b: &Value, hits: &mut Vec<Hit>) {
        let name = b.get("name").and_then(Value::as_str).unwrap_or("");
        let input = b.get("input");
        let field = |k: &str| input.and_then(|i| i.get(k));
        let tool = Some(name);
        match name {
            "Read" => self.one(field("file_path"), "read", tool, hits),
            "Edit" | "MultiEdit" => self.one(field("file_path"), "edit", tool, hits),
            "NotebookEdit" => self.one(field("notebook_path"), "edit", tool, hits),
            "Write" => self.one(field("file_path"), "write", tool, hits),
            "Grep" => self.one(field("path"), "grep", tool, hits),
            "Glob" => self.one(field("path"), "text", tool, hits),
            "Bash" | "PowerShell" => {
                if let Some(cmd) = field("command").and_then(Value::as_str) {
                    self.scan(cmd, "bash", tool, hits);
                    self.bare_tokens(cmd, "bash", tool, hits);
                }
            }
            _ => {
                // その他の道具 (Artifact の公開など): パスの欄は中身が渡ったとみなし、残りの文は名前だけ
                for k in ["file_path", "path", "notebook_path"] {
                    self.one(field(k), "tool", tool, hits);
                }
                for k in ["file_paths", "paths"] {
                    for v in field(k).and_then(Value::as_array).into_iter().flatten() {
                        self.one(Some(v), "tool", tool, hits);
                    }
                }
                if let Some(i) = input {
                    self.strings(i, tool, hits);
                }
            }
        }
    }

    /// 入力の中の文字列を全部、名前だけの出方で見る
    fn strings(&mut self, v: &Value, tool: Option<&str>, hits: &mut Vec<Hit>) {
        match v {
            Value::String(s) => self.scan(s, "text", tool, hits),
            Value::Array(a) => a.iter().for_each(|x| self.strings(x, tool, hits)),
            Value::Object(m) => m.values().for_each(|x| self.strings(x, tool, hits)),
            _ => {}
        }
    }

    /// 人の発言。CLAUDE.md などを差し込んだ system-reminder は飛ばし、エディタで選んだ範囲は中身が送られたとみなす
    fn prompt(&mut self, s: &str, hits: &mut Vec<Hit>) {
        let p = patterns();
        let s = p.reminder.replace_all(s, " ");
        for c in p.selection.captures_iter(&s) {
            self.scan(&c[1], "attached", Some("ide_selection"), hits);
        }
        for c in p.opened.captures_iter(&s) {
            self.scan(&c[1], "text", Some("ide_opened_file"), hits);
        }
        let no_sel = p.selection.replace_all(&s, " ");
        let rest = p.opened.replace_all(&no_sel, " ");
        self.scan(&rest, "text", None, hits);
    }

    fn text(&mut self, v: Option<&Value>, kind: &'static str, tool: Option<&str>, hits: &mut Vec<Hit>) {
        if let Some(s) = v.and_then(Value::as_str) {
            self.scan(s, kind, tool, hits);
        }
    }

    fn one(&mut self, v: Option<&Value>, kind: &'static str, tool: Option<&str>, hits: &mut Vec<Hit>) {
        if let Some(s) = v.and_then(Value::as_str) {
            self.push(s, kind, tool, hits);
        }
    }

    fn push(&mut self, cand: &str, kind: &'static str, tool: Option<&str>, hits: &mut Vec<Hit>) {
        let mut c = cand.to_string();
        if let Some(rest) = c.strip_prefix('~').filter(|r| r.starts_with(['/', '\\'])) {
            c = format!("{}{}", self.home, rest);
        } else if c.get(..13).is_some_and(|h| h.eq_ignore_ascii_case("%USERPROFILE%")) && c.len() > 13 {
            c = format!("{}{}", self.home, &c[13..]);
        }
        if let Some(t) = self.known.lookup(&c, &self.cwd) {
            hits.push(Hit { kind, tool: tool.map(str::to_string), target: t.clone() });
        }
    }

    /// 文の中のパスらしいものを拾う
    fn scan(&mut self, s: &str, kind: &'static str, tool: Option<&str>, hits: &mut Vec<Hit>) {
        let p = patterns();
        let mut cands: Vec<String> = Vec::new();
        for c in p.win.captures_iter(s) {
            cands.push(c[1].to_string());
        }
        for c in p.gitbash.captures_iter(s) {
            cands.push(c[1].to_string());
        }
        for m in p.home.find_iter(s) {
            cands.push(m.as_str().to_string());
        }
        for c in p.mdlink.captures_iter(s) {
            let x = &c[1];
            if x.contains("://") && !x.to_ascii_lowercase().starts_with("file:") {
                continue;
            }
            cands.push(x.to_string());
        }
        for c in p.backtick.captures_iter(s) {
            let x = &c[1];
            if (x.contains('/') || x.contains('\\')) && !x.contains("://") {
                cands.push(x.to_string());
            }
        }
        for c in cands {
            self.push(&c, kind, tool, hits);
        }
    }

    /// コマンドの中の、区切りを含む素の語 (相対パス)
    fn bare_tokens(&mut self, cmd: &str, kind: &'static str, tool: Option<&str>, hits: &mut Vec<Hit>) {
        for tok in cmd.split(|c: char| c.is_whitespace() || "\"'`;|&<>()=".contains(c)) {
            if (tok.contains('/') || tok.contains('\\')) && !tok.contains("://") && !is_absolute(tok) {
                self.push(tok, kind, tool, hits);
            }
        }
    }
}

/// 会話の 1 こま (場面の前後を見せるため)
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Turn {
    pub line_no: i64,
    /// user (人の発言) / assistant (Claude の文) / tool (道具の呼び出し) / result (道具の結果) / attachment (添付)
    pub role: &'static str,
    /// 道具の名前・添付の種類
    pub label: Option<String>,
    pub text: String,
    /// 結び付けた行か
    pub hit: bool,
    pub timestamp: Option<String>,
}

/// 1 こまの文の長さの上限 (それより長い結果は切る)
const TURN_TEXT_MAX: usize = 4000;
/// 場面より後に見せるこまの数
const TURNS_AFTER: usize = 8;
/// 直前の人の発言を探しにさかのぼる行数
const LOOK_BACK: i64 = 400;

fn cut(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() <= TURN_TEXT_MAX {
        return s.to_string();
    }
    let head: String = s.chars().take(TURN_TEXT_MAX).collect();
    format!("{head}\n…（{} 文字を省略）", s.chars().count() - TURN_TEXT_MAX)
}

/// 道具の呼び出しを 1 行の説明にする
fn tool_summary(name: &str, input: Option<&Value>) -> String {
    let f = |k: &str| input.and_then(|i| i.get(k)).and_then(Value::as_str).unwrap_or("");
    match name {
        "Read" | "Write" => f("file_path").to_string(),
        "Edit" | "MultiEdit" => {
            let (o, n) = (f("old_string"), f("new_string"));
            if o.is_empty() && n.is_empty() {
                f("file_path").to_string()
            } else {
                format!("{}\n- {}\n+ {}", f("file_path"), o, n)
            }
        }
        "NotebookEdit" => f("notebook_path").to_string(),
        "Bash" | "PowerShell" => f("command").to_string(),
        "Grep" => format!("{}  {}", f("pattern"), f("path")).trim().to_string(),
        "Glob" => format!("{}  {}", f("pattern"), f("path")).trim().to_string(),
        _ => input.map(|i| i.to_string()).unwrap_or_default(),
    }
}

/// 1 行を、見せるこまにする。見せないもの (題・内部の記録・system-reminder だけの行) は空
fn turns_of(line_no: i64, o: &Value) -> Vec<Turn> {
    let ts = o.get("timestamp").and_then(Value::as_str).map(str::to_string);
    let mk = |role, label: Option<String>, text: String| Turn { line_no, role, label, text: cut(&text), hit: false, timestamp: ts.clone() };
    let mut out = Vec::new();
    let content = o.pointer("/message/content");
    match o.get("type").and_then(Value::as_str).unwrap_or("") {
        "user" => {
            let strip = |s: &str| patterns().reminder.replace_all(s, "").trim().to_string();
            match content {
                Some(Value::String(s)) => {
                    let t = strip(s);
                    if !t.is_empty() {
                        out.push(mk("user", None, t));
                    }
                }
                Some(Value::Array(blocks)) => {
                    for b in blocks {
                        match b.get("type").and_then(Value::as_str) {
                            Some("text") => {
                                let t = strip(b.get("text").and_then(Value::as_str).unwrap_or(""));
                                if !t.is_empty() {
                                    out.push(mk("user", None, t));
                                }
                            }
                            Some("tool_result") => {
                                let t = match b.get("content") {
                                    Some(Value::String(s)) => s.clone(),
                                    Some(Value::Array(parts)) => parts
                                        .iter()
                                        .filter_map(|p| p.get("text").and_then(Value::as_str))
                                        .collect::<Vec<_>>()
                                        .join("\n"),
                                    _ => String::new(),
                                };
                                out.push(mk("result", None, t));
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        "assistant" => {
            for b in content.and_then(Value::as_array).into_iter().flatten() {
                match b.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        let t = b.get("text").and_then(Value::as_str).unwrap_or("").to_string();
                        if !t.trim().is_empty() {
                            out.push(mk("assistant", None, t));
                        }
                    }
                    Some("tool_use") => {
                        let name = b.get("name").and_then(Value::as_str).unwrap_or("").to_string();
                        let s = tool_summary(&name, b.get("input"));
                        out.push(mk("tool", Some(name), s));
                    }
                    _ => {}
                }
            }
        }
        "attachment" => {
            let a = o.get("attachment");
            let at = a.and_then(|a| a.get("type")).and_then(Value::as_str).unwrap_or("");
            let name = |v: Option<&Value>| v.and_then(Value::as_str).unwrap_or("").to_string();
            match at {
                "file" | "edited_text_file" | "compact_file_reference" => {
                    out.push(mk("attachment", Some(at.into()), name(a.and_then(|a| a.get("filename")))))
                }
                "instructions" => {
                    let files: Vec<String> = a
                        .and_then(|a| a.get("files"))
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .map(|f| name(f.get("path")))
                        .collect();
                    out.push(mk("attachment", Some(at.into()), files.join("\n")));
                }
                _ => {}
            }
        }
        _ => {}
    }
    out
}

/// 場面 (記録の `line_no` 行目) の前後のやり取り: 直前の人の発言から、場面の後 [`TURNS_AFTER`] こままで
pub fn excerpt(log: &Path, line_no: i64) -> Result<Vec<Turn>, String> {
    use std::io::BufRead;
    let f = std::fs::File::open(log).map_err(|e| format!("{}: {e}", log.display()))?;
    let from = (line_no - LOOK_BACK).max(1);
    let mut before: Vec<Turn> = Vec::new();
    let mut after: Vec<Turn> = Vec::new();
    let mut found = false;
    for (i, line) in std::io::BufReader::new(f).split(b'\n').enumerate() {
        let n = i as i64 + 1;
        if n < from {
            continue;
        }
        let Ok(raw) = line else { break };
        let Ok(o) = serde_json::from_slice::<Value>(&raw) else { continue };
        let mut ts = turns_of(n, &o);
        if n < line_no {
            // 人の発言 (道具の結果ではないもの) が来たら、そこから数え直す
            if ts.iter().any(|t| t.role == "user") {
                before.clear();
            }
            before.append(&mut ts);
        } else if n == line_no {
            found = true;
            for t in &mut ts {
                t.hit = true;
            }
            before.append(&mut ts);
        } else {
            after.append(&mut ts);
            if after.len() >= TURNS_AFTER {
                break;
            }
        }
    }
    if !found {
        return Err(format!("記録の {line_no} 行目が見つかりません (記録が書き直された可能性があります)"));
    }
    after.truncate(TURNS_AFTER);
    before.extend(after);
    Ok(before)
}

/// 結び付けの結果の数
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LinkStats {
    pub logs_read: usize,
    pub logs_skipped: usize,
    pub bytes_read: u64,
    pub lines: usize,
    pub hits: usize,
}

/// 会話に出た 1 件 (画面と試験用)
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mention {
    pub log_path: String,
    pub line_no: i64,
    pub session_id: String,
    pub timestamp: Option<String>,
    pub kind: String,
    pub tool: Option<String>,
}

fn jsonl_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        let p = e.path();
        if ft.is_dir() {
            jsonl_files(&p, out);
        } else if ft.is_file() && p.extension().is_some_and(|x| x == "jsonl") {
            out.push(p);
        }
    }
}

/// Claude の会話の記録の置き場所 `~/.claude/projects`
pub fn default_projects_dir() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(home).join(".claude").join("projects"))
}

impl Store {
    /// `projects_dir` の下の会話の記録を読み、結び付けを記録する (増えた分だけ)
    pub fn link_sessions(&mut self, projects_dir: &Path, known: &Known) -> Result<LinkStats, String> {
        let mut logs = Vec::new();
        jsonl_files(projects_dir, &mut logs);
        logs.sort();
        let mut stats = LinkStats::default();
        let tx = self.conn.transaction().map_err(|e| e.to_string())?;
        for log in logs {
            let key = log.to_string_lossy().into_owned();
            let Ok(meta) = std::fs::metadata(&log) else { continue };
            let size = meta.len() as i64;
            let prev: Option<(i64, i64, String)> = tx
                .query_row(
                    "SELECT offset, lines, cwd FROM session_logs WHERE path = ?1",
                    params![key],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .ok();
            let (mut offset, mut lines, cwd) = prev.unwrap_or((0, 0, String::new()));
            if size < offset {
                // 短くなった (書き直された): 最初から読み直す
                tx.execute("DELETE FROM mentions WHERE log_path = ?1", params![key]).map_err(|e| e.to_string())?;
                offset = 0;
                lines = 0;
            }
            if size == offset {
                stats.logs_skipped += 1;
                continue;
            }
            let mut f = std::fs::File::open(&log).map_err(|e| format!("{key}: {e}"))?;
            f.seek(SeekFrom::Start(offset as u64)).map_err(|e| e.to_string())?;
            let mut buf = Vec::with_capacity((size - offset) as usize);
            f.take((size - offset) as u64).read_to_end(&mut buf).map_err(|e| e.to_string())?;
            // 書きかけの最後の行は次に回す
            let Some(end) = buf.iter().rposition(|b| *b == b'\n') else {
                stats.logs_skipped += 1;
                continue;
            };
            let body = &buf[..=end];
            let stem = log.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let mut reader = LineReader::new(known, &cwd);
            for raw in body.split(|b| *b == b'\n') {
                if raw.is_empty() {
                    continue;
                }
                lines += 1;
                stats.lines += 1;
                let text = String::from_utf8_lossy(raw);
                let (obj, hits) = reader.line(&text);
                if hits.is_empty() {
                    continue;
                }
                let o = obj.unwrap_or(Value::Null);
                let session = o.get("sessionId").and_then(Value::as_str).unwrap_or(&stem).to_string();
                let ts = o.get("timestamp").and_then(Value::as_str).map(str::to_string);
                for h in hits {
                    stats.hits += 1;
                    tx.execute(
                        "INSERT OR IGNORE INTO mentions
                           (log_path, line_no, session_id, ts, kind, tool, repo, rel_path, origin)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                        params![key, lines, session, ts, h.kind, h.tool, h.target.repo, h.target.rel_path, h.target.origin],
                    )
                    .map_err(|e| e.to_string())?;
                }
            }
            offset += body.len() as i64;
            stats.bytes_read += body.len() as u64;
            stats.logs_read += 1;
            tx.execute(
                "INSERT INTO session_logs (path, offset, lines, cwd) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(path) DO UPDATE SET offset = excluded.offset, lines = excluded.lines, cwd = excluded.cwd",
                params![key, offset, lines, reader.cwd],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(stats)
    }

    /// Offstage が読んだ会話の記録か (画面から渡されたパスをそのまま開かないため)
    pub fn is_known_log(&self, path: &str) -> Result<bool, String> {
        self.conn
            .query_row("SELECT COUNT(*) FROM session_logs WHERE path = ?1", params![path], |r| r.get::<_, i64>(0))
            .map(|n| n > 0)
            .map_err(|e| e.to_string())
    }

    /// 結び付けを全部消して、次に最初から読み直させる (規則を変えたとき)
    pub fn reset_links(&self) -> Result<(), String> {
        self.conn
            .execute_batch("DELETE FROM mentions; DELETE FROM session_logs;")
            .map_err(|e| e.to_string())
    }

    /// そのファイルが会話に出た場面 (古い順)
    pub fn mentions_of(&self, repo: &str, rel_path: &str) -> Result<Vec<Mention>, String> {
        let mut st = self
            .conn
            .prepare(
                "SELECT log_path, line_no, session_id, ts, kind, tool FROM mentions
                 WHERE repo = ?1 AND rel_path = ?2 ORDER BY ts, log_path, line_no",
            )
            .map_err(|e| e.to_string())?;
        let rows = st
            .query_map(params![repo, rel_path], |r| {
                Ok(Mention {
                    log_path: r.get(0)?,
                    line_no: r.get(1)?,
                    session_id: r.get(2)?,
                    timestamp: r.get(3)?,
                    kind: r.get(4)?,
                    tool: r.get(5)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
    }

    /// 機密の疑いのファイルで、中身が会話に送られたもの (RepoTether に印を出す元)。(リポジトリ, パス, 回数)
    pub fn secrets_sent(&self) -> Result<Vec<(String, String, i64)>, String> {
        let kinds = CONTENT_KINDS.iter().map(|k| format!("'{k}'")).collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT m.repo, m.rel_path, COUNT(*) FROM mentions m
             JOIN files f ON f.repo = m.repo AND f.rel_path = m.rel_path
             WHERE f.class = 'secret' AND m.kind IN ({kinds})
             GROUP BY m.repo, m.rel_path ORDER BY m.repo, m.rel_path"
        );
        let mut st = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = st
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
    }

    /// 出方 × 区分ごとの件数 (区分は doc か、Offstage の記録の区分)
    pub fn mention_counts(&self) -> Result<Vec<(String, String, i64)>, String> {
        let mut st = self
            .conn
            .prepare(
                "SELECT m.kind, COALESCE(f.class, m.origin), COUNT(*) FROM mentions m
                 LEFT JOIN files f ON f.repo = m.repo AND f.rel_path = m.rel_path
                 GROUP BY 1, 2 ORDER BY 2, 1",
            )
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

    fn known_for(repo: &str, offstage: &[&str], docs: &[&str]) -> Known {
        let mut map = HashMap::new();
        for r in offstage {
            map.insert(norm_key(&format!("{repo}\\{r}")), Target { repo: repo.into(), rel_path: r.to_string(), origin: "offstage" });
        }
        for r in docs {
            map.insert(norm_key(&format!("{repo}\\{r}")), Target { repo: repo.into(), rel_path: r.to_string(), origin: "doc" });
        }
        Known { map, folded: HashMap::new(), roots: vec![norm_key(repo)] }
    }

    fn kinds(hits: &[Hit]) -> Vec<(&str, &str)> {
        hits.iter().map(|h| (h.kind, h.target.rel_path.as_str())).collect()
    }

    const R: &str = "C:\\Demo\\proj";

    fn read_line(path: &str) -> String {
        serde_json::json!({"type": "assistant", "message": {"content": [
            {"type": "tool_use", "name": "Read", "input": {"file_path": path}}
        ]}})
        .to_string()
    }

    #[test]
    fn excerpt_starts_at_the_prompt_and_marks_the_hit() {
        let t = std::env::temp_dir().join(format!("offstage-excerpt-{}.jsonl", std::process::id()));
        let lines = [
            serde_json::json!({"type": "user", "message": {"content": "前の話"}}),
            serde_json::json!({"type": "user", "message": {"content": "<system-reminder>x</system-reminder>メモを読んで"}}),
            serde_json::json!({"type": "ai-title", "aiTitle": "t"}),
            serde_json::json!({"type": "assistant", "timestamp": "2026-11-12T01:00:00Z", "message": {"content": [
                {"type": "text", "text": "読みます"},
                {"type": "tool_use", "name": "Read", "input": {"file_path": "C:\\p\\memo.md"}}]}}),
            serde_json::json!({"type": "user", "message": {"content": [{"type": "tool_result", "content": "中身"}]}}),
            serde_json::json!({"type": "assistant", "message": {"content": [{"type": "text", "text": "読みました"}]}}),
        ];
        let body: String = lines.iter().map(|l| format!("{l}\n")).collect();
        std::fs::write(&t, body).unwrap();
        let got = excerpt(&t, 4).unwrap();
        let view: Vec<(&str, &str, bool)> = got.iter().map(|x| (x.role, x.text.as_str(), x.hit)).collect();
        assert_eq!(
            view,
            vec![
                ("user", "メモを読んで", false),
                ("assistant", "読みます", true),
                ("tool", "C:\\p\\memo.md", true),
                ("result", "中身", false),
                ("assistant", "読みました", false),
            ]
        );
        assert!(excerpt(&t, 99).is_err());
        let _ = std::fs::remove_file(&t);
    }

    #[test]
    fn files_inside_folded_folders() {
        let mut k = known_for(R, &["out", "sandbox/webview"], &[]);
        // out は生成物のフォルダ (畳んであるが folded には入れない)、webview はブラウザの利用者データ
        let wv = k.map.get(&norm_key(&format!("{R}\\sandbox\\webview"))).unwrap().clone();
        k.folded.insert(norm_key(&format!("{R}\\sandbox\\webview")), wv);
        let mut r = LineReader::new(&k, R);
        assert!(r.line(&read_line("C:\\Demo\\proj\\out\\shot.png")).1.is_empty());
        assert_eq!(
            kinds(&r.line(&read_line("C:\\Demo\\proj\\sandbox\\webview\\Default\\Cookies")).1),
            vec![("read", "sandbox/webview")]
        );
    }

    #[test]
    fn candidates_starting_with_japanese_do_not_panic() {
        // 2026-11-12、実際の記録で「ト」の途中をバイト数で切って落ちた
        assert_eq!(clean("テスト/a.md"), "テスト/a.md");
        assert_eq!(clean("C:\\a\\b.md:12"), "C:\\a\\b.md");
        let k = known_for(R, &["sandbox/メモ.md"], &[]);
        let mut r = LineReader::new(&k, R);
        let line = serde_json::json!({"type": "assistant", "message": {"content": [
            {"type": "text", "text": "`テスト/x` と `%USERPROF` と `sandbox/メモ.md` を見た"}
        ]}});
        assert_eq!(kinds(&r.line(&line.to_string()).1), vec![("text", "sandbox/メモ.md")]);
    }

    #[test]
    fn norm_key_folds_separators_case_and_dots() {
        assert_eq!(norm_key("C:/Demo/Proj/./a/../B.md"), "c:\\demo\\proj\\b.md");
        assert_eq!(norm_key("/c/Demo/proj/x"), "c:\\demo\\proj\\x");
    }

    #[test]
    fn tool_uses_and_their_kinds() {
        let k = known_for(R, &["sandbox/memo.md", ".work/api_tokens.json", "sandbox/out.txt"], &["docs/plan.md"]);
        let mut r = LineReader::new(&k, R);
        let line = serde_json::json!({
            "type": "assistant", "cwd": R, "sessionId": "s1",
            "message": {"content": [
                {"type": "tool_use", "name": "Read", "input": {"file_path": "C:\\Demo\\proj\\.work\\api_tokens.json"}},
                {"type": "tool_use", "name": "Write", "input": {"file_path": "C:/Demo/proj/sandbox/out.txt", "content": "x"}},
                {"type": "tool_use", "name": "Bash", "input": {"command": "cat sandbox/memo.md | head"}},
                {"type": "text", "text": "書いたのは [plan.md](docs/plan.md) と C:\\Demo\\proj\\sandbox\\memo.mdです。"},
                {"type": "tool_use", "name": "Read", "input": {"file_path": "C:\\Demo\\proj\\src\\main.rs"}}
            ]}
        });
        let (_, hits) = r.line(&line.to_string());
        assert_eq!(
            kinds(&hits),
            vec![
                ("read", ".work/api_tokens.json"),
                ("write", "sandbox/out.txt"),
                ("bash", "sandbox/memo.md"),
                ("text", "sandbox/memo.md"),
                ("text", "docs/plan.md"),
            ]
        );
    }

    #[test]
    fn prompts_skip_reminders_and_count_selections_as_content() {
        let k = known_for(R, &["sandbox/a.md", "sandbox/b.md", "sandbox/c.md"], &[]);
        let mut r = LineReader::new(&k, R);
        let text = "<system-reminder>C:\\Demo\\proj\\sandbox\\c.md</system-reminder>\
                    <ide_opened_file>The user opened C:\\Demo\\proj\\sandbox\\a.md</ide_opened_file>\
                    <ide_selection>selected from C:\\Demo\\proj\\sandbox\\b.md: secret line</ide_selection> これを見て";
        let line = serde_json::json!({"type": "user", "message": {"content": text}});
        let (_, hits) = r.line(&line.to_string());
        assert_eq!(kinds(&hits), vec![("attached", "sandbox/b.md"), ("text", "sandbox/a.md")]);
    }

    #[test]
    fn attachments_and_git_bash_paths_and_line_suffixes() {
        let k = known_for(R, &["sandbox/a.md", "sandbox/b.md"], &["docs/x.png"]);
        let mut r = LineReader::new(&k, R);
        let at = serde_json::json!({"type": "attachment", "attachment": {"type": "file", "filename": "C:\\Demo\\proj\\sandbox\\a.md", "content": {}}});
        assert_eq!(kinds(&r.line(&at.to_string()).1), vec![("attached", "sandbox/a.md")]);
        let res = serde_json::json!({"type": "user", "message": {"content": [
            {"type": "tool_result", "content": "/c/Demo/proj/sandbox/b.md:12: hit\nsee `docs/x.png`."}
        ]}});
        assert_eq!(kinds(&r.line(&res.to_string()).1), vec![("result", "sandbox/b.md"), ("result", "docs/x.png")]);
    }

    #[test]
    fn links_incrementally_and_flags_secrets_sent() {
        let t = std::env::temp_dir().join(format!("offstage-link-{}", std::process::id()));
        let _ = fs::remove_dir_all(&t);
        let repo = t.join("proj");
        fs::create_dir_all(repo.join(".work")).unwrap();
        assert!(Command::new("git").arg("-C").arg(&repo).args(["init", "-q"]).status().unwrap().success());
        fs::write(repo.join(".gitignore"), ".work/\n").unwrap();
        fs::write(repo.join(".work/demo_token.json"), "{}").unwrap();
        fs::write(repo.join(".work/notes.md"), "n").unwrap();
        let mut store = Store::open(&t.join("idx/index.sqlite")).unwrap();
        store.apply_run(&[scan_repo(&repo).unwrap()]).unwrap();
        let known = Known::build(&store, &[]).unwrap();
        assert_eq!(known.len(), 2);

        let projects = t.join("projects/demo");
        fs::create_dir_all(&projects).unwrap();
        let log = projects.join("s1.jsonl");
        let rp = repo.to_string_lossy().into_owned();
        let read = |p: &str| {
            serde_json::json!({"type": "assistant", "cwd": rp, "sessionId": "s1", "timestamp": "2026-11-12T00:00:00Z",
                "message": {"content": [{"type": "tool_use", "name": "Read", "input": {"file_path": format!("{rp}\\{p}")}}]}})
            .to_string()
        };
        fs::write(&log, format!("{}\n", read(".work\\notes.md"))).unwrap();
        let s1 = store.link_sessions(&t.join("projects"), &known).unwrap();
        assert_eq!((s1.logs_read, s1.hits), (1, 1));
        assert!(store.secrets_sent().unwrap().is_empty());

        // 書き足された分だけ読む (書きかけの行は次に回す)
        let mut more = format!("{}\n", read(".work\\demo_token.json"));
        more.push_str("{\"type\":\"assistant\""); // 書きかけ
        let mut f = fs::OpenOptions::new().append(true).open(&log).unwrap();
        std::io::Write::write_all(&mut f, more.as_bytes()).unwrap();
        drop(f);
        let s2 = store.link_sessions(&t.join("projects"), &known).unwrap();
        assert_eq!((s2.lines, s2.hits), (1, 1));
        let sent = store.secrets_sent().unwrap();
        assert_eq!(sent, vec![(rp.clone(), ".work/demo_token.json".into(), 1)]);
        let m = store.mentions_of(&rp, ".work/demo_token.json").unwrap();
        assert_eq!((m[0].line_no, m[0].kind.as_str(), m[0].session_id.as_str()), (2, "read", "s1"));

        // 変わっていなければ読まない
        let s3 = store.link_sessions(&t.join("projects"), &known).unwrap();
        assert_eq!((s3.logs_read, s3.logs_skipped), (0, 1));
        drop(store);
        let _ = fs::remove_dir_all(&t);
    }
}
