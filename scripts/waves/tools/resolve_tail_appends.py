#!/usr/bin/env python3
"""Resolve a merge where BOTH sides appended code at the END of a file (the theme files flat.rs / flora.rs).

git's diff3 aligns the two appended blocks on whatever lines they share (a struct literal's closing
`marker: None, }`, a blank line, `}`), so the conflict comes out as several hunks with shared lines between
them - and keep_both.py would interleave the two blocks. Here everything from the first conflict marker to the
end of the file is appended text: ours = every ours-side line plus every shared line, in order; theirs = every
theirs-side line plus every shared line, in order; the result is the merged prefix, then ours, a blank line, then
theirs. (2026-10-03, the WIDGETS9B merge into flat.rs.)

    python3 scripts/waves/tools/resolve_tail_appends.py layout/src/widgets/themes/flat.rs ...
"""
import sys


def resolve(text):
    lines = text.splitlines(keepends=True)
    try:
        first = next(i for i, l in enumerate(lines) if l.startswith('<<<<<<< '))
    except StopIteration:
        return text, 0
    prefix = lines[:first]
    ours, theirs = [], []
    state = 'shared'
    hunks = 0
    for l in lines[first:]:
        if l.startswith('<<<<<<< '):
            state = 'ours'
            hunks += 1
            continue
        if l.startswith('||||||| '):
            state = 'base'
            continue
        if l.rstrip('\n') == '=======' and state in ('ours', 'base'):
            state = 'theirs'
            continue
        if l.startswith('>>>>>>> '):
            state = 'shared'
            continue
        if state == 'ours':
            ours.append(l)
        elif state == 'theirs':
            theirs.append(l)
        elif state == 'shared':
            ours.append(l)
            theirs.append(l)
    ours_text = ''.join(ours).rstrip('\n') + '\n'
    theirs_text = ''.join(theirs).lstrip('\n')
    return ''.join(prefix) + ours_text + '\n' + theirs_text, hunks


for path in sys.argv[1:]:
    with open(path) as f:
        text = f.read()
    out, n = resolve(text)
    if '<<<<<<< ' in out or '\n>>>>>>> ' in out:
        sys.exit(f'{path}: markers left')
    with open(path, 'w') as f:
        f.write(out)
    print(f'{path}: {n} tail hunk(s) resolved as ours-then-theirs')
