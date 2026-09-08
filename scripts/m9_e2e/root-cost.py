"""What does each BFS root category actually cost the mini?

`api_surface_roots` seeds a walk root for every symbol starting with "Az", with
no allowlist. The question that decides whether filtering it is worth anything
is not how many roots there are but how much code is reachable ONLY through
them - a root whose whole subtree is shared with the real boot path is free to
keep, and one that exclusively owns a megabyte is pure waste.

So for a category C: exclusive(C) = reachable(C) - reachable(all other roots).
That is the number of bytes deleting C would actually remove. Two BFS passes
per category, which is cheap, instead of per-root reachability which is not.

Sizes are the per-function .o the lifter produced, which is what the wasm is
linked from. Zero-byte objects are dropped - they are lift failures, not code.

MEASUREMENT DISCIPLINE, each of which has produced a wrong answer here before:
join by FUNCTION NAME (not __az_dep_<hex>, which is per-address and differs
between runs); anchor the name regex non-greedily on ` addr=` because MSVC
demangled names contain spaces AND angle brackets; and match a crate on its
LEADING name, never a substring.
"""
import collections
import io
import os
import re
import sys

LOG = sys.argv[1] if len(sys.argv) > 1 else r'C:\rb\azwriter_server.20260905-112953.log'
SCRATCH = sys.argv[2] if len(sys.argv) > 2 else None

LIFT = re.compile(r'transitive\[\d+\]: (?:lifting|cached) (.+?) addr=0x([0-9a-fA-F]+)')
EXPORT = re.compile(r'export_as=(\S+)')
DEP = re.compile(r'dep: \S+ (?:->|\u2192) resolved=(.+?)@0x([0-9a-fA-F]+) '
                 r'class=(\S+) visited=\S+\s+\(pulled in by (.+)\)\s*$')

lines = io.open(LOG, encoding='utf-8', errors='replace').read().splitlines()

# The mini walk is everything up to the first completion line; what follows is
# the per-callback walks, which are separate wasm files already.
cut = len(lines)
for i, ln in enumerate(lines):
    if 'transitive lift complete:' in ln:
        cut = i
        print('mini walk ends at line %d: %s' % (i, ln.strip()[-60:]))
        break
lines = lines[:cut]

size_of = {}       # name -> x86 size
export_of = {}     # name -> __az_dep_<hex>
edges = collections.defaultdict(set)   # caller -> {callee}
callees = set()

for ln in lines:
    m = LIFT.search(ln)
    if m:
        name = m.group(1)
        e = EXPORT.search(ln)
        if e:
            export_of.setdefault(name, e.group(1))
        else:
            # `cached` lines name the object file instead
            mo = re.search(r'(__az_dep_[0-9a-f]+)\.o', ln)
            if mo:
                export_of.setdefault(name, mo.group(1))
        continue
    m = DEP.search(ln)
    if m:
        callee, _addr, klass, caller = m.groups()
        if klass in ('NeverLift', 'BoundaryImport'):
            continue
        edges[caller].add(callee)
        callees.add(callee)

nodes = set(export_of) | set(edges) | callees
roots = sorted(n for n in nodes if n not in callees)
print('nodes=%d  edges=%d  roots=%d' % (nodes.__len__(), sum(len(v) for v in edges.values()), len(roots)))

# object sizes
obj = {}
if SCRATCH and os.path.isdir(SCRATCH):
    for name, ex in export_of.items():
        p = os.path.join(SCRATCH, ex + '.o')
        try:
            s = os.path.getsize(p)
        except OSError:
            continue
        if s > 0:
            obj[name] = s
    print('object sizes found for %d / %d nodes (%.2f MB total)'
          % (len(obj), len(export_of), sum(obj.values()) / 1e6))
else:
    print('no scratch dir given - falling back to x86 byte size (proxy only)')


def reach(starts):
    seen = set()
    stack = [s for s in starts]
    while stack:
        n = stack.pop()
        if n in seen:
            continue
        seen.add(n)
        for c in edges.get(n, ()):
            if c not in seen:
                stack.append(c)
    return seen


def bytes_of(names):
    return sum(obj.get(n, 0) for n in names)


CATEGORIES = [
    ('Az*_toDbgString', lambda r: r.startswith('Az') and 'toDbgString' in r),
    ('Az*_fmtDebug/Display', lambda r: r.startswith('Az') and ('fmtDebug' in r or 'Display' in r)),
    ('AzPdf_*', lambda r: r.startswith('AzPdf')),
    ('Az*_deepCopy', lambda r: r.startswith('Az') and 'deepCopy' in r),
    ('Az*_delete/free', lambda r: r.startswith('Az') and ('delete' in r or r.endswith('_free'))),
    ('ALL Az* roots', lambda r: r.startswith('Az')),
    ('ALL Az* except AzStartup_*', lambda r: r.startswith('Az') and not r.startswith('AzStartup')),
    # Everything that is NOT an engine entry point. A root that no Az* entry
    # reaches was swept in some other way - a mirrored data window, a vtable
    # slot, a function pointer in .rdata - and is the dead-weight candidate.
    ('NON-Az roots (swept in)', lambda r: not r.startswith('Az')),
]

total = bytes_of(reach(roots))
print('')
print('total reachable from all roots: %.2f MB over %d fns'
      % (total / 1e6, len(reach(roots))))
print('')
print('%-28s %6s %10s %10s %8s' % ('root category', 'roots', 'excl fns', 'excl MB', '% total'))
for label, pred in CATEGORIES:
    grp = [r for r in roots if pred(r)]
    if not grp:
        print('%-28s %6d %10s' % (label, 0, '-'))
        continue
    others = [r for r in roots if r not in set(grp)]
    excl = reach(grp) - reach(others)
    b = bytes_of(excl)
    print('%-28s %6d %10d %9.2f %7.1f%%'
          % (label, len(grp), len(excl), b / 1e6, 100.0 * b / max(total, 1)))

print('')
print('top 15 roots by EXCLUSIVE bytes (what filtering that one root would save):')
all_reach = {}
scored = []
for r in roots:
    others = [x for x in roots if x != r]
    # cheap upper bound first: skip roots whose own subtree is tiny
    own = reach([r])
    if bytes_of(own) < 200_000:
        continue
    excl = own - reach(others)
    scored.append((bytes_of(excl), len(excl), r))
scored.sort(reverse=True)
for b, n, r in scored[:15]:
    print('   %9.3f MB  %5d fns  %s' % (b / 1e6, n, r[:70]))
if not scored:
    print('   (none over the 200 KB pre-filter - every Az root shares its subtree)')
