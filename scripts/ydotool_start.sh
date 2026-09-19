#!/bin/bash
# Start (or repair) ydotoold so that REAL pointer and keyboard input can be
# replayed into a running azul window on Wayland - the one thing the E2E
# `mouse_move` / `key_down` ops cannot do, because they only write window
# state and never run the platform's own input handlers.
#
#   ./scripts/ydotool_start.sh          # start the daemon, fix the socket, self-test
#   ./scripts/ydotool_start.sh stop     # stop it again
#
# Afterwards, in the shell that drives the tests:
#   export YDOTOOL_SOCKET=/tmp/.ydotool_socket
#   ydotool mousemove --absolute -x 960 -y 540
#   ydotool click 0xC0                  # left click (0x40 press, 0x80 release, 0xC1 right)
#   ydotool type --key-delay 20 'text'
#   ydotool key 1:1 1:0                 # Escape (evdev code 1, press then release)
#
# Why a script: /dev/uinput is root-only, so the daemon needs sudo; the socket
# it creates is root-only by default (0600), so clients fail with "backend
# unavailable ... failed to open uinput device" even while it runs; and a
# daemon started with a trailing `&` from an interactive helper dies with that
# shell, leaving a STALE socket file behind that looks alive but is not.
# `systemd-run` keeps it alive as a transient unit instead.
set -u

SOCKET=/tmp/.ydotool_socket
UNIT=ydotoold-azul

if ! command -v ydotoold >/dev/null 2>&1; then
    echo "ydotoold is not installed (apt install ydotool)" >&2
    exit 1
fi

if [ "${1:-}" = "stop" ]; then
    sudo systemctl stop "$UNIT.service" 2>/dev/null
    sudo rm -f "$SOCKET"
    echo "ydotoold stopped"
    exit 0
fi

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
if ydotool mousemove -x 0 -y 0 2>&1 | grep -q 'backend unavailable'; then
    echo "the client still cannot reach the daemon (socket $SOCKET)" >&2
    exit 1
fi
echo "ydotoold running (unit $UNIT), socket $SOCKET is $(stat -c %a "$SOCKET")"
echo "export YDOTOOL_SOCKET=$SOCKET"
