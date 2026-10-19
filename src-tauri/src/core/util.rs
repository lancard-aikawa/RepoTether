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

/// リポジトリの中で git を動かすときに必ず付ける設定。
/// core.fsmonitor はリポジトリの中の設定 (.git/config) に書けて、書かれたコマンドを git status などが動かす。
/// zip を展開したフォルダのように .git ごと人から来たリポジトリでは他人が書けるので、使わないと決めておく
pub const GIT_SAFE_CONFIG: &[&str] = &["-c", "core.fsmonitor=false"];

/// `git -C <dir> <args>` を実行して標準出力を返す。失敗時は標準エラーを返す。
/// 読むだけの操作なので、リポジトリの中の設定で決めた filter も使わない (repo_filter_overrides)
pub fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(dir)
        .args(GIT_SAFE_CONFIG)
        .args(["-c", "core.quotepath=false", "-c", "color.ui=never"])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0");
    add_config_env(&mut cmd, &repo_filter_overrides(dir));
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

/// 設定を環境変数 (GIT_CONFIG_COUNT / GIT_CONFIG_KEY_n / GIT_CONFIG_VALUE_n) で渡す。
/// `-c 項目=値` と違い、項目名に = があっても区切りを取り違えない (filter の名前は誰でも決められる)。
/// 利用者が自分で渡している分は残し、その後ろに足す
fn add_config_env(cmd: &mut Command, pairs: &[(String, String)]) {
    if pairs.is_empty() {
        return;
    }
    let start: usize = std::env::var("GIT_CONFIG_COUNT").ok().and_then(|n| n.trim().parse().ok()).unwrap_or(0);
    for (i, (k, v)) in pairs.iter().enumerate() {
        cmd.env(format!("GIT_CONFIG_KEY_{}", start + i), k).env(format!("GIT_CONFIG_VALUE_{}", start + i), v);
    }
    cmd.env("GIT_CONFIG_COUNT", (start + pairs.len()).to_string());
}

/// filter のコマンドの項目 (filter.<名前>.<種類>)
const FILTER_COMMANDS: &[&str] = &["clean", "smudge", "process"];

/// `git config --list --show-scope --includes -z` の出力から、打ち消す filter の設定を作る。
/// リポジトリの中の設定 (local / worktree。include.path で持ち込んだものも含む) で決めた filter のうち、
/// 全体の設定 (global / system) に同じコマンドが無いもの。git status は、変わったかもしれないファイルを
/// このコマンドに通して読むので、.git ごと人から来たリポジトリでは他人のコマンドが動く。
/// 全体の設定と同じコマンド (git lfs install で入る filter.lfs など) は自分で入れたものなので、そのまま使う
/// 2 つ目は、リポジトリの中の設定が include.path で別のファイルを持ち込んでいるか
fn filter_overrides_from(listing: &[u8]) -> (Vec<(String, String)>, bool) {
    let text = String::from_utf8_lossy(listing);
    // スコープと「項目名\n値」が NUL で区切られて交互に並ぶ
    let tokens: Vec<&str> = text.split('\0').collect();
    let mut trusted: std::collections::HashSet<(&str, &str)> = Default::default();
    let mut repo: Vec<(&str, &str, &str)> = vec![];
    let mut repo_includes = false;
    for pair in tokens.chunks(2) {
        let [scope, entry] = pair else { continue };
        let (key, value) = entry.split_once('\n').unwrap_or((entry, ""));
        let from_repo = !matches!(*scope, "global" | "system" | "command");
        if from_repo && (key == "include.path" || (key.starts_with("includeif.") && key.ends_with(".path"))) {
            repo_includes = true;
        }
        // 項目名は filter.<名前>.<種類>。名前には . も入りうるので、最後の . で分ける
        let Some((name, kind)) = key.strip_prefix("filter.").and_then(|r| r.rsplit_once('.')) else { continue };
        if !FILTER_COMMANDS.contains(&kind) {
            continue;
        }
        match *scope {
            "global" | "system" => {
                trusted.insert((key, value));
            }
            // 起動のときに渡した設定 (-c や環境変数) は、他人には書けない
            "command" => {}
            _ => repo.push((name, key, value)),
        }
    }
    let names: std::collections::BTreeSet<&str> =
        repo.iter().filter(|(_, k, v)| !trusted.contains(&(*k, *v))).map(|(n, _, _)| *n).collect();
    let overrides = names
        .into_iter()
        .flat_map(|n| {
            // コマンドを空にすると使わない。required だと「filter が失敗した」で止まるので外す
            FILTER_COMMANDS
                .iter()
                .map(move |k| (format!("filter.{n}.{k}"), String::new()))
                .chain([(format!("filter.{n}.required"), "false".to_string())])
        })
        .collect();
    (overrides, repo_includes)
}

/// 打ち消す filter の設定を、リポジトリの設定ファイルが変わらないあいだ覚えておく (git を呼ぶたびに数えない)
type FilterCache = std::collections::HashMap<PathBuf, (Vec<Option<SystemTime>>, Vec<(String, String)>)>;
static FILTER_CACHE: std::sync::Mutex<Option<FilterCache>> = std::sync::Mutex::new(None);

/// dir のリポジトリで打ち消す filter の設定。設定を読むだけの git を 1 回動かす (何も実行しない)
fn repo_filter_overrides(dir: &Path) -> Vec<(String, String)> {
    // 覚えておく目印: リポジトリの設定ファイルと全体の設定ファイルの更新時刻
    let stamp: Vec<Option<SystemTime>> = super::discover::git_dir(dir)
        .map(|g| {
            let common = super::discover::common_dir(&g);
            vec![g.join("config"), g.join("config.worktree"), common.join("config")]
        })
        .unwrap_or_default()
        .into_iter()
        .chain(home_dir().map(|h| h.join(".gitconfig")))
        .map(|p| mtime(&p))
        .collect();
    if let Some((s, o)) = FILTER_CACHE.lock().unwrap().get_or_insert_with(Default::default).get(dir) {
        if *s == stamp {
            return o.clone();
        }
    }
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(dir)
        .args(["config", "--list", "--show-scope", "--includes", "-z"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    hide_window(&mut cmd);
    let Ok(out) = cmd.output() else { return vec![] };
    let (overrides, includes) = filter_overrides_from(&out.stdout);
    // include.path で持ち込んだファイルは、変わっても上の目印に出ないので覚えない (毎回数え直す)
    if !includes {
        FILTER_CACHE.lock().unwrap().get_or_insert_with(Default::default).insert(dir.to_path_buf(), (stamp, overrides.clone()));
    }
    overrides
}

/// 表示用のパス。区切りをバックスラッシュに、ドライブ文字を大文字にそろえる。
#[cfg(windows)]
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

/// 表示用のパス。末尾の区切りだけ落とす。
#[cfg(not(windows))]
pub fn display_path(p: &str) -> String {
    let mut s = p.to_string();
    while s.len() > 1 && s.ends_with('/') {
        s.pop();
    }
    s
}

/// 照合用のキー。Windows と macOS (APFS の既定) はパスの大文字小文字を区別しないので小文字にする。
/// Claude のログには `c:\Repos\...` と `C:\Repos\...` が混ざっている。
pub fn path_key(p: &str) -> String {
    let s = display_path(p);
    if cfg!(any(windows, target_os = "macos")) {
        s.to_lowercase()
    } else {
        s
    }
}

/// path_key の子孫か (同じパスを含む)。"foo" と "foobar" を取り違えないよう区切りで判定する
pub fn is_under(key: &str, parent_key: &str) -> bool {
    key == parent_key
        || key
            .strip_prefix(parent_key)
            .is_some_and(|rest| rest.starts_with(std::path::MAIN_SEPARATOR))
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

    /// `git config --list --show-scope -z` の形 (スコープと「項目名\n値」を NUL で交互に並べる)
    fn listing(entries: &[(&str, &str, &str)]) -> Vec<u8> {
        entries.iter().map(|(s, k, v)| format!("{s}\0{k}\n{v}\0")).collect::<String>().into_bytes()
    }

    /// 打ち消す filter の名前 (required を外す項目は名前ごとに 1 つ)
    fn names(o: &[(String, String)]) -> Vec<&str> {
        o.iter().filter_map(|(k, _)| k.strip_prefix("filter.")?.strip_suffix(".required")).collect()
    }

    #[test]
    fn overrides_only_filters_the_repo_brought_in() {
        let lfs = "git-lfs clean -- %f";
        let (o, inc) = filter_overrides_from(&listing(&[
            ("global", "filter.lfs.clean", lfs),
            ("global", "filter.lfs.required", "true"),
            ("global", "include.path", "~/.gitconfig.local"),
            // 全体の設定と同じコマンドは自分で入れたもの (git lfs install --local など)
            ("local", "filter.lfs.clean", lfs),
            ("local", "core.hookspath", ".husky"),
            // 名前に . や = があっても 1 つの名前として扱う
            ("local", "filter.x=y.clean", "echo ran"),
            ("worktree", "filter.a.b.process", "evil"),
            // 起動のときに自分で渡した設定
            ("command", "filter.c.clean", "mine"),
        ]));
        assert_eq!(names(&o), ["a.b", "x=y"]);
        assert!(o.contains(&("filter.x=y.clean".into(), String::new())));
        assert!(o.contains(&("filter.x=y.smudge".into(), String::new())));
        assert!(o.contains(&("filter.a.b.process".into(), String::new())));
        assert!(o.contains(&("filter.x=y.required".into(), "false".into())));
        // 全体の設定の include は数えない
        assert!(!inc);

        // 同じ名前でもコマンドが違えば打ち消す
        let (o, _) = filter_overrides_from(&listing(&[("global", "filter.lfs.clean", lfs), ("local", "filter.lfs.clean", "evil %f")]));
        assert_eq!(names(&o), ["lfs"]);

        // リポジトリの中の include は数える (持ち込んだファイルが変わっても気付けるよう、覚えないため)
        let (o, inc) = filter_overrides_from(&listing(&[("local", "include.path", "../x.cfg"), ("local", "core.bare", "false")]));
        assert!(o.is_empty() && inc);
        let (_, inc) = filter_overrides_from(&listing(&[("local", "includeif.gitdir:c:/x/.path", "../x.cfg")]));
        assert!(inc);
        assert_eq!(filter_overrides_from(b""), (vec![], false));
    }

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

    #[cfg(windows)]
    #[test]
    fn path_normalization() {
        assert_eq!(display_path("c:/Repos/x/"), r"C:\Repos\x");
        assert_eq!(path_key(r"C:\Repos\X"), path_key("c:/repos/x"));
        assert_eq!(display_path(r"C:\"), r"C:\");
        assert!(is_under(r"c:\repos\foo\sub", r"c:\repos\foo"));
        assert!(is_under(r"c:\repos\foo", r"c:\repos\foo"));
        assert!(!is_under(r"c:\repos\foobar", r"c:\repos\foo"));
    }
}
