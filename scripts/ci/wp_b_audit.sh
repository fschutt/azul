#!/usr/bin/env bash
# wp_b_audit.sh: the host pictures of AzNotes, AzContacts and AzTasks in ONE job on ONE Linux runner: the desktop QA build of the three
# apps (the shipped library plus the debug server and the e2e executor, scripts/ci/desktop_e2e_build.sh), then scripts/wp_b_audit.py walks
# each app's window through the acceptance sizes (desktop, Pixel 8, iPhone, a small phone, landscape, a tablet) and the states of its
# card, and names every picture.
#
#   scripts/remote.sh run <full sha> ubuntu-22.04 'WP_B_LABEL=before bash scripts/ci/wp_b_audit.sh' --codegen --artifacts 'demo-out/wp-b/**
#   demo-out/results/*.json'
#
# WP_B_LABEL=before|after names the walk (the ids of the old windows or of the widget family's frame); WP_B_ONLY=notes|contacts|tasks
# runs one app only; WP_B_SKIP_BUILD=<dir> takes the QA builds of an earlier run (a directory holding AzNotes-e2e-linux, ...) instead of
# building the library again. Steps do not stop each other; the last lines are one `STEP <name> <exit>` each, the exit code is 0 only when
# every step passed.
set -uo pipefail
names=()
codes=()
step() {
  local name="$1"
  shift
  echo "=== STEP $name: $* ==="
  "$@"
  local code=$?
  names+=("$name")
  codes+=("$code")
  echo "=== STEP $name: exit $code ==="
  return 0
}

label="${WP_B_LABEL:-before}"
only="${WP_B_ONLY:-}"
if [ -n "${WP_B_SKIP_BUILD:-}" ]; then
  bins="$WP_B_SKIP_BUILD"
else
  step build bash scripts/ci/desktop_e2e_build.sh linux AzNotes AzContacts AzTasks
  bins=demo-out/apps
fi
mkdir -p "demo-out/wp-b/$label"
for f in "$bins"/*-e2e-linux; do [ -f "$f" ] && chmod +x "$f"; done

port=8801
for pair in notes:AzNotes contacts:AzContacts tasks:AzTasks; do
  key="${pair%%:*}"
  bin="${pair##*:}"
  if [ -z "$only" ] || [ "$only" = "$key" ]; then
    step "audit_$key" python3 scripts/wp_b_audit.py "$key" --bin "$bins/$bin-e2e-linux" --out "demo-out/wp-b/$label/$key" --label "$label" --debug-port "$port"
  fi
  port=$((port + 1))
done

echo
bad=0
for i in "${!names[@]}"; do
  echo "STEP ${names[$i]} ${codes[$i]}"
  [ "${codes[$i]}" -eq 0 ] || bad=1
done
ls -la "demo-out/wp-b/$label"/* 2>/dev/null | head -150
exit "$bad"
