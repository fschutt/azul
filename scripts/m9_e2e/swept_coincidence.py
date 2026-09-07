"""How much of the mini is swept in by COINCIDENCE rather than by a real pointer?

MEASUREMENT ONLY - this changes nothing in the transpiler. It quantifies the
audit's W5 finding ("N fn(s) reached ONLY via a mirrored data window - no call
site, no address-take anywhere"), which run 72 put at 1067 functions.

The data-window sweep exists for a good reason: a vtable slot is a code pointer
with no address-take in code, so scanning `.rdata` around a referenced address is
how those targets get found at all. The cost is that any 8-aligned qword in that
window which merely LOOKS like a code pointer becomes a root too.

`.reloc` separates the two exactly. Every genuine 64-bit pointer in an image's
data carries a base relocation, because the loader has to fix it up when the
image moves. Bytes that happen to resemble a pointer do not. So:

  reached = called directly | address-taken in code | at a DIR64 relocation site

and a walked function in none of those was swept in by coincidence. Dropping
those is safe in a way that dropping all W5 functions is not - the vtable slots
this mechanism exists to find are exactly the ones that DO carry a relocation.

Usage: swept_coincidence.py [<scratch>]
"""
import bisect
import io
import os
import re
import struct
import sys

EXE = (r'C:\Users\felix\Development\azul\target\x86_64-pc-windows-msvc'
       r'\release\AzWriter.exe')

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


text = next(s for s in secs if s[0] == '.text')
TEXT_LO, TEXT_HI = text[1], text[1] + max(text[2], text[4])

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
scratch = sys.argv[1] if len(sys.argv) > 1 else None
if scratch is None:
    tmp = os.environ.get('TEMP', r'C:\Users\felix\AppData\Local\Temp')
    c = [os.path.join(tmp, d) for d in os.listdir(tmp)
         if d.startswith('azul-web-transpiler-')]
    c.sort(key=os.path.getmtime, reverse=True)
    scratch = c[0]

vas, names = set(), {}
pat = re.compile(r'^(.*)_([0-9a-f]{8,16})\.lifted\.ll$')
for f in os.listdir(scratch):
    m = pat.search(f)
    if m:
        v = int(m.group(2), 16)
        vas.add(v)
        names[v] = m.group(1)

by_low = {}
for b in begins:
    by_low.setdefault(b & 0xFFFF, []).append(b)
votes = {}
for v in vas:
    for b in by_low.get(v & 0xFFFF, []):
        if v - b >= 0x10000:
            votes[v - b] = votes.get(v - b, 0) + 1
base = max(votes.items(), key=lambda kv: kv[1])[0]
print('scratch %s' % scratch)
print('walked %d  module base 0x%x' % (len(vas), base))

walked = {}
for v in vas:
    walked[v - base] = names[v]


def extent(rva):
    if rva in bstart:
        return rva
    i = bisect.bisect_right(begins, rva) - 1
    if i >= 0 and rva < bstart[begins[i]]:
        return begins[i]
    return None


walked_ext = {}
for r, n in walked.items():
    e = extent(r)
    if e is not None:
        walked_ext.setdefault(e, n)
print('walked .pdata extents: %d' % len(walked_ext))

# --- reached: direct calls + address-taken in code ---------------------------
called, taken = set(), set()
for r in walked_ext:
    e = bstart[r]
    o0 = r2o(r)
    if o0 is None:
        continue
    body = raw[o0:o0 + (e - r)]
    n = len(body)
    for p in range(n - 7):
        b0 = body[p]
        if b0 in (0xE8, 0xE9):
            d, = struct.unpack_from('<i', body, p + 1)
            t = r + p + 5 + d
            if TEXT_LO <= t < TEXT_HI:
                called.add(t)
        elif b0 in (0x48, 0x4C) and body[p + 1] == 0x8D \
                and (body[p + 2] & 0xC7) == 0x05:
            d, = struct.unpack_from('<i', body, p + 3)
            t = r + p + 7 + d
            if TEXT_LO <= t < TEXT_HI:
                taken.add(t)
print('direct-call targets in walked code: %d' % len(called))
print('addresses taken by lea in walked code: %d' % len(taken))

# --- reached: a real relocated pointer in data -------------------------------
ro = r2o(rel_rva)
reloc_targets = set()
pos = 0
while pos + 8 <= rel_sz:
    page, blk = struct.unpack_from('<II', raw, ro + pos)
    if blk < 8:
        break
    for k in range((blk - 8) // 2):
        ent, = struct.unpack_from('<H', raw, ro + pos + 8 + k * 2)
        if (ent >> 12) != 10:
            continue
        site = page + (ent & 0xFFF)
        sn, so = sec_of(site)
        if sn not in ('.rdata', '.data') or so is None:
            continue
        val, = struct.unpack_from('<Q', raw, so)
        t = val - pref_base
        if TEXT_LO <= t < TEXT_HI:
            reloc_targets.add(t)
    pos += blk
print('relocated code pointers in data: %d distinct target(s)' % len(reloc_targets))
print('')

# Reference sets are keyed on the TARGET ADDRESS; map each to its extent, so a
# pointer into the middle of an ICF-folded function does not read as a reference
# to nothing.
reached = called | taken | reloc_targets
reached_ext = set()
for t in reached:
    e = extent(t)
    if e is not None:
        reached_ext.add(e)
coincidence = [(r, n) for r, n in walked_ext.items() if r not in reached_ext]

tot = sum(bstart[r] - r for r in walked_ext)
cost = sum(bstart[r] - r for r, _n in coincidence)
print('=== walked functions with NO call, NO lea, NO relocated pointer ===')
print('%d of %d extents   %s of %s native bytes  (%.1f%%)'
      % (len(coincidence), len(walked_ext), '{:,}'.format(cost),
         '{:,}'.format(tot), 100.0 * cost / tot if tot else 0.0))
print('')
by_crate = {}
for r, n in coincidence:
    crate = n.split('__')[0] if '__' in n else n.split('::')[0]
    b = by_crate.setdefault(crate, [0, 0])
    b[0] += 1
    b[1] += bstart[r] - r
print('top crates by native bytes:')
for crate, (c, bts) in sorted(by_crate.items(), key=lambda kv: -kv[1][1])[:20]:
    print('  %-44s %5d fn  %12s B' % (crate[:44], c, '{:,}'.format(bts)))
