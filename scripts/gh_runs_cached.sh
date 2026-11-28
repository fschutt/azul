#!/usr/bin/env bash
# scripts/gh_runs_cached.sh [N] [workflow-file]: the newest N (default 30) workflow runs of the repository, one per line:
#   <id> <status>/<conclusion> <head sha, 10 chars> <name> | <display title>
# ONE conditional request (If-None-Match): every agent and coordinator shares ONE token with a 5000/h REST quota, and an answer
# 304 (not modified) does NOT count against it, so polling this every minute costs nothing while nothing changes (a call per
# `gh run view` per run costs a call each time). Use it instead of `gh run list` / `gh run view` loops; poll no more often than every
# 60 s. REMOTE_REPO overrides the repository (default fschutt/azul).
set -euo pipefail
REPO="${REMOTE_REPO:-fschutt/azul}"
N="${1:-30}"
WF="${2:-}"
dir="${TMPDIR:-/tmp}/remote-sh-cache"; mkdir -p "$dir"
if [ -n "$WF" ]; then endpoint="repos/$REPO/actions/workflows/$WF/runs?per_page=$N"; else endpoint="repos/$REPO/actions/runs?per_page=$N"; fi
name="runs-$(printf '%s-%s-%s' "$REPO" "$N" "${WF:-all}" | tr '/ ' '__')"
etag=""
[ -f "$dir/$name.etag" ] && [ -f "$dir/$name.body" ] && etag="$(cat "$dir/$name.etag")"
if [ -n "$etag" ]; then out="$(gh api -i -H "If-None-Match: $etag" "$endpoint" 2>/dev/null)" || true; else out="$(gh api -i "$endpoint" 2>/dev/null)" || true; fi
first="${out%%$'\n'*}"; first="${first%$'\r'}"
case "$first" in
  *" 304 "*) body="$(cat "$dir/$name.body")";;
  *" 200 "*)
    out="${out//$'\r'/}"; head="${out%%$'\n\n'*}"; body="${out#*$'\n\n'}"
    etag="$(printf '%s\n' "$head" | sed -n 's/^[Ee][Tt][Aa][Gg]: *\(.*\)$/\1/p')"; etag="${etag%%$'\n'*}"
    if [ -n "$etag" ] && [ -n "$body" ]; then printf '%s\n' "$etag" > "$dir/$name.etag"; printf '%s\n' "$body" > "$dir/$name.body"; fi;;
  *) echo "gh_runs_cached: no answer ($first): rate limit? see: gh api rate_limit --jq .resources.core" >&2; exit 1;;
esac
printf '%s\n' "$body" | python3 -c 'import json,sys
for r in json.load(sys.stdin).get("workflow_runs", []):
    print("%s %s/%s %s %s | %s" % (r["id"], r.get("status"), r.get("conclusion") or "-", str(r.get("head_sha", ""))[:10], r.get("name"), str(r.get("display_title", ""))[:70]))'
