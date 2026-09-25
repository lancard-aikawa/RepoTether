pub mod core;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, State};

use crate::core::config::Config;
use crate::core::model::Snapshot;
use crate::core::util;

struct AppState {
    config: Mutex<Config>,
    config_path: PathBuf,
    cache_dir: PathBuf,
    refreshing: AtomicBool,
}

impl AppState {
    fn snapshot_path(&self) -> PathBuf {
        self.cache_dir.join("snapshot.json")
    }
}

#[tauri::command]
fn get_config(state: State<AppState>) -> Config {
    state.config.lock().unwrap().clone()
}

#[tauri::command]
fn save_config(state: State<AppState>, config: Config) -> Result<(), String> {
    config.save(&state.config_path)?;
    *state.config.lock().unwrap() = config;
    Ok(())
}

#[tauri::command]
fn load_snapshot(state: State<AppState>) -> Option<Snapshot> {
    core::load_snapshot(&state.snapshot_path())
}

/// ローカルを読み直す。include_remote ならリモート一覧も取り直し、そうでなければ前回分を引き継ぐ。
/// 進み具合は "refresh-progress" イベントで送る。
#[tauri::command]
async fn refresh(app: AppHandle, include_remote: bool) -> Result<Snapshot, String> {
    let state = app.state::<AppState>();
    if state.refreshing.swap(true, Ordering::SeqCst) {
        return Err("更新中です".into());
    }
    let result = refresh_inner(&app, include_remote).await;
    state.refreshing.store(false, Ordering::SeqCst);
    result
}

async fn refresh_inner(app: &AppHandle, include_remote: bool) -> Result<Snapshot, String> {
    let state = app.state::<AppState>();
    let cfg = state.config.lock().unwrap().clone();
    let cache_dir = state.cache_dir.clone();
    let snapshot_path = state.snapshot_path();

    let app2 = app.clone();
    let cfg2 = cfg.clone();
    let local = tauri::async_runtime::spawn_blocking(move || {
        core::build_local(&cfg2, &cache_dir, &|m| {
            let _ = app2.emit("refresh-progress", m);
        })
    })
    .await
    .map_err(|e| e.to_string())?;

    let snap = if include_remote && cfg.accounts.iter().any(|a| a.enabled) {
        let _ = app.emit("refresh-progress", "リモートの一覧を取得しています".to_string());
        let (repos, errors) = core::fetch_remotes(&cfg).await;
        let mut s = local;
        s.remote_repos = repos;
        s.errors.extend(errors);
        s.remote_fetched_at = Some(chrono::Local::now().to_rfc3339());
        s
    } else {
        core::carry_remote(local, core::load_snapshot(&snapshot_path).as_ref())
    };
    core::save_snapshot(&snapshot_path, &snap)?;
    Ok(snap)
}

/// url を dest にクローンする。dest は存在しないフォルダ。
#[tauri::command]
async fn clone_repo(url: String, dest: String) -> Result<String, String> {
    let dest_path = PathBuf::from(&dest);
    tauri::async_runtime::spawn_blocking(move || core::git::clone(&url, &dest_path))
        .await
        .map_err(|e| e.to_string())??;
    Ok(util::display_path(&dest))
}

/// target: "vscode" / "terminal" / "explorer"
#[tauri::command]
fn open_in(target: String, path: String) -> Result<(), String> {
    let p = PathBuf::from(util::display_path(&path));
    if !p.exists() {
        return Err(format!("{} がありません", p.display()));
    }
    match target.as_str() {
        "vscode" => {
            let exe = find_vscode().ok_or("VS Code が見つかりません")?;
            let mut cmd = Command::new(exe);
            cmd.arg(&p);
            util::hide_window(&mut cmd);
            cmd.spawn().map_err(|e| e.to_string())?;
        }
        "terminal" => open_terminal(&p)?,
        "explorer" => {
            Command::new("explorer.exe").arg(&p).spawn().map_err(|e| e.to_string())?;
        }
        t => return Err(format!("未対応の開き方です: {t}")),
    }
    Ok(())
}

/// git の所有者チェック (dubious ownership) で読めないリポジトリを、
/// ユーザーのグローバル設定の safe.directory に追加する。
#[tauri::command]
fn trust_repo(path: String) -> Result<(), String> {
    let p = util::display_path(&path).replace('\\', "/");
    let mut cmd = Command::new("git");
    cmd.args(["config", "--global", "--add", "safe.directory", &p]);
    util::hide_window(&mut cmd);
    let out = cmd.output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(())
}

#[tauri::command]
fn write_text_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, content).map_err(|e| e.to_string())
}

fn find_vscode() -> Option<PathBuf> {
    // PATH の ...\Microsoft VS Code\bin\code.cmd から Code.exe を引く
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            if dir.join("code.cmd").is_file() {
                if let Some(exe) = dir.parent().map(|p| p.join("Code.exe")).filter(|e| e.is_file()) {
                    return Some(exe);
                }
            }
        }
    }
    let mut cands = vec![];
    if let Some(l) = std::env::var_os("LOCALAPPDATA") {
        cands.push(PathBuf::from(l).join("Programs").join("Microsoft VS Code").join("Code.exe"));
    }
    if let Some(p) = std::env::var_os("ProgramFiles") {
        cands.push(PathBuf::from(p).join("Microsoft VS Code").join("Code.exe"));
    }
    cands.into_iter().find(|c| c.is_file())
}

fn open_terminal(dir: &Path) -> Result<(), String> {
    // Windows Terminal があればそれを使う
    let mut wt = Command::new("wt.exe");
    wt.arg("-d").arg(dir);
    if wt.spawn().is_ok() {
        return Ok(());
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        Command::new("powershell.exe")
            .arg("-NoExit")
            .current_dir(dir)
            .creation_flags(CREATE_NEW_CONSOLE)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(windows))]
    Err("端末を開けません".into())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let config_path = app.path().app_config_dir()?.join("config.json");
            let cache_dir = app.path().app_cache_dir()?;
            app.manage(AppState {
                config: Mutex::new(Config::load(&config_path)),
                config_path,
                cache_dir,
                refreshing: AtomicBool::new(false),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            load_snapshot,
            refresh,
            clone_repo,
            open_in,
            trust_repo,
            write_text_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
