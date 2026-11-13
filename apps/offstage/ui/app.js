// Offstage の画面。データは Rust の側 (invoke) からだけ取る。外とは通信しない (CSP でも止めている)
"use strict";

const invoke = window.__TAURI__.core.invoke;
const $ = (id) => document.getElementById(id);

const CLASS = { personal: "個人", secret: "機密", tooldata: "道具", doc: "資料", generated: "生成" };
const STATE = { present: "あり", missing: "消失", moved: "移動", dismissed: "了解" };
// 出方。content: 中身が送られた (かもしれない) もの
const KIND = {
  read: ["読込", true, "Read で中身が送られた"],
  attached: ["添付", true, "@ で渡した・外で直した・CLAUDE.md や memory・エディタで選んだ範囲として中身が送られた"],
  grep: ["検索", true, "Grep の対象になり、当たった行が送られた"],
  edit: ["編集", true, "Edit で、直す前後の文が送られた"],
  bash: ["命令", true, "コマンドに出た。出力に中身が出たかもしれない"],
  tool: ["道具", true, "その他の道具に渡した"],
  write: ["作成", false, "Claude が Write で書いた"],
  result: ["結果", false, "道具の結果に名前が出た"],
  text: ["本文", false, "本文に名前が出た"],
};
const ICONS = {
  refresh: '<svg viewBox="0 0 24 24"><path d="M21 12a9 9 0 1 1-2.6-6.4"/><path d="M21 4v5h-5"/></svg>',
  clear: '<svg viewBox="0 0 24 24"><path d="M6 6l12 12M18 6L6 18"/></svg>',
  missing: '<svg viewBox="0 0 24 24"><path d="M14 3H6a1 1 0 0 0-1 1v16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1V8z"/><path d="M9 13h6"/></svg>',
  moved: '<svg viewBox="0 0 24 24"><path d="M5 12h14"/><path d="M13 6l6 6-6 6"/></svg>',
  sent: '<svg viewBox="0 0 24 24"><path d="M4 12l16-8-6 16-3-7z"/></svg>',
  gone: '<svg viewBox="0 0 24 24"><path d="M3 7h6l2 2h10v10H3z"/><path d="M9 14h6"/></svg>',
  repo: '<svg viewBox="0 0 24 24"><path d="M5 4h11l3 3v13H5z"/><path d="M9 4v6l2-1.5L13 10V4"/></svg>',
  open: '<svg viewBox="0 0 24 24"><path d="M3 7h6l2 2h10v10H3z"/></svg>',
  ok: '<svg viewBox="0 0 24 24"><path d="M5 12l5 5 9-10"/></svg>',
  apply: '<svg viewBox="0 0 24 24"><path d="M12 5v14M5 12h14"/></svg>',
  undo: '<svg viewBox="0 0 24 24"><path d="M9 14L4 9l5-5"/><path d="M4 9h10a6 6 0 0 1 0 12h-3"/></svg>',
  talk: '<svg viewBox="0 0 24 24"><path d="M4 5h16v11H9l-5 4z"/></svg>',
  raw: '<svg viewBox="0 0 24 24"><path d="M8 6l-5 6 5 6M16 6l5 6-5 6"/></svg>',
};

function esc(s) {
  return String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);
}
function ico(name) {
  return `<span class="ico">${ICONS[name] || ""}</span>`;
}
function tag(cls, text, title) {
  return `<span class="tag ${esc(cls)}"${title ? ` title="${esc(title)}"` : ""}>${esc(text)}</span>`;
}
function base(p) {
  return String(p).split(/[\\/]/).filter(Boolean).pop() || p;
}
function size(n) {
  if (n == null) return "";
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}
function date(sec) {
  if (!sec) return "";
  const d = new Date(sec * 1000);
  const p = (x) => String(x).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}
function isoDate(s) {
  return s ? date(Date.parse(s) / 1000) : "";
}

let rows = [];
let selected = null; // {repo, relPath}
let alertFilter = ""; // missing / moved / sent / ""

document.querySelectorAll("[data-ico]").forEach((el) => (el.innerHTML = ICONS[el.dataset.ico] || ""));

function query() {
  const cls = $("q-class").value;
  const q = {
    text: $("q-text").value,
    classes: cls ? [cls] : ["personal", "secret", "tooldata", "doc"],
    states: $("q-state").value ? [$("q-state").value] : [],
    repo: $("q-repo").value,
    mention: $("q-mention").value.startsWith("kind:") ? "" : $("q-mention").value,
    kinds: $("q-mention").value.startsWith("kind:") ? [$("q-mention").value.slice(5)] : [],
    ext: $("q-ext").value.trim().replace(/^\./, ""),
    limit: viewMode === "tree" ? TREE_LIMIT : TABLE_LIMIT,
  };
  if (alertFilter === "missing") q.states = ["missing"];
  if (alertFilter === "moved") q.states = ["moved"];
  if (alertFilter === "sent") {
    q.classes = ["secret"];
    q.mention = "content";
  }
  return q;
}

async function loadOverview() {
  const o = await invoke("overview");
  const a = o.alerts || {};
  const cards = [
    ["missing", "消失", a.missing, "前回あって、今はディスクに無いファイル", "hot"],
    ["moved", "移動", a.moved, "同じ中身のファイルが別の場所に現れた", "warm"],
    ["sent", "送信", a.secretsSent, "機密の疑いのファイルの中身が Claude の会話に出た", "hot"],
    ["gone", "不明", a.reposGone, "フォルダごと見当たらないリポジトリ", "warm"],
  ];
  $("alerts").innerHTML = cards
    .map(([key, name, n, tip, heat]) => {
      const lit = n > 0 ? heat : "";
      const on = alertFilter === key ? " on" : "";
      const disabled = n > 0 && key !== "gone" ? "" : " disabled";
      return `<button type="button" class="card ${lit}${on}" data-alert="${key}" title="${esc(tip)}"${disabled}>
        <span class="name">${ico(key)}${esc(name)}</span><span class="n">${n ?? "-"}</span><span class="sub">${esc(tip)}</span></button>`;
    })
    .join("");
  const status = [];
  if (o.error) status.push(`<span class="err">${esc(o.error)}</span>`);
  if (a.lastScan) status.push(`最後の取り込み ${esc(date(a.lastScan))}`);
  else status.push("まだ取り込んでいません。「更新」を押してください");
  if (a.repos != null) status.push(`リポジトリ ${a.repos} ・ git の外 ${a.files} ・ 資料 ${a.docs}`);
  $("status").innerHTML = status.join(" ・ ");
  $("status").title = `探すフォルダ: ${o.settings.roots.join(", ")}（${o.settings.depth} 階層、${o.settings.rootsFrom}）\n記録: ${o.settings.db}`;
  const sel = $("q-repo");
  const cur = sel.value;
  sel.innerHTML = `<option value="">すべて</option>` + o.repos.map(([r, gone]) => `<option value="${esc(r)}">${esc(base(r))}${gone ? "（不明）" : ""}</option>`).join("");
  sel.value = cur;
}

async function loadRows() {
  try {
    rows = await invoke("search", { q: query() });
  } catch (e) {
    rows = [];
    $("empty").hidden = false;
    $("empty").textContent = String(e);
    return;
  }
  const limit = viewMode === "tree" ? TREE_LIMIT : TABLE_LIMIT;
  $("count").textContent = `${rows.length} 件${rows.length >= limit ? `（先頭の ${limit} 件）` : ""}`;
  // 少なければ (絞り込んだあとなど) ツリーは全部開いて見せる
  if (viewMode === "tree" && rows.length <= TREE_AUTO_EXPAND) expanded = new Set(folderKeys());
  renderList();
  $("empty").hidden = rows.length > 0;
  $("empty").textContent = rows.length ? "" : "当てはまるものがありません";
}

const TABLE_LIMIT = 1000;
const TREE_LIMIT = 5000;
const TREE_AUTO_EXPAND = 200;
let viewMode = "table";
try {
  if (localStorage.getItem("offstage.view") === "tree") viewMode = "tree";
} catch {}
/// ツリーで開いているフォルダ (リポジトリ|パス)
let expanded = new Set();

function talkTag(r) {
  return r.mentions
    ? r.contentMentions
      ? tag("content", `中身 ${r.contentMentions}`, `会話に ${r.mentions} 回、うち中身が送られたのが ${r.contentMentions} 回`)
      : tag("", `名前 ${r.mentions}`, `会話に名前が ${r.mentions} 回出た`)
    : "";
}

function isSel(r) {
  return !!selected && selected.repo === r.repo && selected.relPath === r.relPath;
}

function renderList() {
  const tree = viewMode === "tree";
  $("table").hidden = tree;
  $("tree").hidden = !tree;
  $("tree-tools").hidden = !tree;
  $("view-table").setAttribute("aria-pressed", String(!tree));
  $("view-tree").setAttribute("aria-pressed", String(tree));
  if (tree) renderTree();
  else renderTable();
}

function renderTable() {
  $("rows").innerHTML = rows
    .map(
      (r, i) => `<tr data-i="${i}" tabindex="0" class="${isSel(r) ? "sel" : ""}">
        <td>${tag(r.state, STATE[r.state] || r.state)}</td>
        <td>${tag(r.class, CLASS[r.class] || r.class)}</td>
        <td class="path"><span class="repo" title="${esc(r.repo)}">${esc(base(r.repo))}</span>${esc(r.relPath)}${r.isDir ? "/" : ""}</td>
        <td class="num">${r.isDir ? "" : esc(size(r.size))}</td>
        <td class="when">${esc(date(r.modified))}</td>
        <td class="num">${talkTag(r)}</td>
      </tr>`,
    )
    .join("");
}

/// rows を リポジトリ → フォルダ → ファイル の木にする。フォルダは中の数と印の数を持つ
function buildTree() {
  const mk = (name, key) => ({ name, key, dirs: new Map(), files: [], n: 0, missing: 0, moved: 0, content: 0 });
  const roots = new Map();
  rows.forEach((r, i) => {
    if (!roots.has(r.repo)) roots.set(r.repo, mk(base(r.repo), `${r.repo}|`));
    let node = roots.get(r.repo);
    const parts = r.relPath.split("/");
    const chain = [node];
    for (let k = 0; k < parts.length - 1; k++) {
      const key = `${r.repo}|${parts.slice(0, k + 1).join("/")}`;
      if (!node.dirs.has(parts[k])) node.dirs.set(parts[k], mk(parts[k], key));
      node = node.dirs.get(parts[k]);
      chain.push(node);
    }
    node.files.push(i);
    for (const c of chain) {
      c.n++;
      if (r.state === "missing") c.missing++;
      if (r.state === "moved") c.moved++;
      if (r.contentMentions) c.content++;
    }
  });
  return [...roots.values()].sort((a, b) => a.name.localeCompare(b.name));
}

function folderKeys() {
  const out = [];
  const walk = (n) => {
    out.push(n.key);
    n.dirs.forEach(walk);
  };
  buildTree().forEach(walk);
  return out;
}

function renderTree() {
  const html = [];
  const folder = (n, depth, isRepo) => {
    const open = expanded.has(n.key);
    const marks = [
      n.missing ? tag("missing", `消失 ${n.missing}`) : "",
      n.moved ? tag("moved", `移動 ${n.moved}`) : "",
      n.content ? tag("content", `中身 ${n.content}`, "中身が会話に送られたファイルの数") : "",
    ].join("");
    html.push(`<div class="node folder${isRepo ? " repo" : ""}" role="treeitem" aria-expanded="${open}" tabindex="0"
      data-key="${esc(n.key)}" style="--d:${depth}" title="${esc(isRepo ? n.key.slice(0, -1) : n.name)}（${open ? "押すと閉じる" : "押すと開く"}）">
      <span class="chev">${open ? "▾" : "▸"}</span><span class="ico">${ICONS[isRepo ? "repo" : "open"]}</span>
      <span class="name">${esc(n.name)}${isRepo ? "" : "/"}</span><span class="cnt">${n.n}</span><span class="marks">${marks}</span></div>`);
    if (!open) return;
    [...n.dirs.values()].sort((a, b) => a.name.localeCompare(b.name)).forEach((c) => folder(c, depth + 1, false));
    n.files
      .slice()
      .sort((a, b) => rows[a].relPath.localeCompare(rows[b].relPath))
      .forEach((i) => {
        const r = rows[i];
        html.push(`<div class="node file${isSel(r) ? " sel" : ""}" role="treeitem" tabindex="0" data-i="${i}" style="--d:${depth + 1}">
          <span class="chev"></span>${r.state !== "present" ? tag(r.state, STATE[r.state] || r.state) : ""}${tag(r.class, CLASS[r.class] || r.class)}
          <span class="name">${esc(base(r.relPath))}${r.isDir ? "/" : ""}</span>
          <span class="size">${r.isDir ? "" : esc(size(r.size))}</span>${talkTag(r)}</div>`);
      });
  };
  buildTree().forEach((n) => folder(n, 0, true));
  $("tree").innerHTML = html.join("");
}

function setView(mode) {
  viewMode = mode;
  try {
    localStorage.setItem("offstage.view", mode);
  } catch {}
  loadRows();
}

$("view-table").onclick = () => setView("table");
$("view-tree").onclick = () => setView("tree");
$("tree-expand").onclick = () => {
  expanded = new Set(folderKeys());
  renderTree();
};
$("tree-collapse").onclick = () => {
  expanded = new Set();
  renderTree();
};
$("tree").addEventListener("click", (ev) => treeActivate(ev.target));
$("tree").addEventListener("keydown", (ev) => {
  if (ev.key === "Enter" || ev.key === " ") {
    ev.preventDefault();
    treeActivate(ev.target);
  }
});

function treeActivate(target) {
  const node = target.closest(".node");
  if (!node) return;
  if (node.dataset.key) {
    const k = node.dataset.key;
    if (expanded.has(k)) expanded.delete(k);
    else expanded.add(k);
    renderTree();
    const again = [...$("tree").querySelectorAll(".node.folder")].find((x) => x.dataset.key === k);
    if (again) again.focus();
  } else if (node.dataset.i != null) {
    const r = rows[node.dataset.i];
    showDetail(r.repo, r.relPath);
  }
}

async function showDetail(repo, relPath) {
  selected = { repo, relPath };
  document.querySelectorAll("#rows tr, #tree .node.file").forEach((el) => {
    const r = rows[el.dataset.i];
    el.classList.toggle("sel", !!r && r.repo === repo && r.relPath === relPath);
  });
  const d = await invoke("detail", { repo, relPath });
  const box = $("detail");
  if (!d) {
    box.innerHTML = `<p class="hint">記録にありません（取り込み直しで外れた可能性があります）</p>`;
    return;
  }
  const r = d.row;
  const lost = r.state === "missing" || r.state === "moved";
  const canClass = r.class !== "doc";
  const ov = d.overrideOf;
  box.innerHTML = `
    <h2>${esc(r.relPath)}${r.isDir ? "/" : ""}</h2>
    <div class="tags">${tag(r.state, STATE[r.state] || r.state)}${tag(r.class, CLASS[r.class] || r.class)}${r.folded ? tag("", "1 件に畳んだフォルダ") : ""}</div>
    <dl>
      <dt>リポジトリ</dt><dd>${esc(r.repo)}</dd>
      <dt>場所</dt><dd>${esc(d.absPath)}${d.exists ? "" : "（今は無い）"}</dd>
      ${r.isDir ? "" : `<dt>大きさ</dt><dd>${esc(size(r.size))}</dd>`}
      <dt>更新</dt><dd>${esc(date(r.modified))}</dd>
      ${r.stateSince && r.state !== "present" ? `<dt>${esc(STATE[r.state])}</dt><dd>${esc(date(r.stateSince))} に気づいた</dd>` : ""}
      ${r.movedTo ? `<dt>移動先</dt><dd>${esc(r.movedTo)}</dd>` : ""}
      ${d.rule ? `<dt>規則</dt><dd>${esc(d.rule)}</dd>` : ""}
      ${ov ? `<dt>上書き</dt><dd>${esc(ov[0])} → ${esc(CLASS[ov[1]] || ov[1])}</dd>` : ""}
    </dl>
    <div class="actions">
      <button type="button" class="btn" id="act-open" title="エクスプローラーで場所を開く（無ければ在る親のフォルダ）">${ico("open")}開く</button>
      ${lost ? `<button type="button" class="btn" id="act-ok" title="消えた・移動したのを承知した。印から外す">${ico("ok")}了解</button>` : ""}
    </div>
    ${
      canClass
        ? `<h3>区分</h3>
    <div class="actions">
      <select id="act-target" title="付け替える範囲">${scopes(r.relPath).map((p) => `<option value="${esc(p)}">${esc(p)}${p === r.relPath ? "" : "/"}</option>`).join("")}</select>
      <select id="act-class" title="付け替える区分">${["personal", "tooldata", "secret", "generated"].map((c) => `<option value="${c}"${c === r.class ? " selected" : ""}>${CLASS[c]}</option>`).join("")}</select>
      <button type="button" class="btn" id="act-set" title="選んだ範囲の区分を付け替える（次の取り込みからも効く）">${ico("apply")}変更</button>
      ${ov ? `<button type="button" class="btn danger" id="act-unset" title="付け替えを外し、規則に戻す（次の取り込みで反映）">${ico("undo")}戻す</button>` : ""}
    </div>
    <p class="note">「生成」にしたフォルダは、次の取り込みから中を見ません。</p>`
        : ""
    }
    <h3>会話 ${d.scenes.length ? `（${d.scenes.length}）` : ""}</h3>
    <div id="scene-kinds" class="roles" role="group" aria-label="表示する出方"></div>
    <div id="scene-list"></div>`;
  renderSceneList(d.scenes, r.relPath);
  $("act-open").onclick = () => invoke("reveal", { repo, relPath }).catch(alertError);
  if ($("act-ok")) $("act-ok").onclick = () => invoke("dismiss", { repo, relPath }).then(reloadAll).catch(alertError);
  if ($("act-set"))
    $("act-set").onclick = () =>
      invoke("set_class", { repo, relPrefix: $("act-target").value, class: $("act-class").value }).then(reloadAll).catch(alertError);
  if ($("act-unset")) $("act-unset").onclick = () => invoke("clear_class", { repo, relPrefix: ov[0] }).then(reloadAll).catch(alertError);
}

const ROLE = { user: "人", assistant: "Claude", tool: "道具", result: "結果", attachment: "添付" };

/// 場面の前後のやり取りを窓で見せる
async function openScene(s, relPath) {
  const dlg = $("scene");
  const body = $("scene-body");
  $("scene-title").textContent = s.title || "（題なし）";
  $("scene-sub").textContent = `${s.sessionId} ・ ${s.lineNo} 行目 ・ ${relPath}`;
  body.innerHTML = `<p class="note">読み込み中…</p>`;
  if (!dlg.open) dlg.showModal();
  let turns;
  try {
    turns = await invoke("scene", { logPath: s.logPath, lineNo: s.lineNo });
  } catch (e) {
    body.innerHTML = `<p class="err">${esc(e)}</p>`;
    return;
  }
  // 同じ行に道具の呼び出しが並ぶことがあるので、強調はこのファイルの名前を含むこまに絞る (どれにも無ければ行全体のまま)
  const name = base(relPath).toLowerCase();
  const named = turns.filter((t) => t.hit && t.text.toLowerCase().includes(name));
  if (named.length) for (const t of turns) t.hit = named.includes(t);
  lastTurns = turns;
  renderTurns();
  const hit = body.querySelector(".hit");
  if (hit) hit.scrollIntoView({ block: "center" });
}

let lastTurns = [];
let rawMode = false;
/// 会話の窓で隠している種類 (この場面のこまは隠さない)
const hiddenRoles = new Set();

/// 種類の切り替えボタン (押すと、その種類のこまを隠す / 出す)
function renderRoleChips() {
  const count = {};
  for (const t of lastTurns) count[t.role] = (count[t.role] || 0) + 1;
  $("scene-roles").innerHTML = Object.keys(ROLE)
    .map((r) => {
      const on = !hiddenRoles.has(r);
      return `<button type="button" class="btn chip" data-role="${r}" aria-pressed="${on}"
        title="${on ? "押すと隠す" : "押すと出す"}（この場面のこまは隠さない）"${count[r] ? "" : " disabled"}>${esc(ROLE[r])} ${count[r] || 0}</button>`;
    })
    .join("");
}

$("scene-roles").addEventListener("click", (ev) => {
  const b = ev.target.closest("[data-role]");
  if (!b || b.disabled) return;
  const r = b.dataset.role;
  if (hiddenRoles.has(r)) hiddenRoles.delete(r);
  else hiddenRoles.add(r);
  renderTurns();
});

/// 人と Claude の文は Markdown として組む (原文の切り替えで元の文字に戻す)。道具の呼び出しと結果はそのまま
function renderTurns() {
  renderRoleChips();
  const shown = lastTurns.filter((t) => t.hit || !hiddenRoles.has(t.role));
  const hidden = lastTurns.length - shown.length;
  $("scene-body").innerHTML = (hidden ? `<p class="note">${hidden} こまを隠しています</p>` : "") + shown
    .map((t) => {
      const asMd = !rawMode && (t.role === "user" || t.role === "assistant");
      return `<div class="turn ${esc(t.role)}${t.hit ? " hit" : ""}">
        <div class="who">${tag(t.role, ROLE[t.role] || t.role)}${t.label ? `<span class="sid">${esc(t.label)}</span>` : ""}
          ${t.timestamp ? `<span class="sid">${esc(isoDate(t.timestamp))}</span>` : ""}${t.hit ? tag("content", "この場面") : ""}</div>
        ${asMd ? `<div class="md">${md(t.text)}</div>` : `<pre>${esc(t.text)}</pre>`}</div>`;
    })
    .join("");
}

/// Markdown を HTML にする。必ず DOMPurify で無害化し (会話の中の HTML からアプリの機能を呼ばせない)、
/// 画像は読み込まず代わりの文字に、リンクは移動しない文字にする (行き先は吹き出しで見せる)
function md(text) {
  if (!window.marked || !window.DOMPurify) return `<pre>${esc(text)}</pre>`;
  const raw = window.marked.parse(text, { async: false, gfm: true, breaks: true });
  const clean = window.DOMPurify.sanitize(raw, {
    FORBID_TAGS: ["style", "form", "input", "button", "textarea", "select", "iframe", "object", "embed", "video", "audio", "source", "picture", "svg", "math"],
    FORBID_ATTR: ["style", "srcset"],
  });
  const doc = new DOMParser().parseFromString(`<div>${clean}</div>`, "text/html");
  for (const img of doc.querySelectorAll("img")) {
    const alt = img.getAttribute("alt") || "画像";
    const span = doc.createElement("span");
    span.className = "md-img";
    span.title = img.getAttribute("src") || "";
    span.textContent = `[${alt}]`;
    img.replaceWith(span);
  }
  for (const a of doc.querySelectorAll("a")) {
    const span = doc.createElement("span");
    span.className = "md-link";
    span.title = a.getAttribute("href") || "";
    span.textContent = a.textContent;
    a.replaceWith(span);
  }
  return doc.body.firstElementChild.innerHTML;
}

$("scene-close").onclick = () => $("scene").close();
$("scene-raw").onclick = () => {
  rawMode = !rawMode;
  $("scene-raw").setAttribute("aria-pressed", String(rawMode));
  renderTurns();
};

/// 詳細の欄で隠している出方 (別のファイルを選んでも保つ)
const hiddenKinds = new Set();

/// 詳細の欄の、会話に出た場面の一覧。上の出方のボタンで隠す / 出す
function renderSceneList(scenes, relPath) {
  const count = {};
  for (const s of scenes) count[s.kind] = (count[s.kind] || 0) + 1;
  const kinds = Object.keys(KIND).filter((k) => count[k]);
  const chips = $("scene-kinds");
  chips.innerHTML = kinds
    .map((k) => {
      const on = !hiddenKinds.has(k);
      return `<button type="button" class="btn chip" data-kind="${k}" aria-pressed="${on}"
        title="${esc(KIND[k][2])}（${on ? "押すと隠す" : "押すと出す"}）">${esc(KIND[k][0])} ${count[k]}</button>`;
    })
    .join("");
  chips.onclick = (ev) => {
    const b = ev.target.closest("[data-kind]");
    if (!b) return;
    if (hiddenKinds.has(b.dataset.kind)) hiddenKinds.delete(b.dataset.kind);
    else hiddenKinds.add(b.dataset.kind);
    renderSceneList(scenes, relPath);
  };
  const list = $("scene-list");
  if (!scenes.length) {
    list.innerHTML = `<p class="note">Claude の会話には出ていません。</p>`;
    return;
  }
  const shown = scenes.map((s, i) => [s, i]).filter(([s]) => !hiddenKinds.has(s.kind));
  const hidden = scenes.length - shown.length;
  list.innerHTML =
    (hidden ? `<p class="note">${hidden} 件を隠しています</p>` : "") +
    shown
      .map(([s, i]) => {
        const k = KIND[s.kind] || [s.kind, false, ""];
        return `<button type="button" class="scene" data-scene="${i}" title="この場面の前後のやり取りを開く">
            <span class="meta">${tag(k[1] ? "content" : "", k[0], k[2])}
            <span>${esc(isoDate(s.timestamp))}</span>${s.tool ? `<span class="sid">${esc(s.tool)}</span>` : ""}
            <span class="go">${ico("talk")}表示</span></span>
            <span class="title">${esc(s.title || "（題なし）")}</span>
            <span class="sid">${esc(s.sessionId)} ・ ${s.lineNo} 行目</span></button>`;
      })
      .join("");
  list.querySelectorAll("[data-scene]").forEach((b) => {
    b.onclick = () => openScene(scenes[b.dataset.scene], relPath);
  });
}

/// 付け替えの範囲の候補: ファイル自身と、上のフォルダ全部
function scopes(rel) {
  const parts = rel.split("/");
  const out = [rel];
  for (let i = parts.length - 1; i >= 1; i--) out.push(parts.slice(0, i).join("/"));
  return out;
}

function alertError(e) {
  $("status").innerHTML = `<span class="err">${esc(e)}</span>`;
}

async function reloadAll() {
  await loadOverview();
  await loadRows();
  if (selected) await showDetail(selected.repo, selected.relPath);
}

$("refresh").onclick = async () => {
  const b = $("refresh");
  b.disabled = true;
  b.querySelector(".label").textContent = "取り込み中…";
  try {
    const r = await invoke("refresh");
    await reloadAll();
    const parts = [`取り込み ${r.seconds.toFixed(1)} 秒`, `追加 ${r.added}`, `変更 ${r.changed}`, `消失 ${r.missing}`, `移動 ${r.moved}`, `会話 +${r.mentionsAdded}`];
    let html = esc(parts.join(" ・ "));
    if (r.errors.length) {
      const tip = r.errors.map(([p, e]) => `${p}: ${e}`).join("\n");
      html += ` ・ <span class="err" title="${esc(tip)}">読めないリポジトリ ${r.errors.length}</span>`;
    }
    $("status").innerHTML = html;
  } catch (e) {
    alertError(e);
  } finally {
    b.disabled = false;
    b.querySelector(".label").textContent = "更新";
  }
};

$("alerts").addEventListener("click", (ev) => {
  const c = ev.target.closest("[data-alert]");
  if (!c || c.disabled) return;
  alertFilter = alertFilter === c.dataset.alert ? "" : c.dataset.alert;
  loadOverview().then(loadRows);
});

$("rows").addEventListener("click", (ev) => {
  const tr = ev.target.closest("tr[data-i]");
  if (tr) showDetail(rows[tr.dataset.i].repo, rows[tr.dataset.i].relPath);
});
$("rows").addEventListener("keydown", (ev) => {
  const tr = ev.target.closest("tr[data-i]");
  if (tr && (ev.key === "Enter" || ev.key === " ")) {
    ev.preventDefault();
    showDetail(rows[tr.dataset.i].repo, rows[tr.dataset.i].relPath);
  }
});

let timer = null;
for (const id of ["q-text", "q-ext"]) $(id).addEventListener("input", () => {
  clearTimeout(timer);
  timer = setTimeout(loadRows, 200);
});
for (const id of ["q-class", "q-state", "q-mention", "q-repo"]) $(id).addEventListener("change", loadRows);
$("clear").onclick = () => {
  for (const id of ["q-text", "q-ext"]) $(id).value = "";
  for (const id of ["q-class", "q-state", "q-mention", "q-repo"]) $(id).value = "";
  alertFilter = "";
  loadOverview().then(loadRows);
};

// 見本の画面を撮るときだけ: 起動してすぐ取り込み、指定の行を選ぶ (環境変数 OFFSTAGE_AUTOREFRESH / OFFSTAGE_SELECT)
(async () => {
  await reloadAll();
  const s = (await invoke("overview")).settings;
  // RepoTether の行から開いたとき: そのリポジトリで絞る (記録に無ければ絞らない)
  if (s.initialRepo) {
    const want = s.initialRepo.replace(/[\\/]+$/, "").toLowerCase();
    const opt = [...$("q-repo").options].find((o) => o.value.replace(/[\\/]+$/, "").toLowerCase() === want);
    if (opt) {
      $("q-repo").value = opt.value;
      await loadRows();
    }
  }
  if (s.view === "tree" || s.view === "table") {
    viewMode = s.view;
    renderList();
  }
  if (s.autoRefresh) await $("refresh").onclick();
  if (s.select) {
    const r = rows.find((x) => `${base(x.repo)}/${x.relPath}` === s.select);
    if (r) await showDetail(r.repo, r.relPath);
    const first = document.querySelector("[data-scene]");
    if (s.openScene && first) first.click();
  }
})().catch(alertError);
