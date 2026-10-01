//! SessionVault (Claude Code のセッションのログを残し・検査し・直す CLI) を呼ぶ。
//!
//! RepoTether は読むだけなので、呼ぶのは `verify --json` だけ。ログを書き換える repair / restore は呼ばない。
//! verify は、ログの壊れ (JSON として読めない行、途中で切れた最後の行、親の切れた会話) と、
//! SessionVault の保管庫との食い違い (元のログが縮んだ・書き換わった) を返す。
//! 保管庫の場所は sessionvault.exe 自身の設定 (exe の隣の sessionvault.json) に従う。

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

/// 設定の場所を使う。無ければ PATH から探す
pub fn resolve_exe(configured: Option<&str>) -> Result<PathBuf, String> {
    if let Some(p) = configured.map(str::trim).filter(|p| !p.is_empty()) {
        let p = PathBuf::from(p);
        return if p.is_file() {
            Ok(p)
        } else {
            Err(format!("SessionVault が見つかりません: {}。設定の「SessionVault の場所」を確かめてください", p.display()))
        };
    }
    std::env::var_os("PATH")
        .iter()
        .flat_map(std::env::split_paths)
        .map(|d| d.join(EXE_NAME))
        .find(|p| p.is_file())
        .ok_or_else(|| {
            format!("{EXE_NAME} が見つかりません。設定の「SessionVault の場所」に sessionvault.exe を指定してください")
        })
}

/// `sessionvault --src <projects_dir> verify --json --project <名前>...`。
/// 終了コード 0 (エラーなし) と 1 (エラーあり) はどちらも結果として返し、2 (引数の誤り) などは Err にする
pub fn verify(exe: &Path, projects_dir: &Path, project_dirs: &[String]) -> Result<Vec<SessionFinding>, String> {
    if project_dirs.is_empty() {
        return Ok(vec![]);
    }
    let mut cmd = Command::new(exe);
    cmd.arg("--src").arg(projects_dir).args(["verify", "--json"]);
    for d in project_dirs {
        cmd.arg("--project").arg(d);
    }
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    util::hide_window(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| format!("SessionVault を起動できません ({}): {e}", exe.display()))?;

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

    match status.code() {
        Some(0) | Some(1) => serde_json::from_slice(&stdout)
            .map_err(|e| format!("SessionVault の結果を読めません: {e}")),
        _ => {
            // 最後の行が要約 ("verify: ...") なので、それ以外の最初の行を見せる
            let msg = stderr.lines().find(|l| !l.trim().is_empty()).unwrap_or("SessionVault が失敗しました");
            Err(util::truncate_chars(msg.trim(), 300))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_path_must_exist() {
        let err = resolve_exe(Some(r"C:\nope\sessionvault.exe")).unwrap_err();
        assert!(err.contains("見つかりません"), "{err}");
    }

    #[test]
    fn no_projects_means_no_call() {
        // exe が無くても、検査するものが無ければ呼ばずに空を返す
        assert!(verify(Path::new("no-such-exe"), Path::new("."), &[]).unwrap().is_empty());
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
