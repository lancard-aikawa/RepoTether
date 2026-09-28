//! Claude Code の設定ファイル (`~/.claude/settings.json`) の cleanupPeriodDays を読み書きする。
//!
//! Claude Code は既定で 30 日より古いセッションのログを起動時に消す。消えると RepoTether の履歴・グラフ・日報からも消えるので、
//! 保存期間を延ばせるようにする。書き込むのはこの 1 項目だけで、ほかの項目は中身も並び順も変えない。

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::util;

const KEY: &str = "cleanupPeriodDays";
/// Claude Code の既定の保存期間 (日)
pub const DEFAULT_DAYS: u32 = 30;
const MAX_DAYS: u32 = 36500;

/// Claude Code の設定の置き場所。CLAUDE_CONFIG_DIR があればそこ、無ければ ~/.claude
pub fn settings_path() -> Option<PathBuf> {
    let dir = match std::env::var_os("CLAUDE_CONFIG_DIR") {
        Some(d) if !d.is_empty() => PathBuf::from(d),
        _ => util::home_dir()?.join(".claude"),
    };
    Some(dir.join("settings.json"))
}

/// 書かれている保存期間。書かれていなければ None (= 既定の 30 日)
pub fn cleanup_days(path: &Path) -> Result<Option<u32>, String> {
    let obj = read(path)?;
    Ok(obj.get(KEY).and_then(|v| v.as_u64()).map(|d| d.min(u32::MAX as u64) as u32))
}

/// 保存期間を書く。None なら項目を消して Claude Code の既定に戻す
pub fn set_cleanup_days(path: &Path, days: Option<u32>) -> Result<(), String> {
    if let Some(d) = days {
        if d == 0 || d > MAX_DAYS {
            return Err(format!("保存期間は 1〜{MAX_DAYS} 日にしてください"));
        }
    }
    let mut obj = read(path)?;
    match days {
        Some(d) => {
            obj.insert(KEY.into(), Value::from(d));
        }
        None => {
            obj.shift_remove(KEY);
        }
    }
    let mut text = serde_json::to_string_pretty(&Value::Object(obj)).map_err(|e| e.to_string())?;
    text.push('\n');

    // 途中で落ちても元のファイルを壊さないよう、隣に書いてから置き換える
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.repotether-tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("{} に書けません: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("{} を置き換えられません: {e}", path.display())
    })
}

/// 設定ファイルを読む。無ければ空。JSON のオブジェクトとして読めなければ、上書きしないようエラーにする
fn read(path: &Path) -> Result<Map<String, Value>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(e) => return Err(format!("{} を読めません: {e}", path.display())),
    };
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) {
        Ok(Value::Object(m)) => Ok(m),
        Ok(_) => Err(format!("{} が JSON のオブジェクトではないので変更しません", path.display())),
        Err(e) => Err(format!("{} を JSON として読めないので変更しません ({e})", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("repotether-claude-settings-test").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("settings.json")
    }

    #[test]
    fn keeps_other_keys_and_their_order() {
        let p = tmp("order");
        std::fs::write(&p, "{\n  \"theme\": \"dark\",\n  \"permissions\": {\"allow\": [\"Bash\"]},\n  \"zeta\": 1\n}\n").unwrap();
        assert_eq!(cleanup_days(&p).unwrap(), None);

        set_cleanup_days(&p, Some(3650)).unwrap();
        assert_eq!(cleanup_days(&p).unwrap(), Some(3650));
        let text = std::fs::read_to_string(&p).unwrap();
        let keys: Vec<&str> = ["theme", "permissions", "zeta", "cleanupPeriodDays"].to_vec();
        let pos: Vec<usize> = keys.iter().map(|k| text.find(&format!("\"{k}\"")).unwrap()).collect();
        assert!(pos.windows(2).all(|w| w[0] < w[1]), "並び順が変わった: {text}");
        assert!(text.contains("\"allow\""));

        // 既定に戻す = 項目を消す
        set_cleanup_days(&p, None).unwrap();
        assert_eq!(cleanup_days(&p).unwrap(), None);
        assert!(!std::fs::read_to_string(&p).unwrap().contains(KEY));
    }

    #[test]
    fn creates_file_and_refuses_broken_json() {
        let p = tmp("create");
        std::fs::remove_file(&p).ok();
        set_cleanup_days(&p, Some(90)).unwrap();
        assert_eq!(cleanup_days(&p).unwrap(), Some(90));

        let broken = tmp("broken");
        std::fs::write(&broken, "{ // コメント入り\n \"a\": 1 }").unwrap();
        assert!(set_cleanup_days(&broken, Some(90)).is_err());
        assert_eq!(std::fs::read_to_string(&broken).unwrap(), "{ // コメント入り\n \"a\": 1 }");

        assert!(set_cleanup_days(&p, Some(0)).is_err());
    }
}
