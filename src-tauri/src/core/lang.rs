//! リポジトリの言語を、置いてあるファイルから見当を付ける。git は使わない。
//!
//! 根と、その 1 つ下のフォルダだけを見る (依存やビルド成果物のフォルダは見ない)。
//! - 目印のファイル (composer.json → PHP、Cargo.toml → Rust など) があれば、その言語
//! - 拡張子で数えて、3 つ以上あれば、その言語。根に直接あるものは 1 つで 3 つと数える
//!   (Roundcube のプラグインは根に PHP が 1 つだけで、JS の方が多い)。
//!   vite.config.js のような設定ファイルは数えない。どれにも当たらなければ一番多いものを 1 つ
//!
//! 言語の名前は GitHub の表記にそろえる (リモート一覧の language と混ぜて使うので)。

use std::collections::HashMap;
use std::path::Path;

/// 根か 1 つ下にあれば、その言語とみなすファイル
const MARKERS: &[(&str, &str)] = &[
    ("composer.json", "PHP"),
    ("Cargo.toml", "Rust"),
    ("go.mod", "Go"),
    ("pyproject.toml", "Python"),
    ("requirements.txt", "Python"),
    ("setup.py", "Python"),
    ("Pipfile", "Python"),
    ("tsconfig.json", "TypeScript"),
    ("svelte.config.js", "Svelte"),
    ("pom.xml", "Java"),
    ("build.gradle", "Java"),
    ("build.gradle.kts", "Kotlin"),
    ("Gemfile", "Ruby"),
    ("pubspec.yaml", "Dart"),
    ("Package.swift", "Swift"),
    ("project.godot", "GDScript"),
];

/// 拡張子 (小文字) → 言語
const EXTENSIONS: &[(&str, &str)] = &[
    ("php", "PHP"),
    ("rs", "Rust"),
    ("go", "Go"),
    ("py", "Python"),
    ("js", "JavaScript"),
    ("mjs", "JavaScript"),
    ("cjs", "JavaScript"),
    ("jsx", "JavaScript"),
    ("ts", "TypeScript"),
    ("mts", "TypeScript"),
    ("tsx", "TypeScript"),
    ("svelte", "Svelte"),
    ("vue", "Vue"),
    ("java", "Java"),
    ("kt", "Kotlin"),
    ("cs", "C#"),
    ("csproj", "C#"),
    ("sln", "C#"),
    ("cpp", "C++"),
    ("cc", "C++"),
    ("cxx", "C++"),
    ("hpp", "C++"),
    ("c", "C"),
    ("rb", "Ruby"),
    ("swift", "Swift"),
    ("dart", "Dart"),
    ("gd", "GDScript"),
    ("lua", "Lua"),
    ("ps1", "PowerShell"),
    ("sh", "Shell"),
];

/// 拡張子だけで言語とみなす数
const MIN_FILES: u32 = 3;
/// 根に直接あるファイルの重み
const ROOT_WEIGHT: u32 = 3;
/// 1 つのフォルダで見るファイルの上限 (大きなフォルダで時間をかけないように)
const MAX_ENTRIES: usize = 2000;

/// 目印のファイルで決まった言語を先に、あとは拡張子の数が多い順
pub fn detect(repo: &Path) -> Vec<String> {
    let mut markers: Vec<&str> = vec![];
    let mut counts: HashMap<&str, u32> = HashMap::new();
    let mut subdirs = vec![];
    scan_dir(repo, &mut markers, &mut counts, Some(&mut subdirs), ROOT_WEIGHT);
    for d in subdirs {
        scan_dir(&d, &mut markers, &mut counts, None, 1);
    }
    // package.json は TS でも置くので、TS が見つからないときだけ JavaScript とする
    if markers.contains(&"JavaScript") && (markers.contains(&"TypeScript") || counts.contains_key("TypeScript")) {
        markers.retain(|l| *l != "JavaScript");
    }

    let mut by_count: Vec<(&str, u32)> = counts.into_iter().collect();
    by_count.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let mut out: Vec<&str> = markers;
    for (l, n) in &by_count {
        if *n >= MIN_FILES && !out.contains(l) {
            out.push(l);
        }
    }
    if out.is_empty() {
        out.extend(by_count.first().map(|(l, _)| *l));
    }
    out.into_iter().map(String::from).collect()
}

fn scan_dir<'a>(
    dir: &Path,
    markers: &mut Vec<&'a str>,
    counts: &mut HashMap<&'a str, u32>,
    mut subdirs: Option<&mut Vec<std::path::PathBuf>>,
    weight: u32,
) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.filter_map(|e| e.ok()).take(MAX_ENTRIES) {
        let Ok(t) = e.file_type() else { continue };
        let name = e.file_name();
        let name = name.to_string_lossy();
        if t.is_dir() {
            if let Some(ds) = subdirs.as_deref_mut() {
                if !name.starts_with('.') && !super::discover::SKIP_DIRS.iter().any(|s| s.eq_ignore_ascii_case(&name)) {
                    ds.push(e.path());
                }
            }
            continue;
        }
        if !t.is_file() {
            continue;
        }
        if name == "package.json" {
            push_marker(markers, "JavaScript");
        }
        if let Some((_, l)) = MARKERS.iter().find(|(m, _)| *m == name) {
            push_marker(markers, l);
        }
        let Some((stem, ext)) = name.rsplit_once('.') else { continue };
        if stem.ends_with(".config") {
            continue;
        }
        let ext = ext.to_ascii_lowercase();
        if let Some((_, l)) = EXTENSIONS.iter().find(|(x, _)| *x == ext) {
            *counts.entry(l).or_default() += weight;
        }
    }
}

fn push_marker<'a>(markers: &mut Vec<&'a str>, l: &'a str) {
    if !markers.contains(&l) {
        markers.push(l);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(root: &Path, files: &[&str]) {
        let _ = std::fs::remove_dir_all(root);
        for f in files {
            let p = root.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, "").unwrap();
        }
    }

    #[test]
    fn detects_by_markers_and_extensions() {
        let base = std::env::temp_dir().join("repotether-lang-test");

        // composer が無い昔ながらの PHP。public の下に少しだけ
        let old_php = base.join("old-php");
        make(&old_php, &["public/index.php", "README.md"]);
        assert_eq!(detect(&old_php), ["PHP"]);

        // Laravel: vendor の中は見ない。設定ファイルの JS は数えない
        let laravel = base.join("laravel");
        make(&laravel, &["composer.json", "package.json", "vite.config.js", "artisan", "vendor/a/b.php", "app/User.php"]);
        assert_eq!(detect(&laravel), ["PHP", "JavaScript"]);

        // Roundcube のプラグイン: 根に PHP が 1 つ、JS がいくつか
        let plugin = base.join("rc-plugin");
        make(&plugin, &["filters.php", "filters.js", "localization/en_US.inc", "skins/a.js", "skins/b.js", "skins/c.js"]);
        assert_eq!(detect(&plugin), ["JavaScript", "PHP"]);

        // Tauri + TypeScript: package.json があっても TS なら JavaScript にしない
        let tauri = base.join("tauri");
        make(&tauri, &["package.json", "tsconfig.json", "src-tauri/Cargo.toml", "node_modules/x/index.js"]);
        assert_eq!(detect(&tauri), ["TypeScript", "Rust"]);

        // 拡張子だけ: 3 つ以上のものを数の多い順に。下のフォルダの 2 つ以下は落とす
        let mixed = base.join("mixed");
        make(&mixed, &["a.py", "b.py", "x/e.go", "x/f.go", "x/g.go", "x/run.sh", "x/build.sh"]);
        assert_eq!(detect(&mixed), ["Python", "Go"]);

        let empty = base.join("empty");
        make(&empty, &["notes.md"]);
        assert!(detect(&empty).is_empty());
    }
}
