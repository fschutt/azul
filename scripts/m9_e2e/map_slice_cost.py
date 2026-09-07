"""What does a family of functions cost COMPRESSED, not raw?

Projecting a compressed saving from a raw one is wrong in a direction that
always flatters: monomorphised template code -- 90 instantiations of the same
BTree node logic over different key types, or the same quicksort over different
element types -- is exactly what brotli deduplicates best, so removing 21% of
the raw bytes removes rather less than 21% of the delivered bytes.

This measures it instead. wasm-ld's map gives each input object's OFFSET and
SIZE in the output file, so the family's real linked bytes can be sliced
straight out of the wasm and compressed on their own.

Two numbers come back and both matter:
  * the family compressed ALONE -- an upper bound on what removing it saves,
    since in-module it also shares a dictionary with everything else;
  * the whole module with those ranges REMOVED -- the honest saving, because it
    accounts for the dictionary the rest of the module loses.

Usage: map_slice_cost.py <.map> <server.log> <linked.wasm> <prefix> [-q N]
"""
import io
import os
import re
import subprocess
import sys
import tempfile

args = [a for a in sys.argv[1:] if not a.startswith('-')]
if len(args) < 4:
    raise SystemExit(__doc__)
MAP, LOG, WASM, PREFIX = args[0], args[1], args[2], args[3]
Q = str(int(sys.argv[sys.argv.index('-q') + 1])) if '-q' in sys.argv else '9'
BROTLI = os.environ.get('BROTLI', r'C:\msys64\mingw64\bin\brotli.exe')
if not os.path.exists(BROTLI):
    BROTLI = 'brotli'

LIFT = re.compile(r'transitive\[\d+\]: (?:lifting|cached) (.+?) addr=0x([0-9a-fA-F]+)')
EXPORT = re.compile(r'export_as=(\S+)')
name_of = {}
for ln in io.open(LOG, encoding='utf-8', errors='replace'):
    m = LIFT.search(ln)
    if m:
        e = EXPORT.search(ln)
        if e:
            name_of.setdefault(e.group(1), m.group(1))

ROW = re.compile(r'^\s*\S+\s+([0-9a-f]+)\s+([0-9a-f]+)\s+.*?([A-Za-z0-9_]+)\.o:\(')
hit, allr = [], []
for ln in io.open(MAP, encoding='utf-8', errors='replace'):
    m = ROW.match(ln)
    if not m:
        continue
    off, size, obj = int(m.group(1), 16), int(m.group(2), 16), m.group(3)
    allr.append((off, size))
    if name_of.get(obj, '').startswith(PREFIX):
        hit.append((off, size))

blob = io.open(WASM, 'rb').read()
hit.sort()
# Objects can repeat a range in the map; merge so bytes are not double counted.
merged = []
for off, size in hit:
    if merged and off <= merged[-1][1]:
        merged[-1][1] = max(merged[-1][1], off + size)
    else:
        merged.append([off, off + size])
fam_bytes = sum(b - a for a, b in merged)

print('%s' % WASM)
print('  module            : %d bytes' % len(blob))
print('  matched "%s": %d object range(s), %d bytes (%.2f%% of the module)'
      % (PREFIX, len(hit), fam_bytes, 100.0 * fam_bytes / max(len(blob), 1)))
if not merged:
    raise SystemExit('  nothing matched -- check the prefix, and that the map and log pair up')


def br(data, label):
    with tempfile.TemporaryDirectory() as d:
        p = os.path.join(d, 'x.bin')
        io.open(p, 'wb').write(data)
        subprocess.run([BROTLI, '-q', Q, '-w', '22', '-f', '-o', p + '.br', p],
                       check=True)
        n = os.path.getsize(p + '.br')
    print('  %-34s %10d raw -> %9d br(q%s)  [%.2fx]'
          % (label, len(data), n, Q, len(data) / max(n, 1)))
    return n


whole = br(blob, 'whole module')
fam = br(b''.join(blob[a:b] for a, b in merged), 'the family, compressed ALONE')
keep = bytearray(blob)
for a, b in reversed(merged):
    del keep[a:b]
rest = br(bytes(keep), 'module WITHOUT the family')

print('')
print('  removing it saves %d compressed bytes (%.1f%% of the module),' %
      (whole - rest, 100.0 * (whole - rest) / max(whole, 1)))
print('  against %.1f%% of the raw bytes.' % (100.0 * fam_bytes / max(len(blob), 1)))
if fam_bytes:
    print('  the family compresses %.2fx alone vs the module\'s %.2fx overall.'
          % (fam_bytes / max(fam, 1), len(blob) / max(whole, 1)))
print('')
print('  NOTE: cutting these ranges out makes an INVALID wasm. This measures')
print('  compressibility only -- it is not a link, and the real saving also')
print('  depends on what the callers then stop needing.')
