"""Writes made-up chats into a demo folder so screenshots show no real data.

    python docs/make-demo-data.py <dir>
    then run the app with APPDATA=<dir>/AppData and CLAUDE_CONFIG_DIR=<dir>/.claude
"""
import json, sys, time, uuid
from pathlib import Path

root = Path(sys.argv[1])
sessions = root / "AppData" / "Claude" / "claude-code-sessions" / "acct" / "org"
projects = root / ".claude" / "projects" / "C--dev-demo"
sessions.mkdir(parents=True, exist_ok=True)
projects.mkdir(parents=True, exist_ok=True)

now = int(time.time() * 1000)

def iso(ms):
    return time.strftime("%Y-%m-%dT%H:%M:%S.000Z", time.gmtime(ms / 1000))

def tool(name, inp, result):
    tid = "toolu_" + uuid.uuid4().hex[:20]
    return [("assistant", [{"type": "tool_use", "id": tid, "name": name, "input": inp}]),
            ("user", [{"type": "tool_result", "tool_use_id": tid, "content": result}])]

chats = [
    ("Add dark mode to the settings page", [
        ("user", "Can you add a dark mode toggle to the settings page? It should remember the choice."),
        ("assistant", "Sure. I'll look at how the settings page and the theme are set up first."),
        *tool("Grep", {"pattern": "theme", "description": "Find theme handling"}, "src/settings.tsx:12\nsrc/theme.ts:3"),
        *tool("Read", {"file_path": r"C:\dev\demo\src\theme.ts"}, "export const light = { bg: '#fff', fg: '#111' };"),
        ("assistant", "The app has a single `light` theme in `src/theme.ts`. Here's the plan:\n\n"
                      "1. Add a `dark` palette next to `light`.\n"
                      "2. Store the choice in `localStorage` under `theme`.\n"
                      "3. Add a toggle to the settings page that switches between the two.\n\n"
                      "```ts\nexport const dark = { bg: '#121212', fg: '#eee' };\n```"),
        *tool("Edit", {"file_path": r"C:\dev\demo\src\theme.ts"}, "The file has been updated."),
        *tool("Bash", {"command": "npm test", "description": "Run the test suite"}, "Tests: 42 passed, 42 total"),
        ("assistant", "Done. The toggle sits under **Appearance**, the choice survives a reload, and all 42 tests pass."),
        ("user", "Perfect, thanks!"),
        ("assistant", "You're welcome! Want me to make it follow the system setting by default too?"),
    ]),
    ("Why is the CI build slow?", [
        ("user", "Our CI takes 14 minutes. Any idea why?"),
        ("assistant", "Most of the time goes into installing dependencies without a cache. Caching `~/.npm` should cut it to about 4 minutes."),
    ]),
    ("Write unit tests for the parser", [
        ("user", "Please write tests for `parse_date`."),
        ("assistant", "I added 9 tests covering ISO dates, time zones and invalid input. All pass."),
    ]),
    ("Rename the API endpoints", [("user", "Rename /v1/items to /v1/products."), ("assistant", "Renamed, with a redirect from the old path.")]),
    ("Fix the login redirect loop", [("user", "After login I land back on /login."), ("assistant", "The session cookie was set for the wrong path. Fixed.")]),
    ("Plan the database migration", [("user", "How should we move from SQLite to Postgres?"), ("assistant", "In three steps: dual-write, backfill, then switch reads.")]),
    ("Explain the build script", [("user", "What does build.rs do?"), ("assistant", "It generates the protobuf bindings before compiling.")]),
    ("Speed up image loading", [("user", "Images load slowly on mobile."), ("assistant", "Serving WebP and lazy-loading below the fold halves the load time.")]),
    ("Set up the release workflow", [("user", "Can you set up releases on tag push?"), ("assistant", "Added a workflow that builds and attaches binaries on every `v*` tag.")]),
]

for n, (title, msgs) in enumerate(chats):
    cli = str(uuid.uuid4())
    last = now - n * 5_400_000
    meta = {"sessionId": f"local_{uuid.uuid4()}", "cliSessionId": cli, "cwd": r"C:\dev\demo",
            "title": title, "createdAt": last - 600_000, "lastActivityAt": last, "isArchived": False}
    (sessions / f"{meta['sessionId']}.json").write_text(json.dumps(meta), encoding="utf8")
    with open(projects / f"{cli}.jsonl", "w", encoding="utf8") as f:
        t = last - 600_000
        for role, content in msgs:
            t += 40_000
            f.write(json.dumps({"type": role, "timestamp": iso(t),
                                "message": {"role": role, "content": content}}) + "\n")
print(f"{len(chats)} demo chats in {root}")
