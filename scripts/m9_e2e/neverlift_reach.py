"""Which NeverLift traps exist, and how reachable is each one?

A NeverLift symbol compiles to `store 0x40048; unreachable`. Reaching one is
always a hard boot failure, and the recorder only tells you WHICH one after the
fact - one relift per discovery. Enumerating them up front, ranked by how many
lifted bodies can call them, turns that serial hunt into a list.

`panic_access_error` (the Windows-TLS one) was found the slow way. This asks
what else is waiting behind it.

Counts CALLERS, not call sites: a stub referenced by one cold body is far less
interesting than one referenced by hundreds.
"""
import collections
import glob
import io
import os
import re
import sys

SCRATCH = sys.argv[1] if len(sys.argv) > 1 else None
if not SCRATCH:
    cands = glob.glob(os.path.join(
        os.environ.get('TEMP', r'C:\Users\felix\AppData\Local\Temp'),
        'azul-web-transpiler-*'))
    cands.sort(key=os.path.getmtime, reverse=True)
    SCRATCH = cands[0]
print('scratch: %s' % SCRATCH)

files = sorted(glob.glob(os.path.join(SCRATCH, '*.ll')))
print('scanning %d .ll files' % len(files))

re_nl = re.compile(r'^; NeverLift trap for (sub_[0-9a-f]+)', re.M)
re_call = re.compile(r'call [^@\n]*@(sub_[0-9a-f]+)\(')

never = set()
callers = collections.Counter()
# Which files DEFINE a NeverLift stub (every helper that links it does).
defining = collections.Counter()

for path in files:
    try:
        txt = io.open(path, encoding='utf-8', errors='replace').read()
    except OSError:
        continue
    local_nl = set(re_nl.findall(txt))
    if local_nl:
        never |= local_nl
        for s in local_nl:
            defining[s] += 1

print('distinct NeverLift symbols: %d' % len(never))
if not never:
    raise SystemExit

# Second pass: who calls them. A stub's own defining file trivially "calls"
# nothing, so count files that CALL the symbol without defining it as a trap.
for path in files:
    try:
        txt = io.open(path, encoding='utf-8', errors='replace').read()
    except OSError:
        continue
    called = set(re_call.findall(txt)) & never
    if not called:
        continue
    base = os.path.basename(path)
    for s in called:
        callers[s] += 1

print('')
print('%-16s %10s %10s' % ('symbol', 'callers', 'defs'))
for s, n in callers.most_common(40):
    print('%-16s %10d %10d' % (s, n, defining.get(s, 0)))

unref = [s for s in never if not callers.get(s)]
print('')
print('NeverLift stubs with no caller in the corpus: %d' % len(unref))
print('')
print('Name the top ones with:')
for s, _ in callers.most_common(8):
    print('  python scripts/m9_e2e/name-synth.py 0x%s <server.log>' % s[4:])
