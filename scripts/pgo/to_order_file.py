#!/usr/bin/env python3
"""IR function names -> an ld64 -order_file (Mach-O symbols carry one more leading `_`; internal-linkage
names come as `file;name`). Reports how many are defined in the dylib (unmatched ones only make ld warn).
Usage: to_order_file.py <ir names> <out> <dylib>"""
import subprocess, sys
src, dst, dylib = sys.argv[1], sys.argv[2], sys.argv[3]
defined = set(l.strip() for l in subprocess.run(["nm", "-U", "-j", dylib], capture_output=True, text=True).stdout.splitlines())
out, seen, hit = [], set(), 0
for name in open(src):
    name = name.strip()
    if not name:
        continue
    sym = "_" + (name.split(";", 1)[1] if ";" in name else name)
    if sym not in seen:
        seen.add(sym)
        out.append(sym)
        hit += sym in defined
open(dst, "w").write("\n".join(out) + "\n")
print("%d ordered functions, %d defined in the dylib (%.0f%%)" % (len(out), hit, 100.0 * hit / max(1, len(out))))
