"""Decode every NeverLift synth address to its symbol, from a run's own log.

Recorder 0x40048 holds the synth of whichever NeverLift stub was reached. The
enumeration showed all 29 of them are panic/abort entry points - unwrap_failed,
panic_bounds_check, handle_alloc_error, and so on - so 0x40048 is effectively a
PANIC REASON register, not a list of pending blockers. Decoding it turns a boot
trap into a one-step diagnosis.

Reads names out of the log's own `dep: sub_<synth> -> resolved=<name>@<native>`
lines rather than name-synth.py, because name-synth resolves through the exe on
disk, and the exe is rebuilt between runs while the log is not.
"""
import io
import re
import sys

LOG = sys.argv[1] if len(sys.argv) > 1 else r'C:\rb\azwriter_server.20260905-163339.log'

# synth -> caller count, from neverlift_reach.py over the matching scratch.
CALLERS = {
    'f05b70': 1815, 'f06250': 1758, 'f05a0e': 1365, 'f05e50': 1023,
    'f06210': 705, 'f05b90': 654, 'f05e60': 429, 'f061f0': 363,
    'f05e40': 309, 'ea1080': 207, 'f06150': 180, 'f049c0': 69,
    'f05b50': 60, 'f05d80': 36, 'f062ec': 18, 'f05dc0': 12,
    'f05d00': 6, '87fc10': 3, 'bface0': 3, 'bfacc0': 3, 'aec100': 3,
    'efffc7': 3, 'ceb130': 3, 'cebf10': 3, '6db9f0': 3, 'd50fd0': 3,
    'f062b1': 3, 'bfac40': 3, 'f058c0': 3,
}

pat = re.compile(r'dep: sub_([0-9a-f]+) \S+ resolved=([^@\s]+)@')
names = {}
with io.open(LOG, encoding='utf-8', errors='replace') as fh:
    for line in fh:
        if 'resolved=' not in line:
            continue
        m = pat.search(line)
        if m and m.group(1) in CALLERS:
            names.setdefault(m.group(1), m.group(2))

print('| synth | callers | symbol |')
print('|---|---|---|')
for s, n in sorted(CALLERS.items(), key=lambda kv: -kv[1]):
    print('| `0x%s` | %d | `%s` |' % (s, n, names.get(s, '(not named in this log)')))

missing = [s for s in CALLERS if s not in names]
if missing:
    print('')
    print('not named: %s' % ' '.join(missing))
