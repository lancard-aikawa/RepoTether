//! Claude Code のセッションログ (~/.claude/projects/<フォルダ>/<セッションID>.jsonl) を要約する。
//!
//! ログは全体で数百 MB になるので、ファイルの更新時刻とサイズが変わっていなければ
//! 前回の要約 (キャッシュ) を使う。

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;
use serde::de::IgnoredAny;
use serde::{Deserialize, Serialize};

use super::model::{Session, TranscriptEntry};
use super::util;

const PROMPT_CHARS: usize = 200;
const REPLY_CHARS: usize = 400;

#[derive(Default, Serialize, Deserialize)]
struct Cache {
    /// 要約の形を変えたら上げる。合わないキャッシュは捨てる
    version: u32,
    files: HashMap<String, CacheEntry>,
}

#[derive(Serialize, Deserialize)]
struct CacheEntry {
    mtime_ms: u128,
    size: u64,
    session: Session,
}

const CACHE_VERSION: u32 = 5;

/// projects_dir 以下の全セッションを要約する。cache_path にキャッシュを読み書きする。
pub fn load_all(projects_dir: &Path, cache_path: &Path) -> Result<Vec<Session>, String> {
    let mut cache: Cache = std::fs::read_to_string(cache_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .filter(|c: &Cache| c.version == CACHE_VERSION)
        .unwrap_or_default();

    let files = list_session_files(projects_dir)
        .map_err(|e| format!("{} を読めません: {e}", projects_dir.display()))?;

    let mut next: HashMap<String, CacheEntry> = HashMap::new();
    let mut sessions = vec![];
    for f in files {
        let Ok(meta) = std::fs::metadata(&f) else { continue };
        let mtime_ms = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let size = meta.len();
        let key = f.to_string_lossy().into_owned();
        let session = match cache.files.remove(&key) {
            Some(e) if e.mtime_ms == mtime_ms && e.size == size => e.session,
            _ => match summarize(&f) {
                Some(s) => s,
                None => continue,
            },
        };
        sessions.push(session.clone());
        next.insert(key, CacheEntry { mtime_ms, size, session });
    }

    let cache = Cache { version: CACHE_VERSION, files: next };
    if let Some(dir) = cache_path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string(&cache) {
        let _ = std::fs::write(cache_path, json);
    }
    Ok(sessions)
}

/// 直下のフォルダにある *.jsonl。さらに下 (tool-results や subagents) は見ない。
fn list_session_files(projects_dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = vec![];
    for d in std::fs::read_dir(projects_dir)? {
        let Ok(d) = d else { continue };
        if !d.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(d.path()) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "jsonl") && p.is_file() {
                out.push(p);
            }
        }
    }
    Ok(out)
}

#[derive(Deserialize)]
struct Rec {
    #[serde(rename = "type")]
    kind: Option<String>,
    timestamp: Option<String>,
    cwd: Option<String>,
    entrypoint: Option<String>,
    #[serde(rename = "gitBranch")]
    git_branch: Option<String>,
    #[serde(rename = "isMeta")]
    is_meta: Option<bool>,
    /// 文脈が長くなったときに Claude Code が書いた「ここまでの要約」。type は user だが人の入力ではない
    #[serde(rename = "isCompactSummary")]
    is_compact_summary: Option<bool>,
    #[serde(rename = "isSidechain")]
    is_sidechain: Option<bool>,
    #[serde(rename = "toolUseResult")]
    tool_use_result: Option<IgnoredAny>,
    message: Option<Msg>,
    #[serde(rename = "aiTitle")]
    ai_title: Option<String>,
}

#[derive(Deserialize)]
struct Msg {
    content: Option<Content>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Content {
    Text(String),
    Blocks(Vec<Block>),
    Other(IgnoredAny),
}

#[derive(Deserialize)]
struct Block {
    #[serde(rename = "type")]
    kind: Option<String>,
    text: Option<String>,
    /// tool_use のツール名
    name: Option<String>,
}

fn summarize(path: &Path) -> Option<Session> {
    let file = std::fs::File::open(path).ok()?;
    let id = path.file_stem()?.to_string_lossy().into_owned();
    let project_dir = path.parent().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned());
    let mut s = Session { id, project_dir, ..Default::default() };
    let mut min_ts: Option<String> = None;
    let mut max_ts: Option<String> = None;

    for line in BufReader::new(file).lines() {
        let Ok(line) = line else { continue };
        let Ok(r) = serde_json::from_str::<Rec>(&line) else { continue };
        if let Some(ts) = &r.timestamp {
            // RFC 3339 の UTC ("...Z") なので文字列比較で順序が決まる
            if min_ts.as_ref().is_none_or(|m| ts < m) {
                min_ts = Some(ts.clone());
            }
            if max_ts.as_ref().is_none_or(|m| ts > m) {
                max_ts = Some(ts.clone());
            }
        }
        if s.cwd.is_none() {
            if let Some(c) = &r.cwd {
                s.cwd = Some(util::display_path(c));
            }
        }
        if s.entrypoint.is_none() {
            s.entrypoint = r.entrypoint.clone();
        }
        // git リポジトリ外のフォルダでは "HEAD" が入るので捨てる
        if r.git_branch.as_deref().is_some_and(|b| !b.is_empty() && b != "HEAD") {
            s.git_branch = r.git_branch.clone();
        }
        let sidechain = r.is_sidechain.unwrap_or(false);
        match r.kind.as_deref() {
            Some("ai-title") => {
                if let Some(t) = r.ai_title {
                    s.title = Some(t);
                }
            }
            Some("user")
                if !sidechain
                    && !r.is_meta.unwrap_or(false)
                    && !r.is_compact_summary.unwrap_or(false)
                    && r.tool_use_result.is_none() =>
            {
                let Some(text) = r.message.and_then(|m| m.content).and_then(user_text) else {
                    continue;
                };
                // バックグラウンド処理の完了通知は人の入力ではない
                if text.trim_start().starts_with("<task-notification>") {
                    continue;
                }
                let text = clean_prompt(&text);
                if text.is_empty() || text.starts_with("[Request interrupted") {
                    continue;
                }
                let short = util::truncate_chars(&text, PROMPT_CHARS);
                if s.first_prompt.is_none() {
                    s.first_prompt = Some(short.clone());
                }
                s.last_prompt = Some(short);
                s.prompt_count += 1;
                if let Some(ts) = r.timestamp {
                    s.prompt_times.push(ts);
                }
            }
            Some("assistant") if !sidechain => {
                if let Some(Content::Blocks(blocks)) = r.message.and_then(|m| m.content) {
                    let text: String = blocks
                        .iter()
                        .filter(|b| b.kind.as_deref() == Some("text"))
                        .filter_map(|b| b.text.as_deref())
                        .collect::<Vec<_>>()
                        .join("\n");
                    let text = text.trim();
                    if !text.is_empty() {
                        s.last_reply = Some(util::truncate_chars(text, REPLY_CHARS));
                    }
                }
            }
            _ => {}
        }
    }

    s.started_at = min_ts;
    s.ended_at = max_ts;
    // SDK (sdk-py / sdk-ts / sdk-cli) からの実行は自動処理として扱う
    s.interactive = !s.entrypoint.as_deref().unwrap_or("").starts_with("sdk") && s.prompt_count > 0;
    // 中身の無いセッション (起動しただけ) は捨てる
    if s.started_at.is_none() && s.title.is_none() {
        return None;
    }
    Some(s)
}

/// 人が入力したテキスト。tool_result を含むものは人の入力ではない。
fn user_text(c: Content) -> Option<String> {
    match c {
        Content::Text(t) => Some(t),
        Content::Blocks(blocks) => {
            if blocks.iter().any(|b| b.kind.as_deref() == Some("tool_result")) {
                return None;
            }
            let mut parts = vec![];
            for b in &blocks {
                match b.kind.as_deref() {
                    Some("text") => parts.extend(b.text.clone()),
                    Some("image") => parts.push("[画像]".to_string()),
                    _ => {}
                }
            }
            (!parts.is_empty()).then(|| parts.join("\n"))
        }
        Content::Other(_) => None,
    }
}

/// セッション ID として受け付ける形 (英数字とハイフン)。コマンドに渡すので、それ以外は通さない
pub fn is_session_id(id: &str) -> bool {
    (8..=64).contains(&id.len()) && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

const TRANSCRIPT_MAX_ENTRIES: usize = 3000;
const TRANSCRIPT_MAX_CHARS: usize = 20000;

/// セッションの会話の全文。人の発言と Claude の返答 (文章と使ったツールの名前) だけで、
/// ツールの結果・考えている途中 (thinking)・サブエージェントの分は省く
pub fn transcript(projects_dir: &Path, session_id: &str) -> Result<Vec<TranscriptEntry>, String> {
    if !is_session_id(session_id) {
        return Err(format!("セッション ID の形ではありません: {session_id}"));
    }
    let file = std::fs::read_dir(projects_dir)
        .map_err(|e| format!("{} を読めません: {e}", projects_dir.display()))?
        .flatten()
        .map(|d| d.path().join(format!("{session_id}.jsonl")))
        .find(|p| p.is_file())
        .ok_or_else(|| "セッションのログが見つかりません (消されたか、別の PC のものです)".to_string())?;

    let mut out: Vec<TranscriptEntry> = vec![];
    let reader = BufReader::new(std::fs::File::open(&file).map_err(|e| e.to_string())?);
    for line in reader.lines() {
        if out.len() >= TRANSCRIPT_MAX_ENTRIES {
            break;
        }
        let Ok(line) = line else { continue };
        let Ok(r) = serde_json::from_str::<Rec>(&line) else { continue };
        if r.is_sidechain.unwrap_or(false) {
            continue;
        }
        match r.kind.as_deref() {
            Some("user") if r.is_compact_summary.unwrap_or(false) => {
                let Some(text) = r.message.and_then(|m| m.content).and_then(user_text) else { continue };
                out.push(TranscriptEntry {
                    role: "summary".into(),
                    at: r.timestamp,
                    text: util::truncate_chars(text.trim(), TRANSCRIPT_MAX_CHARS),
                    tools: vec![],
                });
            }
            Some("user") if !r.is_meta.unwrap_or(false) && r.tool_use_result.is_none() => {
                let Some(text) = r.message.and_then(|m| m.content).and_then(user_text) else { continue };
                if text.trim_start().starts_with("<task-notification>") {
                    continue;
                }
                let text = strip_tags(&text, true);
                if text.is_empty() || text.starts_with("[Request interrupted") {
                    continue;
                }
                out.push(TranscriptEntry {
                    role: "user".into(),
                    at: r.timestamp,
                    text: util::truncate_chars(&text, TRANSCRIPT_MAX_CHARS),
                    tools: vec![],
                });
            }
            Some("assistant") => {
                let Some(Content::Blocks(blocks)) = r.message.and_then(|m| m.content) else { continue };
                let mut text = String::new();
                let mut tools: Vec<String> = vec![];
                for b in &blocks {
                    match b.kind.as_deref() {
                        Some("text") => {
                            if let Some(t) = &b.text {
                                text.push_str(t);
                            }
                        }
                        Some("tool_use") => tools.push(b.name.clone().unwrap_or_else(|| "tool".into())),
                        _ => {}
                    }
                }
                if text.trim().is_empty() && tools.is_empty() {
                    continue;
                }
                // 続けて届いた Claude の記録は 1 件の返答にまとめる
                match out.last_mut() {
                    Some(last) if last.role == "assistant" => {
                        if !text.trim().is_empty() {
                            if !last.text.is_empty() {
                                last.text.push_str("\n\n");
                            }
                            last.text.push_str(text.trim());
                            last.text = util::truncate_chars(&last.text, TRANSCRIPT_MAX_CHARS);
                        }
                        for t in tools {
                            if last.tools.last() != Some(&t) {
                                last.tools.push(t);
                            }
                        }
                    }
                    _ => {
                        let mut ts: Vec<String> = vec![];
                        for t in tools {
                            if ts.last() != Some(&t) {
                                ts.push(t);
                            }
                        }
                        out.push(TranscriptEntry {
                            role: "assistant".into(),
                            at: r.timestamp,
                            text: util::truncate_chars(text.trim(), TRANSCRIPT_MAX_CHARS),
                            tools: ts,
                        });
                    }
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

/// IDE やフックが差し込むタグを取り除き、スラッシュコマンドはコマンド名だけにする (一覧用に 1 行へ)。
fn clean_prompt(text: &str) -> String {
    strip_tags(text, false).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// タグを取り除く。keep_pasted なら貼り付けた中身は残し (全文を見るとき)、そうでなければ [貼り付け] にまとめる。
/// 改行は残す
fn strip_tags(text: &str, keep_pasted: bool) -> String {
    static NOISE: OnceLock<Vec<Regex>> = OnceLock::new();
    static COMMAND: OnceLock<Regex> = OnceLock::new();
    static PASTED: OnceLock<Regex> = OnceLock::new();
    let noise = NOISE.get_or_init(|| {
        [
            "system-reminder",
            "ide_selection",
            "ide_opened_file",
            "ide_diagnostics",
            "local-command-stdout",
            "local-command-stderr",
            "local-command-caveat",
            "command-message",
            "command-args",
        ]
        .iter()
        .map(|t| Regex::new(&format!(r"(?s)<{t}(?:\s[^>]*)?>.*?</{t}>")).unwrap())
        .collect()
    });
    let command = COMMAND.get_or_init(|| Regex::new(r"(?s)<command-name>(.*?)</command-name>").unwrap());

    let pasted =
        PASTED.get_or_init(|| Regex::new(r"(?s)<pasted_content(?:\s[^>]*)?>.*?</pasted_content>").unwrap());

    static LONE_TAG: OnceLock<Regex> = OnceLock::new();
    // 閉じタグが無いもの (途中で切れた貼り付け) はタグだけ落として中身を残す
    let lone = LONE_TAG.get_or_init(|| Regex::new(r"</?pasted_content(?:\s[^>]*)?>").unwrap());

    let mut s = if keep_pasted { text.to_string() } else { pasted.replace_all(text, " [貼り付け] ").into_owned() };
    s = lone.replace_all(&s, if keep_pasted { "" } else { " " }).into_owned();
    for re in noise {
        s = re.replace_all(&s, "").into_owned();
    }
    s = command.replace_all(&s, "$1").into_owned();
    s.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_cleaning() {
        assert_eq!(
            clean_prompt("<ide_opened_file>x</ide_opened_file>  直して\nください"),
            "直して ください"
        );
        assert_eq!(
            clean_prompt("<command-message>m</command-message><command-name>/review</command-name><command-args></command-args>"),
            "/review"
        );
        assert_eq!(clean_prompt("<system-reminder>a\nb</system-reminder>"), "");
        assert_eq!(
            clean_prompt("見て <pasted_content id=\"x\">長い\n文章</pasted_content> どう?"),
            "見て [貼り付け] どう?"
        );
    }
}

#[cfg(test)]
mod transcript_tests {
    use super::*;

    #[test]
    fn reads_conversation_only() {
        let dir = std::env::temp_dir().join("repotether-transcript-test");
        let proj = dir.join("c--repos-demo");
        std::fs::create_dir_all(&proj).unwrap();
        let id = "0123abcd-4567-89ab-cdef-0123456789ab";
        let lines = [
            r#"{"type":"user","timestamp":"2026-09-01T00:00:00Z","message":{"content":"<ide_opened_file>x</ide_opened_file>直して\n2 行目"}}"#,
            r#"{"type":"assistant","timestamp":"2026-09-01T00:00:01Z","message":{"content":[{"type":"thinking","thinking":"..."},{"type":"text","text":"見ます"}]}}"#,
            r#"{"type":"assistant","timestamp":"2026-09-01T00:00:02Z","message":{"content":[{"type":"tool_use","name":"Read"}]}}"#,
            r#"{"type":"user","timestamp":"2026-09-01T00:00:03Z","toolUseResult":{},"message":{"content":[{"type":"tool_result"}]}}"#,
            r#"{"type":"assistant","timestamp":"2026-09-01T00:00:04Z","message":{"content":[{"type":"tool_use","name":"Read"},{"type":"tool_use","name":"Edit"},{"type":"text","text":"直しました"}]}}"#,
            r#"{"type":"user","isSidechain":true,"message":{"content":"サブエージェントへの指示"}}"#,
            r#"{"type":"user","isMeta":true,"message":{"content":"メタ"}}"#,
            r#"{"type":"user","message":{"content":"<task-notification>done</task-notification>"}}"#,
            r#"{"type":"user","timestamp":"2026-09-01T00:01:00Z","message":{"content":"見て <pasted_content id=\"a\">貼った\n中身</pasted_content>"}}"#,
            r#"{"type":"user","isCompactSummary":true,"isVisibleInTranscriptOnly":true,"message":{"content":"This session is being continued..."}}"#,
        ];
        std::fs::write(proj.join(format!("{id}.jsonl")), lines.join("\n")).unwrap();

        let t = transcript(&dir, id).unwrap();
        let roles: Vec<&str> = t.iter().map(|e| e.role.as_str()).collect();
        assert_eq!(roles, ["user", "assistant", "user", "summary"]);
        assert_eq!(t[0].text, "直して\n2 行目");
        assert_eq!(t[1].text, "見ます\n\n直しました");
        assert_eq!(t[1].tools, ["Read", "Edit"]);
        // 全文では貼り付けた中身を残す
        assert_eq!(t[2].text, "見て 貼った\n中身");

        assert!(transcript(&dir, "../../etc/passwd").is_err());
        assert!(transcript(&dir, "ffffffff-0000-0000-0000-000000000000").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn session_id_shape() {
        assert!(is_session_id("4100683c-53d0-4d9a-bdce-35ac93ecc378"));
        assert!(!is_session_id("abc"));
        assert!(!is_session_id("4100683c & calc"));
    }
}
