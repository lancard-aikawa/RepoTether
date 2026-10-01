//! SessionVault (Claude Code のセッションのログを残し・検査し・直す CLI) を呼ぶ。
//!
//! RepoTether は読むだけなので、呼ぶのは `verify --json` だけ。ログを書き換える repair / restore は呼ばない。
//! verify は、ログの壊れ (JSON として読めない行、途中で切れた最後の行、親の切れた会話) と、
//! SessionVault の保管庫との食い違い (元のログが縮んだ・書き換わった) を返す。
//!
//! 呼び方は 2 通り。
//! - sessionvault.exe: 保管庫の場所は exe 自身の設定 (exe の隣の sessionvault.json) に従う
//! - フォルダ (Claude History Viewer のフォルダか、SessionVault のリポジトリ): その中の SessionVault を
//!   `python -m sessionvault` で呼ぶ。Viewer なら、Viewer の settings.json (sessionvault_src / sessionvault_config) に
//!   合わせて、Viewer と同じ保管庫を見る

use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::model::SessionFinding;
use super::util;

/// 全部のログを読むと 10 秒近くかかる。それを大きく超えたら止まっているとみなす
const VERIFY_TIMEOUT: Duration = Duration::from_secs(120);

#[cfg(windows)]
const EXE_NAME: &str = "sessionvault.exe";
#[cfg(not(windows))]
const EXE_NAME: &str = "sessionvault";

/// SessionVault の呼び方
#[derive(Debug, PartialEq)]
pub enum Runner {
    Exe(PathBuf),
    Python {
        /// python 本体と、その直後に付ける引数 (py ランチャーなら "-3")
        python: PathBuf,
        python_args: Vec<String>,
        /// PYTHONPATH に入れる SessionVault の src
        src: PathBuf,
        /// サブコマンドより前に付ける引数 (--config / --vault)
        global_args: Vec<OsString>,
    },
}

/// 設定の場所を使う。空なら PATH の sessionvault.exe を探す
pub fn resolve(configured: Option<&str>) -> Result<Runner, String> {
    if let Some(p) = configured.map(str::trim).filter(|p| !p.is_empty()) {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Ok(Runner::Exe(p));
        }
        if p.is_dir() {
            let (src, global_args) = python_source(&p)?;
            let (python, python_args) = find_python()?;
            return Ok(Runner::Python { python, python_args, src, global_args });
        }
        return Err(format!("SessionVault が見つかりません: {}。設定の「SessionVault の場所」を確かめてください", p.display()));
    }
    on_path(EXE_NAME).map(Runner::Exe).ok_or_else(|| {
        format!("{EXE_NAME} が見つかりません。設定の「SessionVault の場所」に sessionvault.exe か Claude History Viewer のフォルダを指定してください")
    })
}

fn on_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")
        .iter()
        .flat_map(std::env::split_paths)
        .map(|d| d.join(name))
        // Microsoft Store へ誘導するだけの python.exe (WindowsApps の中) は使えない
        .find(|p| p.is_file() && !p.to_string_lossy().contains("WindowsApps"))
}

/// py ランチャーを優先し、無ければ python
fn find_python() -> Result<(PathBuf, Vec<String>), String> {
    if cfg!(windows) {
        if let Some(py) = on_path("py.exe") {
            return Ok((py, vec!["-3".into()]));
        }
        if let Some(p) = on_path("python.exe") {
            return Ok((p, vec![]));
        }
    } else if let Some(p) = on_path("python3") {
        return Ok((p, vec![]));
    }
    Err("フォルダの SessionVault を動かす Python (3.10 以上) が見つかりません。Python を入れるか、sessionvault.exe を指定してください".into())
}

fn has_package(src: &Path) -> bool {
    src.join("sessionvault").join("__init__.py").is_file()
}

/// フォルダから、SessionVault の src と、付ける引数を決める。
/// Viewer のフォルダなら、Viewer の settings.json に合わせる (Viewer の claudehistory/archive.py と同じ規則)
fn python_source(dir: &Path) -> Result<(PathBuf, Vec<OsString>), String> {
    let is_viewer = dir.join("claude_chat_viewer.py").is_file();
    if !is_viewer {
        let src = dir.join("src");
        return if has_package(&src) {
            Ok((src, vec![]))
        } else {
            Err(format!(
                "{} は Claude History Viewer のフォルダでも SessionVault のリポジトリでもありません",
                dir.display()
            ))
        };
    }
    let settings: serde_json::Value = std::fs::read_to_string(dir.join("settings.json"))
        .ok()
        .and_then(|t| serde_json::from_str(t.trim_start_matches('\u{feff}')).ok())
        .unwrap_or_default();
    let text = |k: &str| settings.get(k).and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty());

    // sessionvault_src が空なら、Viewer に同梱したサブモジュール
    let src = match text("sessionvault_src") {
        Some(s) => PathBuf::from(s),
        None => dir.join("vendor").join("SessionVault").join("src"),
    };
    if !has_package(&src) {
        return Err(format!(
            "Viewer の SessionVault がありません: {}。Viewer のフォルダで `git submodule update --init` を実行してください",
            src.display()
        ));
    }
    // sessionvault_config が空なら SessionVault の既定 (src の上の sessionvault.json) なので、何も付けない。
    // 指定した設定に vault が無ければ、その設定ファイルの隣の vault/ (Viewer の決まり。SessionVault の CLI の既定とは違う)
    let mut args: Vec<OsString> = vec![];
    if let Some(cfg) = text("sessionvault_config") {
        let cfg = PathBuf::from(cfg);
        let has_vault = std::fs::read_to_string(&cfg)
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(t.trim_start_matches('\u{feff}')).ok())
            .and_then(|v| v.get("vault").and_then(|x| x.as_str()).map(|s| !s.trim().is_empty()))
            .unwrap_or(false);
        args.push("--config".into());
        args.push(cfg.clone().into());
        if !has_vault {
            args.push("--vault".into());
            args.push(cfg.parent().unwrap_or(Path::new(".")).join("vault").into());
        }
    }
    Ok((src, args))
}

fn command(runner: &Runner) -> Command {
    match runner {
        Runner::Exe(exe) => Command::new(exe),
        Runner::Python { python, python_args, src, global_args } => {
            let mut cmd = Command::new(python);
            cmd.args(python_args)
                .args(["-m", "sessionvault"])
                .args(global_args)
                .env("PYTHONPATH", src)
                // 読むだけなので、Viewer のフォルダに __pycache__ を作らない
                .env("PYTHONDONTWRITEBYTECODE", "1")
                .env("PYTHONIOENCODING", "utf-8");
            cmd
        }
    }
}

/// `sessionvault --src <projects_dir> verify --json --project <名前>...`。
/// 終了コード 0 (エラーなし) と 1 (エラーあり) はどちらも結果として返し、2 (引数の誤り) などは Err にする
pub fn verify(runner: &Runner, projects_dir: &Path, project_dirs: &[String]) -> Result<Vec<SessionFinding>, String> {
    if project_dirs.is_empty() {
        return Ok(vec![]);
    }
    let mut cmd = command(runner);
    cmd.arg("--src").arg(projects_dir).args(["verify", "--json"]);
    for d in project_dirs {
        cmd.arg("--project").arg(d);
    }
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    util::hide_window(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| format!("SessionVault を起動できません: {e}"))?;

    // 出力が多いとパイプが詰まって子が止まるので、終わるのを待ちながら別のスレッドで読む
    let mut out = child.stdout.take().ok_or("SessionVault の出力を読めません")?;
    let mut err = child.stderr.take().ok_or("SessionVault の出力を読めません")?;
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
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() > VERIFY_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("SessionVault が {} 秒で終わらないので打ち切りました", VERIFY_TIMEOUT.as_secs()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(e.to_string()),
        }
    };
    let stdout = out_reader.join().unwrap_or_default();
    // パイプに出すときは UTF-8 (SessionVault 側でそうしている)
    let stderr = String::from_utf8_lossy(&err_reader.join().unwrap_or_default()).trim().to_string();

    let parsed = matches!(status.code(), Some(0) | Some(1))
        .then(|| serde_json::from_slice::<Vec<SessionFinding>>(&stdout).ok())
        .flatten();
    parsed.ok_or_else(|| {
        // SessionVault のエラーは 1 行目、Python の例外 (古い Python など) は最後の行が要点。
        // "verify: ..." の要約は除く
        let lines: Vec<&str> = stderr.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with("verify:")).collect();
        let msg = if stderr.contains("Traceback") { lines.last() } else { lines.first() };
        util::truncate_chars(msg.copied().unwrap_or("SessionVault が失敗しました"), 300)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("repotether-sv-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn package(src: &Path) {
        std::fs::create_dir_all(src.join("sessionvault")).unwrap();
        std::fs::write(src.join("sessionvault").join("__init__.py"), "").unwrap();
    }

    #[test]
    fn configured_path_must_exist() {
        let err = resolve(Some(r"C:\nope\sessionvault.exe")).unwrap_err();
        assert!(err.contains("見つかりません"), "{err}");
    }

    #[test]
    fn viewer_folder_uses_vendored_submodule_by_default() {
        let d = tmp("viewer");
        std::fs::write(d.join("claude_chat_viewer.py"), "").unwrap();
        package(&d.join("vendor/SessionVault/src"));
        let (src, args) = python_source(&d).unwrap();
        assert_eq!(src, d.join("vendor").join("SessionVault").join("src"));
        assert!(args.is_empty());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn viewer_settings_choose_src_and_config() {
        let d = tmp("viewer-settings");
        std::fs::write(d.join("claude_chat_viewer.py"), "").unwrap();
        let own = d.join("own");
        package(&own.join("src"));
        // vault を書いていない設定 → その隣の vault/
        std::fs::write(own.join("sessionvault.json"), r#"{"vault": null}"#).unwrap();
        let settings = serde_json::json!({
            "sessionvault_src": own.join("src"),
            "sessionvault_config": own.join("sessionvault.json"),
        });
        std::fs::write(d.join("settings.json"), settings.to_string()).unwrap();
        let (src, args) = python_source(&d).unwrap();
        assert_eq!(src, own.join("src"));
        assert_eq!(
            args,
            vec![
                OsString::from("--config"),
                own.join("sessionvault.json").into(),
                OsString::from("--vault"),
                own.join("vault").into()
            ]
        );
        // vault を書いた設定なら --vault は付けない
        std::fs::write(own.join("sessionvault.json"), r#"{"vault": "D:/v"}"#).unwrap();
        assert_eq!(python_source(&d).unwrap().1.len(), 2);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn viewer_without_submodule_is_explained() {
        let d = tmp("viewer-empty");
        std::fs::write(d.join("claude_chat_viewer.py"), "").unwrap();
        let err = python_source(&d).unwrap_err();
        assert!(err.contains("git submodule update --init"), "{err}");
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn sessionvault_repo_folder() {
        let d = tmp("repo");
        package(&d.join("src"));
        assert_eq!(python_source(&d).unwrap(), (d.join("src"), vec![]));
        let other = tmp("other");
        assert!(python_source(&other).is_err());
        let _ = std::fs::remove_dir_all(d);
        let _ = std::fs::remove_dir_all(other);
    }

    #[test]
    fn no_projects_means_no_call() {
        // exe が無くても、検査するものが無ければ呼ばずに空を返す
        let r = Runner::Exe(PathBuf::from("no-such-exe"));
        assert!(verify(&r, Path::new("."), &[]).unwrap().is_empty());
    }

    #[test]
    fn parses_findings() {
        let json = r#"[{"session": "s1", "project": "C--x", "path": "s1.jsonl", "check": "bad-json",
                        "severity": "error", "line": 3, "detail": "Expecting value"}]"#;
        let f: Vec<SessionFinding> = serde_json::from_str(json).unwrap();
        assert_eq!(f[0].check, "bad-json");
        assert_eq!(f[0].line, Some(3));
    }
}
