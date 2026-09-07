"""What actually RAN at first paint, and what it costs to ship what did not.

The chunk graph answers a different question. It says what is REACHABLE from an
entry point, which is a transitive closure over vtables and fn-pointer tables and
is therefore obliged to keep almost everything. Coverage says what EXECUTED, and
a function that never executes at first paint is a lazy candidate wherever it
sits in that graph.

Inputs, and they must come from the SAME run - coverage indices are assigned per
run, so a manifest from a different one silently mislabels every function:

    coverage.json           {"total": <slots>, "hits": [idx, ...]}  read-coverage.js
    coverage-manifest.tsv   idx \\t export_as \\t name              the lift scratch
    <scratch>               for the per-function object sizes

Usage: coverage_report.py <coverage.json> <manifest.tsv> <scratch-dir> [--top N]
"""
import io
import json
import os
import sys

args = [a for a in sys.argv[1:] if not a.startswith('--')]
if len(args) < 3:
    raise SystemExit(__doc__)
COV, MAN, SCRATCH = args[0], args[1], args[2]
TOP = 20
for i, a in enumerate(sys.argv):
    if a == '--top' and i + 1 < len(sys.argv):
        TOP = int(sys.argv[i + 1])

cov = json.load(io.open(COV, encoding='utf-8'))
hits = set(cov.get('hits', []))
print('%s: %d slot(s) scanned, %d entered' % (COV, cov.get('total', 0), len(hits)))

rows = []
for line in io.open(MAN, encoding='utf-8', errors='replace'):
    parts = line.rstrip('\n').split('\t')
    if len(parts) < 3:
        continue
    try:
        idx = int(parts[0])
    except ValueError:
        continue
    rows.append((idx, parts[1], parts[2]))
print('%s: %d manifest entr(ies)' % (MAN, len(rows)))

if not rows:
    raise SystemExit('empty manifest - the bitmap cannot be decoded')

# A manifest index beyond the scanned range was never observable, which is a
# different thing from "did not run"; count it separately rather than folding it
# into the cold set.
cap = cov.get('total', 0)
sizes = {}
for _idx, ex, _n in rows:
    if ex in sizes:
        continue
    try:
        sizes[ex] = os.path.getsize(os.path.join(SCRATCH, ex + '.o'))
    except OSError:
        sizes[ex] = 0

hot, cold, unobservable = [], [], []
for idx, ex, name in rows:
    bucket = unobservable if idx >= cap else (hot if idx in hits else cold)
    bucket.append((sizes.get(ex, 0), name, ex))

def mb(rs):
    return sum(s for s, _n, _e in rs) / 1e6

print('')
print('  ENTERED at first paint : %5d fn  %8.2f MB objects' % (len(hot), mb(hot)))
print('  never entered          : %5d fn  %8.2f MB objects' % (len(cold), mb(cold)))
if unobservable:
    print('  beyond the bitmap cap  : %5d fn  %8.2f MB  (index >= %d — NOT measured)'
          % (len(unobservable), mb(unobservable), cap))
tot = mb(hot) + mb(cold) + mb(unobservable)
if tot > 0:
    print('')
    print('  cold share of measured object bytes: %.1f%%'
          % (100.0 * mb(cold) / max(mb(hot) + mb(cold), 1e-9)))

print('')
print('=== biggest never-entered functions ===')
for s, name, _ex in sorted(cold, reverse=True)[:TOP]:
    print('  %9.3f MB  %s' % (s / 1e6, name[:88]))

by_crate = {}
for s, name, _ex in cold:
    crate = name.split('::')[0].split('<')[0]
    b = by_crate.setdefault(crate, [0, 0])
    b[0] += 1
    b[1] += s
print('')
print('=== never-entered, by leading crate ===')
for crate, (c, b) in sorted(by_crate.items(), key=lambda kv: -kv[1][1])[:TOP]:
    print('  %-44s %5d fn  %9.3f MB' % (crate[:44], c, b / 1e6))
