#!/usr/bin/env python3
"""Drive REAL input through ydotoold's socket: press, hold, drag, release.

ydotoold (started by scripts/ydotool_start.sh) replays every
`struct input_event` a client writes to /tmp/.ydotool_socket through its
uinput device. The 0.1.x `ydotool` CLI only knows click = press+release, so
it cannot hold a button while moving - i.e. it cannot drag, select text,
move a slider or a scrollbar thumb. This speaks the socket directly.

Positions are SCREEN pixels; `moveto` parks the pointer top-left first,
so it is absolute - provided the compositor moves the virtual device 1:1
(scripts/ydotool_start.sh sets KWin's flat acceleration for it).

Usage:  ydotool_input.py <cmd> [args...] ...   (executed in order)
  reset                 move the pointer to the top-left corner (relative -10000,-10000)
  moveto X Y            absolute screen position (reset + relative move)
  move DX DY            relative move
  down [left|right]     press and hold a mouse button
  up [left|right]       release it
  click [left|right]
  key NAME              press+release a key (Esc, Tab, Enter, Left, Right, Up, Down,
                        Home, End, Backspace, Delete, a-z, 0-9, space, ctrl+a, ctrl+c, ctrl+v, shift+Left ...)
  type TEXT             type ascii text (letters, digits, space, punctuation subset)
  sleep SECONDS
  path X1 Y1 X2 Y2 N    N-step move from (X1,Y1) to (X2,Y2), absolute, ~4ms apart (a real drag)
  wheel N               N wheel clicks (positive = up/away, negative = down)

Example - a fast drag-select that leaves the line band:
  ydotool_input.py moveto 700 460 down path 700 460 900 520 3 up
"""
import socket, struct, sys, time

SOCK = "/tmp/.ydotool_socket"
EV_SYN, EV_KEY, EV_REL = 0, 1, 2
REL_X, REL_Y, REL_WHEEL = 0, 1, 8
BTN = {"left": 0x110, "right": 0x111, "middle": 0x112}
KEYS = {
    "esc": 1, "escape": 1, "1": 2, "2": 3, "3": 4, "4": 5, "5": 6, "6": 7, "7": 8, "8": 9, "9": 10, "0": 11,
    "minus": 12, "equal": 13, "backspace": 14, "tab": 15, "q": 16, "w": 17, "e": 18, "r": 19, "t": 20,
    "y": 21, "u": 22, "i": 23, "o": 24, "p": 25, "enter": 28, "return": 28, "ctrl": 29, "a": 30, "s": 31,
    "d": 32, "f": 33, "g": 34, "h": 35, "j": 36, "k": 37, "l": 38, "semicolon": 39, "shift": 42,
    "z": 44, "x": 45, "c": 46, "v": 47, "b": 48, "n": 49, "m": 50, "comma": 51, "dot": 52, "slash": 53,
    "space": 57, "home": 102, "up": 103, "pageup": 104, "left": 105, "right": 106, "end": 107,
    "down": 108, "pagedown": 109, "delete": 111,
}
CHARS = {" ": "space", ".": "dot", ",": "comma", "-": "minus", "=": "equal", "/": "slash", ";": "semicolon", "\n": "enter"}


class Dev:
    def __init__(self):
        self.s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.s.connect(SOCK)

    def emit(self, typ, code, val):
        self.s.sendall(struct.pack("<qqHHi", 0, 0, typ, code, val))

    def syn(self):
        self.emit(EV_SYN, 0, 0)

    def move(self, dx, dy):
        # uinput REL events are clamped to i32; keep chunks sane
        while dx or dy:
            sx = max(-4000, min(4000, dx)); sy = max(-4000, min(4000, dy))
            if sx: self.emit(EV_REL, REL_X, sx)
            if sy: self.emit(EV_REL, REL_Y, sy)
            self.syn(); dx -= sx; dy -= sy
        time.sleep(0.01)

    def button(self, name, val):
        self.emit(EV_KEY, BTN[name], val); self.syn(); time.sleep(0.02)

    def key(self, spec):
        parts = spec.lower().split("+")
        mods = [p for p in parts[:-1]]
        k = parts[-1]
        for m in mods: self.emit(EV_KEY, KEYS[m], 1); self.syn()
        self.emit(EV_KEY, KEYS[k], 1); self.syn(); time.sleep(0.01)
        self.emit(EV_KEY, KEYS[k], 0); self.syn()
        for m in reversed(mods): self.emit(EV_KEY, KEYS[m], 0); self.syn()
        time.sleep(0.02)


def main(argv):
    d = Dev()
    pos = [None, None]
    i = 0
    while i < len(argv):
        c = argv[i]; i += 1
        if c == "reset":
            d.move(-10000, -10000); pos = [0, 0]
        elif c == "moveto":
            x, y = int(argv[i]), int(argv[i + 1]); i += 2
            d.move(-10000, -10000); d.move(x, y); pos = [x, y]
        elif c == "move":
            dx, dy = int(argv[i]), int(argv[i + 1]); i += 2
            d.move(dx, dy)
            if pos[0] is not None: pos = [pos[0] + dx, pos[1] + dy]
        elif c in ("down", "up", "click"):
            b = "left"
            if i < len(argv) and argv[i] in BTN: b = argv[i]; i += 1
            if c == "down": d.button(b, 1)
            elif c == "up": d.button(b, 0)
            else: d.button(b, 1); d.button(b, 0)
        elif c == "key":
            d.key(argv[i]); i += 1
        elif c == "type":
            for ch in argv[i]:
                name = CHARS.get(ch, ch.lower())
                if ch.isupper(): d.key("shift+" + name)
                else: d.key(name)
                time.sleep(0.02)
            i += 1
        elif c == "sleep":
            time.sleep(float(argv[i])); i += 1
        elif c == "wheel":
            d.emit(EV_REL, REL_WHEEL, int(argv[i])); d.syn(); time.sleep(0.03); i += 1
        elif c == "path":
            x1, y1, x2, y2, n = (int(v) for v in argv[i:i + 5]); i += 5
            # From where the pointer IS when that is known: parking it in the
            # corner first would drag a held button across the screen.
            if pos[0] is None:
                d.move(-10000, -10000); d.move(x1, y1)
            else:
                d.move(x1 - pos[0], y1 - pos[1])
            cx, cy = x1, y1
            for k in range(1, n + 1):
                nx = x1 + (x2 - x1) * k // n; ny = y1 + (y2 - y1) * k // n
                d.move(nx - cx, ny - cy); cx, cy = nx, ny
                time.sleep(0.004)
            pos = [x2, y2]
        else:
            raise SystemExit(f"unknown command {c}")
    d.s.close()


if __name__ == "__main__":
    main(sys.argv[1:])
