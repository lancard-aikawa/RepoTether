//! アプリを起動せずに取り込みだけを実行し、結果を JSON で書き出す。
//! 使い方: cargo run --release --example dump -- <出力先.json> [--remote] [--fetch] [--active <日数> | --under <パス>]
//! --active / --under は、出力先に前回の結果があれば、アプリの自動更新 / フォルダを選んでの更新と同じく読むものを絞る。
//! 設定はアプリと同じ %APPDATA%\com.repotether.app\config.json を読む。
//! 環境変数 REPOTETHER_CONFIG で別の設定ファイルを指定できる。

use std::path::PathBuf;

use repotether_lib::core::{self, config::Config};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = args.get(1).cloned().unwrap_or_else(|| "snapshot.json".into());
    let with_remote = args.iter().any(|a| a == "--remote");
    let with_fetch = args.iter().any(|a| a == "--fetch");
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let scope = if let Some(d) = opt("--active") {
        core::model::Scope::Active { days: d.parse().expect("--active には日数") }
    } else if let Some(p) = opt("--under") {
        core::model::Scope::Under { path: p }
    } else {
        core::model::Scope::All
    };
    let prev = core::load_snapshot(std::path::Path::new(&out));

    let appdata = PathBuf::from(std::env::var_os("APPDATA").expect("APPDATA"));
    let cfg_path = std::env::var_os("REPOTETHER_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| appdata.join("com.repotether.app").join("config.json"));
    let cfg = Config::load(&cfg_path);
    let cache_dir = std::env::temp_dir().join("repotether-dump-cache");

    let t = std::time::Instant::now();
    let mut snap = core::build_local(&cfg, &cache_dir, with_fetch, &scope, prev.as_ref(), &|m| eprintln!("{m}"));
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
