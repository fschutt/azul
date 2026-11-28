#!/usr/bin/env python3
"""gh_cond.py: GitHub REST reads as CONDITIONAL requests (If-None-Match), for agents and coordinators.

Every agent and coordinator shares ONE token with a 5000/h REST quota, and an answer 304 (not modified) does NOT count against it:
polling an unchanged run or list costs nothing. The ETag and the body are kept in ${TMPDIR:-/tmp}/remote-sh-cache. Plain python
(no bash string operations: macOS has bash 3.2, where splitting a 400 KB answer is quadratic and burned a CPU for minutes).

  gh_cond.py state <endpoint> <cache-name>              queued | in_progress | <the conclusion of a finished run>   (endpoint: repos/<repo>/actions/runs/<id>)
  gh_cond.py find-tag <endpoint> <cache-name> <tag>      the id of the run whose display_title ends with <tag> (endpoint: .../workflows/remote.yml/runs?per_page=30)
  gh_cond.py runs <endpoint> <cache-name>                one line per run: <id> <status>/<conclusion> <sha10> <name> | <title>
  gh_cond.py body <endpoint> <cache-name>                the JSON body

Exit 0 with the answer; exit 1 when there is none (an error, the rate limit, the network): treat that as 'no verdict yet'.
"""
import json
import os
import subprocess
import sys


def cache_dir():
    d = os.path.join(os.environ.get("TMPDIR", "/tmp"), "remote-sh-cache")
    os.makedirs(d, exist_ok=True)
    return d


def cond_get(endpoint, name):
    """The body (bytes) of GET endpoint, from the cache on a 304; None when there is no answer."""
    base = os.path.join(cache_dir(), name)
    etag = None
    if os.path.exists(base + ".etag") and os.path.exists(base + ".body"):
        etag = open(base + ".etag").read().strip()
    cmd = ["gh", "api", "-i"]
    if etag:
        cmd += ["-H", "If-None-Match: " + etag]
    cmd.append(endpoint)
    try:
        run = subprocess.run(cmd, capture_output=True, timeout=120)
    except (OSError, subprocess.TimeoutExpired):
        return None
    raw = run.stdout.replace(b"\r", b"")
    head, _, body = raw.partition(b"\n\n")
    first = head.split(b"\n", 1)[0].decode("latin1") if head else ""
    if " 304 " in first and os.path.exists(base + ".body"):
        return open(base + ".body", "rb").read()
    if " 200 " in first and body:
        new = ""
        for line in head.split(b"\n"):
            if line.lower().startswith(b"etag:"):
                new = line.split(b":", 1)[1].strip().decode("latin1")
                break
        if new:
            tmp = base + ".body.tmp"
            with open(tmp, "wb") as f:
                f.write(body)
            os.replace(tmp, base + ".body")
            with open(base + ".etag", "w") as f:
                f.write(new + "\n")
        return body
    return None


def main(argv):
    if len(argv) < 4:
        sys.stderr.write(__doc__)
        return 2
    mode, endpoint, name = argv[1], argv[2], argv[3]
    body = cond_get(endpoint, name)
    if body is None:
        sys.stderr.write("gh_cond: no answer (rate limit? see: gh api rate_limit --jq .resources.core)\n")
        return 1
    if mode == "body":
        sys.stdout.buffer.write(body)
        return 0
    try:
        data = json.loads(body)
    except ValueError:
        return 1
    if mode == "state":
        print(data.get("conclusion") if data.get("status") == "completed" else data.get("status", ""))
        return 0
    if mode == "find-tag":
        tag = argv[4] if len(argv) > 4 else ""
        for run in data.get("workflow_runs", []):
            if str(run.get("display_title", "")).endswith(tag):
                print(run["id"])
                return 0
        return 1
    if mode == "runs":
        for run in data.get("workflow_runs", []):
            print("%s %s/%s %s %s | %s" % (run["id"], run.get("status"), run.get("conclusion") or "-",
                                           str(run.get("head_sha", ""))[:10], run.get("name"),
                                           str(run.get("display_title", ""))[:70]))
        return 0
    sys.stderr.write(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
