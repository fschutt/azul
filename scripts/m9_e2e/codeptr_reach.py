"""Which function pointers stored in DATA point at code that was never walked?

The import scan closed one failure class: an indirect call to a native address
with no dispatcher case. This is the sibling class - an indirect call to a
LIFTED address with no dispatcher case. Both present identically (unmatched
dispatch, a recorded PC); the PC's band is what tells them apart.

The walk follows calls it can see. A function whose address is stored in data -
a vtable slot, a `fn` item in a static, a callback table - is reached through
that pointer, which the walk cannot follow. If such a target was never walked,
an indirect call through it misses every case.

Where the pointers are: every 64-bit code pointer in an image's data carries a
base relocation, so `.reloc` enumerates them exactly. A DIR64 entry whose stored
value resolves into `.text` IS a code pointer, with no disassembly needed.

Unbounded, that lists every vtable in the binary, most of which no walked code
can ever load. So the default narrows it to pointers sitting in a data region
that walked code actually references (`lea`/`mov r64, [rip+d]` into .rdata or
.data), within a window either side of the referenced address - a vtable slot is
a short hop from the vtable's own address. Pass --all to drop the narrowing.

Usage: codeptr_reach.py [<scratch>] [--all] [--window N]
"""
import bisect
import io
import os
import re
import struct
import subprocess
import sys

EXE = (r'C:\Users\felix\Development\azul\target\x86_64-pc-windows-msvc'
       r'\release\AzWriter.exe')

args = [a for a in sys.argv[1:] if not a.startswith('--')]
ALL = '--all' in sys.argv
WINDOW = 512
for i, a in enumerate(sys.argv):
    if a == '--window' and i + 1 < len(sys.argv):
        WINDOW = int(sys.argv[i + 1])

raw = io.open(EXE, 'rb').read()
pe = struct.unpack_from('<I', raw, 0x3c)[0]
nsec, = struct.unpack_from('<H', raw, pe + 6)
optsz, = struct.unpack_from('<H', raw, pe + 20)
opt = pe + 24
pref_base, = struct.unpack_from('<Q', raw, opt + 24)
exc_rva, exc_sz = struct.unpack_from('<II', raw, opt + 112 + 3 * 8)
rel_rva, rel_sz = struct.unpack_from('<II', raw, opt + 112 + 5 * 8)

secs = []
off = pe + 24 + optsz
for _ in range(nsec):
    name = raw[off:off + 8].rstrip(b'\0').decode('ascii', 'replace')
    vsize, vaddr, rsize, praw = struct.unpack_from('<IIII', raw, off + 8)
    secs.append((name, vaddr, vsize, praw, rsize))
    off += 40


def sec_of(rva):
    for n, vaddr, vsize, praw, rsize in secs:
        if vaddr <= rva < vaddr + max(vsize, rsize):
            return n, praw + (rva - vaddr)
    return None, None


def r2o(rva):
    return sec_of(rva)[1]


text = next((s for s in secs if s[0] == '.text'), None)
if text is None:
    raise SystemExit('no .text')
TEXT_LO, TEXT_HI = text[1], text[1] + max(text[2], text[4])

# --- .pdata ------------------------------------------------------------------
po = r2o(exc_rva)
funcs = []
for j in range(exc_sz // 12):
    b, e, _u = struct.unpack_from('<III', raw, po + j * 12)
    if b == 0 and e == 0:
        break
    funcs.append((b, e))
funcs.sort()
begins = [b for b, _e in funcs]
bstart = dict(funcs)

# --- walked set --------------------------------------------------------------
scratch = args[0] if args else None
if scratch is None:
    tmp = os.environ.get('TEMP', r'C:\Users\felix\AppData\Local\Temp')
    cands = [os.path.join(tmp, d) for d in os.listdir(tmp)
             if d.startswith('azul-web-transpiler-')]
    cands.sort(key=os.path.getmtime, reverse=True)
    if not cands:
        raise SystemExit('no lift scratch found')
    scratch = cands[0]

vas, names = set(), {}
pat = re.compile(r'^(.*)_([0-9a-f]{8,16})\.lifted\.ll$')
for f in os.listdir(scratch):
    m = pat.search(f)
    if m:
        v = int(m.group(2), 16)
        vas.add(v)
        names[v] = m.group(1)
print('scratch %s' % scratch)
print('walked functions: %d' % len(vas))

by_low = {}
for b in begins:
    by_low.setdefault(b & 0xFFFF, []).append(b)
votes = {}
for v in vas:
    for b in by_low.get(v & 0xFFFF, []):
        x = v - b
        if x >= 0x10000:
            votes[x] = votes.get(x, 0) + 1
if not votes:
    raise SystemExit('could not recover the module base')
base = max(votes.items(), key=lambda kv: kv[1])[0]
walked = set(v - base for v in vas)
hit = sum(1 for r in walked if r in bstart)
print('module base 0x%x  (%d/%d walked VAs land on a .pdata begin)'
      % (base, hit, len(walked)))
if hit * 2 < len(walked):
    print('  ** under half matches - scratch and PE are from different builds')

# A walked interior address covers its whole extent.
walked_extents = set()
for r in walked:
    if r in bstart:
        walked_extents.add(r)
        continue
    i = bisect.bisect_right(begins, r) - 1
    if i >= 0 and r < bstart[begins[i]]:
        walked_extents.add(begins[i])


def covered(rva):
    if rva in walked_extents:
        return True
    i = bisect.bisect_right(begins, rva) - 1
    return i >= 0 and rva < bstart[begins[i]] and begins[i] in walked_extents


# --- data regions walked code references -------------------------------------
refs = []
if not ALL:
    for r in sorted(walked_extents):
        e = bstart[r]
        o0 = r2o(r)
        if o0 is None:
            continue
        body = raw[o0:o0 + (e - r)]
        for p in range(len(body) - 7):
            if body[p] in (0x48, 0x4C) and body[p + 1] in (0x8B, 0x8D) \
                    and (body[p + 2] & 0xC7) == 0x05:
                d, = struct.unpack_from('<i', body, p + 3)
                t = r + p + 7 + d
                if sec_of(t)[0] in ('.rdata', '.data'):
                    refs.append(t)
    refs.sort()
    print('data addresses referenced by walked code: %d' % len(refs))


def near_ref(rva):
    i = bisect.bisect_left(refs, rva - WINDOW)
    return i < len(refs) and refs[i] <= rva + WINDOW


# --- .reloc: every 64-bit code pointer in data -------------------------------
ro = r2o(rel_rva)
ptrs = []
pos = 0
while pos + 8 <= rel_sz:
    page, blk = struct.unpack_from('<II', raw, ro + pos)
    if blk < 8:
        break
    for k in range((blk - 8) // 2):
        ent, = struct.unpack_from('<H', raw, ro + pos + 8 + k * 2)
        if (ent >> 12) != 10:               # IMAGE_REL_BASED_DIR64
            continue
        site = page + (ent & 0xFFF)
        sn, so = sec_of(site)
        if sn not in ('.rdata', '.data') or so is None:
            continue
        val, = struct.unpack_from('<Q', raw, so)
        tgt = val - pref_base
        if TEXT_LO <= tgt < TEXT_HI:
            ptrs.append((site, tgt))
    pos += blk
print('64-bit code pointers in data: %d' % len(ptrs))

cand = [(s, t) for s, t in ptrs if ALL or near_ref(s)]
missing = {}
for s, t in cand:
    if not covered(t):
        missing.setdefault(t, []).append(s)
print('%s: %d, of which NOT walked: %d'
      % ('all code pointers' if ALL else 'code pointers in referenced data',
         len(cand), len(missing)))
print('')

# An un-walked target has no .ll file by definition, and the lift log names only
# functions something CALLED - so neither source can name these. The PDB can.
# llvm-symbolizer answers for any address in the image and returns the source
# location too; addresses go in at the PREFERRED base, since it reads the file
# on disk rather than a running process.
SYMBOLIZER = (r'C:\Users\felix\Development\azul\third_party\remill'
              r'\dependencies\install\bin\llvm-symbolizer.exe')


def symbolize(rvas):
    if not rvas or not os.path.exists(SYMBOLIZER):
        return {}
    stdin = '\n'.join('0x%x' % (pref_base + r) for r in rvas) + '\n'
    try:
        p = subprocess.run(
            [SYMBOLIZER, '--obj=' + EXE, '--functions=short'],
            input=stdin, capture_output=True, text=True, timeout=300)
    except Exception as exc:
        print('  (symbolizer failed: %s)' % exc)
        return {}
    out = {}
    # Each address answers with a name line, a location line, then a blank.
    blocks = p.stdout.replace('\r\n', '\n').split('\n\n')
    for r, blk in zip(rvas, blocks):
        lines = [x for x in blk.split('\n') if x.strip()]
        if not lines:
            continue
        name = lines[0].strip()
        loc = lines[1].strip() if len(lines) > 1 else ''
        if name and name != '??':
            out[r] = (name, loc)
    return out


log_names = {}
LOG = os.environ.get('AZ_LIFT_LOG', r'C:\rb\azwriter_server.log')
if os.path.exists(LOG):
    pr = re.compile(r'resolved=(\S+)@0x([0-9a-fA-F]{16})')
    for line in io.open(LOG, encoding='utf-8', errors='replace'):
        m = pr.search(line)
        if m:
            log_names.setdefault(int(m.group(2), 16) - base, m.group(1))
    print('names recovered from the lift log: %d' % len(log_names))

# A target that is not even a .pdata begin is data misread as a pointer far more
# often than it is a real entry point, so separate the two.
real = sorted(t for t in missing if t in bstart)
odd = sorted(t for t in missing if t not in bstart)
print('=== NOT walked, and a real function entry: %d  <-- these can miss ==='
      % len(real))
sym = symbolize(real)
print('names recovered from the PDB: %d of %d' % (len(sym), len(real)))
print('')
rows = []
for t in real:
    nm, loc = sym.get(t, (log_names.get(t, ''), ''))
    rows.append((nm or '(unnamed)', loc, t))
for nm, loc, t in sorted(rows):
    print('  %-64s %d site(s)  0x%x' % (nm[:64], len(missing[t]), t))
    if loc:
        print('        %s' % loc)
print('')
print('=== NOT walked and not a .pdata begin (likely not a pointer): %d ===' % len(odd))
