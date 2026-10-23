//! LockWatch (lock ファイルを osv-scanner にかけて脆弱性の一覧を作る CLI) との受け渡し。
//! 約束は LockWatch の docs/design.md §3。
//!
//! RepoTether がするのは 3 つだけ。
//! - 手元のリポジトリの一覧を targets.json に書く (LockWatch の受け渡し場所。RepoTether が書くのはここだけ)
//! - LockWatch の結果 (results/latest.json) を読む
//! - 詳細パネルの「今すぐ調べる」で `lockwatch scan --id <id>` を呼ぶ
//!
//! private を外 (api.osv.dev) に送らない約束は LockWatch の側で守る。RepoTether は visibility を
//! 分からないものを unknown (= LockWatch は手元の DB で照合) にして渡す。

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::model::{LocalRepo, RemoteRepo};
use super::{sessionvault, util};

/// 1 つの照合は 2〜15 秒。脆弱性 DB の取り直し (数百 MB) が重なっても足りるだけ待つ
const SCAN_TIMEOUT: Duration = Duration::from_secs(20 * 60);
const CONFIG_TIMEOUT: Duration = Duration::from_secs(30);
/// Claude Code にリポジトリを読ませる (`assess`)。1 回で数分。LockWatch の側の打ち切り (30 分) より少し長く待つ
const ASSESS_TIMEOUT: Duration = Duration::from_secs(35 * 60);

/// LockWatch の呼び方 (`python -m lockwatch`)
#[derive(Debug, Clone, PartialEq)]
pub struct Runner {
    python: PathBuf,
    python_args: Vec<String>,
    src: PathBuf,
}

/// 設定の「LockWatch の場所」(LockWatch のリポジトリのフォルダ) から呼び方を決める。
/// そのフォルダの .venv の Python があればそれを、無ければ py ランチャー (python) を使う
pub fn resolve(dir: &str) -> Result<Runner, String> {
    let dir = PathBuf::from(dir.trim());
    let src = dir.join("src");
    if !src.join("lockwatch").join("__init__.py").is_file() {
        return Err(format!(
            "{} は LockWatch のリポジトリではありません。設定の「LockWatch の場所」を確かめてください",
            dir.display()
        ));
    }
    let venv = if cfg!(windows) { dir.join(".venv").join("Scripts").join("python.exe") } else { dir.join(".venv").join("bin").join("python") };
    if venv.is_file() {
        return Ok(Runner { python: venv, python_args: vec![], src });
    }
    let (python, python_args) = sessionvault::find_python()
        .map_err(|_| "LockWatch を動かす Python (3.11 以上) が見つかりません。LockWatch のフォルダで uv sync を実行してください".to_string())?;
    Ok(Runner { python, python_args, src })
}

fn command(r: &Runner) -> Command {
    let mut cmd = Command::new(&r.python);
    cmd.args(&r.python_args)
        .args(["-m", "lockwatch"])
        .env("PYTHONPATH", &r.src)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    util::hide_window(&mut cmd);
    cmd
}

struct Output {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// 終わるまで待つ (出力は別のスレッドで読む。パイプが詰まって子が止まらないように)
fn run(r: &Runner, args: &[&str], timeout: Duration) -> Result<Output, String> {
    let mut child = command(r).args(args).spawn().map_err(|e| format!("LockWatch を起動できません: {e}"))?;
    let mut out = child.stdout.take().ok_or("LockWatch の出力を読めません")?;
    let mut err = child.stderr.take().ok_or("LockWatch の出力を読めません")?;
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
                return Err(format!("LockWatch が {} 秒で終わらないので打ち切りました", timeout.as_secs()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(e.to_string()),
        }
    };
    Ok(Output {
        code: status.code(),
        stdout: String::from_utf8_lossy(&out_reader.join().unwrap_or_default()).into_owned(),
        stderr: String::from_utf8_lossy(&err_reader.join().unwrap_or_default()).trim().to_string(),
    })
}

/// LockWatch のエラーは 1 行目、Python の例外は最後の行が要点
fn error_line(o: &Output, fallback: &str) -> String {
    let lines: Vec<&str> = o.stderr.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let msg = if o.stderr.contains("Traceback") { lines.last() } else { lines.first() };
    util::truncate_chars(msg.copied().unwrap_or(fallback), 300)
}

/// LockWatch のデータの場所と targets.json の場所 (LockWatch 自身の設定に従う)
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Locations {
    pub data_dir: PathBuf,
    pub targets: PathBuf,
}

impl Locations {
    pub fn latest(&self) -> PathBuf {
        self.data_dir.join("results").join("latest.json")
    }
}

/// `lockwatch config show` の _effective_data_dir / _effective_targets を読む
pub fn locations(r: &Runner) -> Result<Locations, String> {
    let o = run(r, &["config", "show"], CONFIG_TIMEOUT)?;
    if o.code != Some(0) {
        return Err(error_line(&o, "LockWatch の設定を読めません"));
    }
    parse_locations(&o.stdout)
}

fn parse_locations(stdout: &str) -> Result<Locations, String> {
    let v: serde_json::Value = serde_json::from_str(stdout).map_err(|e| format!("LockWatch の設定を読めません: {e}"))?;
    let path = |k: &str| v.get(k).and_then(|x| x.as_str()).map(PathBuf::from).ok_or(format!("LockWatch の設定に {k} がありません"));
    Ok(Locations { data_dir: path("_effective_data_dir")?, targets: path("_effective_targets")? })
}

/// 窓を出す側の Python (python.exe → pythonw.exe、py.exe → pyw.exe)。無ければそのまま
fn windowed(python: &Path) -> PathBuf {
    let name = python.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    let alt = match name.as_str() {
        "python.exe" => "pythonw.exe",
        "py.exe" => "pyw.exe",
        _ => return python.to_path_buf(),
    };
    let w = python.with_file_name(alt);
    if w.is_file() { w } else { python.to_path_buf() }
}

/// `sys.base_prefix` (本体の Python のフォルダ) にある pythonw.exe。無ければ None
fn pythonw_in(base_prefix: &str) -> Option<PathBuf> {
    let base = base_prefix.trim();
    if base.is_empty() {
        return None;
    }
    let w = Path::new(base).join("pythonw.exe");
    w.is_file().then_some(w)
}

/// 窓を出さずに画面を開ける Python: 本体 (`sys.base_prefix`) の pythonw.exe。
/// venv の pythonw.exe は使わない。uv 0.11 の venv ではコンソール用の起動役 (PE の subsystem が console) で、
/// 黒い窓が一緒に開く (2026-10-05 に「LockWatch を開く」で確認。LockWatch の scripts/find-pythonw.ps1 と同じ考え方)
fn base_pythonw(r: &Runner) -> Option<PathBuf> {
    let mut cmd = Command::new(&r.python);
    cmd.args(&r.python_args)
        .args(["-c", "import sys; print(sys.base_prefix)"])
        .env("PYTHONIOENCODING", "utf-8")
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    util::hide_window(&mut cmd);
    let out = cmd.output().ok()?;
    pythonw_in(&String::from_utf8_lossy(&out.stdout))
}

/// LockWatch の画面 (`lockwatch gui`) を開く。終わるのを待たない
pub fn open_gui(r: &Runner) -> Result<(), String> {
    let mut cmd = match base_pythonw(r) {
        // 本体の pythonw.exe には、py ランチャー用の引数 (-3) は付けない
        Some(w) => Command::new(w),
        None => {
            let mut c = Command::new(windowed(&r.python));
            c.args(&r.python_args);
            c
        }
    };
    cmd.args(["-m", "lockwatch", "gui"])
        .env("PYTHONPATH", &r.src)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .current_dir(r.src.parent().unwrap_or(&r.src))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // 本体の pythonw.exe が見つからず、コンソール用の Python で開くときも、黒い窓は出さない
    util::hide_window(&mut cmd);
    cmd.spawn().map(|_| ()).map_err(|e| format!("LockWatch の画面を開けません: {e}"))
}

// ---- 使える状態か (`lockwatch status --json`) ----

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"), default)]
pub struct ScannerStatus {
    pub path: Option<String>,
    pub version: Option<String>,
    /// 見つからない・動かないときの理由
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"), default)]
pub struct LatestStatus {
    pub scanned_at: Option<String>,
    pub repos: u32,
    pub errors: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"), default)]
pub struct TaskStatus {
    pub name: String,
    /// Windows 以外や調べられないときは None
    pub registered: Option<bool>,
}

/// `lockwatch status --json` の答え
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"), default)]
pub struct Status {
    /// LockWatch の版
    pub lockwatch: String,
    pub osv_scanner: ScannerStatus,
    pub data_dir: String,
    pub targets: String,
    pub targets_count: Option<u32>,
    pub targets_error: Option<String>,
    /// 最後の照合。まだなら None
    pub latest: Option<LatestStatus>,
    /// 生態系 → 手元の脆弱性 DB を取った時刻
    pub db: BTreeMap<String, String>,
    pub task: TaskStatus,
}

/// LockWatch が使える状態か (osv-scanner・受け渡し・最後の照合・定期実行)。読むだけで、通信もしない
pub fn status(r: &Runner) -> Result<Status, String> {
    let o = run(r, &["status", "--json"], CONFIG_TIMEOUT)?;
    if o.code != Some(0) {
        return Err(error_line(&o, "LockWatch の状態を読めません (古い LockWatch かもしれません。0.1.0 以上にしてください)"));
    }
    serde_json::from_str(&o.stdout).map_err(|e| format!("LockWatch の状態を読めません: {e}"))
}

// ---- targets.json (RepoTether → LockWatch) ----

/// targets.json の 1 件。キーは LockWatch に合わせて snake_case
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Target {
    pub id: String,
    /// public / private / unknown
    pub visibility: String,
    pub local_path: String,
    pub fetched_path: Option<String>,
    /// 社員向けの一覧での見出し。載せないなら None
    pub share: Option<String>,
    /// 廃止 (照合しない)
    pub retired: bool,
    /// 確定した言語の版 ({"php": "8.2.20"})。無ければ書かない (LockWatch が見当で決める)
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub runtimes: BTreeMap<String, String>,
}

/// targets.json の決め方に使う設定
#[derive(Debug, Default, Clone, Copy)]
pub struct Rules<'a> {
    /// 非表示にしたもの (入れない)
    pub hidden: &'a [String],
    /// 社員向けの一覧に載せる場所 (Config.share_locations)
    pub share: &'a [String],
    /// 廃止の印 (Config.retired)
    pub retired: &'a [String],
    /// 確定した言語の版 (Config.runtimes)
    pub runtimes: Option<&'a BTreeMap<String, BTreeMap<String, String>>>,
}

/// key (<ホスト>/<owner>/<name>) が当てはまる「載せる場所」。区切りの途中では当てない (github.com/lancard は github.com/lancardcom に当てない)。
/// いくつも当てはまれば長いもの
fn share_of(key: &str, locations: &[String]) -> Option<String> {
    locations
        .iter()
        .map(|l| l.trim().trim_end_matches('/'))
        .filter(|l| !l.is_empty())
        .filter(|l| {
            let l = l.to_lowercase();
            key == l || key.strip_prefix(&l).is_some_and(|rest| rest.starts_with('/'))
        })
        .max_by_key(|l| l.len())
        .map(str::to_string)
}

/// インターネットに公開されうるのは github.com だけ。Gogs・Gitea・GitHub Enterprise などの自前のサーバーは、
/// そこで「公開」でも社内などの中での公開なので、外 (api.osv.dev) に送らない
const PUBLIC_HOSTS: &[&str] = &["github.com"];

fn on_public_host(key: &str) -> bool {
    key.split('/').next().is_some_and(|h| PUBLIC_HOSTS.contains(&h))
}

/// 手元のリポジトリから targets.json の中身を作る。返すのは (LocalRepo.id, Target) の組 (パスの順)。
/// 決め方は LockWatch の design.md §3.1 の「RepoTether 側の決め方」
pub fn build_targets(repos: &[LocalRepo], remotes: &[RemoteRepo], rules: Rules) -> Vec<(String, Target)> {
    // 自前のサーバーのリポジトリは、一覧で公開でも private とみなす
    let private: HashMap<&str, bool> =
        remotes.iter().map(|r| (r.key.as_str(), r.private || !on_public_host(&r.key))).collect();
    let hidden: HashSet<&str> = rules.hidden.iter().map(String::as_str).collect();
    let retired: HashSet<&str> = rules.retired.iter().map(String::as_str).collect();
    let mut sorted: Vec<&LocalRepo> = repos.iter().filter(|r| !hidden.contains(r.id.as_str())).collect();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));

    let mut used: HashSet<String> = HashSet::new();
    let mut out = vec![];
    for r in sorted {
        let path = util::display_path(&r.path).replace('\\', "/");
        let primary = r
            .remotes
            .iter()
            .find(|x| x.name == "origin" && x.key.is_some())
            .or_else(|| r.remotes.iter().find(|x| x.key.is_some()))
            .and_then(|x| x.key.clone());
        let any_private = r.remotes.iter().filter_map(|x| x.key.as_deref()).any(|k| private.get(k) == Some(&true));
        let visibility = if any_private {
            "private"
        } else if primary.as_deref().is_some_and(|k| private.get(k) == Some(&false)) {
            "public"
        } else {
            "unknown"
        };
        let share = primary.as_deref().and_then(|k| share_of(k, rules.share));
        // 同じリモートのクローンが 2 つ以上あれば、2 つ目からはパスで区別する (id の重複は LockWatch で誤りになる)
        let id = match primary {
            Some(k) if !used.contains(&k) => k,
            _ => format!("local:{path}"),
        };
        used.insert(id.clone());
        out.push((
            r.id.clone(),
            Target {
                id,
                visibility: visibility.into(),
                local_path: path,
                fetched_path: None,
                share,
                retired: retired.contains(r.id.as_str()),
                runtimes: rules
                    .runtimes
                    .and_then(|m| m.get(&r.id))
                    .map(|m| m.iter().filter(|(_, v)| !v.trim().is_empty()).map(|(k, v)| (k.clone(), v.trim().to_string())).collect())
                    .unwrap_or_default(),
            },
        ));
    }
    out
}

/// targets.json を書く。repos の中身が前と同じなら書かない (書いたら true)。
/// 一時ファイルに書いてから入れ替える (書きかけを LockWatch に読ませない)
pub fn write_targets(path: &Path, targets: &[Target]) -> Result<bool, String> {
    let old: Option<Vec<Target>> = std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| serde_json::from_value(v.get("repos")?.clone()).ok());
    if old.as_deref() == Some(targets) {
        return Ok(false);
    }
    let doc = serde_json::json!({
        "format": 1,
        "generated_at": chrono::Local::now().to_rfc3339(),
        "generator": format!("RepoTether {}", env!("CARGO_PKG_VERSION")),
        "repos": targets,
    });
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{} を作れません: {e}", dir.display()))?;
    }
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())? + "\n";
    std::fs::write(&tmp, text).map_err(|e| format!("{} に書けません: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{} に書けません: {e}", path.display()))?;
    Ok(true)
}

// ---- 結果 (LockWatch → RepoTether) ----

/// latest.json の 1 件の脆弱性。LockWatch の JSON は snake_case、画面へは camelCase
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"), default)]
pub struct Finding {
    pub lockfile: String,
    pub ecosystem: String,
    pub package: String,
    pub version: String,
    pub id: String,
    pub aliases: Vec<String>,
    /// critical / high / medium / low / unknown
    pub severity: String,
    pub score: Option<f64>,
    pub fixed: Vec<String>,
    /// 脆弱性ではない知らせの種類 (unmaintained / unsound / notice)。知らせでなければ None
    pub informational: Option<String>,
    /// 悪意あるコードの記録 (OSV の MAL-) か。LockWatch 0.2.0 までの結果には無い (false)
    pub malicious: bool,
    pub summary: String,
    /// 脆弱性の発行日 (OSV の published)。古い LockWatch の結果には無い
    pub published: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"), default)]
pub struct RepoResult {
    /// ok / no-lockfile / error
    pub status: String,
    /// online / offline
    pub mode: String,
    pub scanned_at: Option<String>,
    pub lockfiles: Vec<String>,
    pub findings: Vec<Finding>,
    /// lock ファイルの健全性の注意。古い LockWatch の結果には無い (空)
    pub notices: Vec<Notice>,
    /// 言語そのもの (PHP・Node.js) の版 (LockWatch の design.md §3.6)。古い LockWatch の結果には無い (空)
    pub runtimes: Vec<Runtime>,
    pub error: Option<String>,
}

/// 言語そのものの版の結果 (LockWatch の design.md §3.6)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"), default)]
pub struct Runtime {
    /// php / node
    pub name: String,
    pub version: String,
    /// どこから (ファイルの名前か「RepoTether で確定」)
    pub source: String,
    pub confirmed: bool,
    /// composer の require.php などの下限から推したもの (系列だけを見る)
    pub lower_bound: bool,
    pub cycle: Option<String>,
    pub eol: Option<String>,
    /// ok / soon / ended / unknown
    pub status: String,
    pub latest: Option<String>,
    pub exact: bool,
    /// 同じ系列で、この版より新しいセキュリティリリース
    pub security: Vec<RuntimeRelease>,
    pub upgrade_to: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RuntimeRelease {
    pub version: String,
    pub date: Option<String>,
}

/// lock ファイルそのものを読んで分かったこと (LockWatch の design.md §3.5)。脆弱性とは別
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"), default)]
pub struct Notice {
    pub lockfile: String,
    /// unpinned / not-registry / no-integrity / recent
    pub kind: String,
    pub package: String,
    pub version: String,
    /// kind ごとの中身 (書かれている版の指定、取得元、件数、公開の時刻)
    pub detail: String,
    /// 前回から新しく出たものか。latest.json には無く、`report` が new_notices から付ける
    pub fresh: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"), default)]
pub struct NewFinding {
    pub repo: String,
    pub package: String,
    pub id: String,
    pub severity: String,
    pub informational: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct NewNotice {
    pub repo: String,
    pub lockfile: String,
    pub kind: String,
    pub package: String,
    pub version: String,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Latest {
    pub scanned_at: Option<String>,
    pub osv_scanner: Option<String>,
    pub db_downloaded_at: Option<String>,
    pub repos: BTreeMap<String, RepoResult>,
    pub new: Vec<NewFinding>,
    pub new_notices: Vec<NewNotice>,
}

/// latest.json を読む。まだ無ければ None
pub fn read_latest(path: &Path) -> Result<Option<Latest>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{} を読めません: {e}", path.display())),
    };
    serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map(Some)
        .map_err(|e| format!("{} の形が違います: {e}", path.display()))
}

/// 画面に渡す、リポジトリ 1 つ分
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoVulns {
    /// LockWatch での id
    pub target_id: String,
    pub visibility: String,
    /// まだ照合していなければ None
    pub result: Option<RepoResult>,
    /// 前回から新しく出たもの (package, id)
    pub new: Vec<(String, String)>,
    /// 社員向けの一覧での見出し。載せないなら None
    pub share: Option<String>,
    /// 廃止 (照合しない)
    pub retired: bool,
}

/// 画面に渡す全体
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VulnReport {
    /// 設定で LockWatch の場所が決まっているか
    pub configured: bool,
    /// LockWatch を呼べない・結果を読めないときの理由
    pub error: Option<String>,
    pub locations: Option<Locations>,
    pub scanned_at: Option<String>,
    pub osv_scanner: Option<String>,
    pub db_downloaded_at: Option<String>,
    /// LocalRepo.id → そのリポジトリの結果
    pub by_repo: BTreeMap<String, RepoVulns>,
}

/// targets と latest.json を突き合わせて、画面に渡す形にする
pub fn report(targets: &[(String, Target)], latest: Option<&Latest>) -> VulnReport {
    let mut by_repo = BTreeMap::new();
    for (local_id, t) in targets {
        let mut result = latest.and_then(|l| l.repos.get(&t.id)).cloned();
        if let (Some(res), Some(l)) = (result.as_mut(), latest) {
            for n in &mut res.notices {
                n.fresh = l.new_notices.iter().any(|x| {
                    x.repo == t.id && x.lockfile == n.lockfile && x.kind == n.kind && x.package == n.package && x.version == n.version
                });
            }
        }
        let new = latest
            .map(|l| l.new.iter().filter(|n| n.repo == t.id).map(|n| (n.package.clone(), n.id.clone())).collect())
            .unwrap_or_default();
        by_repo.insert(
            local_id.clone(),
            RepoVulns {
                target_id: t.id.clone(),
                visibility: t.visibility.clone(),
                // 廃止にする前の結果が残っていても見せない (照合していないので古いまま)
                result: if t.retired { None } else { result },
                new: if t.retired { vec![] } else { new },
                share: t.share.clone(),
                retired: t.retired,
            },
        );
    }
    VulnReport {
        configured: true,
        error: None,
        locations: None,
        scanned_at: latest.and_then(|l| l.scanned_at.clone()),
        osv_scanner: latest.and_then(|l| l.osv_scanner.clone()),
        db_downloaded_at: latest.and_then(|l| l.db_downloaded_at.clone()),
        by_repo,
    }
}

/// 事前チェック (`lockwatch scan --id <id> --check`) の答え
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"), default)]
pub struct Check {
    pub id: String,
    pub mode: String,
    /// 今照合したら前回の結果がそのまま返る
    pub cached: bool,
    /// cached のとき、その結果を作った時刻
    pub scanned_at: Option<String>,
    pub lockfiles: Vec<String>,
    /// cached / no-cache / db-update / no-lockfile / error
    pub reason: String,
    pub error: Option<String>,
}

/// 照合はせず、今照合したらキャッシュの結果が返るかを聞く (読むだけ。通信もしない)
pub fn check_one(r: &Runner, target_id: &str) -> Result<Check, String> {
    let o = run(r, &["scan", "--id", target_id, "--check"], CONFIG_TIMEOUT)?;
    if o.code != Some(0) {
        return Err(error_line(&o, "LockWatch の事前チェックが失敗しました"));
    }
    serde_json::from_str(&o.stdout).map_err(|e| format!("LockWatch の事前チェックの答えを読めません: {e}"))
}

/// `lockwatch scan --id <id>`。latest.json のそのリポジトリだけが差し替わる。
/// fresh ならキャッシュを使わずに照合し直す (`--no-cache`)
pub fn scan_one(r: &Runner, target_id: &str, fresh: bool) -> Result<(), String> {
    let mut args = vec!["scan", "--id", target_id];
    if fresh {
        args.push("--no-cache");
    }
    let o = run(r, &args, SCAN_TIMEOUT)?;
    match o.code {
        Some(0) => Ok(()),
        // 照合に失敗したリポジトリは結果に error として書かれる。osv-scanner が無いなど、結果を書けなかったときだけ Err
        Some(4) if o.stderr.is_empty() => Ok(()),
        Some(3) => Err("LockWatch が実行中です (定期実行など)。終わってからもう一度押してください".into()),
        _ => Err(error_line(&o, "LockWatch が失敗しました")),
    }
}

/// `lockwatch report --share`。社員向けの一覧を `<data>/share` に書き、その index.html の場所を返す。照合はしない
pub fn share_report(r: &Runner, loc: &Locations) -> Result<PathBuf, String> {
    let o = run(r, &["report", "--share"], CONFIG_TIMEOUT)?;
    if o.code != Some(0) {
        return Err(error_line(&o, "LockWatch が社員向けの一覧を書けませんでした (LockWatch を新しくしてください)"));
    }
    Ok(loc.data_dir.join("share").join("index.html"))
}

// ---- ビルド・リリースへの影響 (`lockwatch assess`。LockWatch の design.md §7.3) ----

/// `assess --id <id> --last` の答え: 前回書いた HTML
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"), default)]
pub struct Assessed {
    pub path: String,
    pub exists: bool,
    pub made_at: Option<String>,
    /// そのとき渡した脆弱性と今の結果の中身が違うか (照合の時刻だけの違いは false)。比べられなければ None
    pub changed: Option<bool>,
}

/// assess の無い古い LockWatch では argparse が「invalid choice」で断る
fn assess_error(o: &Output, fallback: &str) -> String {
    if o.stderr.contains("invalid choice: 'assess'") {
        return "この LockWatch には「影響を調べる」がありません (0.5.0 から)。LockWatch を更新してください (git pull と uv sync)".into();
    }
    error_line(o, fallback)
}

/// 前回の HTML の場所と時刻 (Claude は動かさない)
pub fn assess_last(r: &Runner, target_id: &str) -> Result<Assessed, String> {
    let o = run(r, &["assess", "--id", target_id, "--last"], CONFIG_TIMEOUT)?;
    if o.code != Some(0) {
        return Err(assess_error(&o, "LockWatch が答えませんでした"));
    }
    serde_json::from_str(&o.stdout).map_err(|e| format!("LockWatch の答えを読めません: {e}"))
}

/// そのリポジトリの脆弱性がビルド・リリースにどう効くかを、LockWatch から Claude Code に調べさせる。書いた HTML の場所を返す
pub fn assess_one(r: &Runner, target_id: &str) -> Result<PathBuf, String> {
    let o = run(r, &["assess", "--id", target_id], ASSESS_TIMEOUT)?;
    if o.code != Some(0) {
        return Err(assess_error(&o, "影響を調べられませんでした"));
    }
    written_path(&o.stdout).ok_or_else(|| "LockWatch が書いた場所を答えませんでした".to_string())
}

/// 「書きました: <場所>」の行
fn written_path(stdout: &str) -> Option<PathBuf> {
    stdout.lines().rev().find_map(|l| l.trim().strip_prefix("書きました: ")).map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::Remote;

    #[test]
    fn assess_output() {
        let out = "Claude Code に調べさせています（2 パッケージ・0 言語。数分かかります）…\n書きました: C:/lw/data/assess/r.html\n";
        assert_eq!(written_path(out), Some(PathBuf::from("C:/lw/data/assess/r.html")));
        assert_eq!(written_path("何も書いていない\n"), None);
        let old = Output {
            code: Some(2),
            stdout: String::new(),
            stderr: "usage: lockwatch\nlockwatch: error: argument command: invalid choice: 'assess'".into(),
        };
        assert!(assess_error(&old, "x").contains("0.5.0"));
        let v: Assessed = serde_json::from_str(r#"{"path": "C:/a.html", "exists": true, "made_at": "2026-10-23T09:00:00+09:00"}"#).unwrap();
        assert_eq!((v.exists, v.made_at.as_deref()), (true, Some("2026-10-23T09:00:00+09:00")));
    }

    fn local(path: &str, remotes: &[(&str, Option<&str>)]) -> LocalRepo {
        LocalRepo {
            id: util::path_key(path),
            path: path.into(),
            remotes: remotes
                .iter()
                .map(|(n, k)| Remote { name: (*n).into(), url: String::new(), key: k.map(Into::into) })
                .collect(),
            ..Default::default()
        }
    }

    fn remote(key: &str, private: bool) -> RemoteRepo {
        RemoteRepo { key: key.into(), private, ..Default::default() }
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("repotether-lw-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn id_and_visibility_follow_the_remote_list() {
        let repos = vec![
            local(r"C:\Repos\pub", &[("origin", Some("github.com/me/pub"))]),
            local(r"C:\Repos\priv", &[("origin", Some("github.com/me/priv"))]),
            // 一覧に無いリモート (他人のもの・一覧をまだ取っていない) は unknown
            local(r"C:\Repos\other", &[("origin", Some("github.com/someone/other"))]),
            local(r"C:\Repos\nolink", &[]),
            // origin が public でも、ほかのリモートが private なら private
            local(r"C:\Repos\mirror", &[("origin", Some("github.com/me/pub2")), ("backup", Some("git.example.com/me/pub2"))]),
            // origin が無ければ最初のリモート
            local(r"C:\Repos\up", &[("upstream", Some("github.com/me/pub"))]),
        ];
        let remotes = vec![
            remote("github.com/me/pub", false),
            remote("github.com/me/priv", true),
            remote("github.com/me/pub2", false),
            remote("git.example.com/me/pub2", true),
        ];
        let t: HashMap<String, Target> = build_targets(&repos, &remotes, Rules::default()).into_iter().collect();
        let get = |p: &str| &t[&util::path_key(p)];
        assert_eq!((get(r"C:\Repos\pub").id.as_str(), get(r"C:\Repos\pub").visibility.as_str()), ("github.com/me/pub", "public"));
        assert_eq!(get(r"C:\Repos\priv").visibility, "private");
        assert_eq!(get(r"C:\Repos\other").visibility, "unknown");
        assert_eq!(get(r"C:\Repos\nolink").visibility, "unknown");
        assert_eq!(get(r"C:\Repos\mirror").visibility, "private");
        // リモートが無ければ local:<パス> (区切りは /)
        let nolink = &get(r"C:\Repos\nolink").id;
        assert!(nolink.starts_with("local:") && nolink.ends_with("/Repos/nolink"), "{nolink}");
        assert!(get(r"C:\Repos\pub").local_path.ends_with("/Repos/pub"));
        assert_eq!(get(r"C:\Repos\pub").fetched_path, None);
        // 同じリモートの 2 つ目のクローン (パスの順で後ろ) はパスで区別する
        assert_eq!(get(r"C:\Repos\up").id.starts_with("local:"), true);
        assert_eq!(get(r"C:\Repos\up").visibility, "public");
    }

    #[test]
    fn self_hosted_servers_are_never_public() {
        // Gogs・Gitea・GitHub Enterprise で「公開」でも、社内などの中での公開なので外に送らない
        let repos = vec![
            local(r"C:\Repos\gogs", &[("origin", Some("gogs.example.com/me/app"))]),
            local(r"C:\Repos\ghe", &[("origin", Some("github.example.co.jp/me/app"))]),
            local(r"C:\Repos\gh", &[("origin", Some("github.com/me/app"))]),
        ];
        let remotes = vec![
            remote("gogs.example.com/me/app", false),
            remote("github.example.co.jp/me/app", false),
            remote("github.com/me/app", false),
        ];
        let t: HashMap<String, Target> = build_targets(&repos, &remotes, Rules::default()).into_iter().collect();
        assert_eq!(t[&util::path_key(r"C:\Repos\gogs")].visibility, "private");
        assert_eq!(t[&util::path_key(r"C:\Repos\ghe")].visibility, "private");
        assert_eq!(t[&util::path_key(r"C:\Repos\gh")].visibility, "public");
    }

    #[test]
    fn hidden_repos_are_left_out() {
        let repos = vec![local(r"C:\Repos\a", &[]), local(r"C:\Repos\b", &[])];
        let hidden = [util::path_key(r"C:\Repos\a")];
        let t = build_targets(&repos, &[], Rules { hidden: &hidden, ..Default::default() });
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].0, util::path_key(r"C:\Repos\b"));
    }

    #[test]
    fn share_and_retired() {
        let repos = vec![
            local(r"C:\Repos\gogs", &[("origin", Some("gogs.example.com/someone/app"))]),
            local(r"C:\Repos\org", &[("origin", Some("github.com/acme/api"))]),
            local(r"C:\Repos\orgdev", &[("origin", Some("github.com/acmedev/web"))]),
            local(r"C:\Repos\mine", &[("origin", Some("github.com/me/tool"))]),
            local(r"C:\Repos\sample", &[("origin", Some("github.com/flutter/codelabs"))]),
            local(r"C:\Repos\nolink", &[]),
        ];
        let share = ["gogs.example.com".to_string(), " github.com/Acme/ ".to_string(), "github.com/acmedev".to_string()];
        let retired = [util::path_key(r"C:\Repos\orgdev")];
        let t: HashMap<String, Target> =
            build_targets(&repos, &[], Rules { share: &share, retired: &retired, ..Default::default() }).into_iter().collect();
        let get = |p: &str| &t[&util::path_key(p)];
        assert_eq!(get(r"C:\Repos\gogs").share.as_deref(), Some("gogs.example.com"));
        // 書いた文字のまま見出しにする (前後の空白と末尾の / は落とす)。github.com/acme は github.com/acmedev に当てない
        assert_eq!(get(r"C:\Repos\org").share.as_deref(), Some("github.com/Acme"));
        assert_eq!(get(r"C:\Repos\orgdev").share.as_deref(), Some("github.com/acmedev"));
        for p in [r"C:\Repos\mine", r"C:\Repos\sample", r"C:\Repos\nolink"] {
            assert_eq!(get(p).share, None, "{p}");
        }
        assert!(get(r"C:\Repos\orgdev").retired);
        assert!(!get(r"C:\Repos\org").retired);
        let v = serde_json::to_value(get(r"C:\Repos\orgdev")).unwrap();
        assert_eq!((v["share"].as_str(), v["retired"].as_bool()), (Some("github.com/acmedev"), Some(true)));
        assert!(v.get("runtimes").is_none(), "確定した版が無ければ書かない");
    }

    #[test]
    fn confirmed_runtimes_are_passed_on() {
        let repos = vec![local(r"C:\Repos\web", &[]), local(r"C:\Repos\other", &[])];
        let mut runtimes = BTreeMap::new();
        runtimes.insert(
            util::path_key(r"C:\Repos\web"),
            BTreeMap::from([("php".to_string(), " 8.2.20 ".to_string()), ("node".to_string(), "".to_string())]),
        );
        let t: HashMap<String, Target> =
            build_targets(&repos, &[], Rules { runtimes: Some(&runtimes), ..Default::default() }).into_iter().collect();
        let web = serde_json::to_value(&t[&util::path_key(r"C:\Repos\web")]).unwrap();
        assert_eq!(web["runtimes"], serde_json::json!({"php": "8.2.20"}), "空の版は書かない");
        assert!(t[&util::path_key(r"C:\Repos\other")].runtimes.is_empty());
        let r: Runtime = serde_json::from_str(
            r#"{"name": "php", "version": "8.2.20", "status": "soon", "exact": true, "security": [{"version": "8.2.35", "date": "2026-10-21"}],
                "upgrade_to": "8.2.35", "future": 1}"#,
        )
        .unwrap();
        assert_eq!((r.security[0].version.as_str(), r.upgrade_to.as_deref()), ("8.2.35", Some("8.2.35")));
        assert_eq!(serde_json::to_value(&r).unwrap()["upgradeTo"], "8.2.35");
    }

    #[test]
    fn targets_are_written_only_when_changed() {
        let d = tmp("targets");
        let path = d.join("data").join("targets.json");
        let t = build_targets(&[local(r"C:\Repos\a", &[])], &[], Rules::default());
        let ts: Vec<Target> = t.into_iter().map(|(_, t)| t).collect();
        assert!(write_targets(&path, &ts).unwrap());
        assert!(!write_targets(&path, &ts).unwrap());
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["format"], 1);
        assert_eq!(v["repos"][0]["visibility"], "unknown");
        assert!(v["repos"][0].get("local_path").is_some(), "LockWatch に合わせて snake_case");
        assert!(!path.with_extension("json.tmp").exists());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn reads_latest_and_matches_by_target_id() {
        let d = tmp("latest");
        let path = d.join("latest.json");
        assert_eq!(read_latest(&path).unwrap(), None);
        std::fs::write(
            &path,
            r#"{"format": 1, "scanned_at": "2026-10-02T09:00:00+09:00", "osv_scanner": "2.6.0", "db_downloaded_at": null,
                "repos": {"github.com/me/pub": {"status": "ok", "mode": "online", "scanned_at": "2026-10-02T09:00:00+09:00",
                  "lockfiles": ["pnpm-lock.yaml"], "findings": [{"lockfile": "pnpm-lock.yaml", "ecosystem": "npm",
                  "package": "vite", "version": "6.0.1", "id": "GHSA-x", "aliases": [], "severity": "high", "score": 7.5,
                  "fixed": ["6.0.9"], "informational": null, "summary": "s", "future_key": 1}],
                  "notices": [{"lockfile": "requirements.txt", "kind": "unpinned", "package": "jinja2", "version": "", "detail": ""},
                              {"lockfile": "uv.lock", "kind": "recent", "package": "fresh", "version": "2.0.0", "detail": "2026-10-03T08:00:00Z"}]}},
                "new": [{"repo": "github.com/me/pub", "package": "vite", "id": "GHSA-x", "severity": "high", "informational": null}],
                "new_notices": [{"repo": "github.com/me/pub", "lockfile": "uv.lock", "kind": "recent", "package": "fresh", "version": "2.0.0"},
                                {"repo": "github.com/me/other", "lockfile": "requirements.txt", "kind": "unpinned", "package": "jinja2", "version": ""}]}"#,
        )
        .unwrap();
        let latest = read_latest(&path).unwrap().unwrap();
        let targets = build_targets(
            &[local(r"C:\Repos\pub", &[("origin", Some("github.com/me/pub"))]), local(r"C:\Repos\x", &[])],
            &[remote("github.com/me/pub", false)],
            Rules::default(),
        );
        let rep = report(&targets, Some(&latest));
        let pubr = &rep.by_repo[&util::path_key(r"C:\Repos\pub")];
        let res = pubr.result.as_ref().unwrap();
        assert_eq!(res.findings[0].fixed, ["6.0.9"]);
        assert!(!res.findings[0].malicious, "malicious の無い古い結果は false");
        // 注意。新しく出た印は、同じリポジトリの new_notices にあるものだけ
        assert_eq!(res.notices.iter().map(|n| (n.package.as_str(), n.fresh)).collect::<Vec<_>>(), [("jinja2", false), ("fresh", true)]);
        let old: RepoResult = serde_json::from_str(r#"{"status": "ok", "findings": []}"#).unwrap();
        assert!(old.notices.is_empty(), "notices の無い古い結果は空");
        let mal: Finding = serde_json::from_str(r#"{"package": "evil", "id": "MAL-2026-1", "severity": "critical", "malicious": true}"#).unwrap();
        assert!(mal.malicious);
        assert_eq!(serde_json::to_value(&mal).unwrap()["malicious"], true);
        assert_eq!(pubr.new, [("vite".to_string(), "GHSA-x".to_string())]);
        assert!(rep.by_repo[&util::path_key(r"C:\Repos\x")].result.is_none(), "まだ照合していない");
        // 画面へは camelCase
        let json = serde_json::to_value(&rep).unwrap();
        assert!(json["byRepo"].as_object().unwrap().values().any(|v| v["result"]["scannedAt"].is_string()));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn gui_uses_windowed_python_when_present() {
        let d = tmp("pyw");
        std::fs::write(d.join("python.exe"), "").unwrap();
        assert_eq!(windowed(&d.join("python.exe")), d.join("python.exe"), "pythonw.exe が無ければそのまま");
        std::fs::write(d.join("pythonw.exe"), "").unwrap();
        assert_eq!(windowed(&d.join("python.exe")), d.join("pythonw.exe"));
        std::fs::write(d.join("pyw.exe"), "").unwrap();
        assert_eq!(windowed(&d.join("py.exe")), d.join("pyw.exe"));
        assert_eq!(windowed(Path::new("/usr/bin/python3")), PathBuf::from("/usr/bin/python3"));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn gui_prefers_pythonw_of_the_base_python() {
        let d = tmp("basepyw");
        let base = d.to_string_lossy().into_owned();
        assert_eq!(pythonw_in(&format!("{base}\r\n")), None, "pythonw.exe が無ければ使わない");
        std::fs::write(d.join("pythonw.exe"), "").unwrap();
        assert_eq!(pythonw_in(&format!("{base}\r\n")), Some(d.join("pythonw.exe")), "python の出力の改行は除く");
        assert_eq!(pythonw_in(""), None, "python が答えなかった");
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn parses_status_answer() {
        let s: Status = serde_json::from_str(
            r#"{"lockwatch": "0.1.0", "data_dir": "C:\\lw\\data", "targets": "C:\\lw\\data\\targets.json",
                "osv_scanner": {"path": null, "version": null, "error": "osv-scanner が PATH にありません"},
                "targets_count": null, "targets_error": "ありません", "latest": null, "db": {},
                "task": {"name": "LockWatch scan", "registered": false}}"#,
        )
        .unwrap();
        assert!(s.osv_scanner.error.is_some());
        assert_eq!(s.task.registered, Some(false));
        let json = serde_json::to_value(&s).unwrap();
        assert!(json["osvScanner"]["error"].is_string());
        assert_eq!(json["targetsError"], "ありません");
    }

    #[test]
    fn parses_check_answer() {
        let c: Check = serde_json::from_str(
            r#"{"id": "github.com/me/pub", "mode": "online", "cached": true, "scanned_at": "2026-10-02T09:37:52+09:00",
                "lockfiles": ["uv.lock"], "reason": "cached"}"#,
        )
        .unwrap();
        assert!(c.cached);
        assert_eq!(c.error, None);
        let json = serde_json::to_value(&c).unwrap();
        assert_eq!(json["scannedAt"], "2026-10-02T09:37:52+09:00");
    }

    #[test]
    fn locations_come_from_config_show() {
        let l = parse_locations(r#"{"data_dir": null, "_effective_data_dir": "C:\\lw\\data", "_effective_targets": "C:\\lw\\data\\targets.json"}"#).unwrap();
        assert_eq!(l.latest(), PathBuf::from(r"C:\lw\data").join("results").join("latest.json"));
        assert!(parse_locations("{}").is_err());
    }

    #[test]
    fn folder_must_be_lockwatch() {
        let d = tmp("notlw");
        assert!(resolve(&d.to_string_lossy()).unwrap_err().contains("LockWatch のリポジトリではありません"));
        let _ = std::fs::remove_dir_all(d);
    }
}
