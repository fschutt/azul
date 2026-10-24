#!/usr/bin/env python3
"""Resolve keep-both conflicts in append-only files (all.rs, Cargo.toml members, test-member lists,
CI steps, CHANGELOG, dependency justifications): ours, then theirs.

TRAP (2026-10-01/02): when both sides APPEND a function at the same place, the shared closing line
after the markers (`}`) belongs to BOTH functions - joining ours+theirs drops one `}`. For theme files
(flat.rs / flora.rs) use --join-functions: ours, then `}`, a blank line, then theirs."""
import re, sys
args = [a for a in sys.argv[1:] if not a.startswith('--')]
join = '--join-functions' in sys.argv
pat = re.compile(r'<<<<<<< [^\n]*\n(.*?)(?:\|\|\|\|\|\|\| [^\n]*\n.*?)?=======\n(.*?)>>>>>>> [^\n]*\n', re.S)
for p in args:
    s = open(p).read()
    if join:
        s2, n = pat.subn(lambda m: m.group(1).rstrip('\n') + '\n}\n\n' + m.group(2), s)
    else:
        s2, n = pat.subn(lambda m: m.group(1) + m.group(2), s)
    if '<<<<<<<' in s2:
        sys.exit(f"unresolved markers left in {p}")
    open(p, 'w').write(s2)
    print(f"{p}: kept both sides of {n} hunk(s)" + (" (functions joined)" if join else ""))
