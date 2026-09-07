"""How much of the corpus's data mirror is a second copy, and how much conflicts?

Every cb/layout module imports `env.memory` from the mini, so all of them share
ONE linear memory. Each still ships its own mirror, replayed into that memory at
instantiate over addresses an earlier module already filled. If those writes are
byte-identical the later ones are pure waste.

The conflict column is the one that decides the design. A segment at the same
address with DIFFERENT content means two modules disagree about what belongs
there, so an address-keyed dedup would silently corrupt it. Only a
CONTENT-keyed dedup is sound: a module may omit a segment exactly when an
eagerly-loaded earlier module shipped byte-identical bytes at that address.

Usage: mirror_dup_tally.py <server.log>
"""
import io
import re
import sys

if len(sys.argv) < 2:
    raise SystemExit(__doc__)

LINE = re.compile(
    r'MIRROR-DUP \((.+?)\): (\d+) of (\d+) segment\(s\) BYTE-IDENTICAL[^—]*— '
    r'(\d+) of (\d+) bytes \(([\d.]+)%\); (\d+) new, (\d+) CONFLICT \((\d+) bytes')

rows = []
for ln in io.open(sys.argv[1], encoding='utf-8', errors='replace'):
    m = LINE.search(ln)
    if m:
        rows.append((m.group(1), int(m.group(2)), int(m.group(3)), int(m.group(4)),
                     int(m.group(5)), int(m.group(7)), int(m.group(8)), int(m.group(9))))

if not rows:
    raise SystemExit('no MIRROR-DUP lines - is this a run with the content-hashed check?')

dup_b = sum(r[3] for r in rows)
tot_b = sum(r[4] for r in rows)
new_b = sum(r[5] for r in rows)
conf_n = sum(r[6] for r in rows)
conf_b = sum(r[7] for r in rows)

print('%d module(s) with a mirror' % len(rows))
print('')
print('  mirror bytes shipped, all modules : %10d (%.2f MB)' % (tot_b, tot_b / 1e6))
print('  BYTE-IDENTICAL to an earlier one  : %10d (%.1f%%)'
      % (dup_b, 100.0 * dup_b / max(tot_b, 1)))
print('  genuinely new                     : %10d' % new_b)
print('')
print('  CONFLICTS (same address, different content):')
print('    %d segment(s), %d bytes -- %.3f%% of the mirror'
      % (conf_n, conf_b, 100.0 * conf_b / max(tot_b, 1)))
if conf_n:
    print('')
    print('  => an ADDRESS-keyed dedup would silently corrupt those bytes.')
    print('     A CONTENT-keyed one is sound and still removes the %.1f%% above.'
          % (100.0 * dup_b / max(tot_b, 1)))
else:
    print('')
    print('  => no module disagrees with another about any address.')

print('')
print('=== the modules with conflicts, worst first ===')
for name, _d, _s, _db, tb, _nb, cn, cb in sorted(rows, key=lambda r: -r[7])[:12]:
    if cn == 0:
        break
    print('  %6d B in %3d seg of %8d B mirror  %s' % (cb, cn, tb, name[:48]))
