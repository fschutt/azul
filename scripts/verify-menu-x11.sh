#!/usr/bin/env bash
# Runtime-verify azul menu popups on X11 by driving REAL input + screenshotting.
#
# The menu/context-menu/dropdown popups are separate always-on-top X windows, so
# we screenshot the whole root to capture them. This is the way to confirm a popup
# actually appears at the right place (synthetic AZ_DEBUG events do NOT drive X11
# hit-test). xdotool + import (ImageMagick) / maim must be installed.
#
# Usage:  scripts/verify-menu-x11.sh <out.png> <xdotool-action...>
#   <out.png>            where to write the screenshot
#   <xdotool-action...>  xdotool args; the token %WIN% is replaced with the
#                        azul-paint window id. Coordinates with `--window %WIN%`
#                        are window-relative (deterministic; the window is first
#                        moved to 50,50).
#
# Examples:
#   # right-click canvas center -> context menu
#   scripts/verify-menu-x11.sh /tmp/ctx.png "mousemove --window %WIN% 400 300 click 3"
#   # left-click the "File" menubar item (top-left) -> dropdown
#   scripts/verify-menu-x11.sh /tmp/file.png "mousemove --window %WIN% 24 46 click 1"
#
# Env: WIN_NAME (default azul-paint), BIN (default target/release/azul-paint),
#      AZ_LOG (default off), SETTLE (post-action sleep, default 0.8),
#      WIN_W/WIN_H (optional forced window size for deterministic coords).
set -u
cd "$(dirname "$0")/.."
OUT="${1:?usage: verify-menu-x11.sh out.png xdotool-action...}"; shift
BIN="${BIN:-target/release/azul-paint}"
WIN_NAME="${WIN_NAME:-Azul Window}"
SETTLE="${SETTLE:-0.8}"

if [ ! -x "$BIN" ]; then echo "FAIL: $BIN not built"; exit 2; fi

pkill -f "$BIN" 2>/dev/null; sleep 0.4
AZ_BACKEND=cpu AZ_LOG="${AZ_LOG:-off}" "$BIN" >/tmp/azul-paint-run.log 2>&1 &
APP_PID=$!

# Wait for the window to map (up to ~7s)
WIN=""
for i in $(seq 1 24); do
  WIN=$(xdotool search --onlyvisible --name "$WIN_NAME" 2>/dev/null | head -1)
  [ -n "$WIN" ] && break
  sleep 0.3
done
if [ -z "$WIN" ]; then
  echo "FAIL: window '$WIN_NAME' never mapped"; echo "--- app log ---"; cat /tmp/azul-paint-run.log
  pkill -f "$BIN" 2>/dev/null; exit 1
fi

# Deterministic placement (and optional size) so window-relative coords are stable.
xdotool windowmove "$WIN" 50 50 2>/dev/null
if [ -n "${WIN_W:-}" ] && [ -n "${WIN_H:-}" ]; then
  xdotool windowsize "$WIN" "$WIN_W" "$WIN_H" 2>/dev/null
fi
xdotool windowactivate --sync "$WIN" 2>/dev/null
sleep 0.8

# Run the requested action, substituting the window id for %WIN%.
ACTION="${*//%WIN%/$WIN}"
echo "+ xdotool $ACTION"
xdotool $ACTION
sleep "$SETTLE"

# Capture the whole screen (popup is a separate window).
if command -v import >/dev/null 2>&1; then import -window root "$OUT"; else maim "$OUT"; fi
echo "screenshot: $OUT"
echo "window=$WIN geom=$(xdotool getwindowgeometry "$WIN" 2>/dev/null | tr '\n' ' ')"
echo "open windows named '$WIN_NAME': $(xdotool search --name "$WIN_NAME" 2>/dev/null | tr '\n' ' ')"

pkill -f "$BIN" 2>/dev/null; sleep 0.2
exit 0
