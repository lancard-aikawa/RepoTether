//! プロジェクトのアイコン。フォルダの中からアプリのアイコンらしい画像を探す。
//! 全体をたどると遅いので、よくある置き場所と、決まったフォルダの直下だけを見る。

use std::path::{Path, PathBuf};

/// これより大きい画像はアイコンとして使わない
pub const MAX_BYTES: u64 = 1024 * 1024;

/// フレームワークごとの決まった置き場所。先にあるものほど優先する
const KNOWN: &[&str] = &[
    // Tauri
    "src-tauri/icons/128x128@2x.png",
    "src-tauri/icons/icon.png",
    "src-tauri/icons/128x128.png",
    // Flutter (flutter_launcher_icons / web / Windows)
    "assets/icon/icon.png",
    "web/icons/Icon-192.png",
    "web/favicon.png",
    "windows/runner/resources/app_icon.ico",
    // Android
    "app/src/main/res/mipmap-xxxhdpi/ic_launcher.png",
    "app/src/main/res/mipmap-xxhdpi/ic_launcher.png",
    "android/app/src/main/res/mipmap-xxxhdpi/ic_launcher.png",
    "android/app/src/main/res/mipmap-xxhdpi/ic_launcher.png",
    // Electron (electron-builder)
    "build/icon.png",
    "resources/icon.png",
    // Next.js (App Router)
    "src/app/icon.png",
    "app/icon.png",
    "src/app/icon.svg",
    "app/icon.svg",
    // Web (Vite / SvelteKit / CRA など)
    "public/apple-touch-icon.png",
    "public/icon.png",
    "public/icon.svg",
    "public/logo.png",
    "public/logo.svg",
    "public/favicon.svg",
    "public/favicon.png",
    "static/icon.png",
    "static/favicon.svg",
    "static/favicon.png",
    "public/favicon.ico",
    "static/favicon.ico",
    "src/app/favicon.ico",
    "app/favicon.ico",
];

/// 名前で探すフォルダ (直下だけ見る)。先にあるものほど優先する
const DIRS: &[&str] = &[
    "",
    "assets",
    "assets/icons",
    "resources",
    "resources/icons",
    "res",
    "icons",
    "icon",
    "images",
    "img",
    "media",
    "branding",
    "packaging",
    "packaging/icons",
    "static",
    "static/icons",
    "public",
    "public/icons",
    ".github",
    "docs",
];

/// パッケージのフォルダ (リポジトリ名と同じ名前。Python などで `<名前>/static` に置く) の中で探すフォルダ
const PACKAGE_DIRS: &[&str] = &["static", "static/icons", "assets", "icons"];

/// 比べるための名前 (小文字、- _ 空白を除く)
fn norm(s: &str) -> String {
    s.chars().filter(|c| !matches!(c, '-' | '_' | ' ')).flat_map(char::to_lowercase).collect()
}

/// 探すフォルダの一覧 (dir からの相対)。直下か src/ の下に、リポジトリ名と同じフォルダがあればその中も
fn search_dirs(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = DIRS.iter().map(PathBuf::from).collect();
    let Some(name) = dir.file_name().map(|n| norm(&n.to_string_lossy())) else { return out };
    for base in ["", "src"] {
        let Ok(entries) = std::fs::read_dir(dir.join(base)) else { continue };
        for e in entries.flatten() {
            if e.file_type().is_ok_and(|t| t.is_dir()) && norm(&e.file_name().to_string_lossy()) == name {
                let pkg = Path::new(base).join(e.file_name());
                out.extend(PACKAGE_DIRS.iter().map(|d| pkg.join(d)));
            }
        }
    }
    out
}

/// 画像の拡張子。小さいほど優先
fn ext_rank(ext: &str) -> Option<u8> {
    match ext {
        "png" | "svg" | "webp" => Some(0),
        "ico" => Some(1),
        "jpg" | "jpeg" | "gif" => Some(2),
        _ => None,
    }
}

/// ファイル名 (拡張子を除く) がアイコンらしいか。小さいほど優先。
/// 同じ種類なら、名前ちょうど (icon) を飾り付き (icon-maskable など) より先にする
fn name_rank(stem: &str) -> Option<u8> {
    let s = stem.to_lowercase();
    let starts = |w: &str| s.starts_with(&format!("{w}-")) || s.starts_with(&format!("{w}_")) || s.starts_with(&format!("{w}@"));
    let kinds: [&[&str]; 3] = [&["icon", "app_icon", "app-icon", "appicon"], &["logo"], &["favicon"]];
    kinds.iter().enumerate().find_map(|(i, words)| {
        let i = i as u8 * 2;
        if words.iter().any(|w| s == *w) {
            Some(i)
        } else if words.iter().any(|w| starts(w)) {
            Some(i + 1)
        } else {
            None
        }
    })
}

fn usable(p: &Path) -> bool {
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.len() > 0 && m.len() <= MAX_BYTES)
}

/// dir の中のアイコンらしい画像。無ければ None
pub fn find(dir: &Path) -> Option<PathBuf> {
    for k in KNOWN {
        let p = dir.join(k);
        if usable(&p) {
            return Some(p);
        }
    }
    let mut best: Option<((u8, u8, usize), PathBuf)> = None;
    for (di, d) in search_dirs(dir).iter().enumerate() {
        // icons / icon というフォルダの中なら、どんな名前の画像でもアイコンとみなす
        let icon_dir = d.file_name().is_some_and(|n| matches!(n.to_string_lossy().to_lowercase().as_str(), "icons" | "icon"));
        let Ok(entries) = std::fs::read_dir(dir.join(d)) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            let (Some(stem), Some(ext)) = (p.file_stem(), p.extension()) else { continue };
            let n = name_rank(&stem.to_string_lossy()).or(if icon_dir { Some(6) } else { None });
            let (Some(n), Some(x)) = (n, ext_rank(&ext.to_string_lossy().to_lowercase())) else {
                continue;
            };
            let rank = (n, x, di);
            if best.as_ref().is_none_or(|(r, _)| rank < *r) && usable(&p) {
                best = Some((rank, p));
            }
        }
    }
    best.map(|(_, p)| p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("repotether-icon-test").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn put(dir: &Path, rel: &str) {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"x").unwrap();
    }

    #[test]
    fn known_place_wins() {
        let d = tmp("known");
        put(&d, "logo.png");
        put(&d, "src-tauri/icons/icon.png");
        assert_eq!(find(&d), Some(d.join("src-tauri/icons/icon.png")));
    }

    #[test]
    fn prefers_icon_over_logo_over_favicon_and_png_over_ico() {
        let d = tmp("rank");
        put(&d, "favicon.png");
        put(&d, "assets/logo.png");
        assert_eq!(find(&d), Some(d.join("assets/logo.png")));
        put(&d, "images/icon.ico");
        assert_eq!(find(&d), Some(d.join("images/icon.ico")));
        put(&d, "packaging/icon.png");
        assert_eq!(find(&d), Some(d.join("packaging/icon.png")));
    }

    #[test]
    fn any_image_in_icons_folder_and_package_static() {
        let d = tmp("glosspop");
        put(&d, "packaging/icons/glosspop-app.ico");
        assert_eq!(find(&d), Some(d.join("packaging/icons/glosspop-app.ico")));
        // 名前がアイコンらしいものが先
        put(&d, "glosspop/static/favicon.svg");
        assert_eq!(find(&d), Some(d.join("glosspop/static/favicon.svg")));
        let d = tmp("Gloss-PopApp");
        put(&d, "src/gloss_popapp/static/icon-maskable.svg");
        put(&d, "src/gloss_popapp/static/icon.svg");
        assert_eq!(find(&d), Some(d.join("src/gloss_popapp/static/icon.svg")));
    }

    #[test]
    fn ignores_other_names_empty_and_large_files() {
        let d = tmp("ignore");
        put(&d, "screenshot.png");
        put(&d, "iconography.txt");
        std::fs::write(d.join("icon.png"), b"").unwrap();
        std::fs::write(d.join("logo.svg"), vec![b' '; (MAX_BYTES + 1) as usize]).unwrap();
        assert_eq!(find(&d), None);
    }

    #[test]
    fn this_repo_uses_tauri_icon() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        assert_eq!(find(root), Some(root.join("src-tauri/icons/128x128@2x.png")));
    }
}
