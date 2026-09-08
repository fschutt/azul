"""Find guest-memory accesses mis-tagged HOST - now following GEP derivation.

The first pass tracked only pointers defined DIRECTLY by `inttoptr`, so it
missed `%d8 = getelementptr inbounds i8, ptr %dst, i64 8` where %dst came from
an inttoptr. That undercounted: the var_os stub's second and third stores are
exactly that shape.

Guest-ness propagates through getelementptr, bitcast and phi/select of guest
pointers, so this closes over those. A load/store through such a pointer that
carries the HOST list (90005) instead of the GUEST list (90004) is a false
noalias claim against the lifted code that reads the same address guest-tagged,
and LLVM may drop or reorder it.

Reports one row per distinct SSA-name shape (the per-wrapper duplication is
noise) plus which stub body it came from.
"""
import collections
import glob
import io
import os
import re
import sys

GUEST_LIST = '90004'
HOST_LIST = '90005'

SCRATCH = sys.argv[1] if len(sys.argv) > 1 else None
if not SCRATCH:
    cands = glob.glob(os.path.join(
        os.environ.get('TEMP', r'C:\Users\felix\AppData\Local\Temp'),
        'azul-web-transpiler-*'))
    cands.sort(key=os.path.getmtime, reverse=True)
    SCRATCH = cands[0]
print('scratch: %s' % SCRATCH)

re_inttoptr = re.compile(r'^\s*%([A-Za-z0-9_.]+) = inttoptr ')
# Any pointer-producing instruction that names an operand pointer.
re_derive = re.compile(
    r'^\s*%([A-Za-z0-9_.]+) = (?:getelementptr[^,]*, ptr %([A-Za-z0-9_.]+)'
    r'|bitcast ptr %([A-Za-z0-9_.]+)'
    r'|select i1 [^,]+, ptr %([A-Za-z0-9_.]+))')
re_memop = re.compile(
    r'^\s*(?:%[A-Za-z0-9_.]+ = )?(load|store) .*?ptr %([A-Za-z0-9_.]+)')
# Which stub body we are inside, for attribution.
re_define = re.compile(r'^define .*?@([A-Za-z0-9_.$]+)\(')

files = sorted(glob.glob(os.path.join(SCRATCH, '*.helper.ll')))
if not files:
    files = sorted(glob.glob(os.path.join(SCRATCH, '*.ll')))
print('scanning %d files' % len(files))

bad = collections.Counter()
examples = {}
seen_guest_ops = 0

for path in files:
    try:
        txt = io.open(path, encoding='utf-8', errors='replace').read()
    except OSError:
        continue
    if 'inttoptr' not in txt:
        continue
    guest = set()
    fn = '?'
    for line in txt.split('\n'):
        m = re_define.match(line)
        if m:
            fn = m.group(1)
            guest = set()          # SSA names are function-scoped
            continue
        m = re_inttoptr.match(line)
        if m:
            guest.add(m.group(1))
            continue
        m = re_derive.match(line)
        if m:
            src = m.group(2) or m.group(3) or m.group(4)
            if src in guest:
                guest.add(m.group(1))
            continue
        m = re_memop.match(line)
        if not m:
            continue
        kind, name = m.group(1), m.group(2)
        if name not in guest:
            continue
        seen_guest_ops += 1
        if ('!alias.scope !' + HOST_LIST) in line:
            # Strip the per-wrapper numeric suffix so shapes collapse.
            shape = re.sub(r'_?\d+$', '', name)
            key = '%s  %s via %%%s' % (fn.split('_7ff')[0][:40], kind, shape)
            bad[key] += 1
            examples.setdefault(key, (os.path.basename(path), line.strip()))
        elif ('!alias.scope !' + GUEST_LIST) not in line:
            key = 'UNTAGGED  %s via %%%s' % (kind, re.sub(r'_?\d+$', '', name))
            bad[key] += 1
            examples.setdefault(key, (os.path.basename(path), line.strip()))

print('')
print('guest-pointer memory ops seen : %d' % seen_guest_ops)
print('not guest-tagged among them   : %d' % sum(bad.values()))
print('')
if not bad:
    print('  none - every guest-pointer access carries the guest scope.')
for key, n in bad.most_common(30):
    f, line = examples[key]
    print('  x%-6d %s' % (n, key))
    print('           %s' % f)
    print('           %s' % line[:150])
