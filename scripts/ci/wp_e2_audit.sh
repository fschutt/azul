#!/usr/bin/env bash
# wp_e2_audit.sh: the host pictures of AzMaps and AzCode in ONE job on ONE Linux runner: the desktop QA build of the two apps (the shipped
# library plus the debug server and the e2e executor, scripts/ci/desktop_e2e_build.sh), then scripts/wp_e2_audit.py walks each app's window
# through the acceptance sizes (desktop, Pixel 8, iPhone, a small phone, landscape) and the states of its card, and names every picture.
#
#   scripts/remote.sh run <ref> ubuntu-22.04 'bash scripts/ci/wp_e2_audit.sh' --codegen --artifacts 'demo-out/wp-e2/**
#   demo-out/results/*.json'
#
# WP_E2_ONLY=maps|code runs one app only; WP_E2_SKIP_BUILD=<dir> takes the QA builds of an earlier run (a directory holding
# AzMaps-e2e-linux and AzCode-e2e-linux) instead of building the library again.
# Steps do not stop each other; the last lines are one `STEP <name> <exit>` each, the exit code is 0 only when every step passed.
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

only="${WP_E2_ONLY:-}"
apps="${WP_E2_APPS:-AzMaps AzCode}"
if [ -n "${WP_E2_SKIP_BUILD:-}" ]; then
  bins="$WP_E2_SKIP_BUILD"
else
  # shellcheck disable=SC2086
  step build bash scripts/ci/desktop_e2e_build.sh linux $apps
  bins=demo-out/apps
fi
mkdir -p demo-out/wp-e2
for f in "$bins"/*-e2e-linux; do [ -f "$f" ] && chmod +x "$f"; done

if [ -z "$only" ] || [ "$only" = maps ]; then
  step audit_maps python3 scripts/wp_e2_audit.py maps --bin "$bins/AzMaps-e2e-linux" --out demo-out/wp-e2/audit/maps --debug-port 8791
fi
if [ -z "$only" ] || [ "$only" = code ]; then
  step audit_code python3 scripts/wp_e2_audit.py code --bin "$bins/AzCode-e2e-linux" --out demo-out/wp-e2/audit/code --debug-port 8792
fi

echo
bad=0
for i in "${!names[@]}"; do
  echo "STEP ${names[$i]} ${codes[$i]}"
  [ "${codes[$i]}" -eq 0 ] || bad=1
done
ls -la demo-out/wp-e2/audit/maps demo-out/wp-e2/audit/code 2>/dev/null | head -80
exit "$bad"
