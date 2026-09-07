"""Name the import whose IAT slot sits at a given synth address.

An unmatched dispatch whose PC is outside the synth band is a raw native
pointer the lifted code read from a mirrored IAT slot. Scanning the mirror finds
WHICH slot; this turns that slot's address into the DLL!function it imports, by
walking the PE import directory's first-thunk arrays.

synth -> rva uses the delta the run's own win-tls line reports (synth = rva +
delta), never the textbook formula: the delta moves between builds.
"""
import io
import struct
import sys

EXE = r'C:\Users\felix\Development\azul\target\x86_64-pc-windows-msvc\release\AzWriter.exe'
SLOT_SYNTH = int(sys.argv[1], 16) if len(sys.argv) > 1 else 0xf085b0
DELTA = int(sys.argv[2], 16) if len(sys.argv) > 2 else 0xff000

RVA = SLOT_SYNTH - DELTA
print('slot synth 0x%x  (delta 0x%x)  -> rva 0x%x' % (SLOT_SYNTH, DELTA, RVA))

raw = io.open(EXE, 'rb').read()
pe = struct.unpack_from('<I', raw, 0x3c)[0]
nsec, = struct.unpack_from('<H', raw, pe + 6)
optsz, = struct.unpack_from('<H', raw, pe + 20)
dd = pe + 24 + 112
imp_rva, imp_size = struct.unpack_from('<II', raw, dd + 1 * 8)

secs = []
off = pe + 24 + optsz
for _ in range(nsec):
    name = raw[off:off + 8].rstrip(b'\0').decode('ascii', 'replace')
    vsize, vaddr, rsize, praw = struct.unpack_from('<IIII', raw, off + 8)
    secs.append((name, vaddr, vsize, praw, rsize))
    off += 40


def r2o(rva):
    for name, vaddr, vsize, praw, rsize in secs:
        if vaddr <= rva < vaddr + max(vsize, rsize):
            return praw + (rva - vaddr)
    return None


def cstr(rva):
    o = r2o(rva)
    if o is None:
        return '?'
    e = raw.index(b'\0', o)
    return raw[o:e].decode('ascii', 'replace')


o = r2o(imp_rva)
if o is None:
    raise SystemExit('no import directory')

found = False
i = 0
while True:
    ent = raw[o + i * 20:o + i * 20 + 20]
    if len(ent) < 20:
        break
    ilt, _ts, _fc, name_rva, iat_rva = struct.unpack('<IIIII', ent)
    if name_rva == 0 and iat_rva == 0:
        break
    dll = cstr(name_rva)
    # Walk the first-thunk (IAT) array; each entry is 8 bytes on PE32+.
    t = iat_rva
    lookup = ilt or iat_rva
    k = 0
    while True:
        to = r2o(t + k * 8)
        lo = r2o(lookup + k * 8)
        if to is None or lo is None:
            break
        thunk = struct.unpack_from('<Q', raw, lo)[0]
        if thunk == 0:
            break
        slot_rva = t + k * 8
        if slot_rva == RVA:
            if thunk & (1 << 63):
                nm = 'ordinal %d' % (thunk & 0xFFFF)
            else:
                nm = cstr((thunk & 0x7FFFFFFF) + 2)
            print('')
            print('  MATCH: %s ! %s' % (dll, nm))
            print('  (IAT slot rva 0x%x, entry #%d of that DLL)' % (slot_rva, k))
            found = True
        k += 1
    i += 1

if not found:
    print('')
    print('  no IAT slot at that rva - the pointer may live in .data, not the IAT')
