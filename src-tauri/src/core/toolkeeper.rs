//! ToolKeeper (PC に入れた道具が古くなっていないかを調べる CLI) との受け渡し。
//! 約束は ToolKeeper の docs/design.md §9。
//!
//! RepoTether は見るだけ。
//! - `toolkeeper status --json` で使える状態か (版・データの場所・定期実行) を聞く
//! - ToolKeeper の結果 (<data>/check.json) を読む
//! - 「ToolKeeper を開く」で ToolKeeper の画面を開く。道具の更新 (UAC が出る) は ToolKeeper の画面でする
//!
//! `toolkeeper update` は呼ばない。

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::{sessionvault, util};

/// `status --json` は winget --version と schtasks を呼ぶだけで 1〜2 秒
const STATUS_TIMEOUT: Duration = Duration::from_secs(30);
/// 読める check.json の形 (ToolKeeper の check.SCHEMA)
pub const SCHEMA: u32 = 1;

/// ToolKeeper の呼び方 (`python -m toolkeeper`)
#[derive(Debug, Clone, PartialEq)]
pub struct Runner {
    dir: PathBuf,
    python: PathBuf,
    python_args: Vec<String>,
    src: PathBuf,
}

/// 設定の「ToolKeeper の場所」(ToolKeeper のリポジトリのフォルダ) から呼び方を決める。
/// そのフォルダの .venv の Python があればそれを、無ければ py ランチャー (python) を使う
/// (ToolKeeper は標準ライブラリだけなので .venv は無くてよい)
pub fn resolve(dir: &str) -> Result<Runner, String> {
    let dir = PathBuf::from(dir.trim());
    let src = dir.join("src");
    if !src.join("toolkeeper").join("__init__.py").is_file() {
        return Err(format!(
            "{} は ToolKeeper のリポジトリではありません。「ToolKeeper の場所」を確かめてください",
            dir.display()
        ));
    }
    let venv = dir.join(".venv").join("Scripts").join("python.exe");
    if venv.is_file() {
        return Ok(Runner { dir, python: venv, python_args: vec![], src });
    }
    let (python, python_args) = sessionvault::find_python()
        .map_err(|_| "ToolKeeper を動かす Python (3.11 以上) が見つかりません".to_string())?;
    Ok(Runner { dir, python, python_args, src })
}

/// 終わるまで待つ (出力は別のスレッドで読む。パイプが詰まって子が止まらないように)
fn run(r: &Runner, args: &[&str], timeout: Duration) -> Result<(Option<i32>, String, String), String> {
    let mut cmd = Command::new(&r.python);
    cmd.args(&r.python_args)
        .args(["-m", "toolkeeper"])
        .args(args)
        .env("PYTHONPATH", &r.src)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .current_dir(&r.dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    util::hide_window(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| format!("ToolKeeper を起動できません: {e}"))?;
    let mut out = child.stdout.take().ok_or("ToolKeeper の出力を読めません")?;
    let mut err = child.stderr.take().ok_or("ToolKeeper の出力を読めません")?;
    let out_reader = std::thread::spawn(move || {
        let mut buf = vec![];
        let _ = out.read_to_end(&mut buf);
        buf
    });
    let err_reader = std::thread::spawn(move || {
        let mut buf = vec![];
        let _ = err.read_to_end(&mut buf);
        buf
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if started.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("ToolKeeper が {} 秒で終わらないので打ち切りました", timeout.as_secs()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(e.to_string()),
        }
    };
    Ok((
        status.code(),
        String::from_utf8_lossy(&out_reader.join().unwrap_or_default()).into_owned(),
        String::from_utf8_lossy(&err_reader.join().unwrap_or_default()).trim().to_string(),
    ))
}

// ---- 使える状態か (`toolkeeper status --json`、ToolKeeper の design.md §9.2) ----

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"), default)]
pub struct TaskStatus {
    pub registered: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"), default)]
pub struct Sources {
    /// winget の版。無ければ None
    pub winget: Option<String>,
    pub choco: Option<String>,
    /// Cygwin の場所。無ければ None
    pub cygwin: Option<String>,
    pub ghostscript: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"), default)]
pub struct Status {
    /// ToolKeeper の版
    pub toolkeeper: String,
    /// ToolKeeper が書く check.json の形
    pub schema: u32,
    pub data_dir: String,
    /// 最後に調べた時刻。まだなら None
    pub last_check: Option<String>,
    pub task: TaskStatus,
    pub sources: Sources,
}

/// ToolKeeper が使える状態か。読むだけで、ネットにも出ない
pub fn status(r: &Runner) -> Result<Status, String> {
    let (code, stdout, stderr) = run(r, &["status", "--json"], STATUS_TIMEOUT)?;
    if code != Some(0) {
        let lines: Vec<&str> = stderr.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
        let msg = if stderr.contains("Traceback") { lines.last() } else { lines.first() };
        return Err(util::truncate_chars(msg.copied().unwrap_or("ToolKeeper の状態を読めません"), 300));
    }
    serde_json::from_str(&stdout).map_err(|e| format!("ToolKeeper の状態を読めません: {e}"))
}

// ---- 結果 (<data>/check.json、ToolKeeper の design.md §9.1) ----

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"), default)]
pub struct Summary {
    /// 区分 A・B で更新できる数
    pub updatable: u32,
    /// サポート終了・もうすぐ終了の数
    pub eol: u32,
    /// 前回の check に無かった更新の数
    pub new: u32,
    pub failed: u32,
    /// 環境変数と PATH で「切れている」と注意の数 (ToolKeeper 0.2.0 から。無ければ 0)
    pub env: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"), default)]
pub struct Eol {
    pub product: String,
    pub cycle: String,
    pub date: Option<String>,
    /// ended・soon・ok
    pub state: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"), default)]
pub struct Packages {
    pub installed: u32,
    pub outdated: u32,
    pub names: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"), default)]
pub struct Item {
    /// winget:<ID>・choco:<名前>・php・ghostscript・cygwin
    pub key: String,
    /// A〜E (ToolKeeper の design.md §2)
    pub tier: String,
    pub name: String,
    pub installed: Option<String>,
    pub latest: Option<String>,
    /// latest の公開日 (YYYY-MM-DD。ToolKeeper 0.2.0 から。分からなければ None)
    pub latest_date: Option<String>,
    pub updatable: bool,
    pub eol: Option<Eol>,
    pub new: bool,
    /// twin:<key>・pinned・pinned:<件数>・running:<名前>・manual・old:<版>・same
    pub notes: Vec<String>,
    pub packages: Option<Packages>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"), default)]
pub struct Failed {
    pub source: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"), default)]
pub struct Check {
    pub schema: u32,
    pub toolkeeper: String,
    pub checked_at: String,
    pub summary: Summary,
    pub items: Vec<Item>,
    pub failed: Vec<Failed>,
}

/// check.json を読む。無ければ None (まだ調べていない)。知らない形 (schema) なら読まずに理由を返す
pub fn read_check(path: &Path) -> Result<Option<Check>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{} を読めません: {e}", path.display())),
    };
    parse_check(&text).map(Some)
}

fn parse_check(text: &str) -> Result<Check, String> {
    let v: serde_json::Value = serde_json::from_str(text).map_err(|e| format!("ToolKeeper の結果を読めません: {e}"))?;
    let schema = v.get("schema").and_then(serde_json::Value::as_u64).unwrap_or(0);
    if schema != u64::from(SCHEMA) {
        return Err(if schema > u64::from(SCHEMA) {
            format!("ToolKeeper の結果の形 (schema {schema}) が新しすぎて読めません。RepoTether を更新してください")
        } else {
            format!("ToolKeeper の結果の形 (schema {schema}) が古くて読めません。ToolKeeper を更新してください (git pull)")
        });
    }
    serde_json::from_value(v).map_err(|e| format!("ToolKeeper の結果を読めません: {e}"))
}

/// 画面に渡すもの
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsReport {
    /// 設定で ToolKeeper の場所が決まっているか
    pub configured: bool,
    /// ToolKeeper を呼べない・結果を読めないときの理由
    pub error: Option<String>,
    /// まだ調べていなければ None
    pub check: Option<Check>,
}

// ---- 画面 ----

/// ToolKeeper の画面を開く。終わるのを待たない。
/// ToolKeeper の scripts/toolkeeper-gui.cmd を使う (本体の pythonw.exe を探して黒い窓を出さずに開く。
/// .venv の pythonw.exe は uv 0.11 ではコンソール用で黒い窓が出るので使わない、という判断を ToolKeeper の側に任せる)
pub fn open_gui(r: &Runner) -> Result<(), String> {
    let launcher = r.dir.join("scripts").join("toolkeeper-gui.cmd");
    if !launcher.is_file() {
        return Err(format!("{} がありません。ToolKeeper を更新してください (git pull)", launcher.display()));
    }
    let mut cmd = Command::new(&launcher);
    cmd.current_dir(&r.dir).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    util::hide_window(&mut cmd);
    cmd.spawn().map(|_| ()).map_err(|e| format!("ToolKeeper の画面を開けません: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHECK: &str = r#"{
      "schema": 1, "toolkeeper": "0.1.0", "checked_at": "2026-10-28T19:40:30+09:00",
      "summary": {"updatable": 13, "eol": 1, "new": 0, "failed": 0},
      "items": [
        {"key": "winget:OpenJS.NodeJS.20", "tier": "E", "name": "Node.js", "installed": "20.9.0", "latest": "20.20.2",
         "updatable": true, "eol": {"product": "nodejs", "cycle": "20", "date": "2026-04-30", "state": "ended"}, "new": false, "notes": []},
        {"key": "cygwin", "tier": "A", "name": "Cygwin", "installed": "3.6.12-1", "latest": "3.6.12-1", "updatable": false,
         "eol": null, "new": false, "notes": [], "packages": {"installed": 80, "outdated": 0, "names": []}},
        {"key": "winget:(pinned)", "tier": "C", "name": "pin で止めた物", "installed": null, "latest": null,
         "updatable": false, "eol": null, "new": false, "notes": ["pinned:1"], "future_field": 1}
      ],
      "failed": [{"source": "choco", "reason": "x"}]
    }"#;

    #[test]
    fn reads_check_json() {
        let c = parse_check(CHECK).unwrap();
        assert_eq!(c.summary.updatable, 13);
        assert_eq!(c.items.len(), 3);
        assert_eq!(c.items[0].eol.as_ref().unwrap().state, "ended");
        assert_eq!(c.items[1].packages.as_ref().unwrap().installed, 80);
        assert_eq!(c.items[2].installed, None, "版の無い行も読む");
        assert_eq!(c.failed[0].source, "choco");
        // ToolKeeper 0.1.0 の結果には latest_date と summary.env が無い
        assert_eq!(c.items[0].latest_date, None);
        assert_eq!(c.summary.env, 0);
    }

    #[test]
    fn reads_release_date_and_env_count() {
        let v2 = CHECK
            .replacen("\"failed\": 0}", "\"failed\": 0, \"env\": 3}", 1)
            .replacen("\"latest\": \"20.20.2\",", "\"latest\": \"20.20.2\", \"latest_date\": \"2026-03-24\",", 1);
        let c = parse_check(&v2).unwrap();
        assert_eq!(c.summary.env, 3);
        assert_eq!(c.items[0].latest_date.as_deref(), Some("2026-03-24"));
        // 画面には camelCase で渡す
        let out = serde_json::to_value(&c).unwrap();
        assert_eq!(out["items"][0]["latestDate"], "2026-03-24");
        assert_eq!(out["summary"]["env"], 3);
    }

    #[test]
    fn unknown_schema_is_not_read() {
        let newer = CHECK.replacen("\"schema\": 1", "\"schema\": 2", 1);
        assert!(parse_check(&newer).unwrap_err().contains("RepoTether を更新"));
        let older = CHECK.replacen("\"schema\": 1,", "", 1);
        assert!(parse_check(&older).unwrap_err().contains("ToolKeeper を更新"));
    }

    #[test]
    fn missing_check_json_is_not_an_error() {
        let p = std::env::temp_dir().join("repotether-toolkeeper-none").join("check.json");
        assert_eq!(read_check(&p).unwrap(), None);
    }

    #[test]
    fn reads_status_json() {
        let s: Status = serde_json::from_str(
            r#"{"toolkeeper": "0.1.0", "schema": 1, "data_dir": "C:\\Repos\\mywork\\ToolKeeper\\data",
                "last_check": "2026-10-28T19:40:30+09:00", "task": {"registered": true},
                "sources": {"winget": "1.29.380", "choco": null, "cygwin": "C:\\cygwin64", "ghostscript": true}}"#,
        )
        .unwrap();
        assert!(s.task.registered);
        assert_eq!(s.sources.choco, None);
        assert_eq!(s.data_dir, r"C:\Repos\mywork\ToolKeeper\data");
        // 画面には camelCase で渡す
        let out = serde_json::to_value(&s).unwrap();
        assert!(out.get("dataDir").is_some() && out.get("lastCheck").is_some());
    }

    /// 手元の ToolKeeper を本当に呼ぶ (ふだんは動かさない)。
    /// `TOOLKEEPER_DIR=C:\Repos\mywork\ToolKeeper cargo test --lib real_toolkeeper -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_toolkeeper() {
        let dir = std::env::var("TOOLKEEPER_DIR").expect("TOOLKEEPER_DIR に ToolKeeper のフォルダを入れる");
        let r = resolve(&dir).unwrap();
        let s = status(&r).unwrap();
        println!("{s:?}");
        let c = read_check(&Path::new(&s.data_dir).join("check.json")).unwrap().expect("check.json が無い");
        println!("checked_at={} summary={:?} items={}", c.checked_at, c.summary, c.items.len());
        assert_eq!(c.schema, SCHEMA);
        // TOOLKEEPER_OPEN=1 なら画面も開く (黒い窓が出ないことを目で確かめる)
        if std::env::var("TOOLKEEPER_OPEN").as_deref() == Ok("1") {
            open_gui(&r).unwrap();
        }
    }

    #[test]
    fn resolve_rejects_other_folders() {
        let dir = std::env::temp_dir().join("repotether-toolkeeper-not-repo");
        let _ = std::fs::create_dir_all(&dir);
        assert!(resolve(&dir.to_string_lossy()).unwrap_err().contains("ToolKeeper のリポジトリではありません"));
    }
}
