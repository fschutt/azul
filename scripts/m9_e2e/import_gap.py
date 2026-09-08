"""Which PE imports have no interception case? (address-based, like the real code)

The first cut compared DLL NAMES and was wrong: the image imports `trunc` from
`api-ms-win-crt-math-l1-1-0.dll` while the interception list names
`ucrtbase.dll`. API-set DLLs forward, so both resolve to the SAME function
address - and `intercepted_import_labels` matches on `GetProcAddress(...) &
0xFFFFFFFF`, not on the name. Name-matching therefore reported ~20 false gaps.

This resolves every import and every WANTED entry to an address and compares
those, which is exactly what the lift does.
"""
import ctypes
import ctypes.wintypes as w
import io
import re
import struct
import sys

EXE = r'C:\Users\felix\Development\azul\target\x86_64-pc-windows-msvc\release\AzWriter.exe'
SRC = r'C:\Users\felix\Development\azul\dll\src\web\transpiler_remill.rs'

k32 = ctypes.WinDLL('kernel32', use_last_error=True)
k32.GetModuleHandleA.restype = w.HMODULE
k32.GetModuleHandleA.argtypes = [w.LPCSTR]
k32.LoadLibraryA.restype = w.HMODULE
k32.LoadLibraryA.argtypes = [w.LPCSTR]
k32.GetProcAddress.restype = ctypes.c_void_p
k32.GetProcAddress.argtypes = [w.HMODULE, w.LPCSTR]


def resolve(dll, func):
    m = k32.GetModuleHandleA(dll.encode()) or k32.LoadLibraryA(dll.encode())
    if not m:
        return None
    return k32.GetProcAddress(m, func.encode()) or None


src = io.open(SRC, encoding='utf-8').read()
wanted_labels = {}
for m in re.finditer(r'\("([A-Za-z0-9_.\-]+\.dll)\\0",\s*"([A-Za-z0-9_]+)\\0"', src, re.I):
    a = resolve(m.group(1), m.group(2))
    if a:
        wanted_labels[a & 0xFFFFFFFF] = '%s!%s' % (m.group(1), m.group(2))
print('interception list resolves to %d distinct labels' % len(wanted_labels))

raw = io.open(EXE, 'rb').read()
pe = struct.unpack_from('<I', raw, 0x3c)[0]
nsec, = struct.unpack_from('<H', raw, pe + 6)
optsz, = struct.unpack_from('<H', raw, pe + 20)
imp_rva, _ = struct.unpack_from('<II', raw, pe + 24 + 112 + 1 * 8)

secs = []
off = pe + 24 + optsz
for _ in range(nsec):
    vsize, vaddr, rsize, praw = struct.unpack_from('<IIII', raw, off + 8)
    secs.append((vaddr, vsize, praw, rsize))
    off += 40


def r2o(rva):
    for vaddr, vsize, praw, rsize in secs:
        if vaddr <= rva < vaddr + max(vsize, rsize):
            return praw + (rva - vaddr)
    return None


def cstr(rva):
    o = r2o(rva)
    return '?' if o is None else raw[o:raw.index(b'\0', o)].decode('ascii', 'replace')


INTERESTING = re.compile(
    r'Heap|Alloc|Free|Virtual|Prng|Random|Crypt|Counter|SystemTime|TickCount|'
    r'Sleep|WaitOnAddress|WakeByAddress|Tls|Fls|mem(cpy|move|set|cmp)|strlen|'
    r'^_?(tan|sin|cos|pow|exp|log|fmod|round|trunc|floor|ceil|fabs|sqrt)f?$', re.I)

o = r2o(imp_rva)
rows, i = [], 0
while True:
    ent = raw[o + i * 20:o + i * 20 + 20]
    if len(ent) < 20:
        break
    ilt, _t, _f, name_rva, iat_rva = struct.unpack('<IIIII', ent)
    if name_rva == 0 and iat_rva == 0:
        break
    dll = cstr(name_rva)
    lookup = ilt or iat_rva
    k = 0
    while True:
        lo = r2o(lookup + k * 8)
        if lo is None:
            break
        th = struct.unpack_from('<Q', raw, lo)[0]
        if th == 0:
            break
        nm = ('ordinal %d' % (th & 0xFFFF)) if th & (1 << 63) else cstr((th & 0x7FFFFFFF) + 2)
        addr = resolve(dll, nm) if not (th & (1 << 63)) else None
        lab = (addr & 0xFFFFFFFF) if addr else None
        rows.append((dll, nm, lab, lab in wanted_labels if lab else False))
        k += 1
    i += 1

cov = sum(1 for *_x, c in rows if c)
print('imports: %d   intercepted (by resolved address): %d' % (len(rows), cov))
print('')
gaps = [(d, n) for d, n, _l, c in rows if not c and INTERESTING.search(n)]
by = {}
for d, n in gaps:
    by.setdefault(d, []).append(n)
print('=== NOT intercepted, and plausibly reachable from lifted std code ===')
for d in sorted(by):
    print('  %s' % d)
    print('      %s' % ', '.join(sorted(by[d])))
print('')
print('%d flagged of %d un-intercepted' % (len(gaps), len(rows) - cov))
