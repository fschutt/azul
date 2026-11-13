#!/usr/bin/env python3
"""Free disk in target/release/deps: delete artifacts superseded by a newer build of the same
crate + extension (older than --hours, default 4). Never touches the newest of each group.
Keep >= 20 GB free on this 256 GB Mac (a full disk aborted a dylib build on 2026-10-02)."""
import collections, os, re, sys, time
hours = float(sys.argv[sys.argv.index('--hours') + 1]) if '--hours' in sys.argv else 4.0
os.chdir('/Users/fschutt/Development/azul/target/release/deps')
cut = time.time() - hours * 3600
groups = collections.defaultdict(list)
for f in os.listdir('.'):
    m = re.match(r'^(.*)-([0-9a-f]{16})(\..*)?$', f)
    if not m or not os.path.isfile(f):
        continue
    st = os.stat(f)
    groups[(m.group(1), m.group(3) or '')].append((st.st_mtime, st.st_size, f))
victims = []
for v in groups.values():
    newest = max(t for t, _, _ in v)
    if newest >= cut:
        victims += [(s, f) for t, s, f in v if t < cut]
for _, f in victims:
    os.remove(f)
print("%d files, %.2f GiB freed" % (len(victims), sum(s for s, _ in victims) / 2**30))
