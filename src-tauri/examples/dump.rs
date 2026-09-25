//! アプリを起動せずに取り込みだけを実行し、結果を JSON で書き出す。
//! 使い方: cargo run --release --example dump -- <出力先.json> [--remote]
//! 設定はアプリと同じ %APPDATA%\com.repotether.app\config.json を読む。

use std::path::PathBuf;

use repotether_lib::core::{self, config::Config};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = args.get(1).cloned().unwrap_or_else(|| "snapshot.json".into());
    let with_remote = args.iter().any(|a| a == "--remote");

    let appdata = PathBuf::from(std::env::var_os("APPDATA").expect("APPDATA"));
    let cfg = Config::load(&appdata.join("com.repotether.app").join("config.json"));
    let cache_dir = std::env::temp_dir().join("repotether-dump-cache");

    let t = std::time::Instant::now();
    let mut snap = core::build_local(&cfg, &cache_dir, &|m| eprintln!("{m}"));
    eprintln!("local: {:?}", t.elapsed());
    if with_remote {
        let (repos, errors) = core::fetch_remotes(&cfg).await;
        snap.remote_repos = repos;
        snap.errors.extend(errors);
        snap.remote_fetched_at = Some(chrono::Local::now().to_rfc3339());
    }
    eprintln!(
        "repos={} commits={} sessions={} remote={} errors={}",
        snap.repos.len(),
        snap.commits.len(),
        snap.sessions.len(),
        snap.remote_repos.len(),
        snap.errors.len()
    );
    core::save_snapshot(std::path::Path::new(&out), &snap).unwrap();
}
