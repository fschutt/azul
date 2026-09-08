"""Name the frames in a browser wasm stack trace.

A boot failure prints something like

    bootstrap FAILED: RuntimeError: unreachable
        at .../az/mini.<hash>.wasm:wasm-function[4949]:0x1bf784f
        at .../az/mini.<hash>.wasm:wasm-function[2434]:0xb4a5b9
        ...

which is ten frames of exactly the information needed to see WHAT reached the
trap - and it has never been used, because `--strip-all` removes the name
section and the indices mean nothing on their own.

They can be recovered without changing the artifact. The module still carries
its export table, and the exports are anchors: `AzStartup_*` and
`__az_indirect_dispatch` have known indices. Everything else is a lifted body,
and the bodies go into the code section in link order, which is the walk order
the lift log prints as `transitive[N]: lifting <name>`.

So: parse the code section for the defined-function index range, subtract the
import count, and line the remaining indices up against the walk order, using the
known exports to check the alignment rather than assume it. The script prints the
residual it measured; a mapping that does not line up says so instead of
inventing names.

Usage:
  wasm-frame-names.py <mini.wasm> <lift.log> [<index> ...]
With no indices it reads them from stdin (paste the trace).
"""
import io
import re
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


def parse(path):
    b = io.open(path, 'rb').read()
    if b[:4] != b'\0asm':
        raise SystemExit('%s: not a wasm module' % path)
    p, imports, exports, nfuncs = 8, 0, {}, 0
    bodies_len = []
    while p < len(b):
        sid = b[p]
        p += 1
        size, p = leb(b, p)
        end = p + size
        if sid == 2:
            n, q = leb(b, p)
            for _ in range(n):
                ml, q = leb(b, q)
                q += ml
                nl, q = leb(b, q)
                q += nl
                k = b[q]
                q += 1
                if k == 0x00:
                    _t, q = leb(b, q)
                    imports += 1
                elif k in (0x01, 0x02):
                    if k == 0x01:
                        q += 1
                    lim = b[q]
                    q += 1
                    _mn, q = leb(b, q)
                    if lim:
                        _mx, q = leb(b, q)
                else:
                    q += 2
        elif sid == 3:
            nfuncs, _q = leb(b, p)
        elif sid == 10:
            n, q = leb(b, p)
            for _ in range(n):
                blen, q = leb(b, q)
                bodies_len.append(blen)
                q += blen
        elif sid == 7:
            n, q = leb(b, p)
            for _ in range(n):
                nl, q = leb(b, q)
                nm = b[q:q + nl].decode('utf-8', 'replace')
                q += nl
                k = b[q]
                q += 1
                idx, q = leb(b, q)
                if k == 0x00:
                    exports[idx] = nm
        p = end
    return imports, nfuncs, exports, bodies_len


def walk_order(log):
    """`transitive[N]: lifting <name> addr=` — the link order of the bodies."""
    pat = re.compile(r'transitive\[(\d+)\]: lifting (.+?) addr=\S+ size=(\d+)')
    out, sizes = {}, {}
    for line in io.open(log, encoding='utf-8', errors='replace'):
        m = pat.search(line)
        if m:
            n = int(m.group(1))
            # 24 walks share one log; the mini is the FIRST, so keep the first
            # value seen for each index and stop at the first wrap-around.
            if n in out:
                break
            out[n] = m.group(2)
            sizes[n] = int(m.group(3))
    return out, sizes


wasm, log = sys.argv[1], sys.argv[2]
frames = [int(x) for x in sys.argv[3:] if not x.startswith('--')]
if not frames:
    txt = sys.stdin.read()
    frames = [int(m) for m in re.findall(r'wasm-function\[(\d+)\]', txt)]

imports, nfuncs, exports, bodies_len = parse(wasm)
order, wsizes = walk_order(log)
print('%s: %d imported func(s), %d defined, %d export(s)'
      % (wasm, imports, nfuncs, len(exports)))
print('lift log: %d walk entries' % len(order))

# The exports are NOT a prefix - the highest AzStartup_* wrapper sits directly
# below the dispatcher - so the mapping is built by REMOVING them, not by
# offsetting past them. The arithmetic is its own check:
#
#     defined - exports == walk entries
#
# and if that identity does not hold exactly, the names would be shifted, so the
# tool refuses rather than sliding them.
bodies = [i for i in range(imports, imports + nfuncs) if i not in exports]
print('defined %d - exports %d = %d bodies vs %d walk entries (residual %d)'
      % (nfuncs, len(exports), len(bodies), len(order), len(bodies) - len(order)))
trust = bool(order) and len(bodies) == len(order)
if not trust:
    print('  ** identity does not hold - every name below would be shifted, so')
    print('  ** they are withheld. Use AZ_WASM_DEBUG=1 for a named build.')
pos = {idx: k for k, idx in enumerate(bodies)}

rho = None
if trust and bodies_len:
    # Spearman over (native size, wasm body size). A correct mapping pairs each
    # function with itself and correlates strongly; a shifted or permuted one
    # pairs unrelated functions and correlates around zero.
    pairs = []
    for idx, k in pos.items():
        j = idx - imports
        if k in wsizes and 0 <= j < len(bodies_len):
            pairs.append((wsizes[k], bodies_len[j]))
    if len(pairs) > 100:
        def ranks(v):
            s = sorted(range(len(v)), key=lambda i: v[i])
            r = [0] * len(v)
            for pos_, i in enumerate(s):
                r[i] = pos_
            return r
        a = ranks([p[0] for p in pairs])
        b = ranks([p[1] for p in pairs])
        n = len(a)
        d2 = sum((a[i] - b[i]) ** 2 for i in range(n))
        rho = 1.0 - (6.0 * d2) / (n * (n * n - 1))
        print('VERIFY: Spearman(native size, wasm body size) = %.3f over %d pairs'
              % (rho, n))
        print('        > 0.5 means the mapping pairs each function with itself;')
        print('        near 0 means it is shifted or permuted and the names are wrong.')
        if rho <= 0.5:
            print('        ** BELOW THRESHOLD - link order is only ROUGHLY walk order,')
            print('        ** so names would drift. Withheld. Use AZ_WASM_DEBUG=1, which')
            print('        ** skips --strip-all and keeps the name section.')
            trust = False
        big = sorted(pos.items(), key=lambda kv: -wsizes.get(kv[1], 0))[:5]
        for idx, k in big:
            j = idx - imports
            print('        native %7d B  wasm %7d B  %s'
                  % (wsizes.get(k, -1),
                     bodies_len[j] if 0 <= j < len(bodies_len) else -1,
                     order.get(k, '?')[:70]))
print('')

for f in frames:
    if f in exports:
        print('  [%5d]  %s   (export)' % (f, exports[f]))
    elif f < imports:
        print('  [%5d]  <imported function>' % f)
    elif not trust:
        print('  [%5d]  (withheld - mapping unverified)' % f)
    else:
        k = pos.get(f)
        nm = order.get(k) if k is not None else None
        print('  [%5d]  %s' % (f, nm if nm else '(walk index %s, not in log)' % k))
