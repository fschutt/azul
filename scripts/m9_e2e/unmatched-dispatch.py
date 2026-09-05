#!/usr/bin/env python3
"""Classify an unmatched indirect-dispatch target, from ONE run's own log.

When the lifted module traps with `unmatched indirect dispatches: N first=0x...`
that synth PC has exactly four explanations, and they need different fixes:

  (a) INSIDE a lifted function  -> a jump-table destination that
      devirtualisation failed to emit as a dispatcher case. Fix devirt.
  (b) a NeverLift-classified function -> a classifier removed it from lifting,
      so no case exists. Narrow the classifier.
  (c) an IAT import synth -> an imported function with no intercept.
  (d) genuinely unknown -> never discovered by the walk.

USE ONE LOG. Synth addresses are assigned per image band and every build lays
out differently - the same function appears at three different synth addresses
across saved logs - so merging logs produces a confident and wrong answer.
azwriter_web.sh rotates the log on start so run N's mapping survives run N+1.

Usage: unmatched-dispatch.py <server.log> <hex-target>
"""
import io
import re
import sys

log = sys.argv[1]
T = int(sys.argv[2], 16)

LIFT = re.compile(r'lifting (.+?) addr=0x([0-9a-fA-F]+) size=(\d+) export_as=(\S+)')
RES = re.compile(r'sub_([0-9a-f]+)\s*(?:→|->)\s*resolved=(.+?)@0x([0-9a-fA-F]+)\s+class=(\S+)')
INT = re.compile(r'intercept: sub_([0-9a-f]+)\s*(?:→|->)\s*synth=0x([0-9a-f]+)\s+class=Some\((\w+)\)')
IAT = re.compile(r'IAT import (\S+)!(\S+)\s*(?:→|->)\s*0x([0-9a-f]+)')

synth_name = {}
synth_class = {}
lifted_size = {}
iat = {}
for line in io.open(log, encoding='utf-8', errors='replace'):
    m = RES.search(line)
    if m:
        try:
            s = int(m.group(1), 16)
            synth_name[s] = m.group(2).strip()
            synth_class[s] = m.group(4)
        except ValueError:
            pass
    m = INT.search(line)
    if m:
        try:
            synth_class.setdefault(int(m.group(2), 16), m.group(3))
        except ValueError:
            pass
    m = IAT.search(line)
    if m:
        try:
            iat[int(m.group(3), 16)] = (m.group(1), m.group(2))
        except ValueError:
            pass
    if ': lifting ' in line:
        m = LIFT.search(line)
        if m:
            lifted_size[m.group(1)] = int(m.group(3))

print('log            : %s' % log)
print('synth->name    : %d   IAT imports: %d   lifted: %d'
      % (len(synth_name), len(iat), len(lifted_size)))
print('target         : 0x%x' % T)
print('')

if T in iat:
    d, n = iat[T]
    print('(c) IAT IMPORT: %s!%s' % (d, n))
    print('    It has an intercept but evidently no dispatcher case for an')
    print('    INDIRECT call to it. Fix: emit a case for intercepted imports.')
    raise SystemExit

if T in synth_name:
    print('EXACT: %s   class=%s' % (synth_name[T], synth_class.get(T, '?')))
    if synth_class.get(T, '').startswith('NeverLift'):
        print('(b) NeverLift - a classifier removed it from lifting, so no case exists.')
    raise SystemExit

# inside a lifted function?
hits = []
for s, n in synth_name.items():
    sz = lifted_size.get(n)
    if sz and s <= T < s + sz:
        hits.append((s, n, sz))
if hits:
    for s, n, sz in hits:
        print('(a) MID-FUNCTION: %d bytes into %s (size %d, class=%s)'
              % (T - s, n, sz, synth_class.get(s, '?')))
    print('    A mid-function indirect target is a JUMP-TABLE destination that')
    print('    devirtualisation did not emit as a dispatcher case.')
    raise SystemExit

keys = sorted(synth_name)
below = [k for k in keys if k < T]
above = [k for k in keys if k > T]
print('(d) UNKNOWN - not an IAT import, not inside any lifted function.')
print('')
print('nearest below:')
for k in below[-5:]:
    n = synth_name[k]
    print('  0x%08x (-%7d) size=%-6s class=%-12s %s'
          % (k, T - k, lifted_size.get(n, '?'), synth_class.get(k, '?'), n[:56]))
print('nearest above:')
for k in above[:5]:
    n = synth_name[k]
    print('  0x%08x (+%7d) size=%-6s class=%-12s %s'
          % (k, k - T, lifted_size.get(n, '?'), synth_class.get(k, '?'), n[:56]))
if iat:
    ik = sorted(iat)
    lo = [k for k in ik if k < T]
    hi = [k for k in ik if k > T]
    print('')
    print('nearest IAT imports: below 0x%s   above 0x%s'
          % (('%08x' % lo[-1]) if lo else '-', ('%08x' % hi[0]) if hi else '-'))
    if lo and hi and (hi[0] - lo[-1]) < 0x20000:
        print('  target sits INSIDE the imported-function band -> likely an import')
        print('  with no intercept and no dispatcher case.')
