// マニュアル (docs/manual.md) の画面画像を、架空のプロジェクトを並べたデモ環境で撮る。
//
//   node tools/manual-shots.mjs            # docs/images/ に撮る
//
// - 実在のリポジトリ・コミット・プロンプトは使わない。デモのデータは毎回ここで作る (日付は今日から数える)
// - ブラウザ表示 (Tauri の外で pnpm dev を開いた状態) で撮る。開発サーバーが動いていなければ起動して、終わったら止める
// - ブラウザは Windows に入っている Microsoft Edge を使う (Playwright のブラウザを別に落とさない)

import { execFileSync, spawn } from "node:child_process";
import { mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright-core";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const OUT = join(ROOT, "docs", "images");
const URL_BASE = "http://localhost:11420/";
const NOW = Date.now();
const DAY = 86400000;

// ---- 乱数 (毎回同じ画面になるよう固定の種) ----
let seed = 20260928;
const rand = () => {
  seed |= 0;
  seed = (seed + 0x6d2b79f5) | 0;
  let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
};
const pick = (xs) => xs[Math.floor(rand() * xs.length)];
const iso = (ms) => new Date(ms).toISOString();
/** days 日前の h 時 m 分 (ローカル時刻) */
const at = (days, h = 10, m = 0) => {
  const d = new Date(NOW - days * DAY);
  d.setHours(h, m, 0, 0);
  return d.getTime();
};
const key = (p) => p.replace(/\//g, "\\").toLowerCase();

const ME = { name: "デモ ユーザー", email: "demo@example.com" };
const MATE = { name: "チームの人", email: "teammate@example.com" };

const SUBJECTS = [
  "feat: 一覧に絞り込みを足す",
  "fix: 日付の表示がずれる",
  "refactor: 設定の読み込みを分ける",
  "docs: README を更新",
  "feat: CSV の書き出し",
  "fix: 空のときに落ちる",
  "test: 境界の値を足す",
  "chore: 依存を更新",
  "feat: ダークモード",
  "fix: 保存に失敗したときの表示",
  "feat: 検索を速くする",
  "style: 余白をそろえる",
];

// ---- デモのプロジェクト ----
// activity: [何日前から, 何日前まで, 1 日にコミットがある確率]
const PROJECTS = [
  {
    path: "C:\\Repos\\work\\shop-frontend",
    remotes: [{ name: "origin", url: "https://github.com/acme-demo/shop-frontend.git", key: "github.com/acme-demo/shop-frontend" }],
    branch: "feature/cart",
    upstream: "origin/feature/cart",
    modified: 3,
    untracked: 1,
    dirtyDays: 0.02,
    activity: [[1, 20, 0.7], [20, 365, 0.12]],
    today: [[9, 20, "fix: カートの合計金額の丸め"], [10, 5, "test: 端数の境界のテストを足す"]],
    mate: 0.4,
    claude: { title: "カートの合計金額の丸め", first: "カートの合計が 1 円ずれるのを直したい", last: "テストも足してください", reply: "丸めを表示の直前だけにし、境界の値のテストを 3 件足しました。", days: 0 },
    tags: ["仕事/ACME"],
  },
  {
    path: "C:\\Repos\\work\\invoice-api",
    remotes: [{ name: "origin", url: "https://git.example.com/team/invoice-api.git", key: "git.example.com/team/invoice-api" }],
    branch: "main",
    upstream: "origin/main",
    ahead: 2,
    branches: [{ name: "feature/tax-rate", upstream: null, ahead: 0, behind: 0, gone: false, merged: false, days: 6 }],
    activity: [[1, 40, 0.5], [40, 365, 0.2]],
    mate: 0.3,
    claude: { title: "消費税率の切り替え", first: "税率を日付で切り替えられるようにしたい", last: "push はまだしないでください", reply: "税率の表を日付つきにしました。push は保留しています。", days: 1 },
    tags: ["仕事/ACME"],
    starred: true,
  },
  {
    path: "C:\\Repos\\work\\batch-jobs",
    remotes: [{ name: "origin", url: "space@space.git.backlog.jp:/OPS/batch-jobs.git", key: "space.git.backlog.jp/ops/batch-jobs" }],
    branch: "main",
    upstream: "origin/main",
    stashes: 1,
    activity: [[110, 300, 0.15]],
    mate: 0.5,
    tags: ["仕事/社内"],
  },
  {
    path: "C:\\Repos\\mywork\\photo-organizer",
    remotes: [{ name: "origin", url: "https://github.com/demo-user/photo-organizer.git", key: "github.com/demo-user/photo-organizer" }],
    branch: "main",
    upstream: "origin/main",
    activity: [[2, 60, 0.45], [120, 200, 0.3]],
    claude: { title: "撮影日でフォルダを分ける", first: "EXIF の撮影日で年月フォルダに振り分けたい", last: "重複した写真の扱いは?", reply: "同じハッシュのものは「重複」フォルダへ移すようにしました。", days: 2 },
    tags: ["自作/アプリ"],
    starred: true,
  },
  {
    path: "C:\\Repos\\mywork\\cli-timer",
    remotes: [],
    branch: "main",
    upstream: null,
    modified: 1,
    dirtyDays: 3,
    activity: [[3, 30, 0.3]],
    claude: { title: "ポモドーロのタイマー", first: "25 分と 5 分を繰り返すタイマーを作りたい", last: "音を鳴らせますか", reply: "終了時にビープ音を鳴らすオプションを足しました。", days: 3 },
    tags: ["自作/ツール"],
  },
  {
    path: "C:\\Repos\\mywork\\blog",
    remotes: [{ name: "origin", url: "https://github.com/demo-user/blog.git", key: "github.com/demo-user/blog" }],
    branch: "main",
    upstream: "origin/main",
    behind: 1,
    activity: [[9, 200, 0.08]],
    tags: ["自作/サイト"],
  },
  {
    path: "C:\\Repos\\mywork\\dotfiles",
    remotes: [{ name: "origin", url: "https://github.com/demo-user/dotfiles.git", key: "github.com/demo-user/dotfiles" }],
    branch: "main",
    upstream: "origin/main",
    activity: [[15, 365, 0.04]],
    tags: ["自作/設定"],
  },
  {
    path: "C:\\Repos\\test\\rust-playground",
    remotes: [],
    branch: "main",
    upstream: null,
    activity: [[45, 80, 0.3]],
  },
  {
    path: "C:\\Repos\\old\\legacy-site",
    remotes: [{ name: "origin", url: "ssh://git@git.oldhost.example/web/legacy-site.git", key: "git.oldhost.example/web/legacy-site" }],
    branch: "develop",
    upstream: "origin/develop",
    branches: [{ name: "hotfix/login", upstream: "origin/hotfix/login", ahead: 0, behind: 0, gone: true, merged: false, days: 900 }],
    lastCommitDays: 1000,
    activity: [],
  },
  {
    path: "D:\\Archive\\scan-tool",
    error: "fatal: detected dubious ownership in repository at 'D:/Archive/scan-tool'",
    activity: [],
  },
];

const REMOTE_ONLY = [
  { name: "weather-widget", desc: "天気を表示する小さなウィジェット", pushed: 40 },
  { name: "forked-lib", desc: "ほかのプロジェクトからフォークしたライブラリ", pushed: 200, fork: true },
  { name: "old-app", desc: "以前のアプリ (アーカイブ済み)", pushed: 700, archived: true },
];

// 状態タブの README の見本
const README = {
  "C:\\Repos\\mywork\\photo-organizer": [
    "# photo-organizer",
    "",
    "写真を撮影日ごとのフォルダに振り分ける小さなツールです。",
    "",
    "## 使い方",
    "",
    "```sh",
    "photo-organizer ./inbox --to ./photos",
    "```",
    "",
    "| オプション | 意味 |",
    "|---|---|",
    "| `--dry-run` | 動かさずに、どこへ移すかだけ表示する |",
    "| `--dedupe` | 同じ写真を「重複」フォルダへ移す |",
    "",
    "- 撮影日が無い写真は「日付なし」フォルダへ",
    "- 詳しくは [Wiki](https://example.com/wiki) を見てください",
  ].join("\n"),
};

// ---- 取り込み結果を作る ----
function buildSnapshot() {
  const repos = [];
  const commits = [];
  const sessions = [];
  let hash = 0;
  const h = () => (++hash).toString(16).padStart(40, "a");

  for (const p of PROJECTS) {
    const id = key(p.path);
    const name = p.path.split("\\").pop();
    const mine = [];
    for (const [from, to, prob] of p.activity) {
      for (let d = from; d < to; d++) {
        const wd = new Date(NOW - d * DAY).getDay();
        const pr = wd === 0 || wd === 6 ? prob * 0.3 : prob;
        if (rand() > pr) continue;
        const n = 1 + Math.floor(rand() * 3);
        for (let i = 0; i < n; i++) {
          const who = rand() < (p.mate ?? 0) ? MATE : ME;
          const t = at(d, 9 + Math.floor(rand() * 10), Math.floor(rand() * 60));
          if (t > NOW) continue; // 今日のこれからの時刻は作らない
          const c = {
            repoId: id,
            hash: h(),
            at: new Date(t).toISOString(),
            authorName: who.name,
            authorEmail: who.email,
            subject: pick(SUBJECTS),
            isMerge: false,
          };
          commits.push(c);
          if (who === ME) mine.push(c);
        }
      }
    }
    // 今朝のコミット (撮る曜日によらず「今週」に活動があるように)
    for (const [hh, mm, subject] of p.today ?? []) {
      const t = at(0, hh, mm);
      if (t > NOW) continue;
      const c = { repoId: id, hash: h(), at: iso(t), authorName: ME.name, authorEmail: ME.email, subject, isMerge: false };
      commits.push(c);
      mine.push(c);
    }
    const lastCommit = commits.filter((c) => c.repoId === id).sort((a, b) => b.at.localeCompare(a.at))[0];
    repos.push({
      id,
      path: p.path,
      name,
      remotes: p.remotes ?? [],
      branch: p.error ? null : p.branch,
      head: p.error ? null : "3f2a9c1d7e5b",
      upstream: p.upstream ?? null,
      ahead: p.ahead ?? 0,
      behind: p.behind ?? 0,
      defaultBranch: p.error ? null : p.branch === "develop" ? "develop" : "main",
      branches: (p.branches ?? []).map((b) => ({ ...b, lastCommitAt: iso(at(b.days, 15)) })),
      staged: 0,
      modified: p.modified ?? 0,
      untracked: p.untracked ?? 0,
      conflicted: 0,
      stashes: p.stashes ?? 0,
      dirtyModifiedAt: p.modified || p.untracked ? iso(NOW - (p.dirtyDays ?? 0) * DAY) : null,
      lastCommitAt: lastCommit ? lastCommit.at : p.lastCommitDays ? iso(at(p.lastCommitDays, 18)) : null,
      lastFetchAt: p.error ? null : iso(NOW - 2 * DAY),
      error: p.error ?? null,
    });

    if (p.claude) {
      // 直前のセッションと、その前のセッションをいくつか
      const list = [p.claude, ...Array.from({ length: 3 }, (_, i) => ({
        title: pick(["テストの追加", "エラーの原因調べ", "README の見直し", "設定画面の改善"]),
        first: "ここを直したい",
        last: "ありがとう",
        reply: "直しました。",
        days: p.claude.days + 4 + i * 6,
      }))];
      for (const [i, s] of list.entries()) {
        const start = at(s.days, 10 + (i % 3) * 2, 5);
        const n = 4 + Math.floor(rand() * 18);
        const times = Array.from({ length: n }, (_, k) => iso(start + k * 7 * 60000));
        sessions.push({
          id: `demo-${name}-${i}`,
          cwd: p.path,
          repoId: id,
          entrypoint: "claude-vscode",
          interactive: true,
          startedAt: times[0],
          endedAt: times[times.length - 1],
          title: s.title,
          firstPrompt: s.first,
          lastPrompt: s.last,
          lastReply: s.reply,
          promptCount: n,
          promptTimes: times,
          gitBranch: p.branch,
        });
      }
    }
  }

  // git 以外のフォルダでの Claude セッション
  sessions.push({
    id: "demo-notes",
    cwd: "C:\\work\\notes",
    repoId: null,
    entrypoint: "claude-vscode",
    interactive: true,
    startedAt: iso(at(0, 13)),
    endedAt: iso(at(0, 14, 20)),
    title: "来週の作業の整理",
    firstPrompt: "来週やることを整理したい",
    lastPrompt: "日報の形にしてください",
    lastReply: "日報の形にまとめました。",
    promptCount: 9,
    promptTimes: Array.from({ length: 9 }, (_, k) => iso(at(0, 13) + k * 9 * 60000)),
    gitBranch: null,
  });

  const remoteRepos = [
    ...PROJECTS.flatMap((p) =>
      (p.remotes ?? [])
        .filter((r) => r.key.startsWith("github.com/"))
        .map((r) => {
          const [, owner, name] = r.key.split("/");
          const full = r.url.replace(/^https:\/\/github\.com\//, "").replace(/\.git$/, "");
          return {
            accountId: "gh",
            key: r.key,
            fullName: full,
            name: full.split("/")[1],
            owner,
            description: null,
            htmlUrl: `https://github.com/${full}`,
            cloneUrl: r.url,
            sshUrl: `git@github.com:${full}.git`,
            private: owner === "acme-demo",
            fork: false,
            archived: false,
            defaultBranch: "main",
            updatedAt: iso(NOW - 3 * DAY),
            pushedAt: iso(NOW - 3 * DAY),
          };
        }),
    ),
    ...REMOTE_ONLY.map((r) => ({
      accountId: "gh",
      key: `github.com/demo-user/${r.name}`,
      fullName: `demo-user/${r.name}`,
      name: r.name,
      owner: "demo-user",
      description: r.desc,
      htmlUrl: `https://github.com/demo-user/${r.name}`,
      cloneUrl: `https://github.com/demo-user/${r.name}.git`,
      sshUrl: `git@github.com:demo-user/${r.name}.git`,
      private: false,
      fork: !!r.fork,
      archived: !!r.archived,
      defaultBranch: "main",
      updatedAt: iso(NOW - r.pushed * DAY),
      pushedAt: iso(NOW - r.pushed * DAY),
    })),
  ];

  commits.sort((a, b) => b.at.localeCompare(a.at));
  sessions.sort((a, b) => (b.startedAt ?? "").localeCompare(a.startedAt ?? ""));
  return {
    generatedAt: iso(NOW - 3 * 60000),
    remoteFetchedAt: iso(NOW - 25 * 60000),
    repos,
    commits,
    sessions,
    remoteRepos,
    errors: [],
  };
}

function buildConfig() {
  const tags = {};
  const starred = [];
  for (const p of PROJECTS) {
    if (p.tags) tags[key(p.path)] = p.tags;
    if (p.starred) starred.push(key(p.path));
  }
  return {
    roots: ["C:\\Repos", "D:\\Archive"],
    scanDepth: 3,
    includeSessionFolders: true,
    claudeDir: null,
    historyDays: 365,
    authorEmails: [ME.email],
    accounts: [
      { id: "gh", kind: "github", label: "GitHub", baseUrl: "", user: "", auth: "gh", token: "", hasToken: false, clearToken: false, enabled: true },
      { id: "acc2", kind: "gogs", label: "社内 Gogs", baseUrl: "https://git.example.com", user: "demo", auth: "token", token: "", hasToken: true, clearToken: false, enabled: true },
    ],
    cloneRoot: "C:\\Repos\\mywork",
    hidden: [],
    tags,
    tagDefs: ["仕事", "仕事/ACME", "仕事/社内", "自作", "自作/アプリ", "自作/ツール", "自作/サイト", "自作/設定"],
    starred,
  };
}

// ---- 開発サーバー ----
async function up() {
  try {
    return (await fetch(URL_BASE)).ok;
  } catch {
    return false;
  }
}

async function ensureServer() {
  if (await up()) return null;
  console.log("開発サーバーを起動します (pnpm dev)");
  // pnpm は Windows では pnpm.cmd なのでシェル経由で起動する (引数は固定の文字列だけ)
  const child = spawn("pnpm dev", { cwd: ROOT, shell: true, stdio: "ignore" });
  for (let i = 0; i < 60; i++) {
    await new Promise((r) => setTimeout(r, 1000));
    if (await up()) return child;
  }
  throw new Error("開発サーバーが起動しません");
}

function stopServer(child) {
  if (!child) return;
  try {
    // Windows は子プロセスごと止める
    execFileSync("taskkill", ["/pid", String(child.pid), "/T", "/F"], { stdio: "ignore" });
  } catch {
    child.kill();
  }
}

// ---- 撮影 ----
async function main() {
  mkdirSync(OUT, { recursive: true });
  const snapshot = buildSnapshot();
  const config = buildConfig();
  const server = await ensureServer();
  const browser = await chromium.launch({ channel: "msedge" });
  try {
    const open = async (prefs = {}, scheme = "light") => {
      const page = await browser.newPage({ viewport: { width: 1280, height: 780 }, colorScheme: scheme });
      page.setDefaultTimeout(8000);
      await page.route("**/dev-snapshot.json", (route) => route.fulfill({ json: snapshot }));
      await page.addInitScript(
        ({ config, readme, prefs }) => {
          window.__mockConfig = config;
          window.__mockGhUser = "demo-user";
          window.__mockReadme = readme;
          localStorage.setItem(
            "repotether.prefs",
            JSON.stringify({ tab: "state", stateView: "time", theme: "system", density: "normal", autoLocalMin: 0, autoRemoteMin: 0, ...prefs }),
          );
        },
        { config, readme: README, prefs },
      );
      await page.goto(URL_BASE);
      // マニュアルには「ブラウザ表示」の印を出さない
      await page.addStyleTag({ content: ".status .badge.info { display: none !important; }" });
      await page.waitForSelector(".list li, .tiles, textarea, .scroll");
      await page.waitForTimeout(600);
      return page;
    };
    const shot = async (page, name) => {
      await page.waitForTimeout(300);
      await page.screenshot({ path: join(OUT, name) });
      console.log("  ", name);
    };
    const selectRow = async (page, name) => {
      await page.locator("ul.list li", { has: page.locator(".name", { hasText: new RegExp(`^${name}$`) }) }).first().locator(".row").click();
    };

    console.log("撮影:");
    // 状態 (時系列 + 詳細)
    let page = await open({ detailTab: "summary" });
    await selectRow(page, "invoice-api");
    await shot(page, "01_state_time.png");
    await page.locator(".detail .tab", { hasText: "リポジトリ" }).click();
    await shot(page, "04_detail_repo.png");
    await page.locator(".detail .tab", { hasText: "コミット" }).click();
    await shot(page, "05_detail_commits.png");
    await selectRow(page, "invoice-api");
    await selectRow(page, "photo-organizer");
    await page.locator(".detail .tab", { hasText: "Claude" }).click();
    await shot(page, "06_detail_claude.png");
    await page.locator(".detail .tab", { hasText: "README" }).click();
    await shot(page, "07_detail_readme.png");
    await page.close();

    page = await open({ stateView: "folder" });
    await shot(page, "02_state_folder.png");
    await page.close();

    page = await open({ stateView: "tag" });
    await shot(page, "03_state_tag.png");
    await page.locator(".group-head", { has: page.locator(".g-label", { hasText: /^ACME$/ }) }).hover();
    await shot(page, "03b_tag_actions.png");
    await page.close();

    // 未クローンとクローン
    page = await open();
    await page.locator(".segmented button", { hasText: "未クローン" }).click();
    await shot(page, "08_not_cloned.png");
    await page.locator("ul.list li").first().hover();
    await page.locator("ul.list li .row-actions button", { hasText: "クローン" }).first().click();
    await shot(page, "09_clone.png");
    await page.close();

    for (const [tab, file] of [
      ["history", "10_history.png"],
      ["graph", "11_graph.png"],
      ["report", "12_report.png"],
    ]) {
      page = await open({ tab });
      await shot(page, file);
      await page.close();
    }

    for (const [stab, file] of [
      ["roots", "13_settings_roots.png"],
      ["authors", "14_settings_authors.png"],
      ["accounts", "15_settings_accounts.png"],
      ["other", "16_settings_other.png"],
    ]) {
      page = await open({ tab: "settings", settingsTab: stab });
      await shot(page, file);
      await page.close();
    }

    // ダーク + コンパクト
    page = await open({ theme: "dark", density: "compact" }, "dark");
    await shot(page, "17_dark_compact.png");
    await page.close();
  } finally {
    await browser.close();
    stopServer(server);
  }
  console.log(`docs/images/ に撮りました`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
