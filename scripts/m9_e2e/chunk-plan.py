"""Partition the mini into an eager core plus lazy chunks, and size each one.

Splitting only pays for code that ONE chunk owns. Anything reachable from two
roots has to stay resident, so the ceiling on lazy bytes is fixed by the graph
and is worth knowing before writing any loader code.

The partition:
  CORE  = every node reachable from >= 2 roots (shared, irreducible)
          + the exclusive subtree of every root on the boot path
  CHUNK = the exclusive subtree of one lazy root

A lazy root is safe precisely when it is a ROOT in the walk graph: that means no
static call edge reaches it, so it is entered only through
`__az_indirect_dispatch` and the eager core never names it. That is the same
property the chunk plan requires, read straight off the graph instead of
asserted.

Usage: chunk-plan.py <server.log> <scratch-dir> [--lazy N]
       --lazy N = put the N biggest non-boot roots in their own chunks.
"""
import collections
import io
import os
import re
import sys

LOG = sys.argv[1]
SCRATCH = sys.argv[2]
NLAZY = 4
if '--lazy' in sys.argv:
    NLAZY = int(sys.argv[sys.argv.index('--lazy') + 1])

LIFT = re.compile(r'transitive\[\d+\]: (?:lifting|cached) (.+?) addr=0x([0-9a-fA-F]+)')
EXPORT = re.compile(r'export_as=(\S+)')
DEP = re.compile(r'dep: \S+ (?:->|→) resolved=(.+?)@0x([0-9a-fA-F]+) '
                 r'class=(\S+) visited=\S+\s+\(pulled in by (.+)\)\s*$')

# Roots that must stay eager: the engine entry points the first paint runs
# through. Everything else is a candidate for its own chunk.
BOOT_PREFIXES = ('AzStartup_', 'AzApp_', 'AzWindow_')

lines = io.open(LOG, encoding='utf-8', errors='replace').read().splitlines()
cut = len(lines)
for i, ln in enumerate(lines):
    if 'transitive lift complete:' in ln:
        cut = i
        break
lines = lines[:cut]

export_of = {}
edges = collections.defaultdict(set)
callees = set()
for ln in lines:
    m = LIFT.search(ln)
    if m:
        name = m.group(1)
        e = EXPORT.search(ln)
        if e:
            export_of.setdefault(name, e.group(1))
        else:
            mo = re.search(r'(__az_dep_[0-9a-f]+)\.o', ln)
            if mo:
                export_of.setdefault(name, mo.group(1))
        continue
    m = DEP.search(ln)
    if m:
        callee, _a, klass, caller = m.groups()
        if klass in ('NeverLift', 'BoundaryImport'):
            continue
        edges[caller].add(callee)
        callees.add(callee)

nodes = set(export_of) | set(edges) | callees
roots = sorted(n for n in nodes if n not in callees)

obj = {}
for name, ex in export_of.items():
    try:
        s = os.path.getsize(os.path.join(SCRATCH, ex + '.o'))
    except OSError:
        continue
    if s > 0:
        obj[name] = s


def reach(starts):
    seen, stack = set(), list(starts)
    while stack:
        n = stack.pop()
        if n in seen:
            continue
        seen.add(n)
        stack.extend(c for c in edges.get(n, ()) if c not in seen)
    return seen


def mb(names):
    return sum(obj.get(n, 0) for n in names) / 1e6


total_nodes = reach(roots)
print('mini: %d nodes reachable, %.2f MB of objects' % (len(total_nodes), mb(total_nodes)))
print('roots: %d' % len(roots))

# How many roots can reach each node? >=2 means it must stay in the core.
owners = collections.Counter()
own_sets = {}
for r in roots:
    s = reach([r])
    own_sets[r] = s
    for n in s:
        owners[n] += 1

shared = {n for n in total_nodes if owners[n] >= 2}
print('')
print('shared by >=2 roots (irreducible core): %d fns, %.2f MB (%.1f%%)'
      % (len(shared), mb(shared), 100.0 * mb(shared) / max(mb(total_nodes), 1e-9)))

boot_roots = [r for r in roots if r.startswith(BOOT_PREFIXES)]
lazy_cands = []
for r in roots:
    if r in boot_roots:
        continue
    excl = own_sets[r] - shared
    if excl:
        lazy_cands.append((mb(excl), len(excl), r, excl))
lazy_cands.sort(reverse=True, key=lambda t: t[0])

print('')
print('top lazy candidates (exclusive, non-boot roots):')
for size, n, r, _ in lazy_cands[:10]:
    print('   %8.3f MB  %5d fns  %s' % (size, n, r[:66]))

# THE CEILING on the whole lever. Every byte owned by exactly one non-boot root
# is a lazy candidate and nothing else ever can be: shared nodes must stay
# resident by definition, and a boot root's subtree is entered by the loader
# directly. Reporting only the top N hides whether the rest is worth a chunk --
# a long tail of small chunks costs a fetch each and saves almost nothing -- and
# hides whether this lever can reach the target at all.
tot_all = mb(total_nodes)
ceiling = sum(t[0] for t in lazy_cands)
print('')
print('LAZY CEILING: %.2f MB of %.2f MB (-%.1f%%) over %d candidate root(s); '
      '%d boot root(s) excluded'
      % (ceiling, tot_all, 100.0 * ceiling / max(tot_all, 1e-9),
         len(lazy_cands), len(boot_roots)))
cum = 0.0
for i, (size, _n, _r, _ex) in enumerate(lazy_cands, start=1):
    cum += size
    if i in (1, 3, 10, 30, 100, 300) or i == len(lazy_cands):
        print('   top-%-4d %8.2f MB  (-%.1f%%)   [%.1f%% of the ceiling]'
              % (i, cum, 100.0 * cum / max(tot_all, 1e-9),
                 100.0 * cum / max(ceiling, 1e-9)))

chosen = lazy_cands[:NLAZY]
lazy_all = set()
for _s, _n, _r, ex in chosen:
    lazy_all |= ex
core = total_nodes - lazy_all

print('')
print('=== proposed split: core + %d lazy chunks ===' % len(chosen))
print('%-26s %8s %8s %8s' % ('chunk', 'fns', 'MB', '% mini'))
tot = mb(total_nodes)
print('%-26s %8d %8.2f %7.1f%%' % ('azul-p0 (eager core)', len(core), mb(core),
                                   100.0 * mb(core) / max(tot, 1e-9)))
for i, (size, n, r, ex) in enumerate(chosen, start=1):
    print('%-26s %8d %8.2f %7.1f%%  <- %s'
          % ('azul-p%d (lazy)' % i, n, size, 100.0 * size / max(tot, 1e-9), r[:44]))
print('')
print('eager first paint would drop from %.2f MB to %.2f MB of objects (-%.1f%%)'
      % (tot, mb(core), 100.0 * (1 - mb(core) / max(tot, 1e-9))))
# --emit <dir>: write the object list for each chunk, so the split can be
# LINKED and measured instead of projected from object bytes.
EMIT = None
for i, a in enumerate(sys.argv):
    if a == '--emit' and i + 1 < len(sys.argv):
        EMIT = sys.argv[i + 1]
if EMIT:
    os.makedirs(EMIT, exist_ok=True)


    def write_list(fname, names):
        seen = set()
        out = []
        for n in sorted(names):
            ex = export_of.get(n)
            if not ex or ex in seen:
                continue
            p = os.path.join(SCRATCH, ex + '.o')
            if os.path.exists(p) and os.path.getsize(p) > 0:
                seen.add(ex)
                out.append(p)
        with io.open(os.path.join(EMIT, fname), 'w', encoding='utf-8',
                     newline='\n') as fh:
            fh.write('\n'.join(out) + '\n')
        return len(out)


    print('')
    print('=== emitted object lists to %s ===' % EMIT)
    print('  p0.txt: %d object(s)' % write_list('p0.txt', core))
    for i, (_s, _n, _r, ex) in enumerate(chosen, start=1):
        print('  p%d.txt: %d object(s)' % (i, write_list('p%d.txt' % i, ex)))
    print('')
    print('  Link one with (the mini uses --no-entry --allow-undefined')
    print('  --gc-sections --strip-all, plus --export= per boundary symbol):')
    print('    wasm-ld --no-entry --allow-undefined --gc-sections --strip-all \\')
    print('            -o p0.wasm @p0.txt')

print('')
print('NOTE: object bytes, not delivered bytes. brotli shares a dictionary across')
print('the whole module, so splitting COSTS ratio - each chunk compresses alone.')
print('Measure the real per-chunk wasm before quoting a delivered saving.')
