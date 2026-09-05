"""Section sizes of a wasm module - how much is CODE and how much is DATA.

This decides what chunking is worth. Lazy chunks share the core's linear
memory, and every instantiate replays that module's data segments over it, so a
chunk loaded after boot would clobber the live heap. The only safe arrangement
is: lazy chunks carry no data segments, and the eager core mirrors the union of
what every chunk reads. Data therefore stays eager no matter how the code is
split, and the achievable saving is bounded by the CODE section, not by the
whole file.

wasm is trivially parseable: magic + version, then (id: u8, size: uleb128,
payload) sections. Section 10 is Code, 11 is Data.
"""
import io
import os
import sys

NAMES = {
    0: 'custom', 1: 'type', 2: 'import', 3: 'function', 4: 'table',
    5: 'memory', 6: 'global', 7: 'export', 8: 'start', 9: 'element',
    10: 'CODE', 11: 'DATA', 12: 'datacount',
}


def uleb(b, i):
    v = 0
    s = 0
    while True:
        x = b[i]
        i += 1
        v |= (x & 0x7F) << s
        if not (x & 0x80):
            return v, i
        s += 7


def main(path):
    b = io.open(path, 'rb').read()
    if b[:4] != b'\0asm':
        raise SystemExit('not a wasm module: %s' % path)
    total = len(b)
    print('%s' % path)
    print('  total: %d bytes (%.2f MB)' % (total, total / 1e6))
    i = 8
    rows = []
    while i < total:
        sid = b[i]
        i += 1
        size, i = uleb(b, i)
        name = NAMES.get(sid, 'id%d' % sid)
        if sid == 0:
            # custom section: read its name for the report
            n, j = uleb(b, i)
            try:
                name = 'custom:' + b[j:j + n].decode('utf-8', 'replace')
            except Exception:
                pass
        rows.append((name, size))
        i += size
    rows.sort(key=lambda r: -r[1])
    print('  %-24s %12s %8s' % ('section', 'bytes', '% file'))
    for name, size in rows:
        print('  %-24s %12d %7.1f%%' % (name, size, 100.0 * size / total))
    code = sum(s for n, s in rows if n == 'CODE')
    data = sum(s for n, s in rows if n == 'DATA')
    print('')
    print('  CODE %.2f MB (%.1f%%)   DATA %.2f MB (%.1f%%)'
          % (code / 1e6, 100.0 * code / total, data / 1e6, 100.0 * data / total))
    print('')
    print('  Chunking moves CODE only - DATA has to stay in the eager core,')
    print('  because a chunk instantiated after boot replays its data segments')
    print('  over the shared live memory. So the ceiling on a first-paint')
    print('  saving is the CODE fraction times the share that is exclusively')
    print('  owned by a lazy root.')


if __name__ == '__main__':
    for p in sys.argv[1:] or [r'C:\rb\mini_verify.wasm']:
        main(p)
