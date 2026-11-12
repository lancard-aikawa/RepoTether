//! Offstage が通信しないことを確かめる (docs/offstage/concept.md)。
//! 1. 依存に通信するクレートが無い (Tauri 自身が使う tokio などは除く)
//! 2. 画面 (WebView) は CSP で ipc: 以外につながない
//! 3. 画面に渡す権限は core:default だけ

use serde_json::Value;
use std::process::Command;

const NETWORK_CRATES: &[&str] = &[
    "reqwest", "hyper", "ureq", "isahc", "curl", "surf", "attohttpc", "native-tls", "rustls",
    "openssl", "h2", "tungstenite", "tauri-plugin-http", "tauri-plugin-opener", "tauri-plugin-shell",
    "tauri-plugin-updater", "tauri-plugin-fs", "tauri-plugin-websocket", "tauri-plugin-upload",
];

#[test]
fn no_network_crates_in_dependency_tree() {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = Command::new(cargo)
        .args(["tree", "-p", "offstage", "-e", "normal,build", "--prefix", "none", "--offline"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo tree");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let tree = String::from_utf8_lossy(&out.stdout);
    let names: Vec<&str> = tree.lines().filter_map(|l| l.split_whitespace().next()).collect();
    assert!(names.contains(&"tauri"), "cargo tree の出力が想定と違う");
    let found: Vec<&&str> = NETWORK_CRATES.iter().filter(|c| names.contains(c)).collect();
    assert!(found.is_empty(), "通信するクレートが依存に入った: {found:?}");
}

fn read_json(rel: &str) -> Value {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

#[test]
fn webview_cannot_reach_the_network() {
    let conf = read_json("tauri.conf.json");
    let csp = &conf["app"]["security"]["csp"];
    assert_eq!(csp["connect-src"], "ipc: http://ipc.localhost");
    assert_eq!(csp["default-src"], "'self'");
    assert_eq!(csp["script-src"], "'self'");
    for (k, v) in csp.as_object().unwrap() {
        let v = v.as_str().unwrap();
        // ipc.localhost は Windows の IPC の口。それ以外の http(s) を許さない
        let rest = v.replace("http://ipc.localhost", "");
        assert!(!rest.contains("http:") && !rest.contains("https:") && !rest.contains('*'), "{k}: {v}");
    }
    assert!(conf["build"].get("devUrl").is_none(), "開発サーバを読まない (画面は ui/ をそのまま埋め込む)");
    assert!(conf["app"]["security"].get("dangerousDisableAssetCspModification").is_none());
}

#[test]
fn only_core_permissions() {
    let cap = read_json("capabilities/default.json");
    assert_eq!(cap["permissions"], serde_json::json!(["core:default"]));
}

/// 同梱したライブラリ (ui/vendor/README.md)。中身が変わったら落とす
const VENDOR: &[(&str, &str)] = &[
    ("marked.umd.js", "b147274a9ce27d17276587167e49483d719f6893eeca3a3667a59797661d3556"),
    ("purify.min.js", "cd37898f9782adf54820c413191b5691803f0f137c799d6edcfd38523b4c2c31"),
];

#[test]
fn vendored_scripts_are_the_pinned_ones() {
    use sha2::{Digest, Sha256};
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui/vendor");
    let mut scripts: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".js"))
        .collect();
    scripts.sort();
    assert_eq!(scripts, VENDOR.iter().map(|(n, _)| n.to_string()).collect::<Vec<_>>());
    for (name, want) in VENDOR {
        let bytes = std::fs::read(dir.join(name)).unwrap();
        let got: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(&got, want, "{name} が変わった");
        let text = String::from_utf8_lossy(&bytes);
        for bad in ["fetch(", "XMLHttpRequest", "WebSocket", "sendBeacon"] {
            assert!(!text.contains(bad), "{name} に {bad} がある");
        }
    }
    // 画面が読み込むスクリプトは、自分のものと同梱したものだけ
    let html = std::fs::read_to_string(dir.join("../index.html")).unwrap();
    let srcs: Vec<&str> = html.split("<script src=\"").skip(1).map(|s| &s[..s.find('"').unwrap()]).collect();
    assert_eq!(srcs, vec!["vendor/purify.min.js", "vendor/marked.umd.js", "app.js"]);
}

#[test]
fn ui_loads_nothing_from_outside() {
    let ui = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui");
    for e in std::fs::read_dir(&ui).unwrap().flatten() {
        let text = std::fs::read_to_string(e.path()).unwrap_or_default();
        for bad in ["http://", "https://", "//cdn", "fetch(", "XMLHttpRequest", "WebSocket", "sendBeacon"] {
            assert!(!text.contains(bad), "{} に {bad} がある", e.path().display());
        }
    }
}
