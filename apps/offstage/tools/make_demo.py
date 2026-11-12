"""Build a fictional environment for Offstage screenshots (no real file names ever appear).

    python -I apps/offstage/tools/make_demo.py <out_dir>

Creates <out_dir>/roots/{notes-app,web-shop} (git repos with ignored personal files and tracked docs)
and <out_dir>/projects/<folder>/<session>.jsonl (a Claude session that reads and mentions some of them).
Then run Offstage with:
    OFFSTAGE_DB=<out_dir>/index.sqlite OFFSTAGE_ROOTS=<out_dir>/roots OFFSTAGE_PROJECTS=<out_dir>/projects OFFSTAGE_AUTOREFRESH=1
"""
import json
import os
import shutil
import subprocess
import sys

out = os.path.abspath(sys.argv[1])
if os.path.exists(out):
    # git objects are read-only: clear the flag and retry
    shutil.rmtree(out, onexc=lambda fn, p, e: (os.chmod(p, 0o666), fn(p)))
roots = os.path.join(out, "roots")
projects = os.path.join(out, "projects")

PNG = bytes.fromhex(
    "89504e470d0a1a0a0000000d4948445200000001000000010806000000"
    "1f15c4890000000d49444154789c6360f8cfc0f01f0005000201e2b0b06a0000000049454e44ae426082"
)


def write(path, data):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    mode = "wb" if isinstance(data, bytes) else "w"
    kw = {} if isinstance(data, bytes) else {"encoding": "utf-8", "newline": "\n"}
    with open(path, mode, **kw) as f:
        f.write(data)


def git(repo, *args):
    subprocess.run(
        ["git", "-C", repo, "-c", "user.name=demo", "-c", "user.email=demo@example.invalid", *args],
        check=True,
        capture_output=True,
    )


repos = {
    "notes-app": {
        ".gitignore": "sandbox/\n.work/\nlogs/\nnode_modules/\n",
        "README.md": "# notes-app\n",
        "docs/plan.md": "# plan\n",
        "docs/screen.png": PNG,
        "sandbox/interview-memo.md": "memo\n",
        "sandbox/customer-list.xlsx": b"PK\x03\x04demo",
        "sandbox/old-draft.md": "old\n",
        "sandbox/_shots/run-01.png": PNG,
        ".work/demo_api_token.json": '{"token": "demo"}\n',
        "logs/sync.log": "ok\n",
        "node_modules/pkg/index.js": "x\n",
    },
    "web-shop": {
        ".gitignore": "sandbox/\nslack/\n.dev.vars\n",
        "README.md": "# web-shop\n",
        "docs/spec.md": "# spec\n",
        "sandbox/price-notes.md": "notes\n",
        "slack/2026-11-10-thread.txt": "thread\n",
        ".dev.vars": "API_KEY=demo\n",
    },
}
for name, files in repos.items():
    repo = os.path.join(roots, name)
    os.makedirs(repo)
    git(repo, "init", "-q")
    for rel, data in files.items():
        write(os.path.join(repo, rel), data)
    git(repo, "add", "-A")
    git(repo, "commit", "-q", "-m", "demo")

na = os.path.join(roots, "notes-app")
ws = os.path.join(roots, "web-shop")
folder = os.path.join(projects, "demo-notes-app")
lines = [
    {"type": "user", "cwd": na, "sessionId": "5f1c2a90-demo", "timestamp": "2026-11-10T09:12:00Z",
     "message": {"content": "sandbox/interview-memo.md をもとに docs/plan.md を直したい"}},
    {"type": "assistant", "cwd": na, "sessionId": "5f1c2a90-demo", "timestamp": "2026-11-10T09:12:30Z",
     "message": {"content": [
         {"type": "tool_use", "name": "Read", "input": {"file_path": os.path.join(na, "sandbox", "interview-memo.md")}},
         {"type": "tool_use", "name": "Edit", "input": {"file_path": os.path.join(na, "docs", "plan.md"), "old_string": "a", "new_string": "b"}},
     ]}},
    {"type": "user", "cwd": na, "sessionId": "5f1c2a90-demo", "timestamp": "2026-11-10T09:12:31Z",
     "message": {"content": [{"type": "tool_result", "content": "1\tmemo\n"}]}},
    {"type": "assistant", "cwd": na, "sessionId": "5f1c2a90-demo", "timestamp": "2026-11-10T09:20:00Z",
     "message": {"content": [
         {"type": "tool_use", "name": "Bash", "input": {"command": "cat .work/demo_api_token.json"}},
     ]}},
    {"type": "assistant", "cwd": na, "sessionId": "5f1c2a90-demo", "timestamp": "2026-11-10T09:25:00Z",
     "message": {"content": [{"type": "text", "text": (
         "## 計画を直しました\n\n"
         "面談メモの要点を **3 つ** に絞って `docs/plan.md` に入れました。\n\n"
         "| 項目 | 前 | 後 |\n|---|---|---|\n| 締め切り | 12 月 | 1 月 |\n| 担当 | 未定 | 2 人 |\n\n"
         "- 残りは [計画](docs/plan.md) の末尾に書いた\n"
         "- 外の資料: [手引き](https://example.invalid/guide)\n\n"
         "```sh\ngit diff docs/plan.md\n```\n\n"
         f"> 下書きは {os.path.join(na, 'sandbox', 'old-draft.md')} に残っています。"
     )}]}},
    {"type": "ai-title", "aiTitle": "面談メモから計画を直す", "sessionId": "5f1c2a90-demo"},
]
write(os.path.join(folder, "5f1c2a90-demo.jsonl"), "".join(json.dumps(l, ensure_ascii=False) + "\n" for l in lines))
folder2 = os.path.join(projects, "demo-web-shop")
lines2 = [
    {"type": "user", "cwd": ws, "sessionId": "9a7e03bd-demo", "timestamp": "2026-11-11T14:02:00Z",
     "message": {"content": "slack/2026-11-10-thread.txt の件、docs/spec.md に反映して"}},
    {"type": "assistant", "cwd": ws, "sessionId": "9a7e03bd-demo", "timestamp": "2026-11-11T14:03:00Z",
     "message": {"content": [
         {"type": "tool_use", "name": "Read", "input": {"file_path": os.path.join(ws, "slack", "2026-11-10-thread.txt")}},
         {"type": "tool_use", "name": "Read", "input": {"file_path": os.path.join(ws, "docs", "spec.md")}},
     ]}},
    {"type": "ai-title", "aiTitle": "Slack の相談を仕様に反映", "sessionId": "9a7e03bd-demo"},
]
write(os.path.join(folder2, "9a7e03bd-demo.jsonl"), "".join(json.dumps(l, ensure_ascii=False) + "\n" for l in lines2))
print(out)
