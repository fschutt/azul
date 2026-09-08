"""Measure lift TRUNCATION: the size a lift used vs the function's real span.

Gap-to-next-symbol sizing truncates any function the linker folded another
symbol into, and the lift then decodes a prologue and emits a missing block at
the fallthrough - an address that is not an entry, so no dispatcher case can
ever key it. That was run 74's last blocker and run 68's before it.

Measured across a whole run, before and after sizing from `.pdata`:

    run 74   TRUNCATED 69   exact 125    over-read 2072   48,221 bytes never decoded
    run 75   TRUNCATED  0   exact  67    over-read    5            0

The worst single case in run 74 lifted **32 bytes of an 8,061-byte function**.
So this was never really about the two addresses that got chased.

A first cut asked how many LIFTED addresses sit inside another record and got 3,
which is the wrong question. `<CssVec as Drop>::drop` at 0xd0acb0 was never
lifted; it exists only as a SYMBOL, and its existence is what set the preceding
symbol's size to the 16-byte gap. The lift that got truncated is the one AT the
record's begin.

So compare, for every lifted address that is a `.pdata` begin, the size the lift
used against the record's real span. `size < span` is a truncated lift, and the
`.pdata` sizing should remove exactly those.

Usage: folded_entries2.py <server.log>
"""
import io
import re
import struct
import sys

EXE = (r'C:\Users\felix\Development\azul\target\x86_64-pc-windows-msvc'
       r'\release\AzWriter.exe')
LOG = sys.argv[1] if len(sys.argv) > 1 else r'C:\rb\azwriter_server.log'

raw = io.open(EXE, 'rb').read()
pe = struct.unpack_from('<I', raw, 0x3c)[0]
nsec, = struct.unpack_from('<H', raw, pe + 6)
optsz, = struct.unpack_from('<H', raw, pe + 20)
exc_rva, exc_sz = struct.unpack_from('<II', raw, pe + 24 + 112 + 3 * 8)

secs = []
off = pe + 24 + optsz
for _ in range(nsec):
    vsize, vaddr, rsize, praw = struct.unpack_from('<IIII', raw, off + 8)
    secs.append((vaddr, vsize, praw, rsize))
    off += 40


def r2o(rva):
    for v, vs, p, rs in secs:
        if v <= rva < v + max(vs, rs):
            return p + (rva - v)
    return None


po = r2o(exc_rva)
spans = {}
for j in range(exc_sz // 12):
    b, e, _u = struct.unpack_from('<III', raw, po + j * 12)
    if b == 0 and e == 0:
        break
    spans[b] = e - b

pat = re.compile(r'addr=0x([0-9a-fA-F]{16}) size=(\d+)')
lifted = {}
for line in io.open(LOG, encoding='utf-8', errors='replace'):
    m = pat.search(line)
    if m:
        va = int(m.group(1), 16)
        size = int(m.group(2))
        # Keep the SMALLEST size seen: that is the truncated lift, and the one
        # whose decode stops early.
        if va not in lifted or size < lifted[va]:
            lifted[va] = size

by_low = {}
for b in spans:
    by_low.setdefault(b & 0xFFFF, []).append(b)
votes = {}
for v in lifted:
    for b in by_low.get(v & 0xFFFF, []):
        if v - b >= 0x10000:
            votes[v - b] = votes.get(v - b, 0) + 1
base = max(votes.items(), key=lambda kv: kv[1])[0]
print('%s: %d distinct lifted addresses, module base 0x%x'
      % (LOG, len(lifted), base))
print('')

trunc = []
exact = over = nonrec = 0
for va, size in lifted.items():
    span = spans.get(va - base)
    if span is None:
        nonrec += 1
    elif size < span:
        trunc.append((span - size, va - base, size, span))
    elif size == span:
        exact += 1
    else:
        over += 1

print('lift size vs the .pdata span, for lifts at a record begin:')
print('  TRUNCATED (size < span) : %d' % len(trunc))
print('  exact     (size == span): %d' % exact)
print('  over-read (size > span) : %d' % over)
print('  no record               : %d' % nonrec)
if trunc:
    trunc.sort(reverse=True)
    lost = sum(d for d, _r, _s, _p in trunc)
    print('')
    print('  bytes never decoded: %s' % '{:,}'.format(lost))
    print('  worst 10:')
    for d, rva, size, span in trunc[:10]:
        print('    rva 0x%-8x lifted %-6d of %-6d  (%d bytes short)'
              % (rva, size, span, d))
