//! RepoTether から入れる道具 (LockWatch・ToolKeeper)。方針は docs/tools-install.md。
//!
//! 道具の GitHub の Release の最新版から、Python の要らない exe の zip (`<Folder>-<版>-win-x64.zip`) を取り、
//! GitHub が答える SHA-256 で確かめてから `%LOCALAPPDATA%\RepoTether\tools\<Folder>\` に展開する。
//! 取る先はここに書いた道具・リポジトリだけで、画面から任意の URL は受けない。
//!
//! 道具は設定とデータを exe の隣に置く (LockWatch の design.md §9.1、ToolKeeper の §9.3)。
//! 更新では zip の中身だけを入れ替え、`keep` の名前 (設定・データ・RepoTether の印) には触らない。
//! 入れ替えは、古いものを全部脇へ動かせてから新しいものを入れる。動かせないもの (画面を開いている exe など) があれば、
//! 何も変えずにやめる。

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// RepoTether が入れた印 (入れた版)。更新でも消さない
const MARKER: &str = "repotether-tool.json";
/// zip の大きさの上限 (LockWatch・ToolKeeper は 15 MB ほど)
const MAX_ZIP: u64 = 200 * 1024 * 1024;
/// osv-scanner の exe の大きさの上限 (2.6.0 で 56 MB)
const MAX_EXE: u64 = 300 * 1024 * 1024;

pub struct Spec {
    pub id: &'static str,
    pub name: &'static str,
    /// GitHub の owner/name
    pub repo: &'static str,
    /// zip の一番上のフォルダの名前で、入れ先のフォルダの名前
    pub folder: &'static str,
    /// コマンド用 (コンソール) の exe。入れたあとに必ずあるもの
    pub exe: &'static str,
    /// 更新で触らない名前 (設定とデータ)。zip に入っていたら入れない
    pub keep: &'static [&'static str],
}

pub const LOCKWATCH: Spec = Spec {
    id: "lockwatch",
    name: "LockWatch",
    repo: "lancard-aikawa/LockWatch",
    folder: "LockWatch",
    exe: "lockwatch.exe",
    keep: &["lockwatch.json", "data", "osv-scanner.exe", MARKER],
};

pub const TOOLKEEPER: Spec = Spec {
    id: "toolkeeper",
    name: "ToolKeeper",
    repo: "lancard-aikawa/ToolKeeper",
    folder: "ToolKeeper",
    exe: "toolkeeper.exe",
    keep: &["toolkeeper.json", "data", MARKER],
};

pub fn spec(id: &str) -> Option<&'static Spec> {
    [&LOCKWATCH, &TOOLKEEPER].into_iter().find(|s| s.id == id)
}

/// LockWatch が使う osv-scanner。LockWatch と同じく版を固定する (新しい版で JSON の形が変わることがあるため)
pub const OSV_REPO: &str = "google/osv-scanner";
pub const OSV_TAG: &str = "v2.6.0";
pub const OSV_ASSET: &str = "osv-scanner_windows_amd64.exe";

/// 道具を入れる場所 (`%LOCALAPPDATA%\RepoTether\tools`)
pub fn tools_root() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty()).map(|b| PathBuf::from(b).join("RepoTether").join("tools"))
}

pub fn install_dir(spec: &Spec) -> Option<PathBuf> {
    tools_root().map(|r| r.join(spec.folder))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Marker {
    version: String,
    asset: String,
    sha256: String,
    installed_at: i64,
}

/// 画面に渡す、入れた道具の状態 (ネットにはつながない)
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub id: String,
    pub name: String,
    /// 入れる場所 (まだ入れていなくても出す)
    pub dir: String,
    /// RepoTether が入れた版。入れていなければ None
    pub installed: Option<String>,
}

fn read_marker(dir: &Path) -> Option<Marker> {
    let text = std::fs::read_to_string(dir.join(MARKER)).ok()?;
    serde_json::from_str(&text).ok()
}

/// 入れた道具の状態。印があり、コマンドの exe もあるときだけ「入れた」とみなす
pub fn info_in(spec: &Spec, dir: &Path) -> Info {
    let installed = read_marker(dir).filter(|_| dir.join(spec.exe).is_file()).map(|m| m.version);
    Info { id: spec.id.into(), name: spec.name.into(), dir: dir.to_string_lossy().into_owned(), installed }
}

pub fn info(spec: &Spec) -> Result<Info, String> {
    let dir = install_dir(spec).ok_or("LOCALAPPDATA が分からないので、入れる場所を決められません")?;
    Ok(info_in(spec, &dir))
}

// ---- GitHub の Release ----

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
    /// "sha256:<16 進 64 桁>"。2025 年 6 月より前に付けたファイルには無い
    digest: Option<String>,
}

/// 入れられる最新版
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Latest {
    pub version: String,
    pub asset: String,
    pub size: u64,
    #[serde(skip)]
    pub url: String,
    #[serde(skip)]
    pub sha256: String,
}

fn sha256_of_digest(digest: Option<&str>) -> Option<String> {
    let hex = digest?.strip_prefix("sha256:")?.to_ascii_lowercase();
    (hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit())).then_some(hex)
}

/// Release から、その版の exe の zip を選ぶ。SHA-256 の無いもの・ほかの場所を指すものは選ばない
fn pick_asset(spec: &Spec, rel: &Release) -> Result<Latest, String> {
    let version = rel.tag_name.trim_start_matches('v').to_string();
    let want = format!("{}-{}-win-x64.zip", spec.folder, version);
    let a = rel.assets.iter().find(|a| a.name == want).ok_or_else(|| {
        format!("{} {} の Release に {want} がありません (exe の zip を付ける前の版です)", spec.name, version)
    })?;
    let sha256 = sha256_of_digest(a.digest.as_deref())
        .ok_or_else(|| format!("{want} の SHA-256 が GitHub から返らないので、確かめられません"))?;
    let prefix = format!("https://github.com/{}/releases/download/", spec.repo);
    if !a.browser_download_url.starts_with(&prefix) {
        return Err(format!("{want} の取り先が {} のものではありません", spec.repo));
    }
    if a.size > MAX_ZIP {
        return Err(format!("{want} が大きすぎます ({} バイト)", a.size));
    }
    Ok(Latest { version, asset: a.name.clone(), size: a.size, url: a.browser_download_url.clone(), sha256 })
}

pub fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("RepoTether")
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|e| e.to_string())
}

async fn release(client: &reqwest::Client, repo: &str, tag: Option<&str>) -> Result<Release, String> {
    let url = match tag {
        Some(t) => format!("https://api.github.com/repos/{repo}/releases/tags/{t}"),
        None => format!("https://api.github.com/repos/{repo}/releases/latest"),
    };
    let resp = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|e| format!("GitHub につながりません: {e}"))?;
    let status = resp.status();
    if status.as_u16() == 404 {
        return Err(format!("{repo} に公開した Release がありません"));
    }
    if status.as_u16() == 403 || status.as_u16() == 429 {
        return Err("GitHub の回数の制限に当たりました。しばらくしてからもう一度押してください".into());
    }
    if !status.is_success() {
        return Err(format!("GitHub が答えません (HTTP {})", status.as_u16()));
    }
    resp.json::<Release>().await.map_err(|e| format!("GitHub の答えを読めません: {e}"))
}

/// その道具の最新版 (GitHub につなぐ)
pub async fn latest(client: &reqwest::Client, spec: &Spec) -> Result<Latest, String> {
    pick_asset(spec, &release(client, spec.repo, None).await?)
}

/// 取って SHA-256 を確かめる。大きさは cap まで
async fn download(client: &reqwest::Client, url: &str, sha256: &str, cap: u64) -> Result<Vec<u8>, String> {
    let mut resp = client.get(url).send().await.map_err(|e| format!("取れません: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("取れません (HTTP {})", resp.status().as_u16()));
    }
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(|e| format!("取っている途中で切れました: {e}"))? {
        buf.extend_from_slice(&chunk);
        if buf.len() as u64 > cap {
            return Err("思ったより大きいので、取るのをやめました".into());
        }
    }
    let got = hex(&Sha256::digest(&buf));
    if got != sha256 {
        return Err(format!("取ったファイルの SHA-256 が GitHub の答えと違います ({got})。入れません"));
    }
    Ok(buf)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// 最新版を取って入れる (入れてあれば入れ替える)。入れた状態を返す
pub async fn install_latest(client: &reqwest::Client, spec: &Spec) -> Result<Info, String> {
    let dir = install_dir(spec).ok_or("LOCALAPPDATA が分からないので、入れる場所を決められません")?;
    let l = latest(client, spec).await?;
    let bytes = download(client, &l.url, &l.sha256, MAX_ZIP).await?;
    let (dir2, l2) = (dir.clone(), l.clone());
    let spec_id = spec.id;
    tauri::async_runtime::spawn_blocking(move || {
        let spec = self::spec(spec_id).expect("known tool");
        install_zip(spec, &dir2, &bytes, &l2, now())
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(info_in(spec, &dir))
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn is_kept(spec: &Spec, name: &str) -> bool {
    spec.keep.iter().any(|k| k.eq_ignore_ascii_case(name))
}

/// zip を dir に入れる。zip は `<folder>/...` の形で、keep の名前を含まないこと
pub fn install_zip(spec: &Spec, dir: &Path, bytes: &[u8], l: &Latest, at: i64) -> Result<(), String> {
    let parent = dir.parent().ok_or("入れる場所がおかしい")?;
    std::fs::create_dir_all(parent).map_err(|e| format!("{} を作れません: {e}", parent.display()))?;
    let staging = parent.join(format!(".{}-new", spec.folder));
    let trash = parent.join(format!(".{}-old", spec.folder));
    let _ = std::fs::remove_dir_all(&staging);
    let _ = std::fs::remove_dir_all(&trash);
    std::fs::create_dir_all(&staging).map_err(|e| format!("{} を作れません: {e}", staging.display()))?;
    let result = (|| {
        extract(spec, bytes, &staging)?;
        if !staging.join(spec.exe).is_file() {
            return Err(format!("zip に {} がありません", spec.exe));
        }
        swap(spec, dir, &staging, &trash)?;
        let m = Marker { version: l.version.clone(), asset: l.asset.clone(), sha256: l.sha256.clone(), installed_at: at };
        let text = serde_json::to_string_pretty(&m).map_err(|e| e.to_string())?;
        std::fs::write(dir.join(MARKER), text).map_err(|e| format!("入れた印を書けません: {e}"))
    })();
    let _ = std::fs::remove_dir_all(&staging);
    let _ = std::fs::remove_dir_all(&trash);
    result
}

/// `<folder>/` の下を staging に書き出す。外へ出る名前・keep の名前・ほかのフォルダの中身があれば止める
fn extract(spec: &Spec, bytes: &[u8], staging: &Path) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| format!("zip を読めません: {e}"))?;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| format!("zip を読めません: {e}"))?;
        let raw = f.name().map_err(|e| format!("zip の名前を読めません: {e}"))?.into_owned();
        let name = f.enclosed_name().ok_or_else(|| format!("zip に外を指す名前があります: {raw}"))?;
        let mut parts = name.components();
        let top = parts.next().map(|c| c.as_os_str().to_string_lossy().into_owned()).unwrap_or_default();
        if top != spec.folder {
            return Err(format!("zip に {} の外のものがあります: {raw}", spec.folder));
        }
        let rest: PathBuf = parts.collect();
        let Some(first) = rest.components().next() else { continue };
        if is_kept(spec, &first.as_os_str().to_string_lossy()) {
            return Err(format!("zip に設定かデータ ({raw}) が入っています。入れません"));
        }
        let out = staging.join(&rest);
        if f.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| format!("{} を作れません: {e}", out.display()))?;
            continue;
        }
        if let Some(p) = out.parent() {
            std::fs::create_dir_all(p).map_err(|e| format!("{} を作れません: {e}", p.display()))?;
        }
        let mut data = Vec::with_capacity(f.size() as usize);
        f.read_to_end(&mut data).map_err(|e| format!("zip の {raw} を読めません: {e}"))?;
        std::fs::write(&out, data).map_err(|e| format!("{} を書けません: {e}", out.display()))?;
    }
    Ok(())
}

/// dir の中の keep 以外を trash へ動かしてから、staging の中身を dir へ動かす。
/// 動かせないものがあれば、動かしたものを戻して何も変えずに Err
fn swap(spec: &Spec, dir: &Path, staging: &Path, trash: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{} を作れません: {e}", dir.display()))?;
    std::fs::create_dir_all(trash).map_err(|e| format!("{} を作れません: {e}", trash.display()))?;
    let mut moved: Vec<String> = Vec::new();
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{} を読めません: {e}", dir.display()))?;
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if is_kept(spec, &name) {
            continue;
        }
        if let Err(err) = std::fs::rename(e.path(), trash.join(&name)) {
            for n in moved.iter().rev() {
                let _ = std::fs::rename(trash.join(n), dir.join(n));
            }
            return Err(format!(
                "{name} を入れ替えられません ({err})。{} の画面を閉じ、定期実行が動いていれば終わるのを待ってから、もう一度押してください",
                spec.name
            ));
        }
        moved.push(name);
    }
    let staged = std::fs::read_dir(staging).map_err(|e| format!("{} を読めません: {e}", staging.display()))?;
    for e in staged.flatten() {
        let name = e.file_name();
        std::fs::rename(e.path(), dir.join(&name))
            .map_err(|err| format!("{} を置けません ({err})。前の版は {} にあります", name.to_string_lossy(), trash.display()))?;
    }
    Ok(())
}

// ---- osv-scanner (LockWatch が使う) ----

/// LockWatch の隣に osv-scanner が要るか (PATH にあれば要らない)
pub fn osv_needed(lockwatch_dir: &Path) -> bool {
    !lockwatch_dir.join("osv-scanner.exe").is_file() && !on_path("osv-scanner.exe")
}

fn on_path(exe: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(exe).is_file()))
}

/// 版を固定した osv-scanner を LockWatch の隣に置く
pub async fn install_osv(client: &reqwest::Client, lockwatch_dir: &Path) -> Result<(), String> {
    let rel = release(client, OSV_REPO, Some(OSV_TAG)).await?;
    let a = rel.assets.iter().find(|a| a.name == OSV_ASSET).ok_or(format!("osv-scanner {OSV_TAG} に {OSV_ASSET} がありません"))?;
    let sha = sha256_of_digest(a.digest.as_deref()).ok_or("osv-scanner の SHA-256 が GitHub から返りません")?;
    if !a.browser_download_url.starts_with(&format!("https://github.com/{OSV_REPO}/releases/download/")) {
        return Err("osv-scanner の取り先がおかしい".into());
    }
    let bytes = download(client, &a.browser_download_url, &sha, MAX_EXE).await?;
    let tmp = lockwatch_dir.join("osv-scanner.exe.part");
    std::fs::write(&tmp, bytes).map_err(|e| format!("osv-scanner を書けません: {e}"))?;
    std::fs::rename(&tmp, lockwatch_dir.join("osv-scanner.exe")).map_err(|e| format!("osv-scanner を置けません: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let opt = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            for (name, data) in entries {
                z.start_file(*name, opt).unwrap();
                z.write_all(data).unwrap();
            }
            z.finish().unwrap();
        }
        buf.into_inner()
    }

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("repotether-tools-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn latest(v: &str) -> Latest {
        Latest { version: v.into(), asset: format!("LockWatch-{v}-win-x64.zip"), size: 1, url: String::new(), sha256: "0".repeat(64) }
    }

    #[test]
    fn install_then_update_keeps_settings_and_data() {
        let root = tmp("update");
        let dir = root.join("LockWatch");
        let v1 = zip_of(&[
            ("LockWatch/lockwatch.exe", b"v1"),
            ("LockWatch/lockwatchw.exe", b"v1"),
            ("LockWatch/_internal/old.pyd", b"old"),
            ("LockWatch/scripts/register-task.ps1", b"ps"),
        ]);
        install_zip(&LOCKWATCH, &dir, &v1, &latest("0.8.0"), 1).unwrap();
        assert_eq!(info_in(&LOCKWATCH, &dir).installed.as_deref(), Some("0.8.0"));
        // 使っているうちに設定とデータと osv-scanner ができる
        std::fs::write(dir.join("lockwatch.json"), "{}").unwrap();
        std::fs::create_dir_all(dir.join("data")).unwrap();
        std::fs::write(dir.join("data").join("latest.json"), "x").unwrap();
        std::fs::write(dir.join("osv-scanner.exe"), "osv").unwrap();

        let v2 = zip_of(&[("LockWatch/lockwatch.exe", b"v2"), ("LockWatch/_internal/new.pyd", b"new")]);
        install_zip(&LOCKWATCH, &dir, &v2, &latest("0.9.0"), 2).unwrap();
        assert_eq!(std::fs::read(dir.join("lockwatch.exe")).unwrap(), b"v2");
        assert!(!dir.join("_internal").join("old.pyd").exists(), "前の版の中身は残さない");
        assert!(!dir.join("lockwatchw.exe").exists(), "新しい zip に無いものは消える");
        assert!(dir.join("_internal").join("new.pyd").is_file());
        assert_eq!(std::fs::read_to_string(dir.join("lockwatch.json")).unwrap(), "{}", "設定は触らない");
        assert_eq!(std::fs::read_to_string(dir.join("data").join("latest.json")).unwrap(), "x", "データは触らない");
        assert!(dir.join("osv-scanner.exe").is_file());
        assert_eq!(info_in(&LOCKWATCH, &dir).installed.as_deref(), Some("0.9.0"));
        assert!(!root.join(".LockWatch-new").exists() && !root.join(".LockWatch-old").exists(), "作業用のフォルダは片付ける");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn refuses_zips_that_carry_data_or_escape() {
        let root = tmp("refuse");
        let dir = root.join("LockWatch");
        for (bad, why) in [
            (zip_of(&[("LockWatch/lockwatch.exe", b"x"), ("LockWatch/lockwatch.json", b"{}")]), "設定"),
            (zip_of(&[("LockWatch/lockwatch.exe", b"x"), ("LockWatch/data/targets.json", b"{}")]), "データ"),
            (zip_of(&[("LockWatch/lockwatch.exe", b"x"), ("Other/evil.exe", b"x")]), "外のもの"),
            (zip_of(&[("LockWatch/../evil.exe", b"x")]), "外"),
            (zip_of(&[("LockWatch/lockwatchw.exe", b"x")]), "lockwatch.exe がありません"),
        ] {
            let e = install_zip(&LOCKWATCH, &dir, &bad, &latest("1.0.0"), 1).unwrap_err();
            assert!(e.contains(why) || e.contains("設定かデータ"), "{why}: {e}");
            assert!(!dir.join("lockwatch.exe").exists(), "{why}: 何も入れない");
        }
        assert!(!root.join("evil.exe").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn picks_only_the_versioned_zip_with_a_digest() {
        let a = |name: &str, digest: Option<&str>, repo: &str| Asset {
            name: name.into(),
            browser_download_url: format!("https://github.com/{repo}/releases/download/v0.8.0/{name}"),
            size: 10,
            digest: digest.map(String::from),
        };
        let d = format!("sha256:{}", "Ab".repeat(32));
        let rel = Release {
            tag_name: "v0.8.0".into(),
            assets: vec![a("LockWatch-0.7.0-win-x64.zip", Some(&d), LOCKWATCH.repo), a("LockWatch-0.8.0-win-x64.zip", Some(&d), LOCKWATCH.repo)],
        };
        let l = pick_asset(&LOCKWATCH, &rel).unwrap();
        assert_eq!((l.version.as_str(), l.asset.as_str()), ("0.8.0", "LockWatch-0.8.0-win-x64.zip"));
        assert_eq!(l.sha256, "ab".repeat(32));

        let no_digest = Release { tag_name: "v0.8.0".into(), assets: vec![a("LockWatch-0.8.0-win-x64.zip", None, LOCKWATCH.repo)] };
        assert!(pick_asset(&LOCKWATCH, &no_digest).unwrap_err().contains("SHA-256"));
        let elsewhere = Release { tag_name: "v0.8.0".into(), assets: vec![a("LockWatch-0.8.0-win-x64.zip", Some(&d), "someone/else")] };
        assert!(pick_asset(&LOCKWATCH, &elsewhere).is_err());
        let source_only = Release { tag_name: "v0.7.0".into(), assets: vec![] };
        assert!(pick_asset(&LOCKWATCH, &source_only).unwrap_err().contains("exe の zip"));
    }

    #[test]
    fn digest_must_be_sha256_hex() {
        assert_eq!(sha256_of_digest(Some(&format!("sha256:{}", "0".repeat(64)))), Some("0".repeat(64)));
        assert_eq!(sha256_of_digest(Some("sha1:abc")), None);
        assert_eq!(sha256_of_digest(Some(&format!("sha256:{}", "g".repeat(64)))), None);
        assert_eq!(sha256_of_digest(None), None);
    }

    /// 手で動かす: 道具の scripts/build-exe.cmd で作った本物の zip を入れ、RepoTether の呼び方で status まで動かす。
    /// TOOLS_ZIP_LOCKWATCH / TOOLS_ZIP_TOOLKEEPER に zip、TOOLS_DIR に入れ先の親 (作業用) を入れる
    #[test]
    #[ignore]
    fn real_zips_install_and_run() {
        let root = PathBuf::from(std::env::var("TOOLS_DIR").expect("TOOLS_DIR"));
        for (spec, var) in [(&LOCKWATCH, "TOOLS_ZIP_LOCKWATCH"), (&TOOLKEEPER, "TOOLS_ZIP_TOOLKEEPER")] {
            let Ok(zip) = std::env::var(var) else { continue };
            let bytes = std::fs::read(&zip).unwrap();
            let dir = root.join(spec.folder);
            let l = Latest { version: "test".into(), asset: zip.clone(), size: bytes.len() as u64, url: String::new(), sha256: hex(&Sha256::digest(&bytes)) };
            install_zip(spec, &dir, &bytes, &l, now()).unwrap();
            // 2 回目 (更新) でも入る
            install_zip(spec, &dir, &bytes, &l, now()).unwrap();
            let d = dir.to_string_lossy();
            if spec.id == "lockwatch" {
                let s = super::super::lockwatch::status(&super::super::lockwatch::resolve(&d).unwrap()).unwrap();
                eprintln!("lockwatch status: {} data={}", s.lockwatch, s.data_dir);
            } else {
                let s = super::super::toolkeeper::status(&super::super::toolkeeper::resolve(&d).unwrap()).unwrap();
                eprintln!("toolkeeper status: data={}", s.data_dir);
            }
        }
    }

    /// 手で動かす (GitHub につなぐ): 版を固定した osv-scanner を取り、SHA-256 を確かめて TOOLS_DIR に置く
    #[test]
    #[ignore]
    fn real_osv_download() {
        let dir = PathBuf::from(std::env::var("TOOLS_DIR").expect("TOOLS_DIR"));
        std::fs::create_dir_all(&dir).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(install_osv(&client().unwrap(), &dir)).unwrap();
        let out = std::process::Command::new(dir.join("osv-scanner.exe")).arg("--version").output().unwrap();
        eprintln!("{}", String::from_utf8_lossy(&out.stdout));
    }

    #[test]
    fn runners_accept_the_installed_exe_folders() {
        let root = tmp("runner");
        let lw = root.join("LockWatch");
        let tk = root.join("ToolKeeper");
        std::fs::create_dir_all(&lw).unwrap();
        std::fs::create_dir_all(&tk).unwrap();
        // 片方の exe だけでは exe の形とみなさない
        std::fs::write(lw.join("lockwatch.exe"), "").unwrap();
        assert!(super::super::lockwatch::resolve(&lw.to_string_lossy()).is_err());
        std::fs::write(lw.join("lockwatchw.exe"), "").unwrap();
        assert!(super::super::lockwatch::resolve(&lw.to_string_lossy()).is_ok());
        std::fs::write(tk.join("toolkeeper.exe"), "").unwrap();
        std::fs::write(tk.join("toolkeeperw.exe"), "").unwrap();
        assert!(super::super::toolkeeper::resolve(&tk.to_string_lossy()).is_ok());
        let _ = std::fs::remove_dir_all(&root);
    }
}
