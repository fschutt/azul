"""Resolve __tls_index's synth address from the lifted IR itself.

Independent of the rva+delta calibration, which has now been wrong once. A
lifted body seeds its whole PC chain from its `pc` argument, so NEXT_PC at any
point is `lift_addr + (sum of the increments stored so far)`. The rip-relative
read of __tls_index appears as

    %N = load i64, ptr %NEXT_PC
    %M = add i64 %N, <large>
    call i32 @__remill_read_memory_32(..., i64 %M)

so the target is lift_addr + cumulative + large. Two independent derivations
agreeing is the check; one alone is not.
"""
import glob
import io
import os
import re
import sys

path = sys.argv[1] if len(sys.argv) > 1 else None
if not path:
    cands = glob.glob(os.path.join(
        os.environ.get('TEMP', r'C:\Users\felix\AppData\Local\Temp'),
        'azul-web-transpiler-*'))
    cands.sort(key=os.path.getmtime, reverse=True)
    hits = glob.glob(os.path.join(cands[0], 'azul_core__task__get_system_time_libstd_*.lifted.ll'))
    path = hits[0]
print('file: %s' % os.path.basename(path))

txt = io.open(path, encoding='utf-8', errors='replace').read()

m = re.search(r'^define ptr @sub_([0-9a-f]+)\(', txt, re.M)
lift = int(m.group(1), 16)
print('lift addr (synth): 0x%x' % lift)

# Track NEXT_PC as lift + cumulative. Each block does
#   %a = load i64, ptr %NEXT_PC ; store i64 %a, ptr %PC ; %b = add i64 %a, K
#   store i64 %b, ptr %NEXT_PC
# so the cumulative advances by K each time a value is STORED to NEXT_PC.
re_add = re.compile(r'^\s*%(\d+) = add i64 %(\d+), (\d+)\s*$')
re_store_npc = re.compile(r'^\s*store i64 %(\d+), ptr %NEXT_PC')
re_read32 = re.compile(r'read_memory_32\(ptr noundef %\d+, i64 noundef %(\d+)\)')

adds = {}          # ssa -> (src_ssa, K)
cum = 0
val = {}           # ssa -> resolved absolute value, when derivable
found = []

for line in txt.split('\n'):
    m = re_add.match(line)
    if m:
        dst, src, k = m.group(1), m.group(2), int(m.group(3))
        adds[dst] = (src, k)
        # If src is the current NEXT_PC value, dst = lift + cum + k
        val[dst] = lift + cum + k
        continue
    m = re_store_npc.match(line)
    if m:
        s = m.group(1)
        if s in adds:
            cum += adds[s][1]
        continue
    m = re_read32.search(line)
    if m:
        a = m.group(1)
        if a in val:
            found.append((val[a], adds[a][1], cum))

print('')
print('%-20s %-14s %s' % ('resolved target', 'displacement', 'cum at that point'))
for target, disp, c in found:
    tag = ''
    if disp > 0x10000:
        tag = '   <- rip-relative data read (large displacement)'
    print('0x%-18x %-14d %d%s' % (target, disp, c, tag))

big = [t for t, d, _ in found if d > 0x10000]
if big:
    print('')
    print('__tls_index synth (from IR)  = 0x%x' % big[0])
