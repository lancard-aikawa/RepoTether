use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// 子プロセスのコンソール窓を出さない (CREATE_NO_WINDOW)。
/// git を何十回も呼ぶので、これが無いと黒い窓が点滅する。
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn hide_window(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// `git -C <dir> <args>` を実行して標準出力を返す。失敗時は標準エラーを返す。
pub fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(dir)
        .args(["-c", "core.quotepath=false", "-c", "color.ui=never"])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0");
    hide_window(&mut cmd);
    let out = cmd
        .output()
        .map_err(|e| format!("git を起動できません: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() {
            format!("git {} が失敗しました", args.join(" "))
        } else {
            err
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// 表示用のパス。区切りをバックスラッシュに、ドライブ文字を大文字にそろえる。
pub fn display_path(p: &str) -> String {
    let mut s: String = p.replace('/', "\\");
    while s.len() > 3 && s.ends_with('\\') {
        s.pop();
    }
    let mut chars: Vec<char> = s.chars().collect();
    if chars.len() >= 2 && chars[1] == ':' {
        chars[0] = chars[0].to_ascii_uppercase();
    }
    chars.into_iter().collect()
}

/// 照合用のキー。Windows のパスは大文字小文字を区別しないので小文字にする。
/// Claude のログには `c:\Repos\...` と `C:\Repos\...` が混ざっている。
pub fn path_key(p: &str) -> String {
    display_path(p).to_lowercase()
}

pub fn system_time_to_rfc3339(t: SystemTime) -> String {
    chrono::DateTime::<chrono::Local>::from(t).to_rfc3339()
}

pub fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).ok()?.modified().ok()
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

/// 文字数 (バイト数ではない) で切り詰める。
pub fn truncate_chars(s: &str, max: usize) -> String {
    let mut out: String = s.chars().take(max).collect();
    if s.chars().count() > max {
        out.push('…');
    }
    out
}

/// リモート URL を `host/owner/name` (小文字) にする。
/// https / ssh / scp 形式に対応。ポートとサブパスは捨て、末尾 2 階層を owner/name とみなす
/// (Gogs をサブパスに置いた https と、ルート直下の ssh を同じキーにするため)。
pub fn remote_key(url: &str) -> Option<String> {
    let url = url.trim();
    let (host, path) = if let Some(rest) = url.split_once("://").map(|(_, r)| r) {
        // scheme://[user@]host[:port]/path
        let (authority, path) = rest.split_once('/')?;
        let host = authority.rsplit('@').next()?;
        let host = host.split(':').next()?;
        (host.to_string(), path.to_string())
    } else if let Some((left, path)) = url.split_once(':') {
        // scp 形式: [user@]host:owner/name.git (C:\ のようなドライブ文字は除外)
        if left.len() <= 1 || left.contains('\\') || left.contains('/') {
            return None;
        }
        let host = left.rsplit('@').next()?;
        (host.to_string(), path.to_string())
    } else {
        return None;
    };
    if host.is_empty() {
        return None;
    }
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segs.len() < 2 {
        return None;
    }
    let owner = segs[segs.len() - 2];
    let name = segs[segs.len() - 1];
    Some(format!("{}/{}/{}", host, owner, name).to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_key_forms() {
        let k = Some("github.com/foo/bar".to_string());
        assert_eq!(remote_key("https://github.com/foo/bar.git"), k);
        assert_eq!(remote_key("https://user@github.com/Foo/Bar"), k);
        assert_eq!(remote_key("git@github.com:foo/bar.git"), k);
        assert_eq!(remote_key("ssh://git@github.com:22/foo/bar.git"), k);
        assert_eq!(
            remote_key("https://git.example.com:3000/gogs/foo/bar.git"),
            Some("git.example.com/foo/bar".to_string())
        );
        assert_eq!(remote_key(r"C:\repos\bar"), None);
        assert_eq!(remote_key("/srv/git/bar.git"), None);
    }

    #[test]
    fn path_normalization() {
        assert_eq!(display_path("c:/Repos/x/"), r"C:\Repos\x");
        assert_eq!(path_key(r"C:\Repos\X"), path_key("c:/repos/x"));
        assert_eq!(display_path(r"C:\"), r"C:\");
    }
}
