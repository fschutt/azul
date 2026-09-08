"""List a wasm module's exports and imports.

The lift log says the mini was linked with "4528 exports", but its export
SECTION is 1121 bytes, which cannot hold 4528 names. One of those two numbers
describes something other than what shipped, and which one decides whether
--gc-sections is doing anything: a module that exports every lifted body cannot
be stripped, and that would be a lever; a module that exports only its entry
points means the 28 MB is genuinely reachable code and the lever is elsewhere.
"""
import io
import sys

path = sys.argv[1] if len(sys.argv) > 1 else r'C:\rb\mini_verify.wasm'
b = io.open(path, 'rb').read()


def uleb(i):
    v = s = 0
    while True:
        x = b[i]
        i += 1
        v |= (x & 0x7F) << s
        if not (x & 0x80):
            return v, i
        s += 7


KIND = {0: 'func', 1: 'table', 2: 'mem', 3: 'global'}
i = 8
exports = []
imports = []
while i < len(b):
    sid = b[i]
    i += 1
    size, i = uleb(i)
    end = i + size
    if sid == 7:
        n, j = uleb(i)
        for _ in range(n):
            ln, j = uleb(j)
            name = b[j:j + ln].decode('utf-8', 'replace')
            j += ln
            k = b[j]
            j += 1
            idx, j = uleb(j)
            exports.append((name, KIND.get(k, k), idx))
    elif sid == 2:
        n, j = uleb(i)
        for _ in range(n):
            ln, j = uleb(j)
            mod = b[j:j + ln].decode('utf-8', 'replace')
            j += ln
            ln, j = uleb(j)
            nm = b[j:j + ln].decode('utf-8', 'replace')
            j += ln
            k = b[j]
            j += 1
            if k == 0:
                _t, j = uleb(j)
            elif k == 1:
                j += 1
                lim = b[j]
                j += 1
                _m, j = uleb(j)
                if lim:
                    _M, j = uleb(j)
            elif k == 2:
                lim = b[j]
                j += 1
                _m, j = uleb(j)
                if lim:
                    _M, j = uleb(j)
            elif k == 3:
                j += 2
            imports.append((mod, nm, KIND.get(k, k)))
    i = end

print('%s' % path)
print('exports: %d   imports: %d' % (len(exports), len(imports)))
print('')
print('exports:')
for name, k, idx in exports:
    print('   %-6s %-6s %s' % (k, idx, name[:90]))
print('')
print('imports:')
for mod, nm, k in imports[:30]:
    print('   %-6s %s.%s' % (k, mod, nm[:70]))
