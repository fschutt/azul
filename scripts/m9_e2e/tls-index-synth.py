"""Print the synth address of `__tls_index` for a given run.

Synth addresses are assigned per build, and AzWriter LIFTS ITSELF - so any edit
under dll/src/web relinks the very image whose addresses are being computed.
Two consecutive runs differing only by a transpiler edit moved both the TLS
template's rva (0x1229de0 -> 0x122b760) and its synth delta (0x100900 ->
0xff000). Hardcoding either one is wrong by the next run, and the textbook
`synth_base + rva - 0x1000` is not dependable.

So calibrate: take the template's synth from the run's own `win-tls` log line,
subtract the template rva read from the exe that run used, and apply that delta
to AddressOfIndex.

    python scripts/m9_e2e/tls-index-synth.py [server.log] [AzWriter.exe]

Prints the address as bare hex, suitable for `tls-probe.js`'s argv[4]:

    node --experimental-websocket scripts/m9_e2e/tls-probe.js \
        http://127.0.0.1:8801/ 22000 $(python .../tls-index-synth.py)
"""
import io
import os
import re
import struct
import sys

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

LOG = sys.argv[1] if len(sys.argv) > 1 else r'C:\rb\azwriter_server.log'
EXE = sys.argv[2] if len(sys.argv) > 2 else os.path.join(
    REPO, 'target', 'x86_64-pc-windows-msvc', 'release', 'AzWriter.exe')


def die(msg):
    sys.stderr.write('tls-index-synth: %s\n' % msg)
    raise SystemExit(1)


# The log is multi-GB; scan line by line and stop at the first match.
pat = re.compile(r'win-tls: template synth=0x([0-9a-f]+)')
tpl_synth = None
try:
    with io.open(LOG, encoding='utf-8', errors='replace') as fh:
        for line in fh:
            if 'win-tls' not in line:
                continue
            m = pat.search(line)
            if m:
                tpl_synth = int(m.group(1), 16)
                break
except OSError as e:
    die('cannot read %s: %s' % (LOG, e))

if tpl_synth is None:
    die('no "win-tls: template synth=" line in %s - the seed never ran, so '
        'there is nothing to calibrate against' % LOG)

try:
    raw = io.open(EXE, 'rb').read()
except OSError as e:
    die('cannot read %s: %s' % (EXE, e))

pe = struct.unpack_from('<I', raw, 0x3c)[0]
if raw[pe:pe + 4] != b'PE\0\0':
    die('%s is not a PE' % EXE)
nsec, = struct.unpack_from('<H', raw, pe + 6)
optsz, = struct.unpack_from('<H', raw, pe + 20)
image_base, = struct.unpack_from('<Q', raw, pe + 24 + 24)
tls_rva, _ = struct.unpack_from('<II', raw, pe + 24 + 112 + 9 * 8)
if not tls_rva:
    die('image has no TLS directory')

secs = []
off = pe + 24 + optsz
for _ in range(nsec):
    vsize, vaddr, rsize, praw = struct.unpack_from('<IIII', raw, off + 8)
    secs.append((vaddr, vsize, praw, rsize))
    off += 40


def rva_to_off(rva):
    for vaddr, vsize, praw, rsize in secs:
        if vaddr <= rva < vaddr + max(vsize, rsize):
            return praw + (rva - vaddr)
    return None


foff = rva_to_off(tls_rva)
if foff is None:
    die('TLS directory rva 0x%x is in no section' % tls_rva)
start, _end, idx_addr, _cbs, _zf, _ch = struct.unpack_from('<QQQQII', raw, foff)

tpl_rva = start - image_base
idx_rva = idx_addr - image_base
delta = tpl_synth - tpl_rva

sys.stderr.write(
    'tls-index-synth: template rva 0x%x -> synth 0x%x (delta 0x%x); '
    '__tls_index rva 0x%x\n' % (tpl_rva, tpl_synth, delta, idx_rva))
print('%x' % (idx_rva + delta))
