#!/usr/bin/env bash
# Trace the menu pointer-grab: open File dropdown, click an item, click outside,
# right-click for a context menu — logging XGrabPointer return + which window
# receives each click.
set -u
cd /home/fs/Development/azul
export DISPLAY=:0.0
LOG=/tmp/menugrab.log
pkill -x azul-paint 2>/dev/null; sleep 0.6
AZ_BACKEND=cpu AZ_LOG=off ./target/release/azul-paint >"$LOG" 2>&1 &
for i in $(seq 1 30); do WID=$(xdotool search --name "Azul Window" 2>/dev/null | head -1); [ -n "$WID" ] && break; sleep 0.3; done
[ -z "${WID:-}" ] && { echo "NO WINDOW"; exit 1; }
AX=$(xwininfo -id "$WID" | awk '/Absolute upper-left X/{print $NF}')
AY=$(xwininfo -id "$WID" | awk '/Absolute upper-left Y/{print $NF}')
echo "main win=$WID at ($AX,$AY)"
echo "--- open File dropdown ---"
xdotool mousemove $((AX+21)) $((AY+12)) click 1; sleep 0.9
MENU=$(xwininfo -root -tree 2>/dev/null | grep '"Menu"' | head -1)
echo "menu line: $MENU"
GEOM=$(echo "$MENU" | grep -oE '[0-9]+x[0-9]+\+[0-9]+\+[0-9]+' | head -1)
MX=$(echo "$GEOM" | sed -E 's/.*\+([0-9]+)\+[0-9]+/\1/'); MY=$(echo "$GEOM" | sed -E 's/.*\+[0-9]+\+([0-9]+)/\1/')
if [ -n "${MX:-}" ]; then
  echo "--- click item 'New' inside dropdown at ($((MX+30)),$((MY+20))) ---"
  xdotool mousemove $((MX+30)) $((MY+20)) click 1; sleep 0.9
fi
echo "--- click far outside ($((AX+300)),$((AY+440))) ---"
xdotool mousemove $((AX+300)) $((AY+440)) click 1; sleep 0.9
echo "menus now: $(xwininfo -root -tree 2>/dev/null | grep -c '"Menu"')"
echo "=== [[MENUDBG]] trace ==="
grep -F "[[MENUDBG]]" "$LOG"
