#!/bin/bash
# Start (or repair) ydotoold so that REAL pointer and keyboard input can be
# replayed into a running azul window on Wayland - the one thing the E2E
# `mouse_move` / `key_down` ops cannot do, because they only write window
# state and never run the platform's own input handlers.
#
#   ./scripts/ydotool_start.sh          # start the daemon, fix the socket, self-test
#   ./scripts/ydotool_start.sh restart  # stop, then start
#   ./scripts/ydotool_start.sh stop     # stop it again
#
# Afterwards (ydotool 0.1.x syntax - the version Ubuntu/Mint ship):
#   ydotool mousemove -- -10000 -10000  # relative; a big negative move parks it top-left
#   ydotool mousemove 960 540           # ... so this lands on (960, 540)
#   ydotool click 1                     # 1 left, 2 right, 3 middle (press + release)
#   ydotool type --key-delay 20 'text'
#   ydotool key Esc                     # key names, combos as ctrl+a
# The 0.1.x CLI cannot HOLD a button, so it cannot drag. The daemon itself
# replays any `struct input_event` a client writes to its socket, which can:
# scripts/ydotool_input.py speaks that directly (down / move / up / wheel).
#
# Why a script: /dev/uinput is root-only, so the daemon needs sudo; the socket
# it creates is root-only by default (0600), so clients fail with "backend
# unavailable ... failed to open uinput device" even while it runs; and a
# daemon started with a trailing `&` from an interactive helper dies with that
# shell, leaving a STALE socket file behind that looks alive but is not.
# `systemd-run` keeps it alive as a transient unit instead.
#
# THE POINTER: KWin applies the pointer ACCELERATION profile to the virtual
# device like to any mouse, so a relative move of N pixels lands anywhere
# but N pixels away - one large "go to (x, y)" jump pins the cursor in a
# screen corner. The script gives THAT device a flat profile at neutral
# speed (1:1); the real mouse keeps its settings.
set -u

SOCKET=/tmp/.ydotool_socket
UNIT=ydotoold-azul

if ! command -v ydotoold >/dev/null 2>&1; then
    echo "ydotoold is not installed (apt install ydotool)" >&2
    exit 1
fi

stop_daemon() {
    sudo systemctl stop "$UNIT.service" 2>/dev/null
    sudo pkill -x ydotoold 2>/dev/null
    sudo rm -f "$SOCKET"
    echo "ydotoold stopped"
}

case "${1:-}" in
    stop) stop_daemon; exit 0 ;;
    restart) stop_daemon ;;
esac

alive() { pgrep -x ydotoold >/dev/null 2>&1; }

if ! alive; then
    # A leftover socket from a dead daemon makes clients fail instead of retry.
    [ -S "$SOCKET" ] && sudo rm -f "$SOCKET"
    echo "starting ydotoold as transient unit $UNIT (needs sudo)"
    sudo systemd-run --quiet --unit="$UNIT" --collect \
        "$(command -v ydotoold)" --socket-path="$SOCKET" --socket-perm=0666 \
        || { echo "systemd-run failed; falling back to nohup" >&2;
             sudo nohup "$(command -v ydotoold)" --socket-path="$SOCKET" --socket-perm=0666 \
                 >/dev/null 2>&1 & }
fi

# Wait for the socket, then make sure it is usable by this user whatever the
# daemon was started with.
for _ in $(seq 1 50); do [ -S "$SOCKET" ] && break; sleep 0.1; done
if [ ! -S "$SOCKET" ]; then
    echo "no socket at $SOCKET after 5s - is the daemon running? (journalctl -u $UNIT)" >&2
    exit 1
fi
[ "$(stat -c %a "$SOCKET")" = "666" ] || sudo chmod 666 "$SOCKET"

export YDOTOOL_SOCKET="$SOCKET"
if ! ydotool mousemove 0 0 2>&1 | grep -q 'Using ydotoold backend'; then
    echo "the client still cannot reach the daemon (socket $SOCKET)" >&2
    exit 1
fi
echo "ydotoold running (unit $UNIT), socket $SOCKET is $(stat -c %a "$SOCKET")"
# Flat acceleration for the virtual device (see THE POINTER above).
dev=$(grep -l 'ydotoold virtual device' /sys/class/input/event*/device/name 2>/dev/null | head -1)
if [ -n "$dev" ]; then
    kdev=/org/kde/KWin/InputDevice/$(basename "$(dirname "$(dirname "$dev")")")
    if busctl --user introspect org.kde.KWin "$kdev" >/dev/null 2>&1; then
        busctl --user set-property org.kde.KWin "$kdev" org.kde.KWin.InputDevice \
            pointerAccelerationProfileFlat b true
        busctl --user set-property org.kde.KWin "$kdev" org.kde.KWin.InputDevice \
            pointerAcceleration d 0
        echo "pointer: flat 1:1 acceleration on $kdev"
    else
        echo "pointer: not KWin - check that the compositor does not accelerate the virtual device" >&2
    fi
fi
echo "export YDOTOOL_SOCKET=$SOCKET"
