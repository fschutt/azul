"""Which State fields does lifted code READ that nothing ever initialises?

GSBASE was found the slow way: boot, trap, name the recorder address, chase it.
That costs an hour per blocker. This finds the whole class at once.

The export wrapper memsets the State buffer to zero and then writes exactly:
SP, the ABI arg slots, and (new) GSBASE. Any OTHER State field that lifted code
loads before storing is reading a zero that nobody chose - which is correct for
most registers (a callee-saved reg genuinely starts undefined) but catastrophic
for anything used as a base address, exactly like GSBASE was.

Method: for each lifted body, find named State pointers (%GSBASE, %FS_BASE,
%CSBASE, ...) that appear in a `load` with no preceding `store` to the same
name in that file. Registers are excluded by name - the interesting population
is the segment bases and control/status words, which no ABI initialises.

Reports how many bodies read each one, so the next blocker is ranked rather
than discovered serially.
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

# General-purpose registers and the PC chain are seeded by the ABI or by remill
# itself; they are not the population of interest.
GPR = set(['RAX', 'RBX', 'RCX', 'RDX', 'RSI', 'RDI', 'RSP', 'RBP', 'RIP',
           'PC', 'NEXT_PC', 'MEMORY', 'STATE'])
GPR |= set('R%d' % i for i in range(8, 16))
GPR |= set('XMM%d' % i for i in range(0, 32))
GPR |= set('EAX EBX ECX EDX ESI EDI ESP EBP'.split())
GPR |= set('R%dD' % i for i in range(8, 16))
GPR |= set('R%dW' % i for i in range(8, 16))
GPR |= set('R%dB' % i for i in range(8, 16))
GPR |= set('AL BL CL DL AH BH CH DH AX BX CX DX SIL DIL SPL BPL'.split())

decl = re.compile(r'^\s*%([A-Za-z_][A-Za-z0-9_]*) = getelementptr inbounds %struct\.State', re.M)
load = re.compile(r'load [^,]+, ptr %([A-Za-z_][A-Za-z0-9_]*)')
store = re.compile(r'store [^,]+, ptr %([A-Za-z_][A-Za-z0-9_]*)')

files = sorted(glob.glob(os.path.join(SCRATCH, '*.ll')))
print('scanning %d .ll files' % len(files))

read_only = collections.Counter()
read_any = collections.Counter()
examples = {}

for path in files:
    try:
        txt = io.open(path, encoding='utf-8', errors='replace').read()
    except OSError:
        continue
    if '%struct.State' not in txt:
        continue
    names = set()
    for m in decl.finditer(txt):
        n = m.group(1)
        if n not in GPR:
            names.add(n)
    if not names:
        continue
    stored = set(m.group(1) for m in store.finditer(txt))
    loaded = set(m.group(1) for m in load.finditer(txt))
    for n in names:
        if n in loaded:
            read_any[n] += 1
            if n not in stored:
                read_only[n] += 1
                examples.setdefault(n, os.path.basename(path))

print('')
print('%-16s %8s %8s  %s' % ('State field', 'read', 'never-set', 'example body'))
for n, c in read_any.most_common(40):
    ro = read_only.get(n, 0)
    flag = '  <-- reads a zero nobody chose' if ro and ro == c else ''
    print('%-16s %8d %8d  %s%s' % (n, c, ro, examples.get(n, '')[:52], flag))

print('')
print('Fields read in EVERY body that reads them without a store are the ones')
print('that behave like GSBASE did: the value is whatever memset left.')
