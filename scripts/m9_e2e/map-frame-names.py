"""Name a browser wasm stack trace from wasm-ld's link map.

The boot prints frames as `wasm-function[N]:0x<offset>`, and the map's `Off`
column is that same offset, so the match is direct - no index arithmetic and no
assumption about link order. (Deriving the mapping from index order was tried and
does not hold: `defined - exports == walk entries` is exact, but the native-vs-wasm
size correlation came out at 0.398, because --gc-sections reorders and drops.)

The map looks like:

    Addr      Off     Size Out     In      Symbol
       -       98      9ab CODE
       -       99      194         /path/AzStartup_alloc.o:(sub_381310)
       -       99      194                 sub_381310
       -      243      294         /path/AzStartup_alloc.o:(AzStartup_alloc)
       -      243      294                 AzStartup_alloc

Symbols are `sub_<canonical synth>`, which the lift log's
`dep: sub_XXXX -> resolved=<name>@0x...` lines turn into Rust names, so pass the
log too and both steps happen at once.

Usage:
  map-frame-names.py <module.map> [<lift.log>]      # trace on stdin
  map-frame-names.py <module.map> [<lift.log>] 0x22f5de7 0x6aa4d0 ...
"""
import bisect
import io
import re
import sys

args = [a for a in sys.argv[1:] if not a.startswith('--')]
if not args:
    raise SystemExit(__doc__)
map_path = args[0]
log_path = args[1] if len(args) > 1 and not args[1].startswith('0x') else None
raw_frames = [a for a in args[1:] if a.startswith('0x')]

# --- the map: offset -> symbol -----------------------------------------------
rows = []
pat = re.compile(r'^\s*(\S+)\s+([0-9a-f]+)\s+([0-9a-f]+)\s+(.*)$')
for line in io.open(map_path, encoding='utf-8', errors='replace'):
    m = pat.match(line.rstrip('\n'))
    if not m:
        continue
    tail = m.group(4)
    # The symbol row is the deepest indentation and carries no ':(' - the row
    # above it names the object file.
    if ':(' in tail or not tail.strip():
        continue
    off = int(m.group(2), 16)
    size = int(m.group(3), 16)
    rows.append((off, size, tail.strip()))
rows.sort()
offs = [r[0] for r in rows]
print('%s: %d symbol row(s)' % (map_path, len(rows)))

# --- the log: sub_<synth> -> Rust name ---------------------------------------
names = {}
if log_path:
    pr = re.compile(r'dep: (sub_[0-9a-f]+) \S+ resolved=(\S+)@')
    for line in io.open(log_path, encoding='utf-8', errors='replace'):
        m = pr.search(line)
        if m:
            names.setdefault(m.group(1), m.group(2))
    print('%s: %d sub_<synth> -> name mapping(s)' % (log_path, len(names)))
print('')

frames = [int(x, 16) for x in raw_frames]
if not frames:
    txt = sys.stdin.read()
    frames = [int(m, 16) for m in re.findall(r'wasm-function\[\d+\]:0x([0-9a-f]+)', txt)]

if not frames:
    raise SystemExit('no frames given and none found on stdin')

for f in frames:
    i = bisect.bisect_right(offs, f) - 1
    if i < 0:
        print('  0x%-9x  (before the first symbol)' % f)
        continue
    off, size, sym = rows[i]
    inside = f < off + size
    nm = names.get(sym, sym)
    print('  0x%-9x  %s%s  (+0x%x into %s)'
          % (f, nm, '' if inside else '   ** past this symbol\'s end **',
             f - off, sym))
