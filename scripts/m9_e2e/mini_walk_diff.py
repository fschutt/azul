"""Diff the MINI walk's function list between two runs, from the logs.

The scratch directory cannot answer this: run 61's scratch holds all 24 walks'
output while run 63's holds only the mini walk so far, and cache hits may not
write a .lifted.ll at all. The log does answer it - it prints one
`transitive[N]: lifting <NAME> addr=0x... size=...` line per function, in walk
order, regardless of cache.

The mini walk is the FIRST section, ending at the first "transitive lift
complete", so stop there rather than reading gigabytes of later walks.

Name regex is non-greedy and anchored on ` addr=`, per the measurement rule that
a greedy name match has produced wrong answers before.
"""
import collections
import io
import re
import sys

A = sys.argv[1] if len(sys.argv) > 1 else r'C:\rb\azwriter_server.20260905-163339.log'
B = sys.argv[2] if len(sys.argv) > 2 else r'C:\rb\azwriter_server.log'

pat = re.compile(r'transitive\[\d+\]: lifting (.+?) addr=')


def mini_walk(path):
    names = []
    with io.open(path, encoding='utf-8', errors='replace') as fh:
        for line in fh:
            if 'transitive lift complete' in line:
                break
            if 'lifting ' not in line:
                continue
            m = pat.search(line)
            if m:
                names.append(m.group(1))
    return names


na, nb = mini_walk(A), mini_walk(B)
sa, sb = set(na), set(nb)
print('A %s: %d lifting lines, %d unique' % (A.split('\\')[-1], len(na), len(sa)))
print('B %s: %d lifting lines, %d unique' % (B.split('\\')[-1], len(nb), len(sb)))
print('')

only_b = sorted(sb - sa)
only_a = sorted(sa - sb)
print('only in B (added): %d' % len(only_b))
print('only in A (removed): %d' % len(only_a))


def crate_of(n):
    # Leading crate name only - never a substring match.
    for sep in ('::', '__'):
        if sep in n:
            return n.split(sep, 1)[0]
    return n


print('')
print('=== added, by leading crate ===')
for k, v in collections.Counter(crate_of(n) for n in only_b).most_common(25):
    print('  %-30s %d' % (k, v))

print('')
print('=== added, sample ===')
for n in only_b[:30]:
    print('  %s' % n[:120])

if only_a:
    print('')
    print('=== removed, by leading crate ===')
    for k, v in collections.Counter(crate_of(n) for n in only_a).most_common(15):
        print('  %-30s %d' % (k, v))
    print('')
    print('=== removed, sample ===')
    for n in only_a[:15]:
        print('  %s' % n[:120])
