//! フォルダを外部アプリで開く。OS ごとに分ける。
//!
//! | 操作 | Windows | macOS |
//! |---|---|---|
//! | VS Code | Code.exe を直接起動 | `open -a "Visual Studio Code"` |
//! | 端末 | 選んだもの。自動なら Windows Terminal (無ければ PowerShell) | 選んだもの。自動なら Terminal |
//! | フォルダ | エクスプローラー | Finder (`open`) |
//! | 資格情報 | 資格情報マネージャー | キーチェーンアクセス |

use std::path::Path;
use std::process::Command;

#[cfg(windows)]
pub use windows::*;

#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(not(any(windows, target_os = "macos")))]
pub use other::*;

/// 選べる端末 (この PC で見つかったもの)
#[derive(serde::Serialize)]
pub struct TerminalChoice {
    pub id: &'static str,
    pub label: &'static str,
}

fn spawn(cmd: &mut Command) -> Result<(), String> {
    clean_env(cmd).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// RepoTether 自体を VS Code (の拡張やターミナル) から起動すると、VS Code 内部用の環境変数を引き継ぐ。
/// 特に ELECTRON_RUN_AS_NODE=1 が残っていると Code.exe が Node として動いて窓が開かないので、
/// 外部アプリには渡さない。
///
/// Claude Code の中から起動した場合も、動いているセッションを指す環境変数 (CLAUDECODE など) を渡さない。
/// そのまま渡すと、起動した claude が「Claude Code の中」だと取り違える。利用者が自分で決める設定
/// (CLAUDE_CONFIG_DIR など) は残す。
fn clean_env(cmd: &mut Command) -> &mut Command {
    const CLAUDE_SESSION_VARS: &[&str] = &[
        "CLAUDECODE",
        "CLAUDE_PID",
        "CLAUDE_EFFORT",
        "CLAUDE_CODE_SESSION_ID",
        "CLAUDE_CODE_CHILD_SESSION",
        "CLAUDE_CODE_ENTRYPOINT",
        "CLAUDE_CODE_MESSAGING_SOCKET",
        "CLAUDE_CODE_SESSION_ATTENDED",
        "CLAUDE_CODE_ENABLE_TASKS",
        "CLAUDE_CODE_ENABLE_SDK_FILE_CHECKPOINTING",
        "CLAUDE_AGENT_SDK_VERSION",
    ];
    for (k, _) in std::env::vars_os() {
        let k = k.to_string_lossy();
        if k.starts_with("ELECTRON_") || k.starts_with("VSCODE_") || CLAUDE_SESSION_VARS.contains(&k.as_ref()) {
            cmd.env_remove(k.as_ref());
        }
    }
    cmd
}

/// 端末に渡してよい引数か (英数字・ハイフンだけ)。シェルを通すので、それ以外は通さない
fn safe_args(args: &[&str]) -> Result<String, String> {
    for a in args {
        if a.is_empty() || !a.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(format!("端末に渡せない引数です: {a}"));
        }
    }
    Ok(args.join(" "))
}

#[cfg(windows)]
mod windows {
    use super::*;
    use crate::core::util;
    use std::os::windows::process::CommandExt;
    use std::path::PathBuf;

    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

    pub fn vscode(dir: &Path) -> Result<(), String> {
        let exe = find_vscode().ok_or("VS Code が見つかりません")?;
        let mut cmd = Command::new(exe);
        cmd.arg(dir);
        util::hide_window(&mut cmd);
        spawn(&mut cmd)
    }

    /// 新しいコンソール窓で開く (PowerShell / cmd / WSL)
    fn console(exe: &str, args: &[&str], dir: &Path) -> Result<(), String> {
        spawn(Command::new(exe).args(args).current_dir(dir).creation_flags(CREATE_NEW_CONSOLE))
    }

    fn on_path(exe: &str) -> bool {
        std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).any(|d| d.join(exe).is_file()))
            .unwrap_or(false)
    }

    fn git_bash() -> Option<PathBuf> {
        let mut cands = vec![];
        for v in ["ProgramFiles", "ProgramW6432"] {
            if let Some(p) = std::env::var_os(v) {
                cands.push(PathBuf::from(p).join("Git").join("git-bash.exe"));
            }
        }
        if let Some(l) = std::env::var_os("LOCALAPPDATA") {
            cands.push(PathBuf::from(l).join("Programs").join("Git").join("git-bash.exe"));
        }
        cands.into_iter().find(|c| c.is_file())
    }

    pub fn terminals() -> Vec<TerminalChoice> {
        let mut out = vec![];
        if on_path("wt.exe") {
            out.push(TerminalChoice { id: "wt", label: "Windows Terminal" });
        }
        if on_path("pwsh.exe") {
            out.push(TerminalChoice { id: "pwsh", label: "PowerShell 7" });
        }
        out.push(TerminalChoice { id: "powershell", label: "Windows PowerShell" });
        out.push(TerminalChoice { id: "cmd", label: "コマンドプロンプト" });
        if git_bash().is_some() {
            out.push(TerminalChoice { id: "gitbash", label: "Git Bash" });
        }
        if on_path("wsl.exe") {
            out.push(TerminalChoice { id: "wsl", label: "WSL" });
        }
        out
    }

    /// kind が空なら自動 (Windows Terminal、無ければ PowerShell)
    pub fn terminal(dir: &Path, kind: &str) -> Result<(), String> {
        match kind {
            "" => {
                if clean_env(Command::new("wt.exe").arg("-d").arg(dir)).spawn().is_ok() {
                    return Ok(());
                }
                console("powershell.exe", &["-NoExit"], dir)
            }
            "wt" => spawn(Command::new("wt.exe").arg("-d").arg(dir)),
            "pwsh" => console("pwsh.exe", &["-NoExit"], dir),
            "powershell" => console("powershell.exe", &["-NoExit"], dir),
            "cmd" => console("cmd.exe", &["/K"], dir),
            "gitbash" => {
                let exe = git_bash().ok_or("Git Bash が見つかりません")?;
                let mut cmd = Command::new(exe);
                cmd.arg(format!("--cd={}", dir.display()));
                spawn(&mut cmd)
            }
            "wsl" => {
                let d = dir.to_string_lossy().into_owned();
                console("wsl.exe", &["--cd", &d], dir)
            }
            k => Err(format!("未対応の端末です: {k}")),
        }
    }

    /// 選んだ端末で dir を開き、その中でコマンドを動かす (終わっても窓は残す)。
    /// program と args は safe_args を通った英数字・ハイフンだけ
    pub fn terminal_run(dir: &Path, kind: &str, program: &str, args: &[&str]) -> Result<(), String> {
        let line = safe_args(&[&[program][..], args].concat())?;
        match kind {
            "" | "wt" => {
                let mut wt = Command::new("wt.exe");
                wt.arg("-d").arg(dir).args(["cmd", "/K", &line]);
                if clean_env(&mut wt).spawn().is_ok() {
                    return Ok(());
                }
                if kind == "wt" {
                    return Err("Windows Terminal を起動できません".into());
                }
                console("cmd.exe", &["/K", &line], dir)
            }
            "pwsh" => console("pwsh.exe", &["-NoExit", "-Command", &line], dir),
            "powershell" => console("powershell.exe", &["-NoExit", "-Command", &line], dir),
            "cmd" => console("cmd.exe", &["/K", &line], dir),
            "gitbash" => {
                let bash = git_bash()
                    .and_then(|g| g.parent().map(|p| p.join("bin").join("bash.exe")))
                    .filter(|b| b.is_file())
                    .ok_or("Git Bash が見つかりません")?;
                let script = format!("{line}; exec bash -i");
                spawn(
                    Command::new(bash)
                        .args(["--login", "-c", &script])
                        .current_dir(dir)
                        // /etc/profile がホームへ移動しないように
                        .env("CHERE_INVOKING", "1")
                        .creation_flags(CREATE_NEW_CONSOLE),
                )
            }
            // WSL の中の claude は Windows 側のセッションを知らないので、コマンドプロンプトで動かす
            "wsl" => console("cmd.exe", &["/K", &line], dir),
            k => Err(format!("未対応の端末です: {k}")),
        }
    }

    pub fn folder(dir: &Path) -> Result<(), String> {
        spawn(Command::new("explorer.exe").arg(dir))
    }

    pub fn secret_store() -> Result<(), String> {
        spawn(Command::new("control.exe").args(["/name", "Microsoft.CredentialManager"]))
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
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;

    /// `open -a <アプリ> <パス>`。アプリが無ければ open が失敗を返す
    fn open_with(app: &str, dir: &Path) -> Result<(), String> {
        let out = clean_env(Command::new("open").arg("-a").arg(app).arg(dir)).output().map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(format!("{app} を開けません: {}", String::from_utf8_lossy(&out.stderr).trim()));
        }
        Ok(())
    }

    pub fn vscode(dir: &Path) -> Result<(), String> {
        open_with("Visual Studio Code", dir)
    }

    pub fn terminals() -> Vec<TerminalChoice> {
        let mut out = vec![TerminalChoice { id: "terminal", label: "ターミナル" }];
        if Path::new("/Applications/iTerm.app").exists() {
            out.push(TerminalChoice { id: "iterm", label: "iTerm" });
        }
        out
    }

    pub fn terminal(dir: &Path, kind: &str) -> Result<(), String> {
        match kind {
            "" | "terminal" => open_with("Terminal", dir),
            "iterm" => open_with("iTerm", dir),
            k => Err(format!("未対応の端末です: {k}")),
        }
    }

    /// ターミナルの新しいウィンドウで dir に移動してコマンドを動かす (iTerm を選んでいてもターミナルで)
    pub fn terminal_run(dir: &Path, _kind: &str, program: &str, args: &[&str]) -> Result<(), String> {
        let line = safe_args(&[&[program][..], args].concat())?;
        let out = clean_env(
            Command::new("osascript")
                .args(["-e", "on run argv"])
                .args(["-e", "tell application \"Terminal\" to do script \"cd \" & quoted form of item 1 of argv & \" && \" & item 2 of argv"])
                .args(["-e", "tell application \"Terminal\" to activate"])
                .args(["-e", "end run"])
                .arg(dir)
                .arg(&line),
        )
        .output()
        .map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(format!("ターミナルを開けません: {}", String::from_utf8_lossy(&out.stderr).trim()));
        }
        Ok(())
    }

    pub fn folder(dir: &Path) -> Result<(), String> {
        spawn(Command::new("open").arg(dir))
    }

    pub fn secret_store() -> Result<(), String> {
        spawn(Command::new("open").args(["-a", "Keychain Access"]))
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod other {
    use super::*;

    pub fn vscode(dir: &Path) -> Result<(), String> {
        spawn(Command::new("code").arg(dir))
    }

    pub fn terminals() -> Vec<TerminalChoice> {
        vec![]
    }

    pub fn terminal(_dir: &Path, _kind: &str) -> Result<(), String> {
        Err("この OS では端末を開けません".into())
    }

    pub fn terminal_run(_dir: &Path, _kind: &str, _program: &str, _args: &[&str]) -> Result<(), String> {
        Err("この OS では端末を開けません".into())
    }

    pub fn folder(dir: &Path) -> Result<(), String> {
        spawn(Command::new("xdg-open").arg(dir))
    }

    pub fn secret_store() -> Result<(), String> {
        Err("この OS では資格情報の保管庫を開けません".into())
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn vscode_internal_env_is_not_passed_on() {
        // VS Code の拡張から起動された状態をまねる
        std::env::set_var("ELECTRON_RUN_AS_NODE", "1");
        std::env::set_var("VSCODE_PID", "123");
        let out = clean_env(Command::new("cmd").args(["/c", "set"])).output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(!text.contains("ELECTRON_RUN_AS_NODE"));
        assert!(!text.contains("VSCODE_PID"));
        // ほかの環境変数は残す
        assert!(text.to_uppercase().contains("PATH="));
    }
}

#[cfg(all(test, windows))]
mod terminal_tests {
    use super::*;

    #[test]
    fn detects_terminals_and_rejects_unknown_kind() {
        let ids: Vec<&str> = terminals().iter().map(|t| t.id).collect();
        println!("見つかった端末: {ids:?}");
        // どの Windows にもあるもの
        assert!(ids.contains(&"powershell") && ids.contains(&"cmd"));
        // 決まった種類以外は開かない
        assert!(terminal(std::path::Path::new("C:\\"), "calc").is_err());
    }
}

#[cfg(test)]
mod safe_args_tests {
    use super::*;

    #[test]
    fn only_plain_tokens_reach_the_shell() {
        assert_eq!(safe_args(&["claude", "-r", "4100683c-53d0", "--fork-session"]).unwrap(), "claude -r 4100683c-53d0 --fork-session");
        assert!(safe_args(&["claude", "&", "calc"]).is_err());
        assert!(safe_args(&["claude", "a;b"]).is_err());
        assert!(safe_args(&["claude", ""]).is_err());
    }
}
