#!/usr/bin/env bash
# CI run monitor — surface failing jobs + their actual error logs fast, so a
# red GitHub Actions run can be triaged without clicking through the web UI.
#
#   scripts/ci_monitor.sh <run-id>            # one snapshot (status + failures)
#   scripts/ci_monitor.sh <run-id> watch      # poll until the run completes
#   scripts/ci_monitor.sh                     # latest run on this branch
#
# Env: GH overrides the gh binary; CI_MONITOR_INTERVAL the poll seconds;
#      LOG_TAIL the number of failed-log lines to print on completion.
set -uo pipefail

GH="${GH:-/c/Program Files/GitHub CLI/gh.exe}"
INTERVAL="${CI_MONITOR_INTERVAL:-45}"

RUN="${1:-}"
MODE="${2:-snapshot}"
if [ -z "$RUN" ]; then
  RUN="$("$GH" run list --workflow=build --limit 1 --json databaseId -q '.[0].databaseId' 2>/dev/null)"
fi

# Prints one status line + failed-job/step list. Echoes "STATUS|CONCLUSION" last.
snapshot() {
  "$GH" run view "$RUN" --json status,conclusion,displayTitle,jobs 2>/dev/null \
    | python - "$RUN" <<'PY'
import json,sys
run=sys.argv[1]
raw=sys.stdin.read()
if not raw.strip():
    print("  (gh returned no data — rate-limited or run not found)")
    print("unknown|")
    sys.exit(0)
d=json.loads(raw)
status=d.get("status"); concl=d.get("conclusion") or ""
jobs=d.get("jobs",[])
from collections import Counter
c=Counter(j.get("conclusion") or "queued/running" for j in jobs)
import datetime
print(f"-- run {run} status={status} conclusion={concl or '(none)'} jobs={dict(c)}")
for j in jobs:
    if j.get("conclusion") in ("failure","cancelled","timed_out"):
        print(f"  x {j['conclusion']:9} {j['name']}")
        for s in j.get("steps",[]):
            if s.get("conclusion") in ("failure","timed_out"):
                print(f"        step: {s['name']}")
print(f"{status}|{concl}")
PY
}

dump_failed_logs() {
  echo
  echo "================= FAILED STEP LOGS (tail ${LOG_TAIL:-300}) ================="
  "$GH" run view "$RUN" --log-failed 2>/dev/null | tail -n "${LOG_TAIL:-300}"
}

run_once() {
  local out line status concl
  out="$(snapshot)"
  echo "$out" | sed '$d'
  line="$(echo "$out" | tail -1)"
  status="${line%%|*}"; concl="${line#*|}"
  echo "$status|$concl"
}

if [ "$MODE" = "watch" ]; then
  while :; do
    line="$(run_once)"
    [ "${line%%|*}" = "completed" ] && { dump_failed_logs; break; }
    sleep "$INTERVAL"
  done
else
  line="$(run_once)"
  [ "${line%%|*}" = "completed" ] && [ "${line#*|}" != "success" ] && dump_failed_logs
fi
