#!/usr/bin/env bash
# Verify the menubar dropdown: open (single popup?), item-click + dismissal.
set -u
cd /home/fs/Development/azul
export DISPLAY=:0.0
LOG=/tmp/mb_run.log
pkill -x azul-paint 2>/dev/null; sleep 0.6
AZ_BACKEND=cpu AZ_LOG=off ./target/release/azul-paint >"$LOG" 2>&1 &
WID=""
for i in $(seq 1 30); do WID=$(xdotool search --name "Azul Window" 2>/dev/null | head -1); [ -n "$WID" ] && break; sleep 0.3; done
[ -z "$WID" ] && { echo "NO WINDOW"; tail -15 "$LOG"; exit 1; }
AX=$(xwininfo -id "$WID" | awk '/Absolute upper-left X/{print $NF}')
AY=$(xwininfo -id "$WID" | awk '/Absolute upper-left Y/{print $NF}')
menu_count(){ xwininfo -root -tree 2>/dev/null | grep -c '"Menu"'; }
echo "main=($AX,$AY)  menus_before=$(menu_count)"

# 1) Open File menu (client 21,12)
xdotool mousemove $((AX+21)) $((AY+12)) click 1; sleep 0.8
echo "[open] menus=$(menu_count)  (expect 1 after double-dispatch fix)"
import -window root /tmp/mb2_open.png 2>/dev/null
MENULINE=$(xwininfo -root -tree 2>/dev/null | grep '"Menu"' | head -1)
echo "[open] menu geom: $MENULINE"
GEOM=$(echo "$MENULINE" | grep -oE '[0-9]+x[0-9]+\+[0-9]+\+[0-9]+' | head -1)
MX=$(echo "$GEOM" | sed -E 's/.*\+([0-9]+)\+[0-9]+/\1/')
MY=$(echo "$GEOM" | sed -E 's/.*\+[0-9]+\+([0-9]+)/\1/')

# 2) Click first menu item (~18px below menu top, ~30px in)
if [ -n "${MX:-}" ] && [ -n "${MY:-}" ]; then
  echo "[item] clicking ($((MX+30)),$((MY+18)))"
  xdotool mousemove $((MX+30)) $((MY+18)) click 1; sleep 0.8
  echo "[item] menus=$(menu_count)  (expect 0 = dismissed)"
  import -window root /tmp/mb2_afteritem.png 2>/dev/null
fi

# 3) Re-open + dismiss by clicking outside
xdotool mousemove $((AX+21)) $((AY+12)) click 1; sleep 0.6
echo "[reopen] menus=$(menu_count)"
xdotool mousemove $((AX+400)) $((AY+400)) click 1; sleep 0.6
echo "[outside] menus=$(menu_count)  (expect 0 = dismissed)"

echo "=== [[MBDBG]] (if any) ==="; grep -F "[[MBDBG]]" "$LOG" 2>/dev/null | tail -12
echo "app pid: $(pgrep -x azul-paint | head -1)"
