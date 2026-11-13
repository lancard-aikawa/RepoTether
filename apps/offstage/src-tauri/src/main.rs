// Offstage の画面。取り込みと記録は offstage-core。ここは画面とのやり取りだけ。
// 通信しない: 依存に通信するクレートを入れず、画面は CSP で ipc: 以外につながない (tests/no_network.rs)
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use offstage_core::classify::{secret_rule_of, Class};
use offstage_core::link::{default_projects_dir, Mention};
use offstage_core::pipeline::{refresh as run_refresh, Report};
use offstage_core::query::{Alerts, Query, Row};
use offstage_core::repos::{find_repos, repotether_config_path, roots_from_config};
use offstage_core::store::{default_store_path, Store};
use regex::Regex;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// どこを見るか。環境変数で差し替えられる (見本の画面を撮るときに、架空のリポジトリと会話を読ませる)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    db: PathBuf,
    roots: Vec<PathBuf>,
    depth: u32,
    projects: Option<PathBuf>,
    /// roots をどこから取ったか
    roots_from: String,
    /// 見本の画面を撮るとき: 起動したらすぐ取り込む (OFFSTAGE_AUTOREFRESH=1)
    auto_refresh: bool,
    /// 見本の画面を撮るとき: この行を選ぶ (OFFSTAGE_SELECT=<リポジトリのフォルダ名>/<パス>)
    select: Option<String>,
    /// 見本の画面を撮るとき: 選んだ行の最初の場面を開く (OFFSTAGE_OPEN_SCENE=1)
    open_scene: bool,
    /// 見本の画面を撮るとき: 表示 (OFFSTAGE_VIEW=table / tree)
    view: Option<String>,
    /// 起動時に絞るリポジトリ (RepoTether の行から `--repo <フォルダ>` で開いたとき)
    initial_repo: Option<String>,
}

fn settings() -> Settings {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty());
    let db = env("OFFSTAGE_DB").map(PathBuf::from).or_else(default_store_path).unwrap_or_else(|| "index.sqlite".into());
    let projects = env("OFFSTAGE_PROJECTS").map(PathBuf::from).or_else(default_projects_dir);
    let depth_env = env("OFFSTAGE_DEPTH").and_then(|v| v.to_string_lossy().parse().ok());
    let auto_refresh = env("OFFSTAGE_AUTOREFRESH").is_some_and(|v| v == "1");
    let select = env("OFFSTAGE_SELECT").map(|v| v.to_string_lossy().into_owned());
    let open_scene = env("OFFSTAGE_OPEN_SCENE").is_some_and(|v| v == "1");
    let view = env("OFFSTAGE_VIEW").map(|v| v.to_string_lossy().into_owned());
    let (roots, depth, roots_from) = if let Some(r) = env("OFFSTAGE_ROOTS") {
        (std::env::split_paths(&r).collect(), depth_env.unwrap_or(3), "OFFSTAGE_ROOTS".to_string())
    } else {
        match repotether_config_path().and_then(|p| roots_from_config(&p).map(|r| (p, r))) {
            Some((p, (roots, depth))) => (roots, depth_env.unwrap_or(depth), p.to_string_lossy().into_owned()),
            None => (Vec::new(), 3, "(none)".to_string()),
        }
    };
    Settings { db, roots, depth, projects, roots_from, auto_refresh, select, open_scene, view, initial_repo: None }
}

struct AppState {
    settings: Settings,
    store: Result<Mutex<Store>, String>,
    titles: Mutex<HashMap<String, String>>,
}

type St<'a> = tauri::State<'a, Arc<AppState>>;

fn with_store<T>(st: &AppState, f: impl FnOnce(&mut Store) -> Result<T, String>) -> Result<T, String> {
    let m = st.store.as_ref().map_err(|e| e.clone())?;
    let mut g = m.lock().map_err(|_| "記録が使えません (前の処理が失敗しました)".to_string())?;
    f(&mut g)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Overview {
    settings: Settings,
    alerts: Option<Alerts>,
    repos: Vec<(String, bool)>,
    error: Option<String>,
}

#[tauri::command]
fn overview(st: St) -> Overview {
    let alerts = with_store(&st, |s| s.alerts());
    let repos = with_store(&st, |s| s.repos()).unwrap_or_default();
    Overview {
        settings: st.settings.clone(),
        error: alerts.as_ref().err().cloned(),
        alerts: alerts.ok(),
        repos,
    }
}

#[tauri::command]
async fn refresh(st: St<'_>) -> Result<Report, String> {
    let st = Arc::clone(&st);
    tauri::async_runtime::spawn_blocking(move || {
        let set = &st.settings;
        let repos = find_repos(&set.roots, set.depth);
        with_store(&st, |s| run_refresh(s, &repos, set.projects.as_deref()))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn search(st: St, q: Query) -> Result<Vec<Row>, String> {
    with_store(&st, |s| s.search(&q))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SceneView {
    #[serde(flatten)]
    m: Mention,
    title: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Detail {
    row: Row,
    abs_path: String,
    exists: bool,
    /// 区分を決めた規則 (機密の疑いと畳んだフォルダ)
    rule: Option<String>,
    /// 当たっている付け替え (パス, 区分)
    override_of: Option<(String, String)>,
    scenes: Vec<SceneView>,
}

#[tauri::command]
fn detail(st: St, repo: String, rel_path: String) -> Result<Option<Detail>, String> {
    let got = with_store(&st, |s| {
        let Some(row) = s.row_of(&repo, &rel_path)? else { return Ok(None) };
        Ok(Some((row, s.override_of(&repo, &rel_path)?, s.mentions_of(&repo, &rel_path)?)))
    })?;
    let Some((row, override_of, mentions)) = got else { return Ok(None) };
    let abs = Path::new(&repo).join(&rel_path);
    let rule = match (&row.folded, row.class.as_str()) {
        (Some(f), _) => Some(f.clone()),
        (None, "secret") => secret_rule_of(&rel_path),
        _ => None,
    };
    let scenes = mentions
        .into_iter()
        .map(|m| SceneView { title: session_title(&st, &m.log_path), m })
        .collect();
    Ok(Some(Detail {
        exists: abs.exists(),
        abs_path: abs.to_string_lossy().into_owned(),
        row,
        rule,
        override_of,
        scenes,
    }))
}

/// 会話の題 (記録の終わり 256KB にある最後の aiTitle)。無ければ空
fn session_title(st: &AppState, log: &str) -> String {
    if let Some(t) = st.titles.lock().ok().and_then(|m| m.get(log).cloned()) {
        return t;
    }
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r#""aiTitle"\s*:\s*"((?:[^"\\]|\\.)*)""#).unwrap());
    let title = (|| {
        use std::io::{Read, Seek, SeekFrom};
        let mut f = std::fs::File::open(log).ok()?;
        let len = f.metadata().ok()?.len();
        let start = len.saturating_sub(256 * 1024);
        f.seek(SeekFrom::Start(start)).ok()?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).ok()?;
        let text = String::from_utf8_lossy(&buf);
        let raw = re.captures_iter(&text).last()?.get(1)?.as_str().to_string();
        serde_json::from_str::<String>(&format!("\"{raw}\"")).ok()
    })()
    .unwrap_or_default();
    if let Ok(mut m) = st.titles.lock() {
        m.insert(log.to_string(), title.clone());
    }
    title
}

/// 場面の前後のやり取り。開くのは Offstage が読んだ会話の記録だけ
#[tauri::command]
fn scene(st: St, log_path: String, line_no: i64) -> Result<Vec<offstage_core::link::Turn>, String> {
    if !with_store(&st, |s| s.is_known_log(&log_path))? {
        return Err("記録にない会話のファイルです".into());
    }
    offstage_core::link::excerpt(Path::new(&log_path), line_no)
}

#[tauri::command]
fn dismiss(st: St, repo: String, rel_path: String) -> Result<bool, String> {
    with_store(&st, |s| {
        let n = s.dismiss(&repo, &rel_path)?;
        let _ = s.write_summary(); // RepoTether の印を数え直す
        Ok(n)
    })
}

#[tauri::command]
fn set_class(st: St, repo: String, rel_prefix: String, class: String) -> Result<(), String> {
    let c = Class::from_label(&class).ok_or_else(|| format!("区分が違います: {class}"))?;
    with_store(&st, |s| {
        s.set_override(&repo, &rel_prefix, c)?;
        let _ = s.write_summary();
        Ok(())
    })
}

#[tauri::command]
fn clear_class(st: St, repo: String, rel_prefix: String) -> Result<bool, String> {
    with_store(&st, |s| s.clear_override(&repo, &rel_prefix))
}

/// エクスプローラーに渡す引数。ファイルがあれば選んだ状態 (`/select,"…"`)、無ければ在る親のフォルダ (`"…"`)。
/// 記録のパスは `/` 区切りなので `\` にそろえる (2026-11-12、`…\sandbox/x.md` を渡すとデスクトップが開いた)。
/// 空白を含むパスのために、引数の形は自分で作り raw_arg で渡す (Command::arg は全体を引用符で囲み、/select が読まれない)
fn explorer_arg(repo: &str, rel_path: &str) -> Option<String> {
    let full = format!("{}\\{}", repo.trim_end_matches(['\\', '/']), rel_path).replace('/', "\\");
    let p = PathBuf::from(&full);
    if p.exists() {
        return Some(format!("/select,\"{full}\""));
    }
    let mut dir = p.parent().map(Path::to_path_buf);
    while let Some(d) = &dir {
        if d.is_dir() {
            return Some(format!("\"{}\"", d.display()));
        }
        dir = d.parent().map(Path::to_path_buf);
    }
    None
}

/// エクスプローラーで場所を開く
#[tauri::command]
fn reveal(repo: String, rel_path: String) -> Result<(), String> {
    let arg = explorer_arg(&repo, &rel_path).ok_or("開ける場所がありません")?;
    let mut cmd = std::process::Command::new("explorer.exe");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.raw_arg(&arg);
    }
    #[cfg(not(windows))]
    cmd.arg(&arg);
    cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::explorer_arg;

    #[test]
    fn explorer_arg_uses_backslashes_and_quotes() {
        let t = std::env::temp_dir().join(format!("offstage reveal {}", std::process::id()));
        std::fs::create_dir_all(t.join("sandbox/sub dir")).unwrap();
        std::fs::write(t.join("sandbox/sub dir/a b.md"), "x").unwrap();
        let repo = t.to_string_lossy().into_owned();
        let arg = explorer_arg(&repo, "sandbox/sub dir/a b.md").unwrap();
        assert_eq!(arg, format!("/select,\"{repo}\\sandbox\\sub dir\\a b.md\""));
        assert!(!arg.contains('/') || arg.starts_with("/select,") && !arg[8..].contains('/'));
        // 消えたファイルは、在る親のフォルダを開く
        assert_eq!(explorer_arg(&repo, "sandbox/sub dir/gone/x.md").unwrap(), format!("\"{repo}\\sandbox\\sub dir\""));
        let _ = std::fs::remove_dir_all(&t);
    }
}

/// 起動の引数
///   --refresh        窓を出さずに取り込みだけして終わる (RepoTether が起動時に裏で走らせる)。終了コードは 0 / 1
///   --repo <フォルダ> そのリポジトリで絞った状態で開く (RepoTether の行から開くとき)
struct Args {
    refresh: bool,
    repo: Option<String>,
}

fn args() -> Args {
    let mut a = Args { refresh: false, repo: None };
    let mut it = std::env::args().skip(1);
    while let Some(x) = it.next() {
        match x.as_str() {
            "--refresh" => a.refresh = true,
            "--repo" => a.repo = it.next(),
            _ => {}
        }
    }
    a
}

/// 窓を出さずに取り込む。Tauri は起動しない (exe は窓のアプリなので、黒い窓も出ない)
fn headless_refresh(settings: &Settings) -> i32 {
    let mut store = match Store::open(&settings.db) {
        Ok(s) => s,
        Err(_) => return 1,
    };
    let repos = find_repos(&settings.roots, settings.depth);
    match run_refresh(&mut store, &repos, settings.projects.as_deref()) {
        Ok(_) => 0,
        Err(_) => 1,
    }
}

fn main() {
    let a = args();
    let mut settings = settings();
    if a.refresh {
        std::process::exit(headless_refresh(&settings));
    }
    settings.initial_repo = a.repo;
    let store = Store::open(&settings.db).map(Mutex::new);
    let state = Arc::new(AppState { settings, store, titles: Mutex::new(HashMap::new()) });
    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            overview, refresh, search, detail, scene, dismiss, set_class, clear_class, reveal
        ])
        .run(tauri::generate_context!())
        .expect("error while running Offstage");
}
