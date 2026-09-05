#!/usr/bin/env python3
"""Are unmatched dispatch targets plausibly FUNCTION ENTRIES at all?

Decides between two failures that look identical in the log but need opposite
fixes:

  aligned target   -> a real function entry with no dispatcher case. Hunt
                      discovery: what stopped it being lifted or cased.
  unaligned target -> not an entry at all, so it is a COMPUTED address. Hunt
                      whatever computed it - a PC-relative base, a jump table.

Measured on a real lift, 99.1%% of function entries are 16-byte aligned and only
0.62%% are not 4-byte aligned, so an unaligned target is conclusive.

This separated what looked like one recurring boot failure into two distinct
bugs: an aligned single target (missing function) and, in a later run, unaligned
targets (bad address computation).

Usage: dispatch-alignment.py <server.log> <hex-target>...
"""
import io
import re
import sys
import collections

log = sys.argv[1]
targets = [int(a, 16) for a in sys.argv[2:]]

RES = re.compile(r'sub_([0-9a-f]+)\s*(?:→|->)\s*resolved=(.+?)@0x([0-9a-fA-F]+)\s+class=(\S+)')

synths = set()
for line in io.open(log, encoding='utf-8', errors='replace'):
    m = RES.search(line)
    if m:
        try:
            synths.add(int(m.group(1), 16))
        except ValueError:
            pass

print('known function-entry synths: %d' % len(synths))
if not synths:
    raise SystemExit('no data')

dist = collections.Counter()
for s in synths:
    for a in (16, 8, 4, 2):
        if s % a == 0:
            dist[a] += 1
            break
    else:
        dist[1] += 1
tot = len(synths)
print('')
print('alignment of known function entries:')
for a in (16, 8, 4, 2, 1):
    if dist.get(a):
        print('   %2d-byte aligned : %6d  %5.1f%%' % (a, dist[a], 100.0 * dist[a] / tot))
odd = dist.get(1, 0) + dist.get(2, 0)
print('')
print('   entries that are NOT 4-byte aligned: %d (%.2f%%)' % (odd, 100.0 * odd / tot))
print('')
print('unmatched targets:')
for t in targets:
    al = 1
    for a in (16, 8, 4, 2):
        if t % a == 0:
            al = a
            break
    print('   0x%08x  %2d-byte aligned%s' % (t, al, '   <== NOT a plausible entry' if al < 4 else ''))
print('')
bad = [t for t in targets if t % 4 != 0]
if bad and odd * 1.0 / tot < 0.01:
    print('VERDICT: %d of %d targets are not 4-byte aligned, while only %.2f%% of'
          % (len(bad), len(targets), 100.0 * odd / tot))
    print('real function entries are. These are COMPUTED ADDRESSES, not missing')
    print('functions - look at what computed them, not at discovery.')
elif not bad:
    print('VERDICT: all targets are aligned like real entries - consistent with')
    print('genuinely missing functions rather than a bad address computation.')
else:
    print('VERDICT: inconclusive - unaligned entries are not rare enough here.')
