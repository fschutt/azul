"""Does splitting a module actually save bytes once brotli is accounted for?

Object-byte partitions always look good: the eager core is smaller than the
whole, so the split "saves" the difference. Delivered bytes need not agree.
Brotli shares a dictionary across a whole module, so every chunk compressed
alone compresses WORSE than the same code did inside the module -- the sum of
the parts EXCEEDS the whole, and the excess is the price of splitting.

Two numbers matter and they answer different questions:

  * p0 alone vs the whole module -- what a FIRST PAINT downloads. This is the
    saving, and it is real whatever the sum does.
  * the sum of all parts vs the whole -- what a user who eventually triggers
    every chunk downloads in total. If that is much larger than the whole, the
    split trades first-paint latency for total bytes, and that is a decision to
    take knowingly rather than discover later.

Reads the run log, so it needs no scratch and works after the fact.

Usage: chunk_sum.py <server.log> [--module azul-mini]
"""
import io
import re
import sys

args = [a for a in sys.argv[1:] if not a.startswith('--')]
if not args:
    raise SystemExit(__doc__)
LOG = args[0]
MODULE = sys.argv[sys.argv.index('--module') + 1] if '--module' in sys.argv else None

# `AZ_CHUNK <label>: p<N> linked <raw> bytes raw -> <br> brotli (q9)...`
LINK = re.compile(
    r'AZ_CHUNK (.+?): p(\d+) linked (\d+) bytes raw -> (\d+) brotli')
# `mini.wasm: <raw> bytes raw -> <br> bytes brotli`
MINI = re.compile(r'mini\.wasm: (\d+) bytes raw -> (\d+) bytes brotli')
ROOT = re.compile(r'<- (.+)$')

per_module = {}
mini_served = None
for ln in io.open(LOG, encoding='utf-8', errors='replace'):
    m = MINI.search(ln)
    if m:
        mini_served = (int(m.group(1)), int(m.group(2)))
    m = LINK.search(ln)
    if not m:
        continue
    label, n, raw, br = m.group(1), int(m.group(2)), int(m.group(3)), int(m.group(4))
    r = ROOT.search(ln.rstrip())
    per_module.setdefault(label, {})[n] = (raw, br, r.group(1) if r else '')

if not per_module:
    raise SystemExit('no `AZ_CHUNK ...: pN linked` lines - was AZ_CHUNK set?')

for label, parts in per_module.items():
    if MODULE and label != MODULE:
        continue
    print('=== %s ===' % label)
    ns = sorted(parts)
    for n in ns:
        raw, br, root = parts[n]
        print('  p%-2d %11d raw %9d br   %s' % (n, raw, br, root[:64]))
    sum_raw = sum(parts[n][0] for n in ns)
    sum_br = sum(parts[n][1] for n in ns)
    print('  %-3s %11d raw %9d br   <- sum of all parts' % ('SUM', sum_raw, sum_br))

    whole_br = None
    if label == 'azul-mini' and mini_served:
        whole_br = mini_served[1]
        print('  %-3s %11d raw %9d br   <- the whole module, as served'
              % ('ALL', mini_served[0], whole_br))

    if 0 in parts:
        p0_br = parts[0][1]
        if whole_br:
            print('')
            print('  FIRST PAINT: %d -> %d = %+d (%+.1f%%)'
                  % (whole_br, p0_br, p0_br - whole_br,
                     -100.0 * (whole_br - p0_br) / max(whole_br, 1)))
            print('  EVERYTHING:  %d -> %d = %+d (%+.1f%%)  <- the price of splitting'
                  % (whole_br, sum_br, sum_br - whole_br,
                     100.0 * (sum_br - whole_br) / max(whole_br, 1)))
        else:
            lazy_br = sum(parts[n][1] for n in ns if n != 0)
            print('')
            print('  p0 %d br, lazy parts %d br, sum %d br' % (p0_br, lazy_br, sum_br))
            print('  (the whole module\'s brotli is not in the log for this module -')
            print('   only the mini is pre-compressed at startup; compress its wasm')
            print('   from the scratch to complete the comparison)')
    print('')
