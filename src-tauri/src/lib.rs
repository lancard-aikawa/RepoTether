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

/// プロジェクトのアイコン (フォルダの中のアプリのアイコンらしい画像) の中身。無ければ空。
/// 画像の種類は画面の側で中身から見分ける
#[tauri::command]
async fn repo_icon(path: String) -> Result<tauri::ipc::Response, String> {
    let dir = PathBuf::from(&path);
    if !dir.is_absolute() || !dir.is_dir() {
        return Err(format!("フォルダではありません: {path}"));
    }
    // 一覧の行の数だけ同時に呼ばれるので、ファイルを探して読む処理は非同期の実行スレッドを塞がないよう別スレッドで
    let bytes = tauri::async_runtime::spawn_blocking(move || match core::icon::find(&dir) {
        Some(f) => std::fs::read(f).map_err(|e| e.to_string()),
        None => Ok(vec![]),
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(tauri::ipc::Response::new(bytes))
}

/// 詳細パネルの「次の 10 件」: 期間に関係なく、skip 件目から limit 件のコミット。
/// mine_only なら設定の「自分のメールアドレス」のものだけ
#[tauri::command]
async fn repo_log(
    state: State<'_, AppState>,
    path: String,
    skip: u32,
    limit: u32,
    mine_only: bool,
) -> Result<Vec<core::model::Commit>, String> {
    let p = PathBuf::from(&path);
    if !p.is_absolute() || !core::discover::is_repo(&p) {
        return Err(format!("git のリポジトリではありません: {path}"));
    }
    let authors = if mine_only { state.config.lock().unwrap().author_emails.clone() } else { vec![] };
    let id = util::path_key(&path);
    tauri::async_runtime::spawn_blocking(move || core::git::log_page(&p, &id, skip, limit, &authors))
        .await
        .map_err(|e| e.to_string())?
}

fn existing_dir(path: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(util::display_path(path));
    if !p.is_absolute() || !p.is_dir() {
        return Err(format!("フォルダがありません: {path}"));
    }
    Ok(p)
}

/// そのフォルダで claude を起動する (選んだ端末で)
#[tauri::command]
fn claude_open(path: String, terminal: Option<String>) -> Result<(), String> {
    let dir = existing_dir(&path)?;
    launch::terminal_run(&dir, terminal.as_deref().unwrap_or(""), "claude", &[])
}

/// セッションを再開する (`claude -r <ID>`)。fork なら元の会話を残して別の会話として続ける (`--fork-session`)。
/// Claude Code はセッションを始めたフォルダごとに記録するので、cwd はそのセッションのフォルダ
#[tauri::command]
fn claude_resume(cwd: String, session_id: String, fork: bool, terminal: Option<String>) -> Result<(), String> {
    if !core::sessions::is_session_id(&session_id) {
        return Err(format!("セッション ID の形ではありません: {session_id}"));
    }
    let dir = existing_dir(&cwd)?;
    let mut args = vec!["-r", session_id.as_str()];
    if fork {
        args.push("--fork-session");
    }
    launch::terminal_run(&dir, terminal.as_deref().unwrap_or(""), "claude", &args)
}

/// セッションの会話の全文
#[tauri::command]
async fn session_transcript(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<core::model::TranscriptEntry>, String> {
    let dir = state
        .config
        .lock()
        .unwrap()
        .claude_projects_dir()
        .ok_or("Claude のログの場所が分かりません")?;
    tauri::async_runtime::spawn_blocking(move || core::sessions::transcript(&dir, &session_id))
        .await
        .map_err(|e| e.to_string())?
}

/// SessionVault の verify で、セッションのログが壊れていないかを検査する (読むだけ)。
/// project_dirs はログのあるフォルダの名前 (Session.project_dir)。空なら何も検査しない
#[tauri::command]
async fn sessionvault_verify(
    state: State<'_, AppState>,
    project_dirs: Vec<String>,
) -> Result<Vec<core::model::SessionFinding>, String> {
    let (exe, dir) = {
        let c = state.config.lock().unwrap();
        (c.sessionvault_path.clone(), c.claude_projects_dir().ok_or("Claude のログの場所が分かりません")?)
    };
    tauri::async_runtime::spawn_blocking(move || {
        let exe = core::sessionvault::resolve_exe(exe.as_deref())?;
        core::sessionvault::verify(&exe, &dir, &project_dirs)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeRetention {
    /// settings.json の場所
    path: String,
    /// 書かれている保存期間。null なら既定
    days: Option<u32>,
    default_days: u32,
}

/// Claude Code のセッションの保存期間 (~/.claude/settings.json の cleanupPeriodDays)
#[tauri::command]
fn get_claude_retention() -> Result<ClaudeRetention, String> {
    let path = core::claude_settings::settings_path().ok_or("Claude Code の設定の場所が分かりません")?;
    Ok(ClaudeRetention {
        days: core::claude_settings::cleanup_days(&path)?,
        path: util::display_path(&path.to_string_lossy()),
        default_days: core::claude_settings::DEFAULT_DAYS,
    })
}

/// 保存期間を書く。null なら項目を消して既定に戻す。ほかの項目は変えない
#[tauri::command]
fn set_claude_retention(days: Option<u32>) -> Result<ClaudeRetention, String> {
    let path = core::claude_settings::settings_path().ok_or("Claude Code の設定の場所が分かりません")?;
    core::claude_settings::set_cleanup_days(&path, days)?;
    get_claude_retention()
}

/// この PC で選べる端末
#[tauri::command]
fn list_terminals() -> Vec<launch::TerminalChoice> {
    launch::terminals()
}

/// GitHub CLI (gh) のログイン状態。ログインしていればアカウント名を返す (トークンは返さない)
#[tauri::command]
async fn check_gh(base_url: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || core::gh::user(&core::gh::host_of(&base_url)))
        .await
        .map_err(|e| e.to_string())?
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
/// scope で git から読み直すリポジトリを絞れる (省略ならすべて)。進み具合は "refresh-progress" イベントで送る。
#[tauri::command]
async fn refresh(
    app: AppHandle,
    include_remote: bool,
    fetch: Option<bool>,
    scope: Option<core::model::Scope>,
) -> Result<Snapshot, String> {
    let state = app.state::<AppState>();
    if state.refreshing.swap(true, Ordering::SeqCst) {
        return Err("更新中です".into());
    }
    let scope = scope.unwrap_or(core::model::Scope::All);
    let result = refresh_inner(&app, include_remote, fetch.unwrap_or(false), scope).await;
    state.refreshing.store(false, Ordering::SeqCst);
    result
}

async fn refresh_inner(
    app: &AppHandle,
    include_remote: bool,
    fetch: bool,
    scope: core::model::Scope,
) -> Result<Snapshot, String> {
    let state = app.state::<AppState>();
    let cfg = state.config.lock().unwrap().clone();
    let cache_dir = state.cache_dir.clone();
    let snapshot_path = state.snapshot_path();
    let app2 = app.clone();
    let cfg2 = cfg.clone();
    let prev_path = snapshot_path.clone();
    // 前回の結果は、読み直さないリポジトリの引き継ぎと、リモート一覧の引き継ぎの両方に使う。
    // 別スレッドで読んで使い、複製せずにそのまま返してもらう
    let (local, prev) = tauri::async_runtime::spawn_blocking(move || {
        let prev = core::load_snapshot(&prev_path);
        let local = core::build_local(&cfg2, &cache_dir, fetch, &scope, prev.as_ref(), &|m| {
            let _ = app2.emit("refresh-progress", m);
        });
        (local, prev)
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
        core::carry_remote(local, prev.as_ref())
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
fn open_in(target: String, path: String, terminal: Option<String>) -> Result<(), String> {
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
        "terminal" => launch::terminal(&p, terminal.as_deref().unwrap_or(""))?,
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
            read_readme,
            repo_icon,
            check_gh,
            list_terminals,
            repo_log,
            claude_open,
            claude_resume,
            session_transcript,
            sessionvault_verify,
            get_claude_retention,
            set_claude_retention
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
