#!/usr/bin/env python3
"""Summarises an `strace -f -tt -T -e trace=execve,exit_group,openat,connect` log of an app start:
the processes the start executed (when, how long each lived, the command) and the directories it opened
files in (how many, the first at). Usage: strace_summary.py LOGFILE. Std only."""
import collections
import re
import sys


def seconds(stamp):
    h, m, s = stamp.split(":")
    return int(h) * 3600 + int(m) * 60 + float(s)


def main():
    lines = open(sys.argv[1], errors="replace").read().splitlines()
    rx = re.compile(r"^(\d+)\s+(\d\d:\d\d:\d\d\.\d+)\s+(.*)$")
    first = None
    starts, ends, execs = {}, {}, []
    opens, opens_at = collections.Counter(), {}
    for line in lines:
        m = rx.match(line)
        if not m:
            continue
        pid, at, rest = m.group(1), seconds(m.group(2)), m.group(3)
        first = at if first is None else first
        if rest.startswith("execve(") and "= 0" in rest:
            cmd = rest[7:rest.find("], ")].replace('"', "") if "], " in rest else rest[:80]
            starts[pid] = at
            execs.append((at - first, pid, cmd[:110]))
        elif rest.startswith("exit_group(") or "+++ exited" in rest:
            ends.setdefault(pid, at)
        elif rest.startswith("openat(") and "ENOENT" not in rest:
            found = re.search(r'"([^"]+)"', rest)
            if found:
                directory = found.group(1).rsplit("/", 1)[0]
                opens[directory] += 1
                opens_at.setdefault(directory, at - first)
    print(f"processes executed (execve that succeeded, the first one is the app itself): {len(execs)}")
    total = 0.0
    for offset, pid, cmd in execs[1:]:
        life = (ends.get(pid, starts[pid]) - starts[pid]) * 1000
        total += life
        print(f"  +{offset * 1000:8.1f} ms  lived {life:7.1f} ms  {cmd}")
    print(f"sum of the lifetimes of the processes it spawned: {total:.0f} ms")
    print("directories it opened files in (count, first at), top 25:")
    for directory, count in opens.most_common(25):
        print(f"  {count:5d}  +{opens_at[directory] * 1000:8.1f} ms  {directory}")


main()
