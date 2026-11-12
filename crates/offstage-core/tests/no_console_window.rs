//! 取り込みで子プロセスを起動するときは、窓を出さない `scan::git_command()` だけを使う。
//! 2026-11-12、Offstage の「更新」で git ごとに黒い窓が開き、操作できなくなった。
//! 本番のコード (各ファイルの #[cfg(test)] より前) に、git_command の中以外の `Command::new` があれば落とす。

#[test]
fn only_git_command_spawns_processes() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut bad = Vec::new();
    for e in std::fs::read_dir(&src).unwrap().flatten() {
        let text = std::fs::read_to_string(e.path()).unwrap();
        let prod = text.split("#[cfg(test)]").next().unwrap();
        let name = e.file_name().to_string_lossy().into_owned();
        let allowed = if name == "scan.rs" { 1 } else { 0 }; // git_command の中の 1 か所
        let n = prod.matches("Command::new(").count();
        if n != allowed {
            bad.push(format!("{name}: Command::new が {n} か所 (許すのは {allowed})"));
        }
    }
    assert!(bad.is_empty(), "{bad:?}");
}

#[cfg(windows)]
#[test]
fn git_command_hides_the_console_window() {
    let src = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/scan.rs")).unwrap();
    let body = src.split("pub fn git_command()").nth(1).unwrap();
    let body = &body[..body.find("\n}").unwrap()];
    assert!(body.contains("creation_flags(CREATE_NO_WINDOW)"));
    assert!(src.contains("const CREATE_NO_WINDOW: u32 = 0x0800_0000;"));
}
