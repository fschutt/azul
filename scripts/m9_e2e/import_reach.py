"""Which imports can the LIFTED corpus actually reach?

`import_gap.py` answers "what is un-intercepted", by name heuristic over all 248
imports. That is a superset: most of those are never reached from the functions
we actually lift, and intercepting them all is noise. This answers the sharper
question - of the un-intercepted imports, which ones does lifted code REFERENCE -
and turns "one blocker discovered per 35-minute run" into one list, now.

Method, all static, no relift:

  1. The lift scratch holds one `<Name>_<va>.lifted.ll` per walked function, so
     the filenames ARE the walked set.
  2. `.pdata` (the exception directory) gives every function's exact
     [begin, end). This is the authoritative extent - notably it does NOT suffer
     the ICF truncation that gap-to-next-symbol sizing does.
  3. Inside each walked extent, scan for `FF 15 disp32` (call [rip+d]) and
     `FF 25 disp32` (jmp [rip+d]) and resolve the target.
  4. A target that lands exactly on an IAT slot names the import.

Step 3 is a byte scan, not a disassembly, so it can in principle match bytes in
the middle of another instruction. Step 4 is what makes that harmless: a false
match must also land exactly on an 8-byte-aligned live IAT slot, and the cost of
one spurious interception case is a few bytes of dispatcher.

The module base is not recorded anywhere, so it is recovered: a base is 64K
aligned, so only `.pdata` begins whose low 16 bits equal a walked VA's low 16
bits can pair with it. Intersecting the candidate sets of a few VAs leaves one.
"""
import ctypes
import ctypes.wintypes as w
import io
import os
import re
import struct
import sys

EXE = (r'C:\Users\felix\Development\azul\target\x86_64-pc-windows-msvc'
       r'\release\AzWriter.exe')
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


# --- PE ----------------------------------------------------------------------
raw = io.open(EXE, 'rb').read()
pe = struct.unpack_from('<I', raw, 0x3c)[0]
nsec, = struct.unpack_from('<H', raw, pe + 6)
optsz, = struct.unpack_from('<H', raw, pe + 20)
opt = pe + 24
imp_rva, _ = struct.unpack_from('<II', raw, opt + 112 + 1 * 8)
exc_rva, exc_sz = struct.unpack_from('<II', raw, opt + 112 + 3 * 8)

secs = []
off = pe + 24 + optsz
for _ in range(nsec):
    name = raw[off:off + 8].rstrip(b'\0').decode('ascii', 'replace')
    vsize, vaddr, rsize, praw = struct.unpack_from('<IIII', raw, off + 8)
    secs.append((name, vaddr, vsize, praw, rsize))
    off += 40


def r2o(rva):
    for _n, vaddr, vsize, praw, rsize in secs:
        if vaddr <= rva < vaddr + max(vsize, rsize):
            return praw + (rva - vaddr)
    return None


def cstr(rva):
    o = r2o(rva)
    return '?' if o is None else raw[o:raw.index(b'\0', o)].decode('ascii', 'replace')


# --- imports: slot rva -> (dll, name) ---------------------------------------
slots = {}
o = r2o(imp_rva)
i = 0
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
        slots[iat_rva + k * 8] = (dll, nm)
        k += 1
    i += 1

# --- interception list -------------------------------------------------------
src = io.open(SRC, encoding='utf-8').read()
wanted = set()
for m in re.finditer(r'\("([A-Za-z0-9_.\-]+\.dll)\\0",\s*"([A-Za-z0-9_]+)\\0"', src, re.I):
    a = resolve(m.group(1), m.group(2))
    if a:
        wanted.add(a & 0xFFFFFFFF)

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

# --- walked set --------------------------------------------------------------
scratch = sys.argv[1] if len(sys.argv) > 1 else None
if scratch is None:
    base_tmp = os.environ.get('TEMP', r'C:\Users\felix\AppData\Local\Temp')
    cands = [os.path.join(base_tmp, d) for d in os.listdir(base_tmp)
             if d.startswith('azul-web-transpiler-')]
    cands.sort(key=os.path.getmtime, reverse=True)
    if not cands:
        raise SystemExit('no lift scratch found')
    scratch = cands[0]

vas = set()
names = {}
pat = re.compile(r'^(.*)_([0-9a-f]{8,16})\.lifted\.ll$')
for f in os.listdir(scratch):
    m = pat.search(f)
    if m:
        v = int(m.group(2), 16)
        vas.add(v)
        names[v] = m.group(1)
print('scratch %s' % scratch)
print('walked functions: %d' % len(vas))
if not vas:
    raise SystemExit('no walked functions - wrong scratch?')

# --- recover the module base -------------------------------------------------
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
base, n = max(votes.items(), key=lambda kv: kv[1])
bset = set(begins)
hit = sum(1 for v in vas if (v - base) in bset)
runner = sorted(votes.values(), reverse=True)[1] if len(votes) > 1 else 0
print('module base 0x%x  (%d/%d walked VAs land on a .pdata begin; '
      'runner-up base scored %d)' % (base, hit, len(vas), runner))
if hit * 2 < len(vas):
    print('  ** under half the walked set matches - is this scratch from THIS '
          'build? every relink moves every RVA.')

# --- scan ---------------------------------------------------------------------
bstart = {b: e for b, e in funcs}
walked_rvas = sorted(v - base for v in vas)
reach = {}
scanned = 0
for rva in walked_rvas:
    e = bstart.get(rva)
    if e is None:                       # not a .pdata entry (thunk, or folded)
        continue
    o0 = r2o(rva)
    if o0 is None:
        continue
    n = e - rva
    body = raw[o0:o0 + n]
    scanned += 1
    p = 0
    while True:
        p = body.find(b'\xff', p)
        if p < 0 or p + 6 > n:
            break
        if body[p + 1] in (0x15, 0x25):
            disp, = struct.unpack_from('<i', body, p + 2)
            tgt = rva + p + 6 + disp
            if tgt in slots:
                reach.setdefault(slots[tgt], set()).add(rva)
        p += 1

print('scanned %d walked functions with a .pdata extent' % scanned)
print('')

by_rva = {}
for v, s in names.items():
    by_rva[v - base] = s


def callers(sites):
    out = sorted(by_rva.get(r, '0x%x' % r) for r in sites)
    return out


miss, have = [], []
for (dll, nm), sites in sorted(reach.items()):
    a = resolve(dll, nm)
    lab = (a & 0xFFFFFFFF) if a else None
    row = (dll, nm, len(sites), callers(sites))
    (have if (lab is not None and lab in wanted) else miss).append(row)

def show(rows):
    for dll, nm, c, cs in rows:
        print('  %-38s %-26s %d site(s)' % (dll, nm, c))
        for s in cs[:8]:
            print('        <- %s' % s)
        if len(cs) > 8:
            print('        ... %d more' % (len(cs) - 8))


print('=== REACHED by lifted code and ALREADY intercepted: %d ===' % len(have))
show(have)
print('')
print('=== REACHED by lifted code and NOT intercepted: %d  <-- the queue ==='
      % len(miss))
show(sorted(miss, key=lambda r: -r[2]))
print('')
print('imports in the PE: %d   reached by lifted code: %d   of those un-intercepted: %d'
      % (len(slots), len(reach), len(miss)))
