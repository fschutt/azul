#!/usr/bin/env bash
# a11y_desktop_job.sh [<App>]: ONE runner job of WP-A11Y on a desktop (Linux, headless): the desktop QA build of the app (scripts/ci/desktop_e2e_build.sh: the
# shipped libazul plus the debug server and the E2E executor) runs every a11y_*.json scenario of examples/azul-widgets/e2e/ on the headless backend, and the
# engine's frames of each (its take_screenshot steps) are kept in demo-out/qa-a11y-desktop/shots/<file stem>/. The settings are the override's
# (set_system_accessibility says them): a desktop headless window has no system setting to flip.
#   scripts/remote.sh run <full sha> ubuntu-22.04 'bash scripts/ci/a11y_desktop_job.sh' --codegen --artifacts 'demo-out/qa-a11y-desktop/**'
set -uxo pipefail
app="${1:-AzWidgets}"
# the scenarios take Chrome's numbers for 16px Arial, which azul draws in Liberation Sans on Linux: the CI's fonts first
if command -v apt-get >/dev/null 2>&1; then
  { sudo apt-get update -qq && sudo apt-get install -y -qq fonts-liberation fonts-dejavu-core; } >/dev/null 2>&1 || true
fi
bash scripts/ci/desktop_e2e_build.sh linux "$app" || { echo "the desktop QA build failed"; exit 3; }
bin="$(ls demo-out/apps/"$app"-e2e-linux 2>/dev/null | head -n 1)"
[ -n "$bin" ] || { echo "no desktop QA binary of $app"; exit 3; }
chmod +x "$bin"
out=demo-out/qa-a11y-desktop
mkdir -p "$out/shots"
for f in examples/azul-widgets/e2e/a11y_*.json; do
  stem="$(basename "$f" .json)"
  mkdir -p "$out/shots/$stem"
  AZ_E2E="$PWD/$f" AZ_BACKEND=headless AZ_LOG=off AZ_E2E_SHOT_DIR="$PWD/$out/shots/$stem" timeout 900 "$bin" >"$out/$stem.log" 2>&1
  echo "$stem: exit $?" | tee -a "$out/summary.txt"
done
find "$out/shots" -name '*.png' | sort | head -n 200
exit 0
