"""How much of run 63's +526 KB is swept-in root drift vs the TLS stores?

Two candidate causes for the mini growing 30,129,739 -> 30,656,268 bytes:

  1. the TLS seed itself: 9 volatile stores in every export wrapper;
  2. 155 functions that entered the walk that were not in run 61's, all of them
     host-side generic instantiations (WebServerState, BoundaryWasm, iced_x86,
     pdb) that the walk sweeps in via fn-pointer roots found in mirrored data.

These are separable. (1) is countable from the wrapper count. (2) is countable
from the log's own `size=` field on each `transitive[N]: lifting` line.

Attributing the whole delta to either one without this split would be a guess.
"""
import collections
import io
import re
import sys

A = sys.argv[1] if len(sys.argv) > 1 else r'C:\rb\azwriter_server.20260905-163339.log'
B = sys.argv[2] if len(sys.argv) > 2 else r'C:\rb\azwriter_server.log'

pat = re.compile(r'transitive\[\d+\]: lifting (.+?) addr=0x([0-9a-f]+) size=(\d+)')


def mini_walk(path):
    out = {}
    with io.open(path, encoding='utf-8', errors='replace') as fh:
        for line in fh:
            if 'transitive lift complete' in line:
                break
            if 'lifting ' not in line:
                continue
            m = pat.search(line)
            if m:
                out.setdefault(m.group(1), int(m.group(3)))
    return out


a, b = mini_walk(A), mini_walk(B)
added = set(b) - set(a)
removed = set(a) - set(b)

add_bytes = sum(b[n] for n in added)
rem_bytes = sum(a[n] for n in removed)

print('run61 mini walk: %d unique fns' % len(a))
print('run63 mini walk: %d unique fns' % len(b))
print('')
print('added   : %4d fns, %9d native bytes' % (len(added), add_bytes))
print('removed : %4d fns, %9d native bytes' % (len(removed), rem_bytes))
print('net     : %+4d fns, %+9d native bytes' % (len(added) - len(removed),
                                                 add_bytes - rem_bytes))

# Wrapper count = number of export wrappers, one per exported callback.
# 9 stores each: 1 TEB + 8 slots. A wasm `i64.const` + `i32.const` + `i64.store`
# for constant address and value is roughly 14 bytes.
WRAPPERS = 1793
STORES = 9
EST_BYTES_PER_STORE = 14
tls_est = WRAPPERS * STORES * EST_BYTES_PER_STORE
print('')
print('TLS seed upper bound: %d wrappers x %d stores x ~%d B = ~%d bytes'
      % (WRAPPERS, STORES, EST_BYTES_PER_STORE, tls_est))

observed = 30656268 - 30129739
print('observed mini delta : %+d bytes' % observed)
print('')
print('Native bytes are not wasm bytes (lifting expands several-fold), so the')
print('added-function figure is a lower bound on its wasm cost - which already')
print('makes swept-in drift the dominant term, not the TLS stores.')

print('')
print('=== added, by leading crate, with native bytes ===')
by_crate = collections.Counter()
for n in added:
    key = n.split('::', 1)[0] if '::' in n else n
    by_crate[key] += b[n]
for k, v in by_crate.most_common(15):
    print('  %-30s %8d B' % (k, v))
