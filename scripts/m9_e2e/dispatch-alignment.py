#!/usr/bin/env python3
"""Are unmatched dispatch targets plausibly FUNCTION ENTRIES at all?

CHECK THE SECTION FIRST - it is the real discriminator, and alignment only a
hint. A target in .rdata is not a function at any alignment, and `.rdata` tables
are 16-byte aligned exactly like code, so alignment alone says nothing. Pass the
binary with --exe to get the section verdict:

    dispatch-alignment.py <server.log> <hex>... --exe <path-to-exe>

This mattered: a target that alignment called "a plausible function entry" (and
sent me hunting discovery for three runs) turned out to sit in .rdata, making it
a data address called as a function pointer - a mirroring/translation bug, not a
missing function.


Decides between two failures that look identical in the log but need opposite
fixes:

  aligned target   -> a real function entry with no dispatcher case. Hunt
                      discovery: what stopped it being lifted or cased.
  unaligned target -> not an entry at all, so it is a COMPUTED address. Hunt
                      whatever computed it - a PC-relative base, a jump table.

Measured on a real lift, 99.1%% of function entries are 16-byte aligned and only
0.62%% are not 4-byte aligned, so an unaligned target is conclusive.

This separated what looked like one recurring boot failure into two distinct
bugs: an aligned single target (missing function) and, in a later run, unaligned
targets (bad address computation).

Usage: dispatch-alignment.py <server.log> <hex-target>...
"""
import io
import re
import sys
import collections

import struct
import os

_argv = [a for a in sys.argv[1:] if a != '--exe']
_exe = None
if '--exe' in sys.argv:
    i = sys.argv.index('--exe')
    if i + 1 < len(sys.argv):
        _exe = sys.argv[i + 1]
        _argv = [a for a in _argv if a != _exe]


def _sections(path):
    b = io.open(path, 'rb').read(0x2000)
    pe = struct.unpack_from('<I', b, 0x3c)[0]
    nsec = struct.unpack_from('<H', b, pe + 6)[0]
    opt = struct.unpack_from('<H', b, pe + 20)[0]
    off = pe + 24 + opt
    out = []
    for i in range(nsec):
        o = off + i * 40
        name = b[o:o + 8].rstrip(b'\0').decode('ascii', 'replace')
        vs = struct.unpack_from('<I', b, o + 8)[0]
        va = struct.unpack_from('<I', b, o + 12)[0]
        ch = struct.unpack_from('<I', b, o + 36)[0]
        out.append((name, va, vs, ch))
    return out


def _section_verdict(rva, secs):
    for name, va, vs, ch in secs:
        if va <= rva < va + vs:
            code = bool(ch & 0x20000000) or bool(ch & 0x20)
            return name, code
    return None, False


log = sys.argv[1]
targets = [int(a, 16) for a in _argv[1:]]

RES = re.compile(r'sub_([0-9a-f]+)\s*(?:→|->)\s*resolved=(.+?)@0x([0-9a-fA-F]+)\s+class=(\S+)')

synths = set()
for line in io.open(log, encoding='utf-8', errors='replace'):
    m = RES.search(line)
    if m:
        try:
            synths.add(int(m.group(1), 16))
        except ValueError:
            pass

print('known function-entry synths: %d' % len(synths))
if not synths:
    raise SystemExit('no data')

dist = collections.Counter()
for s in synths:
    for a in (16, 8, 4, 2):
        if s % a == 0:
            dist[a] += 1
            break
    else:
        dist[1] += 1
tot = len(synths)
print('')
print('alignment of known function entries:')
for a in (16, 8, 4, 2, 1):
    if dist.get(a):
        print('   %2d-byte aligned : %6d  %5.1f%%' % (a, dist[a], 100.0 * dist[a] / tot))
odd = dist.get(1, 0) + dist.get(2, 0)
print('')
print('   entries that are NOT 4-byte aligned: %d (%.2f%%)' % (odd, 100.0 * odd / tot))
print('')
if _exe and os.path.exists(_exe):
    _secs = _sections(_exe)
    print('')
    print('SECTION CHECK (decisive - a target in a data section is not a function):')
    for t_ in targets:
        nm, code = _section_verdict(t_, _secs)
        if nm is None:
            print('   0x%08x  not in any section' % t_)
        elif code:
            print('   0x%08x  %-10s CODE' % (t_, nm))
        else:
            print('   0x%08x  %-10s DATA  <== NOT A FUNCTION. The bug is whatever'
                  % (t_, nm))
            print('                            computed or loaded this value.')
    print('')

print('unmatched targets:')
for t in targets:
    al = 1
    for a in (16, 8, 4, 2):
        if t % a == 0:
            al = a
            break
    print('   0x%08x  %2d-byte aligned%s' % (t, al, '   <== NOT a plausible entry' if al < 4 else ''))
print('')
bad = [t for t in targets if t % 4 != 0]
if bad and odd * 1.0 / tot < 0.01:
    print('VERDICT: %d of %d targets are not 4-byte aligned, while only %.2f%% of'
          % (len(bad), len(targets), 100.0 * odd / tot))
    print('real function entries are. These are COMPUTED ADDRESSES, not missing')
    print('functions - look at what computed them, not at discovery.')
elif not bad:
    print('VERDICT: all targets are aligned like real entries - consistent with')
    print('genuinely missing functions rather than a bad address computation.')
else:
    print('VERDICT: inconclusive - unaligned entries are not rare enough here.')
