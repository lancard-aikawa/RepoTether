//! 取り込みの流れ: リポジトリを探す → 洗い出して記録する → git の資料の一覧 → 会話と結び付ける。

use crate::link::{LinkStats, Known};
use crate::scan::{scan_repo_with, tracked_docs};
use crate::store::{Changes, Store};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// 1 回の取り込みの結果
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub repos: usize,
    /// 洗い出せなかったリポジトリ (所有者が違って git が止まった、など) と理由
    pub errors: Vec<(String, String)>,
    pub added: usize,
    pub changed: usize,
    pub missing: usize,
    pub moved: usize,
    pub returned: usize,
    pub dropped: usize,
    pub repos_gone: usize,
    pub logs_read: usize,
    pub mentions_added: usize,
    pub seconds: f64,
}

impl Report {
    fn take(&mut self, c: &Changes, l: &LinkStats) {
        self.added = c.added.len();
        self.changed = c.changed.len();
        self.missing = c.missing.len();
        self.moved = c.moved.len();
        self.returned = c.returned.len();
        self.dropped = c.dropped.len();
        self.repos_gone = c.repos_gone.len();
        self.logs_read = l.logs_read;
        self.mentions_added = l.hits;
    }
}

/// 取り込む。`projects_dir` が None なら会話との結び付けはしない
pub fn refresh(store: &mut Store, repos: &[PathBuf], projects_dir: Option<&Path>) -> Result<Report, String> {
    let t0 = Instant::now();
    let mut rep = Report { repos: repos.len(), ..Default::default() };
    // git を呼ぶ部分 (無視されたものと資料の一覧) は、リポジトリごとに並べて回す。記録への書き込みは後でまとめて
    let jobs: Vec<(PathBuf, Vec<(String, crate::classify::Class)>)> = repos
        .iter()
        .map(|r| Ok((r.clone(), store.overrides_for(&r.to_string_lossy())?)))
        .collect::<Result<_, String>>()?;
    let results = parallel(&jobs, WORKERS, |(r, ov)| (scan_repo_with(r, ov), tracked_docs(r).ok()));
    let mut scans = Vec::new();
    let mut docs = Vec::new();
    for ((r, _), (scan, d)) in jobs.iter().zip(results) {
        match scan {
            Ok(s) => {
                scans.push(s);
                if let Some(d) = d {
                    docs.push((r.clone(), d));
                }
            }
            Err(e) => rep.errors.push((r.to_string_lossy().into_owned(), first_line(&e))),
        }
    }
    let changes = store.apply_run(&scans)?;
    for (root, d) in &docs {
        store.set_docs(root, d)?;
    }
    let stats = match projects_dir {
        Some(p) => {
            let known = Known::build(store, &docs)?;
            store.link_sessions(p, &known)?
        }
        None => LinkStats::default(),
    };
    rep.take(&changes, &stats);
    rep.seconds = t0.elapsed().as_secs_f64();
    Ok(rep)
}

/// 同時に git を動かす数
const WORKERS: usize = 6;

/// `items` を `workers` 本で並べて処理し、元の順で返す
fn parallel<T: Sync, R: Send>(items: &[T], workers: usize, f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<Option<R>>> = Mutex::new((0..items.len()).map(|_| None).collect());
    std::thread::scope(|s| {
        for _ in 0..workers.min(items.len()).max(1) {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= items.len() {
                    break;
                }
                let r = f(&items[i]);
                out.lock().unwrap()[i] = Some(r);
            });
        }
    });
    out.into_inner().unwrap().into_iter().map(|r| r.expect("every item is processed")).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn parallel_keeps_order() {
        let items: Vec<u32> = (0..50).collect();
        assert_eq!(super::parallel(&items, 6, |x| x * 2), (0..50).map(|x| x * 2).collect::<Vec<_>>());
        assert!(super::parallel(&Vec::<u32>::new(), 6, |x| *x).is_empty());
    }
}

fn first_line(s: &str) -> String {
    s.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string()
}
