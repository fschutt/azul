"""List a wasm module's imports.

The lift audit's `import_is_provided` is one global list, so it answers "is this
name provided somewhere" rather than "by the module that imports it" - which is
how a whole module missing the whole math table stayed invisible. Comparing what
each module actually imports against what its own JS builder provides is the
check that has the right granularity.

Usage: wasm-imports.py <file.wasm> [<file.wasm> ...]
"""
import io
import sys


def leb(b, p):
    r, s = 0, 0
    while True:
        x = b[p]
        p += 1
        r |= (x & 0x7F) << s
        if not (x & 0x80):
            return r, p
        s += 7


def imports(path):
    b = io.open(path, 'rb').read()
    if b[:4] != b'\0asm':
        raise SystemExit('%s: not a wasm module' % path)
    p = 8
    out = []
    while p < len(b):
        sid = b[p]
        p += 1
        size, p = leb(b, p)
        end = p + size
        if sid == 2:
            n, q = leb(b, p)
            for _ in range(n):
                ml, q = leb(b, q)
                mod = b[q:q + ml].decode('utf-8', 'replace')
                q += ml
                nl, q = leb(b, q)
                nm = b[q:q + nl].decode('utf-8', 'replace')
                q += nl
                kind = b[q]
                q += 1
                if kind == 0x00:
                    _t, q = leb(b, q)
                elif kind == 0x01:
                    q += 1
                    lim = b[q]
                    q += 1
                    _mn, q = leb(b, q)
                    if lim:
                        _mx, q = leb(b, q)
                elif kind == 0x02:
                    lim = b[q]
                    q += 1
                    _mn, q = leb(b, q)
                    if lim:
                        _mx, q = leb(b, q)
                elif kind == 0x03:
                    q += 2
                out.append((mod, nm, kind))
        p = end
    return out


KIND = {0: 'func', 1: 'table', 2: 'mem', 3: 'global'}

for path in sys.argv[1:]:
    rows = imports(path)
    print('=== %s: %d import(s) ===' % (path, len(rows)))
    # sub_<hex> imports are the boundary-shard mechanism and there can be
    # thousands; collapse them so the hand-written names stay readable.
    subs = [r for r in rows if r[1].startswith('sub_')]
    rest = [r for r in rows if not r[1].startswith('sub_')]
    for mod, nm, k in sorted(rest, key=lambda r: (r[0], r[1])):
        print('  %-6s %s.%s' % (KIND.get(k, '?'), mod, nm))
    if subs:
        print('  ... plus %d sub_<hex> boundary import(s)' % len(subs))
    print('')
