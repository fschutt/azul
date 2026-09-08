"""How much of the lifted code is state-struct traffic, and how much could go?

`%state` arrives as a POINTER PARAMETER, so mem2reg/SROA cannot promote it and
every guest register access stays a real load/store. `privatize_flag_storage`
already proves the fix works for one slice of the struct - moving the flag GEPs
to a function-local alloca made them promotable and cut 43%.

The same trick can extend to the general registers, but only if calls are rare:
at every call the callee expects the state coherent in memory, so a privatized
register file must be written back before the call and reloaded after. The trade
is therefore (state loads+stores eliminated) against (2 x registers x calls)
spill/reload pairs.

This measures both sides over a sample of real lifted functions:
  * what fraction of instructions is state traffic (the prize), and
  * how many calls per function (the cost).
"""
import collections
import glob
import io
import os
import random
import re
import sys

SCRATCH = sys.argv[1] if len(sys.argv) > 1 else \
    r'C:\Users\felix\AppData\Local\Temp\azul-web-transpiler-6448'
N = int(sys.argv[2]) if len(sys.argv) > 2 else 400

files = glob.glob(os.path.join(SCRATCH, '__az_dep_*.opt.ll'))
random.seed(5)
random.shuffle(files)

INSN = re.compile(r'^\s+(?:%[\w.]+\s*=\s*)?([a-z][a-z0-9.]*)\s')
tot_insn = tot_state_ld = tot_state_st = tot_gep = tot_call = 0
tot_all_ld = tot_all_st = 0
nfiles = 0
per_fn_calls = []
big = []

for f in files[:N]:
    try:
        if os.path.getsize(f) > 3_000_000:
            continue
        txt = io.open(f, encoding='utf-8', errors='replace').read()
    except OSError:
        continue
    nfiles += 1
    # names of GEPs rooted at %state - loads/stores through them are state traffic
    state_ptrs = set(re.findall(r'(%[\w.]+)\s*=\s*getelementptr[^\n]*ptr %state', txt))
    ins = st_ld = st_st = gep = call = ald = ast = 0
    for line in txt.splitlines():
        m = INSN.match(line)
        if not m:
            continue
        op = m.group(1)
        if op in ('define', 'declare', 'attributes', 'target', 'source'):
            continue
        ins += 1
        if op == 'getelementptr':
            gep += 1
        elif op == 'load':
            ald += 1
            if any(p in line for p in state_ptrs) or 'ptr %state' in line:
                st_ld += 1
        elif op == 'store':
            ast += 1
            if any(p in line for p in state_ptrs) or 'ptr %state' in line:
                st_st += 1
        elif op in ('call', 'tail'):
            call += 1
    tot_insn += ins
    tot_state_ld += st_ld
    tot_state_st += st_st
    tot_gep += gep
    tot_call += call
    tot_all_ld += ald
    tot_all_st += ast
    per_fn_calls.append(call)
    if ins > 200:
        big.append((ins, st_ld + st_st, call, os.path.basename(f)))

print('sampled %d lifted functions, %d instructions' % (nfiles, tot_insn))
print('')
print('%-34s %10s %7s' % ('category', 'count', '% insn'))


def row(label, n):
    print('%-34s %10d %6.1f%%' % (label, n, 100.0 * n / max(tot_insn, 1)))


row('loads (all)', tot_all_ld)
row('  of which through %state', tot_state_ld)
row('stores (all)', tot_all_st)
row('  of which through %state', tot_state_st)
row('getelementptr', tot_gep)
row('calls', tot_call)
print('')
state_traffic = tot_state_ld + tot_state_st + tot_gep
print('state traffic (state ld+st+gep): %d = %.1f%% of all instructions'
      % (state_traffic, 100.0 * state_traffic / max(tot_insn, 1)))
per = sorted(per_fn_calls)
if per:
    med = per[len(per) // 2]
    print('calls per function: median %d, mean %.1f, p90 %d'
          % (med, tot_call / max(nfiles, 1), per[int(len(per) * 0.9)]))
print('')
print('THE TRADE: privatizing the register file removes state ld/st but adds a')
print('write-back + reload around every call. With ~%.1f calls per function and'
      % (tot_call / max(nfiles, 1)))
print('%.1f%% of instructions being state traffic, the prize is large and the'
      % (100.0 * state_traffic / max(tot_insn, 1)))
print('cost is bounded by the call count - the same shape that made flag')
print('privatization win 43%.')
print('')
print('largest sampled functions (insn, state ld/st, calls):')
for ins, stx, call, name in sorted(big, reverse=True)[:8]:
    print('   %6d insn  %5d state ld/st  %4d calls  %s' % (ins, stx, call, name))
