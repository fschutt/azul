"""Attribute a linked wasm's REAL bytes by crate, from wasm-ld's --Map side-car.

Object-file bytes are the wrong unit for this question twice over: they include
relocations and symbol tables the linker drops, and they count code that
`--gc-sections` then discards. The map reports what actually survived into the
module, per input object, so it is the only attribution that matches the
artifact being shipped.

The map keys objects by `__az_dep_<native_hex>`; the run log names them. USE A
LOG AND A MAP FROM THE SAME RUN -- every rebuild shifts the image base, so a
mismatched pair joins almost nothing and reads as "the module is unattributable"
rather than as an error.

Usage: map_attribution.py <wasm-ld .map> <server.log> [--top N] [--fn N]
"""
import collections
import io
import re
import sys

args = [a for a in sys.argv[1:] if not a.startswith('--')]
if len(args) < 2:
    raise SystemExit(__doc__)
MAP, LOG = args[0], args[1]


def opt(flag, default):
    return int(sys.argv[sys.argv.index(flag) + 1]) if flag in sys.argv else default


TOP, NFN = opt('--top', 25), opt('--fn', 20)

# Non-greedy, anchored on ` addr=`: MSVC names contain spaces (`Vec<Box<T> >`),
# and `(\S+)` silently drops about a quarter of them.
LIFT = re.compile(r'transitive\[\d+\]: (?:lifting|cached) (.+?) addr=0x([0-9a-fA-F]+)')
EXPORT = re.compile(r'export_as=(\S+)')
ROOT = re.compile(r'root|extra fn-pointer root|: lifting ')

name_of = {}
for ln in io.open(LOG, encoding='utf-8', errors='replace'):
    m = LIFT.search(ln)
    if not m:
        continue
    e = EXPORT.search(ln)
    if e:
        name_of.setdefault(e.group(1), m.group(1))
print('%s: %d object->name mapping(s)' % (LOG, len(name_of)))

# `       -    9e209      29d   <path>__az_dep_<hex>.o:(sub_372820)`
ROW = re.compile(r'^\s*\S+\s+[0-9a-f]+\s+([0-9a-f]+)\s+.*?([A-Za-z0-9_]+)\.o:\(')

by_obj = collections.Counter()
total = 0
for ln in io.open(MAP, encoding='utf-8', errors='replace'):
    m = ROW.match(ln)
    if not m:
        continue
    size = int(m.group(1), 16)
    by_obj[m.group(2)] += size
    total += size
print('%s: %d object(s), %d linked bytes (%.2f MB)'
      % (MAP, len(by_obj), total, total / 1e6))

matched = sum(v for k, v in by_obj.items() if k in name_of)
print('joined: %d of %d objects, %.1f%% of the bytes'
      % (sum(1 for k in by_obj if k in name_of), len(by_obj),
         100.0 * matched / max(total, 1)))
if by_obj and matched < 0.5 * total:
    print('*** LESS THAN HALF THE BYTES JOINED -- the map and the log are almost')
    print('*** certainly from different runs. Do not read the table below.')

# Leading crate name only. A substring match puts `alloc::string::` under `ring`
# and `DisplayList` under `Display`; both have happened.
by_crate = collections.Counter()
fns = []
for obj, size in by_obj.items():
    name = name_of.get(obj)
    if name is None:
        by_crate['(unjoined: %s)' % ('infrastructure' if not obj.startswith('__az_dep_')
                                     else 'no log entry')] += size
        continue
    by_crate[re.split(r'::|<', name, 1)[0]] += size
    fns.append((size, name))

print('')
print('=== linked bytes by leading crate ===')
for crate, size in by_crate.most_common(TOP):
    print('  %9.3f MB  %5.1f%%  %s' % (size / 1e6, 100.0 * size / max(total, 1), crate[:56]))

print('')
print('=== biggest single functions in the linked module ===')
for size, name in sorted(fns, reverse=True)[:NFN]:
    print('  %9.3f MB  %s' % (size / 1e6, name[:96]))
