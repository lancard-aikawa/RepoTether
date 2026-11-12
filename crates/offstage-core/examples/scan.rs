//! 分類の規則を手元で直すための一覧。
//!
//!   cargo run -p offstage-core --example scan -- <リポジトリを並べたフォルダ> [--list]
//!
//! 既定はリポジトリごとの件数だけ。--summary で個人は上 2 段のフォルダごとの件数、
//! 機密の疑いは当たった規則ごとの件数 (ファイル名は出さない)。--list で個人・機密の疑いのパスも出す。
//! 出力は手元の端末だけに出す (人に渡す資料や公開のリポジトリに貼らない)。

use offstage_core::classify::Class;
use offstage_core::scan::{child_repos, scan_repo};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let list = args.iter().any(|a| a == "--list");
    let summary = args.iter().any(|a| a == "--summary");
    let Some(root) = args.iter().find(|a| !a.starts_with("--")).map(PathBuf::from) else {
        eprintln!("usage: scan <folder> [--list]");
        std::process::exit(2);
    };

    const CLASSES: [Class; 4] = [Class::Generated, Class::ToolData, Class::Personal, Class::Secret];
    let row = |name: &str, c: &[usize; 4]| {
        println!("{name:<28} {:>9} {:>8} {:>8} {:>6}", c[0], c[1], c[2], c[3]);
    };
    println!("{:<28} {:>9} {:>8} {:>8} {:>6}", "repo", "generated", "tooldata", "personal", "secret");
    let mut total = [0usize; 4];
    for repo in child_repos(&root) {
        let name = repo.file_name().unwrap().to_string_lossy().into_owned();
        match scan_repo(&repo) {
            Ok(s) => {
                let c = CLASSES.map(|k| s.count(k));
                for i in 0..4 {
                    total[i] += c[i];
                }
                if c[1] + c[2] + c[3] == 0 && !list {
                    continue;
                }
                row(&name, &c);
                if list {
                    for e in s.entries.iter().filter(|e| e.class != Class::Generated) {
                        println!("    {:<9} {:>10}  {}", e.class.label(), e.size, e.rel_path);
                    }
                } else if summary {
                    // ファイル名を出さずに規則を直すための内訳
                    let mut groups: BTreeMap<(Class, String), usize> = BTreeMap::new();
                    for e in &s.entries {
                        let key = match e.class {
                            Class::Generated => continue,
                            Class::Secret => e.rule().unwrap_or_default(),
                            _ => {
                                let mut parts: Vec<&str> = e.rel_path.split('/').collect();
                                if !e.is_dir {
                                    parts.pop();
                                }
                                parts.truncate(2);
                                if parts.is_empty() { "(root)".into() } else { parts.join("/") + "/" }
                            }
                        };
                        *groups.entry((e.class, key)).or_default() += 1;
                    }
                    for ((class, key), n) in groups {
                        println!("    {:<9} {n:>6}  {key}", class.label());
                    }
                }
            }
            Err(e) => println!("{name:<28} error: {e}"),
        }
    }
    row("(total)", &total);

    // --db <path>: 記録して前回との違いを件数だけ出す
    if let Some(i) = args.iter().position(|a| a == "--db") {
        let Some(db) = args.get(i + 1) else {
            eprintln!("--db needs a path");
            std::process::exit(2);
        };
        let t0 = std::time::Instant::now();
        let scans: Vec<_> = child_repos(&root).iter().filter_map(|r| scan_repo(r).ok()).collect();
        let mut store = offstage_core::store::Store::open(std::path::Path::new(db)).unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(1);
        });
        let ch = store.apply_run(&scans).expect("apply");
        println!(
            "\nrecorded in {:.1}s: added {} changed {} missing {} moved {} dropped {} returned {} repos_gone {}",
            t0.elapsed().as_secs_f64(),
            ch.added.len(),
            ch.changed.len(),
            ch.missing.len(),
            ch.moved.len(),
            ch.dropped.len(),
            ch.returned.len(),
            ch.repos_gone.len()
        );
        for (state, class, n) in store.counts().expect("counts") {
            println!("    {state:<10} {class:<10} {n}");
        }

        // --link: Claude の会話の記録と結び付ける (件数だけ出す)
        if args.iter().any(|a| a == "--link") {
            use offstage_core::link::{default_projects_dir, Known};
            let t1 = std::time::Instant::now();
            let docs: Vec<_> = child_repos(&root)
                .into_iter()
                .filter_map(|r| offstage_core::scan::tracked_docs(&r).ok().map(|d| (r, d)))
                .collect();
            let known = Known::build(&store, &docs).expect("known");
            let st = store.link_sessions(&default_projects_dir().expect("home"), &known).expect("link");
            println!(
                "\nlinked in {:.1}s: known {} logs read {} skipped {} MB {:.0} lines {} hits {}",
                t1.elapsed().as_secs_f64(),
                known.len(),
                st.logs_read,
                st.logs_skipped,
                st.bytes_read as f64 / 1e6,
                st.lines,
                st.hits
            );
            for (kind, class, n) in store.mention_counts().expect("counts") {
                println!("    {class:<10} {kind:<9} {n}");
            }
            let sent = store.secrets_sent().expect("secrets");
            println!("    secret files whose content reached a conversation: {}", sent.len());
        }
    }
}
