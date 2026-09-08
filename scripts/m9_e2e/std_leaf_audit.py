"""Which std symbols are stubbed as Leaf but look like they must return a value?

A Leaf stub writes 0 to the return slot. That is harmless for a function whose
result is ignored, and fatal for one whose result is a pointer the caller
dereferences or null-checks. Rust's thread-local getter compiles to
`test rax,rax / je panic_access_error`, so a stubbed accessor turns its first
use into a panic - which is exactly how the boot died in
`std::hash::random::...::KEYS::...::VAL`.

The classification rule is blanket: everything in crate std except
hashmap_random_keys becomes Leaf. So the KEYS accessor is unlikely to be the
only casualty. This ranks the rest by how much their names suggest a
pointer-returning accessor.

Reads the log's own `dep: sub_<synth> -> resolved=<name> class=<C>` lines.
"""
import collections
import io
import re
import sys

LOG = sys.argv[1] if len(sys.argv) > 1 else r'C:\rb\azwriter_server.log'

pat = re.compile(r'dep: (sub_[0-9a-f]+) \S+ resolved=(\S+?)@0x[0-9a-f]+ class=(\w+)')

seen = {}
with io.open(LOG, encoding='utf-8', errors='replace') as fh:
    for line in fh:
        if 'class=' not in line or 'resolved=' not in line:
            continue
        m = pat.search(line)
        if m:
            seen.setdefault(m.group(1), (m.group(2), m.group(3)))

leaf_std = [(s, n) for s, (n, c) in seen.items()
            if c == 'Leaf' and (n.startswith('std::') or n.startswith('core::')
                                or n.startswith('alloc::'))]

print('distinct dep symbols        : %d' % len(seen))
print('Leaf-classified std/core/alloc: %d' % len(leaf_std))

# Accessor-shaped names: a lazy static's value getter, a `get`/`new`/`init`
# returning something, or a TLS entry point.
ACCESSOR = re.compile(
    r'::VAL$|::VAL[^A-Za-z]|::get$|::get<|::get_or|::try_get|::instance|'
    r'::new$|::new<|::init$|::with$|::with<|lazy|once|LOCAL|KEYS|::__getit')

hits = [(s, n) for s, n in leaf_std if ACCESSOR.search(n)]
print('  of those, accessor-shaped : %d' % len(hits))
print('')

by_mod = collections.Counter()
for _, n in hits:
    parts = n.split('::')
    by_mod['::'.join(parts[:3])] += 1

print('=== accessor-shaped Leaf symbols, by module ===')
for k, v in by_mod.most_common(25):
    print('  %-52s %d' % (k[:52], v))

print('')
print('=== the individual names (first 40) ===')
for s, n in sorted(hits, key=lambda x: x[1])[:40]:
    print('  %-12s %s' % (s, n[:110]))

print('')
print('A Leaf here is only a bug if the caller USES the result. Check each')
print('against its call site before changing a classification.')
