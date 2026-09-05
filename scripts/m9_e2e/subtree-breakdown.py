"""What is actually inside one root's exclusive subtree, by crate?

`app_state_from_json` is a JSON deserializer and owns 4.37 MB / 227 functions
exclusively, which is not what a deserializer should cost. This groups that
subtree by LEADING crate name (never a substring - `ring::` once matched
`alloc::string::` and booked 86 functions of string handling as a TLS stack)
and lists the biggest individual functions, so the answer is measured rather
than assumed.
"""
import collections
import io
import os
import re
import sys

LOG = sys.argv[1]
SCRATCH = sys.argv[2]
ROOT = sys.argv[3] if len(sys.argv) > 3 else 'azwriter::web_state::app_state_from_json'

LIFT = re.compile(r'transitive\[\d+\]: (?:lifting|cached) (.+?) addr=0x([0-9a-fA-F]+)')
EXPORT = re.compile(r'export_as=(\S+)')
DEP = re.compile(r'dep: \S+ (?:->|→) resolved=(.+?)@0x([0-9a-fA-F]+) '
                 r'class=(\S+) visited=\S+\s+\(pulled in by (.+)\)\s*$')

lines = io.open(LOG, encoding='utf-8', errors='replace').read().splitlines()
cut = len(lines)
for i, ln in enumerate(lines):
    if 'transitive lift complete:' in ln:
        cut = i
        break
lines = lines[:cut]

export_of, edges, callees = {}, collections.defaultdict(set), set()
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
roots = [n for n in nodes if n not in callees]
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


others = [r for r in roots if r != ROOT]
excl = reach([ROOT]) - reach(others)
print('root: %s' % ROOT)
print('exclusive subtree: %d fns, %.3f MB' % (len(excl), sum(obj.get(n, 0) for n in excl) / 1e6))
print('')


def crate_of(n):
    """Leading crate name, the same way the classifier extracts it."""
    n = n.strip()
    m = re.match(r'^([A-Za-z_][A-Za-z0-9_]*)::', n)
    if m:
        return m.group(1)
    if n.startswith('Az'):
        return '<Az C-API>'
    return '<other>'


by = collections.Counter()
cnt = collections.Counter()
for n in excl:
    c = crate_of(n)
    by[c] += obj.get(n, 0)
    cnt[c] += 1

print('%-26s %6s %10s %7s' % ('crate', 'fns', 'MB', '%'))
tot = sum(by.values())
for c, b in by.most_common(14):
    print('%-26s %6d %10.3f %6.1f%%' % (c, cnt[c], b / 1e6, 100.0 * b / max(tot, 1)))

print('')
print('biggest individual functions:')
for n in sorted(excl, key=lambda x: -obj.get(x, 0))[:18]:
    print('   %8.3f MB  %s' % (obj.get(n, 0) / 1e6, n[:104]))
