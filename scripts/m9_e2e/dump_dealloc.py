"""Dump the bytes of __rust_dealloc and what follows it.

The unmatched dispatch is at synth 0xe7cbd2 = rva 0xd7dbd2, which name-synth
places +18 bytes into `__rust_dealloc` - a function the symbol table says is 16
bytes. So the target is PAST the end of its own entry, in whatever begins next
(`__rdl_dealloc`, per the symbolizer).

Reading the actual bytes is the only way to settle what the 16-byte body does
and why the branch target lands at +18 rather than +16. Decoding by hand from a
CONFIRMED entry, per the rule that an instruction boundary must be re-verified
rather than inferred from a size field.
"""
import io
import struct
import sys

EXE = sys.argv[1] if len(sys.argv) > 1 else \
    r'C:\Users\felix\Development\azul\target\x86_64-pc-windows-msvc\release\AzWriter.exe'
RVA = int(sys.argv[2], 16) if len(sys.argv) > 2 else 0xd7dbc0
N = int(sys.argv[3]) if len(sys.argv) > 3 else 64

raw = io.open(EXE, 'rb').read()
pe = struct.unpack_from('<I', raw, 0x3c)[0]
nsec, = struct.unpack_from('<H', raw, pe + 6)
optsz, = struct.unpack_from('<H', raw, pe + 20)

secs = []
off = pe + 24 + optsz
for _ in range(nsec):
    name = raw[off:off + 8].rstrip(b'\0').decode('ascii', 'replace')
    vsize, vaddr, rsize, praw = struct.unpack_from('<IIII', raw, off + 8)
    secs.append((name, vaddr, vsize, praw, rsize))
    off += 40


def rva_to_off(rva):
    for name, vaddr, vsize, praw, rsize in secs:
        if vaddr <= rva < vaddr + max(vsize, rsize):
            return praw + (rva - vaddr), name
    return None, None


o, sec = rva_to_off(RVA)
if o is None:
    raise SystemExit('rva 0x%x is in no section' % RVA)

b = raw[o:o + N]
print('rva 0x%x  section %s  file off 0x%x' % (RVA, sec, o))
print('')
for i in range(0, N, 16):
    chunk = b[i:i + 16]
    print('  +0x%02x  %-48s %s' % (
        i,
        ' '.join('%02x' % c for c in chunk),
        ''.join(chr(c) if 32 <= c < 127 else '.' for c in chunk)))

print('')
# Decode the handful of shapes that matter here rather than pulling in a
# disassembler: a rel32 jmp/call, and the MSVC 2-byte hotpatch pad.
i = 0
while i < min(N, 32):
    op = b[i]
    if op == 0xe9 and i + 5 <= N:
        rel = struct.unpack_from('<i', b, i + 1)[0]
        tgt = RVA + i + 5 + rel
        print('  +0x%02x  e9 rel32  -> JMP rva 0x%x' % (i, tgt))
        i += 5
    elif op == 0xe8 and i + 5 <= N:
        rel = struct.unpack_from('<i', b, i + 1)[0]
        tgt = RVA + i + 5 + rel
        print('  +0x%02x  e8 rel32  -> CALL rva 0x%x' % (i, tgt))
        i += 5
    elif op == 0xff and i + 6 <= N and b[i + 1] == 0x25:
        rel = struct.unpack_from('<i', b, i + 2)[0]
        tgt = RVA + i + 6 + rel
        print('  +0x%02x  ff 25     -> JMP [rip+0x%x] = rva 0x%x (indirect)' % (i, rel, tgt))
        i += 6
    elif op == 0x8b and i + 2 <= N and b[i + 1] == 0xff:
        print('  +0x%02x  8b ff     -> mov edi,edi (MSVC 2-byte hotpatch pad)' % i)
        i += 2
    elif op == 0xcc:
        print('  +0x%02x  cc        -> int3 (padding)' % i)
        i += 1
    else:
        i += 1
