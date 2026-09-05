"""Name a synthetic address: what code is there, and was it lifted?

Every unmatched-dispatch / missing-block hunt starts with a bare synth address
and needs the same four answers, so this is the tool rather than a fresh script
each time:

  * which native address and RVA it maps to, with the mapping SELF-CHECKED
    against a function whose name the log already gives - synth is NOT the RVA,
    and assuming it is has produced confidently wrong conclusions before;
  * which PE section it lands in (.text is code, .rdata is data being branched
    to, and 16-byte alignment proves nothing - .rdata tables are aligned too);
  * which function CONTAINS it and at what offset. Dispatcher cases are keyed
    by function ENTRIES, so a mid-function target can never match no matter how
    good discovery is - that distinction decides the whole diagnosis;
  * whether that function was lifted in this run.

Usage:  python scripts/m9_e2e/name-synth.py 0x8ba1e6 [server.log]

The log must be from the SAME run as the address: synth addresses are assigned
per build, so naming run N's address with run N+1's log gives a wrong answer.
"""
import io
import os
import re
import struct
import subprocess
import sys

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
EXE_DEFAULT = os.path.join(REPO, 'target', 'x86_64-pc-windows-msvc', 'release',
                           'AzWriter.exe')
SYMBOLIZER = os.path.join(REPO, 'third_party', 'remill', 'dependencies', 'install',
                          'bin', 'llvm-symbolizer.exe')
TEXT_RVA = 0x1000

REBASE = re.compile(r'(\S+) (?:->|\u2192) synth_base=0x([0-9a-fA-F]+), '
                    r'native=\[0x([0-9a-fA-F]+)\.\.0x([0-9a-fA-F]+)\]')
LIFT = re.compile(r'lifting (.+?) addr=0x([0-9a-fA-F]+) size=(\d+) export_as=(\S+)')


def pe_info(path):
    raw = io.open(path, 'rb').read()
    pe = struct.unpack_from('<I', raw, 0x3c)[0]
    if raw[pe:pe + 4] != b'PE\0\0':
        raise SystemExit('not a PE: %s' % path)
    nsec, = struct.unpack_from('<H', raw, pe + 6)
    optsz, = struct.unpack_from('<H', raw, pe + 20)
    image_base, = struct.unpack_from('<Q', raw, pe + 24 + 24)
    secs = []
    off = pe + 24 + optsz
    for _ in range(nsec):
        name = raw[off:off + 8].rstrip(b'\0').decode('ascii', 'replace')
        vsize, vaddr = struct.unpack_from('<II', raw, off + 8)
        secs.append((name, vaddr, vsize))
        off += 40
    return image_base, secs


def section_of(secs, rva):
    for name, vaddr, vsize in secs:
        if vaddr <= rva < vaddr + vsize:
            return name
    return '<none>'


def symbolize(exe, va, inlines=False):
    if not os.path.exists(SYMBOLIZER):
        return []
    cmd = [SYMBOLIZER, '--obj=' + exe, '--demangle', '--functions=linkage']
    if inlines:
        cmd.append('--inlines')
    cmd.append('0x%x' % va)
    p = subprocess.run(cmd, capture_output=True, text=True, timeout=300)
    out = []
    for line in (p.stdout or '').splitlines():
        line = line.strip()
        if line and not line.startswith('C:\\') and '\\' not in line:
            out.append(line)
    return out


def main():
    if len(sys.argv) < 2:
        raise SystemExit(__doc__)
    target_synth = int(sys.argv[1], 16)
    log = sys.argv[2] if len(sys.argv) > 2 else r'C:\rb\azwriter_server.log'

    synth_base = native_base = None
    exe = EXE_DEFAULT
    lifted = {}
    for line in io.open(log, encoding='utf-8', errors='replace'):
        m = REBASE.search(line)
        if m and synth_base is None:
            exe = m.group(1)
            synth_base = int(m.group(2), 16)
            native_base = int(m.group(3), 16)
        if ': lifting ' in line:
            m = LIFT.search(line)
            if m:
                lifted.setdefault(int(m.group(2), 16), (m.group(1), int(m.group(3))))
    if synth_base is None:
        raise SystemExit('no rebase line in %s' % log)

    image_base, secs = pe_info(exe)
    native = native_base + (target_synth - synth_base)
    rva = native - native_base + TEXT_RVA
    va = image_base + rva

    print('image      : %s' % exe)
    print('  synth_base=0x%x native_base=0x%x ImageBase=0x%x'
          % (synth_base, native_base, image_base))
    print('')

    # Self-check the mapping on a function whose address the log already names.
    control = None
    for addr, (name, size) in lifted.items():
        if size > 256:
            control = (addr, name)
            break
    if control:
        c_rva = control[0] - native_base + TEXT_RVA
        got = symbolize(exe, image_base + c_rva)
        short = re.split(r'::h[0-9a-f]{16}', control[1])[0].split('<')[0]
        ok = any(part and part in (got[0] if got else '')
                 for part in [short.split('::')[-1]])
        print('mapping check: %s' % control[1][:60])
        print('   -> %s' % (got[0][:88] if got else '<no symbol>'))
        print('   => %s' % ('VERIFIED' if ok else
                            'UNVERIFIED - treat the result below with suspicion'))
        print('')

    print('target     : synth 0x%x -> native 0x%x -> rva 0x%x -> va 0x%x'
          % (target_synth, native, rva, va))
    print('  section  : %s' % section_of(secs, rva))
    for i, line in enumerate(symbolize(exe, va, inlines=True)):
        print('  %s: %s' % ('symbol ' if i == 0 else 'inlined', line[:96]))

    # Containing function: walk back to the lowest rva with the same outer name.
    outer = symbolize(exe, va, inlines=True)
    outer = outer[-1] if outer else None
    if outer:
        lo, hi = max(0, rva - 0x8000), rva
        while lo < hi:
            mid = (lo + hi) // 2
            got = symbolize(exe, image_base + mid, inlines=True)
            if got and got[-1] == outer:
                hi = mid
            else:
                lo = mid + 1
        entry_rva = lo
        off = rva - entry_rva
        entry_native = native_base + entry_rva - TEXT_RVA
        entry_synth = synth_base + (entry_native - native_base)
        print('')
        print('containing function:')
        print('  entry rva 0x%x  native 0x%x  synth 0x%x' % (entry_rva, entry_native, entry_synth))
        print('  target is +%d bytes into it' % off)
        was = lifted.get(entry_native)
        print('  lifted this run: %s' % ('YES  %s (size=%d)' % (was[0][:60], was[1])
                                         if was else 'NO'))
        print('')
        if off == 0:
            print('  => the target IS a function entry. It was simply never')
            print('     lifted, so no dispatcher case exists. A discovery bug.')
        else:
            print('  => the target is MID-FUNCTION. Dispatcher cases are keyed by')
            print('     ENTRIES, so no case can ever exist for it. This is NOT a')
            print('     discovery bug - nothing should be branching here at all.')
            print('     Disassemble the branching function and compare its bytes')
            print('     against its IR before blaming the walker.')


if __name__ == '__main__':
    main()
