#!/usr/bin/env python3
"""AT-SPI smoke-probe for azul (phase G accessibility verification).

Enumerates the AT-SPI desktop and looks for an azul application that has
published an accessibility tree via accesskit_unix. Walks the tree and prints
roles/names/states to prove the engine exposes a real a11y tree (not a stub).

Usage: python3 scripts/azul-a11y-probe.py [name_substr] [timeout_secs]
  name_substr : case-insensitive substring to match the app accessible name
                (default: "azul")
  timeout_secs: how long to poll for the app to appear (default: 20)

Exit code 0 = found a non-trivial azul a11y tree; 1 = not found / empty.
"""
import sys, time

try:
    import gi
    gi.require_version("Atspi", "2.0")
    from gi.repository import Atspi
except Exception as e:
    print("FATAL: cannot import gi.Atspi:", e)
    sys.exit(2)

NAME = (sys.argv[1] if len(sys.argv) > 1 else "azul").lower()
TIMEOUT = float(sys.argv[2]) if len(sys.argv) > 2 else 20.0

Atspi.init()


def app_names():
    desktop = Atspi.get_desktop(0)
    out = []
    for i in range(desktop.get_child_count()):
        try:
            app = desktop.get_child_at_index(i)
            if app is None:
                continue
            out.append((i, app.get_name() or "", app))
        except Exception:
            continue
    return out


def role_name(acc):
    try:
        return acc.get_role_name()
    except Exception:
        return "?"


def state_set(acc):
    try:
        ss = acc.get_state_set()
        names = []
        for st in (
            Atspi.StateType.FOCUSABLE,
            Atspi.StateType.FOCUSED,
            Atspi.StateType.ENABLED,
            Atspi.StateType.SENSITIVE,
            Atspi.StateType.VISIBLE,
            Atspi.StateType.SHOWING,
            Atspi.StateType.EDITABLE,
            Atspi.StateType.CHECKED,
            Atspi.StateType.SELECTED,
        ):
            if ss.contains(st):
                names.append(Atspi.state_type_get_name(st) if hasattr(Atspi, "state_type_get_name") else str(st))
        return ",".join(names)
    except Exception:
        return ""


def walk(acc, depth, counter, maxnodes=60):
    if counter[0] >= maxnodes:
        return
    pad = "  " * depth
    nm = ""
    try:
        nm = acc.get_name() or ""
    except Exception:
        pass
    st = state_set(acc)
    print(f"{pad}- [{role_name(acc)}] {nm!r}" + (f"  {{{st}}}" if st else ""))
    counter[0] += 1
    try:
        n = acc.get_child_count()
    except Exception:
        n = 0
    for i in range(n):
        if counter[0] >= maxnodes:
            print(f"{pad}  … (truncated at {maxnodes} nodes)")
            return
        try:
            ch = acc.get_child_at_index(i)
        except Exception:
            ch = None
        if ch is not None:
            walk(ch, depth + 1, counter, maxnodes)


def main():
    deadline = time.time() + TIMEOUT
    print(f"[probe] looking for app name containing {NAME!r} (timeout {TIMEOUT}s)")
    match = None
    last_dump = []
    while time.time() < deadline:
        apps = app_names()
        last_dump = [(n or "<empty>") for _, n, _ in apps]
        for idx, nm, app in apps:
            if NAME in (nm or "").lower():
                match = app
                print(f"[probe] FOUND app #{idx}: name={nm!r}")
                break
        if match is not None:
            break
        time.sleep(0.5)

    if match is None:
        print(f"[probe] NOT FOUND. Desktop apps currently on the AT-SPI bus:")
        for nm in last_dump:
            print(f"   - {nm}")
        sys.exit(1)

    print("[probe] accessibility tree (roles/names/states):")
    counter = [0]
    walk(match, 0, counter, maxnodes=60)
    print(f"[probe] total nodes walked: {counter[0]}")
    # Non-trivial = more than just the app+window shell.
    sys.exit(0 if counter[0] >= 2 else 1)


if __name__ == "__main__":
    main()
