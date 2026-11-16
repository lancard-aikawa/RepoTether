//! gitignore されたファイルを「生成物 / 機密の疑い / データ / 道具のデータ / 個人」に分ける規則。
//!
//! 中身は読まない。パスの名前だけで決める。人の上書き (未分類の振り分け) はこの上に重ねる。
//!
//! 決める順:
//! 1. 途中に生成物のフォルダがあれば生成物 (その下の機密らしい名前も含めて、作り直せるものとして扱う)
//! 2. 生成物のファイル名
//! 3. 機密らしい名前 (道具のデータのフォルダの中でも機密の疑いにする。漏れの印を落とさないため)
//! 4. 途中に `data` のフォルダがあればデータ (道具のデータのフォルダより先に見る)
//! 5. 途中に道具のデータのフォルダがある・道具のデータのファイル名
//! 6. それ以外は個人
//!
//! 名前で分からないものは、洗い出しのとき ([`crate::scan`]) に目印のファイル (`pyvenv.cfg` など) と
//! ファイルの数 (多すぎれば道具のデータの 1 件に畳む) で見分ける

/// 分け方の版。規則を変えたら上げる。記録にはこれを残し、より新しい版で書かれた記録は古い Offstage では取り込まない
/// (RepoTether に同梱した古い offstage.exe が、裏の取り込みで分け方を古いものに戻さないように)。
/// 1: Offstage 0.1.2 まで (記録に版は書いていない) / 2: データの区分・置き場所での判定・数で畳む
pub const RULES_VERSION: i64 = 2;

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
    /// `data/` の中のもの。取ってきたデータ・アプリが作ったもの・セーブデータなどが混じり、
    /// 作り直せるかどうかが名前では分からないので、個人とも道具のデータとも分けて見せる
    Data,
    /// 手元だけに置く、人が書いた・持ち込んだファイル
    Personal,
}

impl Class {
    pub fn label(self) -> &'static str {
        match self {
            Class::Generated => "generated",
            Class::Secret => "secret",
            Class::ToolData => "tooldata",
            Class::Data => "data",
            Class::Personal => "personal",
        }
    }

    pub fn from_label(s: &str) -> Option<Class> {
        [Class::Generated, Class::Secret, Class::ToolData, Class::Data, Class::Personal]
            .into_iter()
            .find(|c| c.label() == s)
    }
}

/// この名前のフォルダの中は道具のデータ
const TOOL_DIRS: &[&str] = &[
    "log", "logs", "history", "vault", "results", "reports", "snapshots",
    // 管理しているフォルダの中で無視されているものは、道具 (拡張・エディタ・Claude Code) が置いたもの
    ".claude", ".github", ".vscode",
    // 道具が撮った画面・書き出した結果・自動のバックアップ
    "shots", "_shots", "screenshots", "_check", "_checks", "output", "outputs", "backup", "backups",
    // 道具の置き場と、Claude が調べものをするときに作る場所。中身は git で管理していて、
    // 無視されて残っているのは作業の副産物 (取ってきたデータ・試しの出力・切り出した部品)
    "tools", "research",
    // アプリが書いた設定・状態 (無視されている config は、手で書く設定の見本ではなく、動かして溜まったもの)
    "config",
    // アプリが保存したもの (Laravel の storage/ など。手で書いたものではないが、作り直せないこともある)
    "storage",
];

/// この拡張子のファイルは道具のデータ
const TOOL_EXTS: &[&str] = &["log", "jsonl", "sqlite", "sqlite3", "db", "bak"];

/// この名前のファイルは道具のデータ
const TOOL_FILES: &[&str] = &["state.json"];

/// この名前のフォルダは中身ごと生成物とみなす
const GENERATED_DIRS: &[&str] = &[
    "node_modules", ".venv", "venv", "target", "build", "dist", "out", "__pycache__",
    ".svelte-kit", "gen", ".dart_tool", ".wrangler", ".playwright-mcp", "test-results",
    ".pytest_cache", ".ruff_cache", ".mypy_cache", ".gradle", ".idea", ".next", ".nuxt",
    ".astro", ".turbo", ".parcel-cache", "coverage", "cache", ".cache", "playwright-report",
    // Python の入れ物、Godot・Android NDK の作業場所、入れ子のリポジトリの中身、その他の道具の作業場所
    "site-packages", "__pypackages__", ".tox", ".nox", ".eggs", ".godot", ".cxx", ".externalnativebuild",
    ".git", ".expo", ".angular", ".vite", ".nx", ".terraform", ".serverless",
    // Flutter が毎回作り直す (ios/Flutter/ephemeral など)
    "ephemeral",
    // 取ってきた学習済みのモデル (Tesseract の言語データなど)
    "tessdata", "tessdata_best", "tessdata_fast",
];

/// この拡張子のファイルは生成物
/// (学習済みのモデルの重みは取り直せるので生成物)
const GENERATED_EXTS: &[&str] = &["vsix", "pid", "db-wal", "db-shm", "pyc", "iml", "onnx", "traineddata", "gguf", "safetensors", "ckpt", "pth"];

/// この名前のファイルは生成物 (Flutter が作り直すものを含む)
const GENERATED_FILES: &[&str] = &[
    ".ds_store", "thumbs.db", "desktop.ini", "local.properties", "generated.xcconfig",
    "flutter_export_environment.sh", ".flutter-plugins", ".flutter-plugins-dependencies",
    // Gradle の wrapper (gradle wrapper・flutter create が作る)
    "gradlew", "gradlew.bat", "gradle-wrapper.jar",
];

/// この名前で始まるファイルは生成物
const GENERATED_PREFIXES: &[&str] = &["generatedpluginregistrant."];

/// この名前で終わるファイルは生成物 (ソースマップ)
const GENERATED_SUFFIXES: &[&str] = &[".js.map", ".css.map", ".mjs.map"];

/// 名前にこの語を含むファイルは機密の疑い (小文字で比べる)
const SECRET_WORDS: &[&str] = &[
    "token", "secret", "credential", "password", "passwd", "service-account",
    "serviceaccount", "apikey", "api_key",
];

/// この名前のファイルは機密の疑い (小文字で比べる)
const SECRET_FILES: &[&str] = &[
    ".env", ".dev.vars", "google-services.json", "googleservice-info.plist", ".npmrc",
    ".pypirc", ".netrc", "id_rsa", "id_ed25519", "id_ecdsa",
    // Android の署名の鍵のパスワード
    "key.properties",
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
    let in_data_dir = parents.iter().any(|p| is_data_dir(p));
    if is_dir {
        return if is_generated_dir(last) {
            Class::Generated
        } else if in_data_dir || is_data_dir(last) {
            Class::Data
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
        || GENERATED_SUFFIXES.iter().any(|s| name.ends_with(s))
    {
        return Class::Generated;
    }
    if secret_rule(&name, ext).is_some() {
        return Class::Secret;
    }
    if in_data_dir {
        return Class::Data;
    }
    if in_tool_dir || TOOL_FILES.contains(&name.as_str()) || TOOL_EXTS.contains(&ext) {
        return Class::ToolData;
    }
    Class::Personal
}

/// この名前のフォルダの中はデータ
fn is_data_dir(name: &str) -> bool {
    name.eq_ignore_ascii_case("data")
}

fn is_tool_dir(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    // `.cl_preprocessor_backups` のような自動のバックアップ
    TOOL_DIRS.contains(&lower.as_str()) || lower.ends_with("_backups") || lower.ends_with("-backups")
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
        // `ytdlp-venv`・`box_venv`・`.venv312` のような名前の venv
        || lower.ends_with("-venv")
        || lower.ends_with("_venv")
        || lower.starts_with(".venv")
        // `azuma_cache`・`.ocr_cache`・`model-cache` のようなキャッシュ
        || lower.ends_with("_cache")
        || lower.ends_with("-cache")
        // `rapidocr_models` のような、取ってきたモデルの置き場
        || lower.ends_with("_models")
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
        assert_eq!(classify("functions/lib/api.js.map", false), Class::Generated);
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
        assert_eq!(classify("app/reports/2026-11.html", false), Class::ToolData);
        assert_eq!(classify("sandbox/logs/run.txt", false), Class::ToolData);
        assert_eq!(classify(".claude/settings.local.json", false), Class::ToolData);
        assert_eq!(classify(".github/modernize/plan.md", false), Class::ToolData);
        assert_eq!(classify("app/state.json", false), Class::ToolData);
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
    fn flutter_scaffold_and_app_state() {
        for p in [
            "ios/Flutter/ephemeral/flutter_lldb_helper.py",
            "macos/Flutter/ephemeral/Flutter-Generated.xcconfig",
            "android/gradlew",
            "android/gradlew.bat",
            "android/gradle/wrapper/gradle-wrapper.jar",
            "android/farmsim_android.iml",
        ] {
            assert_eq!(classify(p, false), Class::Generated, "{p}");
        }
        assert_eq!(classify("android/key.properties", false), Class::Secret);
        assert_eq!(classify("config/splitter_161e8ca7.json", false), Class::ToolData);
        assert_eq!(classify("config/api_token.json", false), Class::Secret);
    }

    #[test]
    fn data_folders_are_data() {
        for p in [
            "data/glossary/terms.csv",
            "data/reports/2026-11.html", // 道具のデータのフォルダより data が先
            "data/state.json",
            "research/data/premodern/src/a.png",
            "sconv/Data/WKLIB/x.dat",
        ] {
            assert_eq!(classify(p, false), Class::Data, "{p}");
        }
        assert_eq!(classify("data", true), Class::Data);
        assert_eq!(classify("data/saves", true), Class::Data);
        // 生成物と機密の疑いは data の中でもそのまま
        assert_eq!(classify("data/__pycache__/a.pyc", false), Class::Generated);
        assert_eq!(classify("data/api_token.json", false), Class::Secret);
        // 名前が data を含むだけのフォルダは当てない
        assert_eq!(classify("metadata/usefile/A01.csv", false), Class::Personal);
        assert_eq!(Class::from_label("data"), Some(Class::Data));
    }

    #[test]
    fn app_storage_and_downloaded_models() {
        assert_eq!(classify("prototype/storage/files/a.pdf", false), Class::ToolData);
        assert_eq!(classify("prototype/storage/db.sqlite.bak-premigrate", false), Class::ToolData);
        for p in [
            "prototype/tessdata_best/jpn.traineddata",
            "prototype/rapidocr_models/keys.txt",
            "models/det.onnx",
            "weights/model.safetensors",
        ] {
            assert_eq!(classify(p, false), Class::Generated, "{p}");
        }
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
    fn venvs_caches_and_tool_workspaces_by_name() {
        for p in [
            "probe/ytdlp-venv/Lib/x.py",
            "sandbox/box_venv/x.py",
            ".venv312/x.py",
            "resources/python-worker/lib/site-packages/numpy/x.py",
            "research/azuma_cache/a.json",
            "tools/.ocr_cache/a.txt",
            "game/.godot/imported/a.ctex",
            "android/app/.cxx/Debug/x.json",
            "gallery/.git/HEAD",
        ] {
            assert_eq!(classify(p, false), Class::Generated, "{p}");
        }
        assert_eq!(classify("probe/ytdlp-venv", true), Class::Generated);
        // 似ているだけの名前は当てない
        assert_eq!(classify("docs/venv-notes.md", false), Class::Personal);
        assert_eq!(classify("sandbox/cache-plan.md", false), Class::Personal);
    }

    #[test]
    fn shots_outputs_and_backups_are_tool_data() {
        for p in [
            "shots/ui/a.png",
            "sandbox/_shots/a.png",
            "prototype/output/r.json",
            "CL/.cl_preprocessor_backups/a.cl",
            "notes/old.md.bak",
            "tools/_check/1_start.png",
            "tools/test_mails/a.eml",
            "bin/tools/temp/x.txt",
            "research/notes/a.png",
        ] {
            assert_eq!(classify(p, false), Class::ToolData, "{p}");
        }
        // 機密らしい名前は、道具の置き場の中でも機密の疑い
        assert_eq!(classify("research/data/api_token.txt", false), Class::Secret);
        // 名前が似ているだけのフォルダは当てない
        assert_eq!(classify("toolset/a.md", false), Class::Personal);
    }

    #[test]
    fn everything_else_is_personal() {
        assert_eq!(classify("sandbox", true), Class::Personal);
        assert_eq!(classify("sandbox/memo.md", false), Class::Personal);
        assert_eq!(classify("notes/glossary/terms.csv", false), Class::Personal);
        assert_eq!(classify("slack/2026-11-12.txt", false), Class::Personal);
        assert_eq!(classify("", false), Class::Personal);
    }
}
