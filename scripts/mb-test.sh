#!/usr/bin/env bash
# Drive azul-paint on X11, hover+click the menubar "File" item, capture geometry,
# window-tree diff, screenshots, and any [[MBDBG]] probe output.
set -u
cd /home/fs/Development/azul
export DISPLAY=:0.0
LOG=/tmp/mb_run.log
pkill -x azul-paint 2>/dev/null; sleep 0.6
AZ_BACKEND=cpu AZ_LOG=off ./target/release/azul-paint >"$LOG" 2>&1 &
WID=""
for i in $(seq 1 30); do
  WID=$(xdotool search --name "Azul Window" 2>/dev/null | head -1)
  [ -n "$WID" ] && break
  sleep 0.3
done
if [ -z "$WID" ]; then echo "NO WINDOW"; tail -20 "$LOG"; exit 1; fi
AX=$(xwininfo -id "$WID" | awk '/Absolute upper-left X/{print $NF}')
AY=$(xwininfo -id "$WID" | awk '/Absolute upper-left Y/{print $NF}')
echo "window=$WID abs=($AX,$AY)"
# Menubar "File" item ~ client (21,12); pass alt coords as $1 $2 (client-relative)
CX=${1:-21}; CY=${2:-12}
FX=$((AX+CX)); FY=$((AY+CY))
echo "=== BEFORE: tree windows=$(xwininfo -root -tree 2>/dev/null | grep -c 0x), main pos:"
xwininfo -id "$WID" | grep "Absolute upper-left"
# Hover
xdotool mousemove "$FX" "$FY"; sleep 0.5
import -window root /tmp/mb_hover.png 2>/dev/null
# Click
xdotool mousemove "$FX" "$FY" click 1; sleep 1.0
echo "=== AFTER click at client($CX,$CY) abs($FX,$FY): tree windows=$(xwininfo -root -tree 2>/dev/null | grep -c 0x), main pos:"
xwininfo -id "$WID" | grep "Absolute upper-left"
import -window root /tmp/mb_afterclick.png 2>/dev/null
import -window "$WID" /tmp/mb_afterclick_win.png 2>/dev/null
echo "=== [[MBDBG]] probe output ==="
grep -F "[[MBDBG]]" "$LOG" | tail -40
echo "=== (app left running pid: $(pgrep -x azul-paint | head -1)) ==="
