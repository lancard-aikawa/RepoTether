pub mod core;
mod launch;

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

/// トークンは返さない (資格情報マネージャーにあるかどうかだけ)
#[tauri::command]
fn get_config(state: State<AppState>) -> Config {
    state.config.lock().unwrap().for_view()
}

/// 新しいトークンや削除の指示があれば資格情報マネージャーに反映してから、トークン抜きで保存する
#[tauri::command]
fn save_config(state: State<AppState>, mut config: Config) -> Result<Config, String> {
    let prev = state.config.lock().unwrap().clone();
    config.store_secrets(&prev)?;
    config.save(&state.config_path)?;
    *state.config.lock().unwrap() = config.clone();
    Ok(config.for_view())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Readme {
    name: String,
    content: String,
    markdown: bool,
    truncated: bool,
}

/// リポジトリ直下の README を読む。フォルダ直下の README という名前のファイルしか読まない。
/// 日本語版 (README.ja.md など) があればそちらを先に使う。無ければ None
#[tauri::command]
fn read_readme(path: String) -> Result<Option<Readme>, String> {
    const MAX: usize = 1024 * 1024;
    const ORDER: &[&str] = &[
        "readme.ja.md",
        "readme_ja.md",
        "readme.md",
        "readme.markdown",
        "readme.rst",
        "readme.txt",
        "readme",
    ];
    let dir = Path::new(&path);
    if !dir.is_absolute() || !dir.is_dir() {
        return Err(format!("フォルダではありません: {path}"));
    }
    let files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    for want in ORDER {
        let Some(f) = files
            .iter()
            .find(|f| f.file_name().is_some_and(|n| n.to_string_lossy().to_lowercase() == *want))
        else {
            continue;
        };
        let bytes = std::fs::read(f).map_err(|e| e.to_string())?;
        let truncated = bytes.len() > MAX;
        let content = String::from_utf8_lossy(&bytes[..bytes.len().min(MAX)]).into_owned();
        return Ok(Some(Readme {
            name: f.file_name().unwrap().to_string_lossy().into_owned(),
            content,
            markdown: want.ends_with(".md") || want.ends_with(".markdown"),
            truncated,
        }));
    }
    Ok(None)
}

/// OS の資格情報の保管庫 (Windows: 資格情報マネージャー / macOS: キーチェーンアクセス) を開く
#[tauri::command]
fn open_credential_manager() -> Result<(), String> {
    launch::secret_store()
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
    // 絶対パスだけ受け付ける (先頭が "-" の値をオプションとして解釈させない)
    if !p.is_absolute() {
        return Err(format!("絶対パスではありません: {path}"));
    }
    if !p.exists() {
        return Err(format!("{} がありません", p.display()));
    }
    match target.as_str() {
        "vscode" => launch::vscode(&p)?,
        "terminal" => launch::terminal(&p)?,
        "explorer" => launch::folder(&p)?,
        t => return Err(format!("未対応の開き方です: {t}")),
    }
    Ok(())
}

/// git の所有者チェック (dubious ownership) で読めないリポジトリを、
/// ユーザーのグローバル設定の safe.directory に追加する。
#[tauri::command]
fn trust_repo(path: String) -> Result<(), String> {
    if !Path::new(&path).is_absolute() {
        return Err(format!("絶対パスではありません: {path}"));
    }
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

/// 保存ダイアログを出し、ユーザーが選んだ場所にだけ書く。
/// 画面側から任意のパスに書けないよう、パスは受け取らない。キャンセルなら None。
#[tauri::command]
async fn save_text_with_dialog(
    app: AppHandle,
    default_name: String,
    content: String,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let picked = app
        .dialog()
        .file()
        .set_file_name(&default_name)
        .add_filter("Markdown", &["md"])
        .add_filter("テキスト", &["txt"])
        .blocking_save_file();
    let Some(fp) = picked else { return Ok(None) };
    let path = fp.into_path().map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let config_path = app.path().app_config_dir()?.join("config.json");
            let cache_dir = app.path().app_cache_dir()?;
            let mut config = Config::load(&config_path);
            // 以前の版の平文トークンを資格情報マネージャーへ移し、設定ファイルから消す
            match config.migrate_plaintext_tokens() {
                Ok(true) => {
                    if let Err(e) = config.save(&config_path) {
                        eprintln!("トークン移行後の保存に失敗しました: {e}");
                    }
                }
                Ok(false) => {}
                Err(e) => eprintln!("トークンを資格情報マネージャーへ移せませんでした: {e}"),
            }
            app.manage(AppState {
                config: Mutex::new(config),
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
            save_text_with_dialog,
            open_credential_manager,
            read_readme
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_this_repos_readme_only_by_name() {
        // src-tauri の親 = RepoTether のリポジトリ直下
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let r = read_readme(root.to_string_lossy().into_owned()).unwrap().expect("README.md があるはず");
        assert_eq!(r.name, "README.md");
        assert!(r.markdown);
        assert!(r.content.starts_with("# RepoTether"));

        // README の無いフォルダは None、相対パスは拒否
        assert!(read_readme(root.join("src-tauri").join("src").to_string_lossy().into_owned())
            .unwrap()
            .is_none());
        assert!(read_readme("relative/path".into()).is_err());
    }
}
