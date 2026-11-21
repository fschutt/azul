#!/usr/bin/env bash
# remote.sh: run a command on a GitHub-hosted runner (.github/workflows/remote.yml) instead of on this machine.
#
#   scripts/remote.sh run <ref> <runner> '<command>' [--codegen] [--artifacts '<globs>']   dispatch; prints the run id and URL
#       --codegen generates the bindings first (cargo run -r -p azul-doc codegen all): ANY command that builds azul-dll
#       (cargo test -p azul-dll ..., cargo check -p azul-dll ...) needs it
#   scripts/remote.sh status <id>                                               queued | in_progress | success | failure
#   scripts/remote.sh wait <id>                                                 poll until it is done; exit 0 when it succeeded
#   scripts/remote.sh log <id> [lines]                                          the end of the log (default 150 lines), also while it runs
#   scripts/remote.sh download <id> <dir>                                       the uploaded log and files
#
# <ref> is a branch (pushed to origin), tag or commit; <runner> is ubuntu-22.04, macos-15 or windows-2022; the
# command runs with bash at the repository root, e.g.
#   scripts/remote.sh run ci-wip/hf ubuntu-22.04 'cargo test -p azul-layout --lib running_text'
# A run is free (the repository is public) and does not queue behind your other builds; several can run at
# once. REMOTE_REPO overrides the repository (default fschutt/azul).
set -euo pipefail
REPO="${REMOTE_REPO:-fschutt/azul}"
cmd="${1:-}"; [ $# -gt 0 ] && shift || true

job_id() { gh run view "$1" --repo "$REPO" --json jobs --jq '.jobs[0].databaseId'; }

case "$cmd" in
  run)
    [ $# -ge 3 ] || { echo "usage: remote.sh run <ref> <runner> '<command>' [--artifacts '<globs>']" >&2; exit 2; }
    ref="$1"; os="$2"; command="$3"; shift 3
    artifacts=""
    codegen="false"
    while [ $# -gt 0 ]; do
      case "$1" in
        --artifacts) shift; artifacts="${1:-}";;
        --codegen) codegen="true";;
        *) echo "unknown option $1" >&2; exit 2;;
      esac
      shift
    done
    tag="r$(date +%s)$$"
    args=(-f ref="$ref" -f os="$os" -f command="$command" -f tag="$tag" -f codegen="$codegen")
    [ -n "$artifacts" ] && args+=(-f artifacts="$artifacts")
    gh workflow run remote.yml --repo "$REPO" --ref master "${args[@]}"
    id=""
    for _ in $(seq 1 40); do
      id="$(gh run list --repo "$REPO" --workflow remote.yml --limit 30 --json databaseId,displayTitle \
            --jq ".[] | select(.displayTitle | endswith(\"$tag\")) | .databaseId" | head -n 1)"
      [ -n "$id" ] && break
      sleep 3
    done
    [ -n "$id" ] || { echo "the run did not appear (tag $tag)" >&2; exit 1; }
    echo "$id"
    echo "https://github.com/$REPO/actions/runs/$id" >&2
    ;;
  status)
    gh run view "$1" --repo "$REPO" --json status,conclusion --jq 'if .status == "completed" then .conclusion else .status end'
    ;;
  wait)
    while true; do
      s="$(gh run view "$1" --repo "$REPO" --json status,conclusion --jq 'if .status == "completed" then .conclusion else .status end')"
      case "$s" in
        success) echo success; exit 0;;
        queued|in_progress|waiting|pending|requested) sleep 20;;
        *) echo "$s"; exit 1;;
      esac
    done
    ;;
  log)
    n="${2:-150}"
    text="$(gh api --allow-escape-sequences "repos/$REPO/actions/jobs/$(job_id "$1")/logs" 2>/dev/null \
      | perl -pe 's/\e\[[0-9;]*[A-Za-z]//g; s/^\S+Z //')"
    # the command's own output (between the markers of remote.yml) when it has begun, else the runner's log so far
    if printf '%s\n' "$text" | grep -q '^=== remote: begins ==='; then
      printf '%s\n' "$text" | awk '/^=== remote: begins ===/ { p = 1 } p { print } /^=== remote: finished/ { exit }' | tail -n "$n"
    else
      printf '%s\n' "$text" | awk '/^Post job cleanup\./ { exit } { print }' | tail -n "$n"
    fi
    ;;
  download)
    [ $# -ge 2 ] || { echo "usage: remote.sh download <id> <dir>" >&2; exit 2; }
    mkdir -p "$2"
    gh run download "$1" --repo "$REPO" --dir "$2"
    ;;
  *)
    sed -n '2,/^set -euo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//' >&2
    exit 2
    ;;
esac
