//! リポジトリの中で gitignore されているファイルを洗い出す。
//!
//! git には「無視されていて手元にあるもの」を `--directory` で畳んで答えさせ、
//! 生成物のフォルダには入らない (node_modules などを 1 件ずつ並べない)。
//! 個人のフォルダだけ自分で降りてファイルを並べる。中身は読まない。

use crate::classify::{classify, Class};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// 子プロセスのコンソール窓を出さない (CREATE_NO_WINDOW)。
/// Offstage は窓のアプリなので、付けないと git を呼ぶたびに黒い窓が開く (2026-11-12、リポジトリの数 × 2 個開いて操作できなくなった)
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 窓を出さない git。取り込みで git を呼ぶときは必ずこれを使う
pub fn git_command() -> Command {
    let mut cmd = Command::new("git");
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// 見つかったファイル 1 件
#[derive(Debug, Clone)]
pub struct Entry {
    /// リポジトリの根からの相対パス (区切りは `/`)
    pub rel_path: String,
    pub class: Class,
    /// 生成物のフォルダは降りないので、件数も大きさも数えない
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    /// フォルダを 1 件に畳んだ理由 (例 `profile:chromium`)。畳んでいなければ None
    pub folded: Option<&'static str>,
}

impl Entry {
    /// 区分を決めた規則 (機密の疑いと畳んだフォルダだけ。画面の「なぜこの区分か」に出す)
    pub fn rule(&self) -> Option<String> {
        match self.folded {
            Some(f) => Some(f.to_string()),
            None if self.class == Class::Secret => crate::classify::secret_rule_of(&self.rel_path),
            None => None,
        }
    }
}

/// Chromium 系 (Chrome・Edge・WebView2・Electron の user-data-dir) の利用者データのフォルダか。
/// 作り直せるが Cookie やログインの情報を持つので、生成物にも個人の 1 件ずつにもせず、機密の疑いの 1 件に畳む
fn is_browser_profile(dir: &Path) -> bool {
    dir.join("Local State").is_file()
}

/// リポジトリ 1 つの結果
#[derive(Debug, Clone)]
pub struct RepoScan {
    pub root: PathBuf,
    pub entries: Vec<Entry>,
}

impl RepoScan {
    pub fn count(&self, class: Class) -> usize {
        self.entries.iter().filter(|e| e.class == class).count()
    }
}

/// git に、無視されていて手元にあるパスを聞く (フォルダは畳まれ、末尾が `/`)
pub fn ignored_paths(repo: &Path) -> Result<Vec<String>, String> {
    let out = git_command()
        .arg("-C")
        .arg(repo)
        // リポジトリの設定から外のプログラムを動かさない (RepoTether の git.rs と同じ考え)
        .args(["-c", "core.fsmonitor=false", "-c", "core.untrackedCache=false"])
        .args(["ls-files", "--others", "--ignored", "--exclude-standard", "--directory", "-z"])
        .output()
        .map_err(|e| format!("git を起動できません: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(out
        .stdout
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect())
}

/// 人の上書き: (リポジトリの中のパス, 区分)。パスはフォルダでもファイルでもよく、いちばん長く当たったものを使う
pub type Overrides = [(String, Class)];

/// 規則で分けたあと、人の上書きを当てる
pub fn class_for(rel: &str, is_dir: bool, ov: &Overrides) -> Class {
    override_match(rel, ov).unwrap_or_else(|| classify(rel, is_dir))
}

fn override_match(rel: &str, ov: &Overrides) -> Option<Class> {
    ov.iter()
        .filter(|(p, _)| rel == p || (rel.len() > p.len() && rel.starts_with(p.as_str()) && rel.as_bytes()[p.len()] == b'/'))
        .max_by_key(|(p, _)| p.len())
        .map(|(_, c)| *c)
}

/// Flutter・Capacitor などのアプリの、機種ごとのフォルダ
const PLATFORM_DIRS: &[&str] = &["android", "ios", "macos", "linux", "windows", "web"];

/// この名前のファイルがあるフォルダは、機種ごとのフォルダを道具が作るアプリのプロジェクト
const APP_PROJECT_MARKERS: &[&str] = &["pubspec.yaml", "capacitor.config.json", "capacitor.config.ts", "capacitor.config.js"];

/// アプリのプロジェクトの機種ごとのフォルダ (`android/`・`ios/`・`web/` など) の下か。
/// そこで無視されているのは、flutter create・Capacitor が作った雛形や、ビルドのたびに作り直されるもの
fn in_app_platform(repo: &Path, rel: &str) -> bool {
    let segs: Vec<&str> = rel.split('/').collect();
    segs[..segs.len().saturating_sub(1)].iter().enumerate().any(|(i, s)| {
        PLATFORM_DIRS.contains(&s.to_ascii_lowercase().as_str()) && {
            let project = segs[..i].iter().fold(repo.to_path_buf(), |p, s| p.join(s));
            APP_PROJECT_MARKERS.iter().any(|m| project.join(m).is_file())
        }
    })
}

/// Cloudflare Workers の設定 (wrangler) に書かれた、配るファイルの置き場 (`assets.directory`・古い `site.bucket`)。
/// フォルダの中からの相対で、`./` と末尾の `/` は外す
fn wrangler_assets(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for name in ["wrangler.jsonc", "wrangler.json", "wrangler.toml"] {
        let Ok(text) = std::fs::read_to_string(dir.join(name)) else { continue };
        for key in ["directory", "bucket"] {
            let mut rest = text.as_str();
            while let Some(i) = rest.find(key) {
                rest = &rest[i + key.len()..];
                // "directory": "./site" (JSON) か directory = "./site" (TOML)
                let after = rest.trim_start().trim_start_matches('"').trim_start();
                let Some(after) = after.strip_prefix(':').or_else(|| after.strip_prefix('=')) else { continue };
                let Some(v) = after.trim_start().strip_prefix('"').and_then(|s| s.split('"').next()) else { continue };
                let v = v.trim_start_matches("./").trim_end_matches('/').replace('\\', "/");
                if !v.is_empty() && !v.starts_with("..") && !v.contains(':') {
                    out.push(v);
                }
            }
        }
    }
    out
}

/// Cloudflare Workers の配るファイルの置き場の下か (ビルドが dist から並べ直す写し)
fn in_wrangler_assets(repo: &Path, rel: &str) -> bool {
    let segs: Vec<&str> = rel.split('/').collect();
    (0..segs.len().saturating_sub(1)).any(|i| {
        let project = segs[..i].iter().fold(repo.to_path_buf(), |p, s| p.join(s));
        let below = segs[i..].join("/");
        wrangler_assets(&project)
            .iter()
            .any(|a| below.len() > a.len() && below.starts_with(a.as_str()) && below.as_bytes()[a.len()] == b'/')
    })
}

/// 洗い出しのときの区分: 人の上書き → 名前の規則 → 置き場所 (アプリの機種ごとのフォルダのファイルは生成物)。
/// フォルダごとは畳まない (`android/key.properties` のような機密の疑いを見落とさないため、ファイルごとに決める)
fn class_in(repo: &Path, rel: &str, is_dir: bool, ov: &Overrides) -> Class {
    if let Some(c) = override_match(rel, ov) {
        return c;
    }
    let c = classify(rel, is_dir);
    if !is_dir && matches!(c, Class::Personal | Class::ToolData | Class::Data) && (in_app_platform(repo, rel) || in_wrangler_assets(repo, rel)) {
        return Class::Generated;
    }
    c
}

/// リポジトリ 1 つを洗い出す
pub fn scan_repo(repo: &Path) -> Result<RepoScan, String> {
    scan_repo_with(repo, &[])
}

/// 人の上書きを当てて洗い出す
pub fn scan_repo_with(repo: &Path, ov: &Overrides) -> Result<RepoScan, String> {
    let mut entries = Vec::new();
    for p in without_nested(ignored_paths(repo)?) {
        let is_dir = p.ends_with('/');
        let rel = p.trim_end_matches('/').to_string();
        let class = class_in(repo, &rel, is_dir, ov);
        if is_dir && class != Class::Generated {
            walk_dir(repo, &rel, ov, &mut entries);
        } else {
            push_entry(repo, rel, class, is_dir, &mut entries);
        }
    }
    entries.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    entries.dedup_by(|a, b| a.rel_path == b.rel_path);
    Ok(RepoScan { root: repo.to_path_buf(), entries })
}

/// git は、無視されたフォルダの中に自分用の .gitignore を持つフォルダがあると、
/// 親のフォルダ (`a/`) と中のパス (`a/b/c`) を両方返す。親は自分で降りるので、中のほうを外す
fn without_nested(mut paths: Vec<String>) -> Vec<String> {
    paths.sort();
    let dirs: Vec<String> = paths.iter().filter(|p| p.ends_with('/')).cloned().collect();
    paths.retain(|p| !dirs.iter().any(|d| p != d && p.starts_with(d.as_str())));
    paths
}

fn push_entry(repo: &Path, rel: String, class: Class, is_dir: bool, out: &mut Vec<Entry>) {
    let (size, modified) = if is_dir {
        (0, None)
    } else {
        match std::fs::symlink_metadata(repo.join(&rel)) {
            Ok(m) => (m.len(), m.modified().ok()),
            Err(_) => (0, None),
        }
    };
    out.push(Entry { rel_path: rel, class, is_dir, size, modified, folded: None });
}

/// この名前のファイルがあるフォルダは、道具が作ったもの (中身ごと生成物として 1 件に畳む)
const GENERATED_MARKERS: &[(&str, &str)] = &[
    ("pyvenv.cfg", "marker:venv"),
    ("CACHEDIR.TAG", "marker:cache"),
    ("base_library.zip", "marker:pyinstaller"),
];

/// 1 つのフォルダの下にこれより多くのファイルがあれば、人が 1 つずつ書いたものではない
/// (地図のタイル・取ってきたデータ・連番の画像など)。道具のデータの 1 件に畳む
pub const BULK_FILES: usize = 1000;

/// 人の上書きが、このフォルダかその上・下に付いているか (付いていれば数では畳まない)
fn has_override(rel: &str, ov: &Overrides) -> bool {
    ov.iter().any(|(p, _)| {
        let under = |a: &str, b: &str| a == b || (a.len() > b.len() && a.starts_with(b) && a.as_bytes()[b.len()] == b'/');
        under(rel, p) || under(p, rel)
    })
}

/// フォルダを 1 つ降りる。目印のファイルがあれば生成物に、中のファイルが多すぎれば道具のデータの 1 件に畳む
fn walk_dir(repo: &Path, rel_dir: &str, ov: &Overrides, out: &mut Vec<Entry>) {
    let dir = repo.join(rel_dir);
    if !has_override(rel_dir, ov) {
        if let Some((_, why)) = GENERATED_MARKERS.iter().find(|(m, _)| dir.join(m).is_file()) {
            out.push(Entry { rel_path: rel_dir.to_string(), class: Class::Generated, is_dir: true, size: 0, modified: None, folded: Some(why) });
            return;
        }
    }
    let start = out.len();
    walk(repo, rel_dir, ov, out);
    let added = &out[start..];
    let files = added.iter().filter(|e| !e.is_dir).count();
    // 機密の疑いが混じっていれば畳まない (漏れの印を落とさない)
    if files > BULK_FILES && !has_override(rel_dir, ov) && added.iter().all(|e| e.class != Class::Secret) {
        let size = added.iter().map(|e| e.size).sum();
        let modified = added.iter().filter_map(|e| e.modified).max();
        // data/ の中ならデータの 1 件、それ以外は道具のデータの 1 件
        let class = if added.iter().all(|e| e.class == Class::Data) { Class::Data } else { Class::ToolData };
        out.truncate(start);
        out.push(Entry { rel_path: rel_dir.to_string(), class, is_dir: true, size, modified, folded: Some("bulk") });
    }
}

/// 個人のフォルダを降りる。下に生成物のフォルダがあれば畳んで 1 件にする。
/// シンボリックリンク・ジャンクションは辿らない (別の場所を二重に数えない)
fn walk(repo: &Path, rel_dir: &str, ov: &Overrides, out: &mut Vec<Entry>) {
    let dir = repo.join(rel_dir);
    if is_browser_profile(&dir) {
        out.push(Entry {
            rel_path: rel_dir.to_string(),
            class: Class::Secret,
            is_dir: true,
            size: 0,
            modified: None,
            folded: Some("profile:chromium"),
        });
        return;
    }
    let Ok(read) = std::fs::read_dir(&dir) else {
        return;
    };
    for item in read.flatten() {
        let Ok(ft) = item.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        let name = item.file_name().to_string_lossy().into_owned();
        let rel = format!("{rel_dir}/{name}");
        let class = class_in(repo, &rel, ft.is_dir(), ov);
        if ft.is_dir() {
            if class == Class::Generated {
                push_entry(repo, rel, class, true, out);
            } else {
                walk_dir(repo, &rel, ov, out);
            }
        } else {
            push_entry(repo, rel, class, false, out);
        }
    }
}

/// git が管理しているファイルのうち、資料の種類 ([`crate::link::is_doc`]) のもの (1-2 の対象)
pub fn tracked_docs(repo: &Path) -> Result<Vec<String>, String> {
    let out = git_command()
        .arg("-C")
        .arg(repo)
        .args(["-c", "core.fsmonitor=false", "-c", "core.quotePath=false", "ls-files", "-z"])
        .output()
        .map_err(|e| format!("git を起動できません: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(out
        .stdout
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .filter(|p| crate::link::is_doc(p))
        // アプリの機種ごとのフォルダのアイコン・起動画面・index.html は、flutter create などが作った雛形で資料ではない
        .filter(|p| !in_app_platform(repo, p))
        .collect())
}

/// `root` の直下で `.git` を持つフォルダ (RepoTether の一覧につなぐまでの仮の探し方)
pub fn child_repos(root: &Path) -> Vec<PathBuf> {
    let mut repos: Vec<PathBuf> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join(".git").exists())
        .collect();
    repos.sort();
    repos
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git").arg("-C").arg(dir).args(args).status().unwrap().success();
        assert!(ok, "git {args:?}");
    }

    #[test]
    fn overrides_take_the_longest_match() {
        let ov = vec![
            ("work/_snap".to_string(), Class::ToolData),
            ("work/_snap/keep.md".to_string(), Class::Personal),
            ("old".to_string(), Class::Generated),
        ];
        assert_eq!(class_for("work/_snap/shot.png", false, &ov), Class::ToolData);
        assert_eq!(class_for("work/_snap/keep.md", false, &ov), Class::Personal);
        assert_eq!(class_for("work/_snaper/a.md", false, &ov), Class::Personal); // 前方一致は区切りで
        assert_eq!(class_for("old", true, &ov), Class::Generated);
        assert_eq!(class_for("sandbox/x.md", false, &ov), Class::Personal);
    }

    #[test]
    fn nested_answers_under_a_reported_folder_are_dropped() {
        let got = without_nested(vec![
            ".tool/sub/x.md".into(),
            ".tool/".into(),
            ".tool/sub/".into(),
            ".toolbox/y.md".into(),
            "z.md".into(),
        ]);
        assert_eq!(got, vec![".tool/", ".toolbox/y.md", "z.md"]);
    }

    /// 架空のリポジトリを作って洗い出す (手元の実際のファイル名は使わない)
    #[test]
    fn scans_personal_folders_and_folds_generated_ones() {
        let dir = std::env::temp_dir().join(format!("offstage-scan-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q"]);
        fs::write(dir.join(".gitignore"), "sandbox/\nnode_modules/\n.work/\n*.vsix\n").unwrap();
        fs::write(dir.join("README.md"), "tracked\n").unwrap();
        fs::create_dir_all(dir.join("sandbox/sub/.venv/lib")).unwrap();
        fs::write(dir.join("sandbox/memo.md"), "x").unwrap();
        fs::write(dir.join("sandbox/sub/note.txt"), "xy").unwrap();
        fs::write(dir.join("sandbox/sub/.venv/lib/a.py"), "z").unwrap();
        fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
        fs::write(dir.join("node_modules/pkg/index.js"), "z").unwrap();
        fs::create_dir_all(dir.join(".work")).unwrap();
        fs::write(dir.join(".work/demo_token.json"), "{}").unwrap();
        fs::write(dir.join("app-0.1.0.vsix"), "z").unwrap();
        // ブラウザの利用者データは中を並べず 1 件に畳む
        fs::create_dir_all(dir.join("sandbox/webview/Default/Network")).unwrap();
        fs::write(dir.join("sandbox/webview/Local State"), "{}").unwrap();
        fs::write(dir.join("sandbox/webview/Default/Network/Trust Tokens"), "z").unwrap();

        let scan = scan_repo(&dir).unwrap();
        let got: Vec<(String, Class)> =
            scan.entries.iter().map(|e| (e.rel_path.clone(), e.class)).collect();
        assert_eq!(
            got,
            vec![
                (".work/demo_token.json".into(), Class::Secret),
                ("app-0.1.0.vsix".into(), Class::Generated),
                ("node_modules".into(), Class::Generated),
                ("sandbox/memo.md".into(), Class::Personal),
                ("sandbox/sub/.venv".into(), Class::Generated),
                ("sandbox/sub/note.txt".into(), Class::Personal),
                ("sandbox/webview".into(), Class::Secret),
            ]
        );
        let profile = scan.entries.iter().find(|e| e.rel_path == "sandbox/webview").unwrap();
        assert_eq!(profile.rule().as_deref(), Some("profile:chromium"));
        let token = scan.entries.iter().find(|e| e.class == Class::Secret && !e.is_dir).unwrap();
        assert_eq!(token.rule().as_deref(), Some("word:token"));
        let note = scan.entries.iter().find(|e| e.rel_path == "sandbox/sub/note.txt").unwrap();
        assert_eq!(note.size, 2);
        assert!(note.modified.is_some());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn app_platform_folders_are_generated_but_secrets_stay() {
        let dir = std::env::temp_dir().join(format!("offstage-flutter-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("android/app/src")).unwrap();
        fs::create_dir_all(dir.join("web/icons")).unwrap();
        fs::create_dir_all(dir.join("sandbox/web")).unwrap();
        git(&dir, &["init", "-q"]);
        fs::write(dir.join("pubspec.yaml"), "name: x").unwrap();
        fs::write(dir.join(".gitignore"), "android/\nweb/\nsandbox/\n").unwrap();
        fs::write(dir.join("android/build.gradle"), "g").unwrap();
        fs::write(dir.join("android/app/src/Main.kt"), "k").unwrap();
        fs::write(dir.join("android/key.properties"), "storePassword=x").unwrap();
        fs::write(dir.join("android/app/google-services.json"), "{}").unwrap();
        fs::write(dir.join("web/index.html"), "<html>").unwrap();
        fs::write(dir.join("web/icons/Icon-192.png"), "p").unwrap();
        // アプリのプロジェクトでないところの web/ は当てない
        fs::write(dir.join("sandbox/web/page.png"), "p").unwrap();

        let scan = scan_repo(&dir).unwrap();
        let class = |p: &str| scan.entries.iter().find(|e| e.rel_path == p).map(|e| e.class);
        for p in ["android/build.gradle", "android/app/src/Main.kt", "web/index.html", "web/icons/Icon-192.png"] {
            assert_eq!(class(p), Some(Class::Generated), "{p}");
        }
        assert_eq!(class("android/key.properties"), Some(Class::Secret));
        assert_eq!(class("android/app/google-services.json"), Some(Class::Secret));
        assert_eq!(class("sandbox/web/page.png"), Some(Class::Personal));
        // 人が付け替えたものはそちらが勝つ
        let ov = vec![("web".to_string(), Class::Personal)];
        let scan = scan_repo_with(&dir, &ov).unwrap();
        assert_eq!(scan.entries.iter().find(|e| e.rel_path == "web/index.html").unwrap().class, Class::Personal);

        // git が管理する雛形のアイコンは資料にしない。人が書いた資料は残す
        fs::create_dir_all(dir.join("ios/Runner/Assets.xcassets")).unwrap();
        fs::create_dir_all(dir.join("docs")).unwrap();
        fs::write(dir.join("ios/Runner/Assets.xcassets/Icon-60.png"), "p").unwrap();
        fs::write(dir.join("docs/guide.md"), "g").unwrap();
        git(&dir, &["add", "ios", "docs"]);
        assert_eq!(tracked_docs(&dir).unwrap(), vec!["docs/guide.md".to_string()]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn wrangler_assets_folder_is_generated() {
        let dir = std::env::temp_dir().join(format!("offstage-wrangler-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("worker/site/lp")).unwrap();
        fs::create_dir_all(dir.join("worker/notes")).unwrap();
        git(&dir, &["init", "-q"]);
        fs::write(dir.join(".gitignore"), "worker/site/\nworker/notes/\nworker/.dev.vars\n").unwrap();
        fs::write(
            dir.join("worker/wrangler.jsonc"),
            "{\n  // comment\n  \"assets\": {\n    \"directory\": \"./site\",\n    \"binding\": \"ASSETS\"\n  }\n}\n",
        )
        .unwrap();
        fs::write(dir.join("worker/site/lp/index.html"), "<html>").unwrap();
        fs::write(dir.join("worker/site/_headers"), "h").unwrap();
        fs::write(dir.join("worker/notes/memo.md"), "m").unwrap();
        fs::write(dir.join("worker/.dev.vars"), "K=v").unwrap();

        let scan = scan_repo(&dir).unwrap();
        let class = |p: &str| scan.entries.iter().find(|e| e.rel_path == p).map(|e| e.class);
        assert_eq!(class("worker/site/lp/index.html"), Some(Class::Generated));
        assert_eq!(class("worker/site/_headers"), Some(Class::Generated));
        assert_eq!(class("worker/notes/memo.md"), Some(Class::Personal));
        assert_eq!(class("worker/.dev.vars"), Some(Class::Secret));
        // TOML の書き方も読む
        assert_eq!(wrangler_assets(&dir.join("worker")), vec!["site".to_string()]);
        fs::write(dir.join("worker/wrangler.toml"), "[assets]\ndirectory = \"./public/\"\n").unwrap();
        assert_eq!(wrangler_assets(&dir.join("worker")), vec!["site".to_string(), "public".to_string()]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn folds_marked_folders_and_bulk_data() {
        let dir = std::env::temp_dir().join(format!("offstage-bulk-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q"]);
        fs::write(dir.join(".gitignore"), "sandbox/\n").unwrap();
        // 名前で分からない venv は pyvenv.cfg で見分ける
        fs::create_dir_all(dir.join("sandbox/env/Lib")).unwrap();
        fs::write(dir.join("sandbox/env/pyvenv.cfg"), "home = x").unwrap();
        fs::write(dir.join("sandbox/env/Lib/a.py"), "z").unwrap();
        // 多すぎるファイル (地図のタイルなど) は道具のデータの 1 件に畳む。中のフォルダで数える
        for x in 0..2 {
            fs::create_dir_all(dir.join(format!("sandbox/tiles/12/{x}"))).unwrap();
        }
        for i in 0..=BULK_FILES {
            fs::write(dir.join(format!("sandbox/tiles/12/{}/{i}.png", i % 2)), "p").unwrap();
        }
        fs::write(dir.join("sandbox/tiles/readme.md"), "r").unwrap();
        fs::write(dir.join("sandbox/memo.md"), "m").unwrap();
        // 機密の疑いが混じっていれば畳まない
        fs::create_dir_all(dir.join("sandbox/keys")).unwrap();
        for i in 0..=BULK_FILES {
            fs::write(dir.join(format!("sandbox/keys/{i}.txt")), "k").unwrap();
        }
        fs::write(dir.join("sandbox/keys/api_token.txt"), "t").unwrap();

        let scan = scan_repo(&dir).unwrap();
        let find = |p: &str| scan.entries.iter().find(|e| e.rel_path == p);
        let env = find("sandbox/env").unwrap();
        assert_eq!((env.class, env.rule().as_deref()), (Class::Generated, Some("marker:venv")));
        let tiles = find("sandbox/tiles/12").unwrap();
        assert_eq!((tiles.class, tiles.rule().as_deref(), tiles.size), (Class::ToolData, Some("bulk"), BULK_FILES as u64 + 1));
        assert!(tiles.modified.is_some());
        assert_eq!(find("sandbox/tiles/readme.md").unwrap().class, Class::Personal);
        assert_eq!(find("sandbox/memo.md").unwrap().class, Class::Personal);
        assert_eq!(find("sandbox/keys/api_token.txt").unwrap().class, Class::Secret);
        assert!(find("sandbox/keys").is_none());

        // 人が付け替えたフォルダは数では畳まない
        let ov = vec![("sandbox/tiles".to_string(), Class::Personal)];
        let scan = scan_repo_with(&dir, &ov).unwrap();
        assert!(scan.entries.iter().any(|e| e.rel_path == "sandbox/tiles/12/0/0.png"));
        let _ = fs::remove_dir_all(&dir);
    }
}
