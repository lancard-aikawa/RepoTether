//! offstage-core が通信するクレートに依存していないことを確かめる (docs/offstage/concept.md の決まり)。
//! 依存を足したときに、ここで止まる。

use std::process::Command;

const NETWORK_CRATES: &[&str] = &[
    "reqwest", "hyper", "ureq", "isahc", "curl", "surf", "attohttpc", "tokio", "async-std",
    "native-tls", "rustls", "openssl", "h2", "tungstenite", "mio", "socket2",
    "tauri-plugin-http", "tauri-plugin-opener",
];

#[test]
fn no_network_crates_in_dependency_tree() {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = Command::new(cargo)
        .args(["tree", "-p", "offstage-core", "-e", "normal,build", "--prefix", "none", "--offline"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo tree");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let tree = String::from_utf8_lossy(&out.stdout);
    let names: Vec<&str> = tree.lines().filter_map(|l| l.split_whitespace().next()).collect();
    assert!(names.contains(&"offstage-core"), "cargo tree の出力が想定と違う: {tree}");
    let found: Vec<&&str> = NETWORK_CRATES.iter().filter(|c| names.contains(c)).collect();
    assert!(found.is_empty(), "通信するクレートが依存に入った: {found:?}\n{tree}");
}
