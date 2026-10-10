#!/usr/bin/env python3
"""Function order from PGO entry counts: every function that ran, hottest first (packed at the start of
__text); the cold rest keeps the linker's order after them. Writes IR names, one per line.
Usage: order_from_counts.py <profdata> <out> <llvm-profdata>"""
import re, subprocess, sys
profdata, out, P = sys.argv[1], sys.argv[2], sys.argv[3]
proc = subprocess.Popen([P, "show", "--all-functions", profdata], stdout=subprocess.PIPE, text=True)
name, rows = None, []
for line in proc.stdout:
    m = re.match(r"^  (\S.*):$", line)
    if m:
        name = m.group(1)
        continue
    m = re.match(r"^\s+Function count: (\d+)", line)
    if m and name is not None:
        rows.append((int(m.group(1)), name))
        name = None
proc.wait()
hot = sorted((r for r in rows if r[0] > 0), key=lambda r: -r[0])
open(out, "w").write("\n".join(n for _, n in hot) + "\n")
print("%d functions in the profile, %d ran (%.1f%%)" % (len(rows), len(hot), 100.0 * len(hot) / max(1, len(rows))))
