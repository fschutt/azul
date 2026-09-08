#!/usr/bin/env python3
"""Audit a lift for payload a browser build should never receive.

Reports, per crate, how much non-web code a run actually carried: an embedded
SQL database, a TLS stack, a GPU renderer, gamepad input - and the lifter own
dependencies, since the lift target is the full desktop DLL.

MATCH ON THE LEADING CRATE NAME, NEVER A SUBSTRING. An unanchored substring is
not a crate test and this bit twice: `ring::` matches `alloc::string::` (the
tail of "string::"), which booked 86 functions of ordinary Rust string handling
as a TLS stack and reported 2.85 MB of savings that did not exist. The earlier
sibling of this bug matched `Display` inside `DisplayList`. crate_of() below
extracts the leading crate the same way the classifier does.

Pairs with is_browser_excluded_crate in dll/src/web/symbol_table.rs - this tells
you what is still getting through.

Usage: wasm-payload-audit.py <server.log> <scratch-dir>
"""
import io
import os
import re
import sys
import collections

LIFT = re.compile(r'lifting (.+?) addr=0x([0-9a-fA-F]+) size=(\d+) export_as=(\S+)')

# crate name (exact, leading) -> why it should not reach a browser
EXCLUDE = {
    'turso_core': 'turso: SQL DB -> browser storage',
    'turso_parser': 'turso: SQL DB -> browser storage',
    'turso_ext': 'turso: SQL DB -> browser storage',
    'regex': 'regex -> browser RegExp',
    'regex_automata': 'regex -> browser RegExp',
    'regex_syntax': 'regex -> browser RegExp',
    'aho_corasick': 'regex -> browser RegExp',
    'accesskit': 'accesskit -> browser DOM a11y',
    'rustls': 'TLS -> fetch()',
    'webpki': 'TLS -> fetch()',
    'ring': 'TLS -> fetch()',
    'der': 'TLS -> fetch()',
    'ash': 'GPU/Vulkan',
    'gl_context_loader': 'GPU/GL loader',
    'glutin': 'GPU/GL loader',
    'gilrs': 'gamepad',
    'gilrs_core': 'gamepad',
    'tinyfiledialogs': 'native dialogs',
    'x11_clipboard': 'native clipboard',
    'brotli': 'transport compression is the browser job',
    'brotli_decompressor': 'transport compression is the browser job',
    'iced_x86': 'THE LIFTER OWN x86 disassembler',
    'goblin': 'THE LIFTER OWN PE parser',
    'webrender': 'GPU renderer (reached via desktop shell)',
    'webrender_api': 'GPU renderer (reached via desktop shell)',
    'webrender_build': 'GPU renderer (reached via desktop shell)',
}


def crate_of(name):
    """Leading crate name, mirroring the classifier's own extraction."""
    lead = name.lstrip('_')
    # a name may start with `<crate::Type as ...>` - look inside the angle
    if lead.startswith('<'):
        lead = lead[1:]
    out = []
    for ch in lead:
        if ch.isalnum() or ch == '_':
            out.append(ch)
        else:
            break
    return ''.join(out)


def main():
    log, scratch = sys.argv[1], sys.argv[2]
    rows = []
    for line in io.open(log, encoding='utf-8', errors='replace'):
        if ': lifting ' not in line:
            continue
        m = LIFT.search(line)
        if not m:
            continue
        p = os.path.join(scratch, m.group(4) + '.o')
        try:
            s = os.path.getsize(p)
        except OSError:
            s = 0
        if s > 0:
            rows.append((m.group(1), s))

    tot = sum(s for _, s in rows)
    print('lifted functions with objects: %d   total %.2f MB' % (len(rows), tot / 1e6))
    print('')
    agg = collections.defaultdict(lambda: [0, 0])
    for n, s in rows:
        c = crate_of(n)
        if c in EXCLUDE:
            agg[c][0] += s
            agg[c][1] += 1
    if not agg:
        print('=== no browser-excluded crate present in this run ===')
    else:
        print('=== browser-excluded payload present (CRATE-ANCHORED) ===')
        print('%-22s %6s %10s %8s  %s' % ('crate', 'fns', 'wasm MB', '% run', 'why'))
        print('-' * 92)
        claimed = 0
        for c, (s, k) in sorted(agg.items(), key=lambda kv: -kv[1][0]):
            claimed += s
            print('%-22s %6d %10.3f %7.2f%%  %s'
                  % (c, k, s / 1e6, 100.0 * s / max(tot, 1), EXCLUDE[c]))
        print('-' * 92)
        print('%-22s %6d %10.3f %7.2f%%' % ('TOTAL', sum(v[1] for v in agg.values()),
                                            claimed / 1e6, 100.0 * claimed / max(tot, 1)))
    print('')
    print('=== crates by wasm bytes (top 18, for orientation) ===')
    by = collections.defaultdict(lambda: [0, 0])
    for n, s in rows:
        c = crate_of(n) or '(none)'
        by[c][0] += s
        by[c][1] += 1
    for c, (s, k) in sorted(by.items(), key=lambda kv: -kv[1][0])[:18]:
        mark = '  <-- EXCLUDED' if c in EXCLUDE else ''
        print('  %-26s %6d fns %9.2f MB %6.1f%%%s'
              % (c, k, s / 1e6, 100.0 * s / max(tot, 1), mark))


main()
