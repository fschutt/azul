#!/usr/bin/env python3
"""Run the install steps azul.rs DOCUMENTS for one language on one platform,
exactly as a user would type them, and require the resulting program to pass
the headless counter e2e.

    run_install_steps.py --api api.json --lang haskell --platform linux \
        --base https://azul.rs --version 0.2.0 [--variant linux-apt] \
        [--workdir DIR] [--e2e tests/e2e/hello_world_counter.json]

WHY: every language's routes live in api.json (installation.languages.<lang>.
install[], one entry per dropdown option on the frontpage: "Linux (apt)",
"pip (Linux / macOS)", "Windows (Scoop)") and are rendered on the frontpage.
Nothing ever executed them. The e2e matrix builds the examples from the
repo's own codegen tree, which proves the binding — not the documented
download-and-build route. This script takes the PUBLISHED api.json (curl'd
from the site), interpolates $HOSTNAME / $VERSION the way the frontpage does,
writes each route's command steps into one shell script (so `cd` persists,
like a terminal), writes `code` steps that carry a `file` target to that file
at that point in the sequence, and runs it with AZ_E2E + AZ_BACKEND=headless
in the environment. The last documented command starts the app; the e2e
runner inside libazul executes the counter script and prints
`test result: ok` (exit 0) or fails.

Which routes run: without --variant, every route whose `os` lists --platform
and that has no `container` (a route marked `container: fedora:40` assumes a
distribution the plain runner is not; the post-release `distros` job runs it
inside that image with --variant). Each route runs in its own directory; the
script fails if any of them fails, and says which.

Unix steps run under bash (`sudo` is shimmed to a no-op when already root, so
the apt steps work in a container). Windows steps run under Git Bash, or
under cmd.exe when they use cmd-isms (%VAR%, `set `, `ren `, `call `, `copy `).
"""
import argparse, json, os, re, shutil, subprocess, sys, tempfile

ap = argparse.ArgumentParser()
ap.add_argument("--api", required=True)
ap.add_argument("--lang", required=True)
ap.add_argument("--platform", required=True, choices=["linux", "macos", "windows"])
ap.add_argument("--variant", help="run only this route (its `id` in api.json), container routes included")
ap.add_argument("--base", default="https://azul.rs")
ap.add_argument("--version", required=True)
ap.add_argument("--workdir")
ap.add_argument("--e2e", default=os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "tests", "e2e", "hello_world_counter.json"))
ap.add_argument("--timeout", type=int, default=1800)
a = ap.parse_args()

doc = json.load(open(a.api, encoding="utf-8"))
inst = doc["installation"] if "installation" in doc else next(iter(doc.values()))["installation"]
cfg = inst["languages"].get(a.lang)
if not cfg:
    sys.exit(f"::error::no install steps for language {a.lang!r} in {a.api}")
routes = cfg.get("install") or []
if a.variant:
    selected = [v for v in routes if v["id"] == a.variant]
    if not selected:
        sys.exit(f"::error::{a.lang} documents no route {a.variant!r} (has: {', '.join(v['id'] for v in routes)})")
    if a.platform not in selected[0]["os"]:
        sys.exit(f"::error::{a.lang}/{a.variant} is documented for {', '.join(selected[0]['os'])}, not {a.platform}")
else:
    applicable = [v for v in routes if a.platform in v["os"]]
    if not applicable:
        sys.exit(f"::error::{a.lang} documents no {a.platform} steps")
    selected = [v for v in applicable if not v.get("container")]
    for v in applicable:
        if v.get("container"):
            print(f"::notice::{a.lang}/{v['id']}: assumes {v['container']}; run with --variant {v['id']} inside that image")
    if not selected:
        sys.exit(f"::error::{a.lang}: every {a.platform} route assumes a container; nothing to run on this runner")

base = a.base.rstrip("/")
def interp(s):
    return (s.replace("$$HOSTNAME$$", base).replace("$$VERSION$$", a.version)
             .replace("%HOSTNAME%", base).replace("%VERSION%", a.version)
             .replace("${VERSION}", a.version)
             .replace("$HOSTNAME", base).replace("$VERSION", a.version))

e2e = os.path.abspath(a.e2e)
if not os.path.isfile(e2e):
    sys.exit(f"::error::e2e script {e2e} is missing")
root = a.workdir or tempfile.mkdtemp(prefix=f"azul-{a.lang}-")
os.makedirs(root, exist_ok=True)

def run_route(v):
    steps = v["steps"]
    work = os.path.join(root, v["id"]) if len(selected) > 1 or a.workdir else root
    os.makedirs(work, exist_ok=True)
    stage = os.path.join(work, ".steps")
    os.makedirs(stage, exist_ok=True)

    cmds = [interp(s["content"]) for s in steps if s["type"] == "command"]
    win = a.platform == "windows"
    use_cmd = win and any(re.search(r"%[A-Za-z_]+%|(^|\s)set\s+\w+=|^(ren|call|copy)\s", c, re.M) for c in cmds)

    lines = []
    if use_cmd:
        lines += ["@echo on", "setlocal"]
    else:
        lines += ["#!/usr/bin/env bash", "set -euo pipefail",
                  # root in a container: the documented `sudo` is a no-op.
                  'sudo() { if [ "$(id -u)" = 0 ]; then "$@"; else command sudo "$@"; fi; }',
                  "export -f sudo 2>/dev/null || true"]
    n = 0
    for s in steps:
        t, content = s["type"], interp(s["content"])
        if t == "text":
            lines.append(("echo " + json.dumps("[doc] " + content)) if not use_cmd else f"echo [doc] {content[:120]}")
        elif t == "code":
            f = s.get("file")
            if not f:
                lines.append(f"echo {json.dumps('[doc] (code shown, not a file) ' + content[:80])}" if not use_cmd else "rem code step")
                continue
            n += 1
            src = os.path.join(stage, f"code{n}")
            open(src, "w", encoding="utf-8", newline="\n").write(content + "\n")
            d = os.path.dirname(f)
            if use_cmd:
                if d: lines.append(f'if not exist "{d}" mkdir "{d}"')
                lines.append(f'copy /y "{src}" "{f}" >nul || exit /b 1')
            else:
                if d: lines.append(f"mkdir -p {json.dumps(d)}")
                lines.append(f"cp {json.dumps(src)} {json.dumps(f)}")
            lines.append(f"echo [doc] wrote {f}")
        elif t == "command":
            if use_cmd:
                lines.append(content)
                lines.append("if errorlevel 1 exit /b 1")
            else:
                lines.append(content)

    script = os.path.join(work, "run.cmd" if use_cmd else "run.sh")
    open(script, "w", encoding="utf-8", newline="\r\n" if use_cmd else "\n").write("\n".join(lines) + "\n")
    print(f"=== {a.lang} / {a.platform} / {v['id']} ({v['label']}): {len(cmds)} documented commands, in {work}")
    print("\n".join(f"    $ {c}" for c in cmds))
    sys.stdout.flush()

    env = dict(os.environ, AZ_E2E=e2e, AZ_BACKEND="headless")
    if use_cmd:
        argv = ["cmd.exe", "/c", script]
    else:
        bash = shutil.which("bash") or "/bin/bash"
        argv = [bash, script]
    try:
        p = subprocess.run(argv, cwd=work, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=a.timeout)
        out = p.stdout.decode("utf-8", "replace"); rc = p.returncode
    except subprocess.TimeoutExpired as ex:
        out = (ex.stdout or b"").decode("utf-8", "replace"); rc = 124
    clean = re.sub(r"\x1b\[[0-9;]*m", "", out)
    print(clean[-12000:])
    ok = rc == 0 and "test result: ok" in clean and "0 failed" in clean
    if ok:
        print(f"::notice::{a.lang}/{v['id']}: the documented steps build and run the counter e2e")
    else:
        print(f"::error::{a.lang}/{v['id']}: the documented steps FAILED (exit {rc}; e2e {'passed' if 'test result: ok' in clean else 'did not run'})")
    sys.stdout.flush()
    return ok

results = {v["id"]: run_route(v) for v in selected}
failed = [k for k, ok in results.items() if not ok]
if failed:
    sys.exit(f"::error::{a.lang}/{a.platform}: {len(failed)} of {len(results)} documented route(s) FAILED: {', '.join(failed)}")
print(f"::notice::{a.lang}/{a.platform}: all {len(results)} documented route(s) build and run the counter e2e")
