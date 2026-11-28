#!/usr/bin/env python3
"""demo_status_now.py: the status table of a `demos` run that is still going (or that did not finish), made from what exists NOW.

    demo_status_now.py <run id> [--out <dir>] [--repo owner/name]

The report job of scripts/../.github/workflows/demos.yml makes the table only when every job of the run has ended. This makes the same
table at any moment: it lists the jobs of the run (`gh api .../actions/runs/<id>/jobs`), downloads the result artifacts that exist
(demoresults-*, demolibinfo-*, demosmoke-*: each only when it is there) and runs `demo_apps.status` over them with the state of the jobs, so
a cell whose job is queued or running says "pending: queued behind other jobs" / "building now" / "waiting for libazul" and not "failed",
while a cell whose job ended without a result is a failure. Writes <out>/demo-status-now.json and .md and prints the markdown.
Needs the `gh` command, logged in, and reads nothing else of the machine. Standard library only.
"""
import argparse
import json
import pathlib
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import demo_apps  # noqa: E402

PATTERNS = ("demoresults-", "demolibinfo-", "demosmoke-")


def gh(*args):
    r = subprocess.run(["gh", *args], capture_output=True, text=True)
    return r.returncode, r.stdout, r.stderr


def wanted_artifacts(names):
    """The artifacts of a run the table reads: the result, library-info and boot-test ones."""
    return sorted(n for n in names if n.startswith(PATTERNS))


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("run_id")
    ap.add_argument("--out", default="demo-status-now")
    ap.add_argument("--repo", default="fschutt/azul")
    a = ap.parse_args(argv)
    out = pathlib.Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    code, text, err = gh("api", "--paginate", f"repos/{a.repo}/actions/runs/{a.run_id}/jobs?per_page=100",
                         "--jq", ".jobs[] | {name, status, conclusion, started_at, completed_at}")
    if code != 0:
        sys.exit(f"demo_status_now: the jobs of run {a.run_id} cannot be listed: {err.strip()[:200]}")
    jobs_file = out / "demo-jobs.json"
    jobs_file.write_text(text, encoding="utf-8")
    code, text, err = gh("api", "--paginate", f"repos/{a.repo}/actions/runs/{a.run_id}/artifacts?per_page=100", "--jq", ".artifacts[].name")
    names = text.split() if code == 0 else []
    dirs = []
    for name in wanted_artifacts(names):
        target = out / name
        code, _, err = gh("run", "download", a.run_id, "--repo", a.repo, "-n", name, "-D", str(target))
        if code == 0:
            dirs.append(target)
        else:
            print(f"demo_status_now: {name} cannot be downloaded: {err.strip()[:120]}", file=sys.stderr)
    minutes = 0.0
    import datetime
    for line in jobs_file.read_text(encoding="utf-8").splitlines():
        try:
            j = json.loads(line)
            f = lambda s: datetime.datetime.fromisoformat(s.replace("Z", "+00:00"))
            if j.get("started_at") and j.get("completed_at"):
                minutes += (f(j["completed_at"]) - f(j["started_at"])).total_seconds() / 60
        except (ValueError, TypeError):
            continue
    data, md = demo_apps.status(dirs, jobs=demo_apps.read_jobs(jobs_file),
                                extra={"artifacts": len(names), "runner_minutes": round(minutes)})
    (out / "demo-status-now.json").write_text(json.dumps(data, indent=2, sort_keys=True), encoding="utf-8")
    (out / "demo-status-now.md").write_text(md, encoding="utf-8")
    print(md)
    s = data["summary"]
    print(f"\ncells pending: {s['pending_cells']}; unexplained failures: {len(s['unexplained_failures'])}; files built: {s['assets']}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
