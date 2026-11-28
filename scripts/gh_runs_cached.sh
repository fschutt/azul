#!/usr/bin/env bash
# gh_runs_cached.sh [N] [workflow-file]: the newest N (default 30) workflow runs of the repository, one per line:
#   <id> <status>/<conclusion> <head sha, 10 chars> <name> | <display title>
# ONE conditional request (If-None-Match) through scripts/gh_cond.py: every agent and coordinator shares ONE token with a 5000/h REST
# quota, and an answer 304 (not modified) does NOT count against it, so polling this every minute costs nothing while nothing
# changes (a `gh run view` per run costs a call each time). Use it instead of `gh run list` / `gh run view` loops; poll no more
# often than every 60 s. REMOTE_REPO overrides the repository (default fschutt/azul).
set -euo pipefail
REPO="${REMOTE_REPO:-fschutt/azul}"
N="${1:-30}"
WF="${2:-}"
here="$(cd "$(dirname "$0")" && pwd)"
if [ -n "$WF" ]; then endpoint="repos/$REPO/actions/workflows/$WF/runs?per_page=$N"; else endpoint="repos/$REPO/actions/runs?per_page=$N"; fi
name="runs-$(printf '%s-%s-%s' "$REPO" "$N" "${WF:-all}" | tr '/ ' '__')"
exec python3 "$here/gh_cond.py" runs "$endpoint" "$name"
