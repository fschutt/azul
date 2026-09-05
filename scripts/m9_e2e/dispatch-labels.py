"""Count dispatcher cases whose LABEL is not the callee's own address.

That count is the blast radius of the pc-seeding bug: a lifted body seeds its
whole PC chain from its `pc` argument, so a case that hands it the label instead
of the callee's lift address skews every rip-relative address the body computes
by (callee - label).

Read the EMITTED dispatcher, not the walk log. The `dep:` lines in the server
log are NOT the case source - scanning them reported 0 mismatches out of 3418
while the emitted dispatcher had 4916.

Usage: dispatch-labels.py <scratch-dir>/az_indirect_dispatch.ll
"""
import io
import re
import sys

PATH = sys.argv[1]
text = io.open(PATH, encoding='utf-8', errors='replace').read()

# Match a whole case: the label line, then its single call line. The
# backreference ties the two together so a mis-paired label can never be
# counted as a match.
CASE = re.compile(r'^c([0-9a-f]+):\s*\n\s*%r\1 = call ptr @sub_([0-9a-f]+)\(', re.M)

labels = re.findall(r'^c([0-9a-f]+):', text, re.M)
pairs = CASE.findall(text)

print('case labels total : %d' % len(labels))
print('parsed pairs      : %d' % len(pairs))
if not pairs:
    print('no cases parsed - is this an az_indirect_dispatch.ll?')
    raise SystemExit

mism = [(a, b) for a, b in pairs if a != b]
print('label != callee   : %d  (%.1f%%)'
      % (len(mism), 100.0 * len(mism) / len(pairs)))

if not mism:
    raise SystemExit

# A constant skew means the label is a truncated NATIVE address:
# native_low32 - synth == native_base_low32 - synth_base for every function.
skews = {}
for a, b in mism:
    d = int(a, 16) - int(b, 16)
    skews[d] = skews.get(d, 0) + 1

print('')
print('skew (label - callee) histogram:')
for d, n in sorted(skews.items(), key=lambda kv: -kv[1])[:6]:
    print('   0x%-10x %6d cases' % (d, n))
print('')
print('examples:')
for a, b in mism[:8]:
    print('   c%-12s -> sub_%s' % (a, b))
