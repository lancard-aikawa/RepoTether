//! フォルダを外部アプリで開く。OS ごとに分ける。
//!
//! | 操作 | Windows | macOS |
//! |---|---|---|
//! | VS Code | Code.exe を直接起動 | `open -a "Visual Studio Code"` |
//! | 端末 | Windows Terminal (無ければ PowerShell) | `open -a Terminal` |
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

fn spawn(cmd: &mut Command) -> Result<(), String> {
    clean_env(cmd).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// RepoTether 自体を VS Code (の拡張やターミナル) から起動すると、VS Code 内部用の環境変数を引き継ぐ。
/// 特に ELECTRON_RUN_AS_NODE=1 が残っていると Code.exe が Node として動いて窓が開かないので、
/// 外部アプリには渡さない。
fn clean_env(cmd: &mut Command) -> &mut Command {
    for (k, _) in std::env::vars_os() {
        let k = k.to_string_lossy();
        if k.starts_with("ELECTRON_") || k.starts_with("VSCODE_") {
            cmd.env_remove(k.as_ref());
        }
    }
    cmd
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

    pub fn terminal(dir: &Path) -> Result<(), String> {
        // Windows Terminal があればそれを使う
        if clean_env(Command::new("wt.exe").arg("-d").arg(dir)).spawn().is_ok() {
            return Ok(());
        }
        spawn(
            Command::new("powershell.exe")
                .arg("-NoExit")
                .current_dir(dir)
                .creation_flags(CREATE_NEW_CONSOLE),
        )
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

    pub fn terminal(dir: &Path) -> Result<(), String> {
        open_with("Terminal", dir)
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

    pub fn terminal(_dir: &Path) -> Result<(), String> {
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
