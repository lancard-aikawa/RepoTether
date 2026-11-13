//! Offstage (同じリポジトリの apps/offstage) とのつなぎ。
//!
//! RepoTether は Offstage の記録 (index.sqlite) には触らず、Offstage が取り込むたびに書く
//! `%USERPROFILE%\.offstage\summary.json` (リポジトリのパスと印の数だけ。ファイル名は無い) を読む。
//! 起動時に `offstage.exe --refresh` を裏で走らせて (窓は出ない)、終わったら読み直す。
//! Offstage が見つからなければ何もしない (設定は要らない)。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::util;

/// summary.json の版 (Offstage の offstage_core::summary::SUMMARY_VERSION と同じ)
const SUMMARY_VERSION: u32 = 1;

/// リポジトリ 1 つの印の数
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Marks {
    #[serde(default)]
    pub missing: i64,
    #[serde(default)]
    pub moved: i64,
    #[serde(default)]
    pub secrets_sent: i64,
    #[serde(default)]
    pub gone: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SummaryFile {
    version: u32,
    generated_at: i64,
    #[serde(default)]
    repos: BTreeMap<String, Marks>,
}

/// 画面に渡す形
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// 見つけた Offstage。None なら Offstage は無い (ボタンも印も出さない)
    pub exe: Option<String>,
    /// summary.json を書いた時刻 (UNIX 秒)。まだ取り込んでいなければ None
    pub generated_at: Option<i64>,
    /// リポジトリのパス → 印の数 (印のあるものだけ)
    pub repos: BTreeMap<String, Marks>,
}

/// Offstage の exe を探す。順に: 環境変数 OFFSTAGE_EXE、RepoTether と同じフォルダ (同じリポジトリでビルドすると並ぶ)、
/// インストーラーの入れ先 (%LOCALAPPDATA%\Offstage・%ProgramFiles%\Offstage)
pub fn locate() -> Option<PathBuf> {
    let mut cands: Vec<PathBuf> = Vec::new();
    if let Some(p) = std::env::var_os("OFFSTAGE_EXE").filter(|v| !v.is_empty()) {
        cands.push(PathBuf::from(p));
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        cands.push(dir.join("offstage.exe"));
    }
    for var in ["LOCALAPPDATA", "ProgramFiles"] {
        if let Some(base) = std::env::var_os(var) {
            cands.push(PathBuf::from(base).join("Offstage").join("offstage.exe"));
        }
    }
    cands.into_iter().find(|p| p.is_file())
}

/// summary.json の場所
pub fn summary_path() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(home).join(".offstage").join("summary.json"))
}

/// summary.json を読む。無い・壊れている・版が違うときは None
pub fn read_summary(path: &Path) -> Option<(i64, BTreeMap<String, Marks>)> {
    let text = std::fs::read_to_string(path).ok()?;
    let s: SummaryFile = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    (s.version == SUMMARY_VERSION).then_some((s.generated_at, s.repos))
}

pub fn status() -> Status {
    let Some(exe) = locate() else { return Status::default() };
    let (generated_at, repos) = match summary_path().and_then(|p| read_summary(&p)) {
        Some((t, r)) => (Some(t), r),
        None => (None, BTreeMap::new()),
    };
    Status { exe: Some(exe.to_string_lossy().into_owned()), generated_at, repos }
}

/// `offstage.exe --refresh` を走らせて終わるまで待つ (窓は出ない)。終了コードが 0 でなければ Err
pub fn run_refresh(exe: &Path) -> Result<(), String> {
    let mut cmd = Command::new(exe);
    cmd.arg("--refresh");
    util::hide_window(&mut cmd);
    let st = cmd.status().map_err(|e| format!("Offstage を起動できません: {e}"))?;
    if st.success() {
        Ok(())
    } else {
        Err(format!("Offstage の取り込みが失敗しました (終了コード {:?})", st.code()))
    }
}

/// Offstage の画面を開く。repo を渡せば、そのリポジトリで絞った状態で開く
pub fn open(exe: &Path, repo: Option<&str>) -> Result<(), String> {
    let mut cmd = Command::new(exe);
    if let Some(r) = repo {
        cmd.arg("--repo").arg(r);
    }
    cmd.spawn().map(|_| ()).map_err(|e| format!("Offstage を起動できません: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_summary_and_rejects_other_versions() {
        let dir = std::env::temp_dir().join(format!("repotether-offstage-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("summary.json");
        std::fs::write(
            &p,
            r#"{"version":1,"generatedAt":100,"repos":{"C:\\Demo\\a":{"missing":2,"moved":0,"secretsSent":1,"gone":false}}}"#,
        )
        .unwrap();
        let (t, repos) = read_summary(&p).unwrap();
        assert_eq!(t, 100);
        assert_eq!(repos["C:\\Demo\\a"], Marks { missing: 2, moved: 0, secrets_sent: 1, gone: false });
        std::fs::write(&p, r#"{"version":2,"generatedAt":1,"repos":{}}"#).unwrap();
        assert!(read_summary(&p).is_none());
        std::fs::write(&p, "not json").unwrap();
        assert!(read_summary(&p).is_none());
        assert!(read_summary(&dir.join("none.json")).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
