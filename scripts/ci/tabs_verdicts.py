#!/usr/bin/env python3
"""tabs_verdicts.py <dir>: the verdicts scripts/ci/e2e_matrix.py recorded under <dir>/*/e2e-desktop.json, one line each.

The matrix exits 0 whatever the verdicts are (it is a report), so a job that runs scenario files and means them to pass reads
them here: the exit code is 1 when any verdict is not PASS or SKIP."""
import glob
import json
import sys

root = sys.argv[1] if len(sys.argv) > 1 else "demo-out/e2e-tabs"
bad = 0
seen = 0
for path in sorted(glob.glob(root + "/*/e2e-desktop.json")):
    for record in json.load(open(path, encoding="utf-8")):
        seen += 1
        verdict = record.get("verdict")
        print("VERDICT", path.split("/")[-2], record.get("test", "")[:80], verdict, (record.get("detail") or "")[:240])
        if verdict not in ("PASS", "SKIP"):
            bad += 1
print("VERDICTS", seen, "not pass:", bad)
sys.exit(1 if bad or not seen else 0)
