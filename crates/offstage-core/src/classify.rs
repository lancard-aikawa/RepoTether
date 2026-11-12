//! gitignore されたファイルを「生成物 / 機密の疑い / 道具のデータ / 個人」に分ける規則 (初版)。
//!
//! 中身は読まない。パスの名前だけで決める。人の上書き (未分類の振り分け) はこの上に重ねる。
//!
//! 決める順:
//! 1. 途中に生成物のフォルダがあれば生成物 (その下の機密らしい名前も含めて、作り直せるものとして扱う)
//! 2. 生成物のファイル名
//! 3. 機密らしい名前 (道具のデータのフォルダの中でも機密の疑いにする。漏れの印を落とさないため)
//! 4. 途中に道具のデータのフォルダがある・道具のデータのファイル名
//! 5. それ以外は個人

/// 分類の結果
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Class {
    /// 作り直せるもの。索引に入れない
    Generated,
    /// 名前から機密と疑われるもの。個人として扱い、中身は許可があっても読まない
    Secret,
    /// 道具が書いて溜めたもの (セッションの写し・ログ・結果・状態)。作り直せないこともあるので索引には入れるが、
    /// 人が書いた個人のファイルと分けて見せる
    ToolData,
    /// 手元だけに置く、人が書いた・持ち込んだファイル
    Personal,
}

impl Class {
    pub fn label(self) -> &'static str {
        match self {
            Class::Generated => "generated",
            Class::Secret => "secret",
            Class::ToolData => "tooldata",
            Class::Personal => "personal",
        }
    }

    pub fn from_label(s: &str) -> Option<Class> {
        [Class::Generated, Class::Secret, Class::ToolData, Class::Personal]
            .into_iter()
            .find(|c| c.label() == s)
    }
}

/// この名前のフォルダの中は道具のデータ
const TOOL_DIRS: &[&str] = &[
    "log", "logs", "history", "vault", "results", "reports", "snapshots",
    // 管理しているフォルダの中で無視されているものは、道具 (拡張・エディタ・Claude Code) が置いたもの
    ".claude", ".github", ".vscode",
];

/// この拡張子のファイルは道具のデータ
const TOOL_EXTS: &[&str] = &["log", "jsonl", "sqlite", "sqlite3", "db"];

/// この名前のファイルは道具のデータ
const TOOL_FILES: &[&str] = &["state.json"];

/// この名前のフォルダは中身ごと生成物とみなす
const GENERATED_DIRS: &[&str] = &[
    "node_modules", ".venv", "venv", "target", "build", "dist", "out", "__pycache__",
    ".svelte-kit", "gen", ".dart_tool", ".wrangler", ".playwright-mcp", "test-results",
    ".pytest_cache", ".ruff_cache", ".mypy_cache", ".gradle", ".idea", ".next", ".nuxt",
    ".astro", ".turbo", ".parcel-cache", "coverage", "cache", ".cache", "playwright-report",
];

/// この拡張子のファイルは生成物
const GENERATED_EXTS: &[&str] = &["vsix", "pid", "db-wal", "db-shm", "pyc"];

/// この名前のファイルは生成物 (Flutter が作り直すものを含む)
const GENERATED_FILES: &[&str] = &[
    ".ds_store", "thumbs.db", "desktop.ini", "local.properties", "generated.xcconfig",
    "flutter_export_environment.sh", ".flutter-plugins", ".flutter-plugins-dependencies",
];

/// この名前で始まるファイルは生成物
const GENERATED_PREFIXES: &[&str] = &["generatedpluginregistrant."];

/// 名前にこの語を含むファイルは機密の疑い (小文字で比べる)
const SECRET_WORDS: &[&str] = &[
    "token", "secret", "credential", "password", "passwd", "service-account",
    "serviceaccount", "apikey", "api_key",
];

/// この名前のファイルは機密の疑い (小文字で比べる)
const SECRET_FILES: &[&str] = &[
    ".env", ".dev.vars", "google-services.json", "googleservice-info.plist", ".npmrc",
    ".pypirc", ".netrc", "id_rsa", "id_ed25519", "id_ecdsa",
];

/// この拡張子のファイルは機密の疑い
const SECRET_EXTS: &[&str] = &["pem", "key", "p12", "pfx", "jks", "keystore"];

/// リポジトリの中の相対パス (区切りは `/` でも `\` でもよい) を分ける。
/// `is_dir` はパスがフォルダを指すとき true。
pub fn classify(rel_path: &str, is_dir: bool) -> Class {
    let parts: Vec<&str> = rel_path
        .split(['/', '\\'])
        .filter(|p| !p.is_empty())
        .collect();
    let Some((last, parents)) = parts.split_last() else {
        return Class::Personal;
    };

    // 途中のフォルダに生成物のフォルダがあれば、その下は全部生成物
    if parents.iter().any(|p| is_generated_dir(p)) {
        return Class::Generated;
    }
    let in_tool_dir = parents.iter().any(|p| is_tool_dir(p));
    if is_dir {
        return if is_generated_dir(last) {
            Class::Generated
        } else if in_tool_dir || is_tool_dir(last) {
            Class::ToolData
        } else {
            Class::Personal
        };
    }

    let name = last.to_ascii_lowercase();
    let ext = name.rsplit_once('.').map(|(_, e)| e).unwrap_or("");

    if GENERATED_FILES.contains(&name.as_str())
        || GENERATED_EXTS.contains(&ext)
        || GENERATED_PREFIXES.iter().any(|p| name.starts_with(p))
    {
        return Class::Generated;
    }
    if secret_rule(&name, ext).is_some() {
        return Class::Secret;
    }
    if in_tool_dir || TOOL_FILES.contains(&name.as_str()) || TOOL_EXTS.contains(&ext) {
        return Class::ToolData;
    }
    Class::Personal
}

fn is_tool_dir(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    TOOL_DIRS.contains(&lower.as_str())
}

/// 機密の疑いにした規則 (例 `word:token`・`file:.env`・`ext:pem`)。当たらなければ None。
/// 画面で「なぜこの区分か」を出すのと、ファイル名を見せずに規則を直すのに使う
pub fn secret_rule_of(rel_path: &str) -> Option<String> {
    let last = rel_path.rsplit(['/', '\\']).next().unwrap_or("");
    let name = last.to_ascii_lowercase();
    let ext = name.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    secret_rule(&name, ext)
}

fn is_generated_dir(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    GENERATED_DIRS.contains(&lower.as_str())
}

fn secret_rule(name: &str, ext: &str) -> Option<String> {
    if SECRET_FILES.contains(&name) {
        return Some(format!("file:{name}"));
    }
    // .env.local・.env.production など。見本の .env.example は除く
    if name.starts_with(".env.") && !name.ends_with(".example") && !name.ends_with(".sample") {
        return Some("file:.env.*".into());
    }
    if SECRET_EXTS.contains(&ext) {
        return Some(format!("ext:{ext}"));
    }
    SECRET_WORDS.iter().find(|w| name.contains(*w)).map(|w| format!("word:{w}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_folders_win_over_everything_below() {
        assert_eq!(classify("node_modules", true), Class::Generated);
        assert_eq!(classify("src-tauri/gen", true), Class::Generated);
        assert_eq!(classify("node_modules/pkg/token.js", false), Class::Generated);
        assert_eq!(classify("sandbox/proj/.venv/lib/x.py", false), Class::Generated);
        assert_eq!(classify("app\\build\\out.txt", false), Class::Generated);
    }

    #[test]
    fn generated_files() {
        assert_eq!(classify("open-outside-0.0.1.vsix", false), Class::Generated);
        assert_eq!(classify(".server.pid", false), Class::Generated);
        assert_eq!(classify("api/.preflight.db-wal", false), Class::Generated);
        assert_eq!(classify("docs/Thumbs.db", false), Class::Generated);
    }

    #[test]
    fn secrets_by_name() {
        for p in [
            ".work/api_tokens.json",
            "oauth_refresh_token.json",
            ".dev.vars",
            ".env",
            ".env.local",
            "android/app/google-services.json",
            "keys/server.pem",
            "my-service-account.json",
            "Secrets.txt",
        ] {
            assert_eq!(classify(p, false), Class::Secret, "{p}");
        }
        assert_eq!(classify(".env.example", false), Class::Personal);
    }

    #[test]
    fn tool_data() {
        assert_eq!(classify("vault", true), Class::ToolData);
        assert_eq!(classify("vault/mirror/s1.jsonl", false), Class::ToolData);
        assert_eq!(classify("data/reports/2026-11.html", false), Class::ToolData);
        assert_eq!(classify("sandbox/logs/run.txt", false), Class::ToolData);
        assert_eq!(classify(".claude/settings.local.json", false), Class::ToolData);
        assert_eq!(classify(".github/modernize/plan.md", false), Class::ToolData);
        assert_eq!(classify("data/state.json", false), Class::ToolData);
        assert_eq!(classify("debug.jsonl", false), Class::ToolData);
        assert_eq!(classify("api/.preflight.db", false), Class::ToolData);
    }

    #[test]
    fn secrets_win_inside_tool_data_but_not_inside_generated() {
        assert_eq!(classify("vault/mirror/memory/credentials-note.md", false), Class::Secret);
        assert_eq!(classify("logs/.env", false), Class::Secret);
        assert_eq!(classify("logs/build/token.txt", false), Class::Generated);
    }

    #[test]
    fn flutter_regenerates_these() {
        assert_eq!(classify("android/local.properties", false), Class::Generated);
        assert_eq!(classify("ios/Flutter/Generated.xcconfig", false), Class::Generated);
        assert_eq!(
            classify("android/app/src/main/java/io/flutter/plugins/GeneratedPluginRegistrant.java", false),
            Class::Generated
        );
    }

    #[test]
    fn everything_else_is_personal() {
        assert_eq!(classify("sandbox", true), Class::Personal);
        assert_eq!(classify("sandbox/memo.md", false), Class::Personal);
        assert_eq!(classify("data/glossary/terms.csv", false), Class::Personal);
        assert_eq!(classify("slack/2026-11-12.txt", false), Class::Personal);
        assert_eq!(classify("", false), Class::Personal);
    }
}
