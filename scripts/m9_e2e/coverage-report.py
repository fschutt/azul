#!/usr/bin/env python3
"""Join an AZ_FN_COVERAGE bitmap with its manifest: what a first paint RUNS.

Static reachability is the set of functions that MIGHT be called; a first paint
executes a fraction of it. Only the executed set says what must ship in the
eager chunk and what can be fetched lazily - which is the number the whole
chunking plan depends on.

Usage:
  coverage-report.py <coverage.json> <coverage-manifest.tsv> <scratch-dir>

The manifest is per-run (indices are assigned in lift order), so it must come
from the SAME run as the bitmap and the same scratch dir as the objects.
"""
import io
import json
import os
import re
import sys
import collections

CATS = [
    ('drop glue', r'^core::ptr::drop_in_place|::impl\$\d+::drop\b|::drop_slow\b'),
    ('fmt/Debug/Display', r'^core::fmt|::fmt::|::fmt$|Formatter|write_fmt|format_args'),
    ('BTreeMap', r'^alloc::collections::btree'),
    ('Vec/RawVec', r'^alloc::vec|^alloc::raw_vec'),
    ('slice sort', r'^core::slice::sort|driftsort|quicksort|small_sort'),
    ('hashbrown', r'^hashbrown'),
    ('azul_layout text3', r'^azul_layout::text3'),
    ('azul_layout solver3', r'^azul_layout::solver3'),
    ('azul_layout other', r'^azul_layout'),
    ('azul_core', r'^azul_core'),
    ('azul_css', r'^azul_css'),
    ('allsorts', r'^allsorts'),
    ('rust_fontconfig', r'^rust_fontconfig'),
    ('pulldown_cmark', r'^pulldown_cmark'),
    ('serde', r'^serde'),
    ('taffy', r'^taffy'),
    ('alloc/core/std', r'^(alloc|core|std)::'),
]
COMPILED = [(n, re.compile(p)) for n, p in CATS]


def cat(n):
    for name, rx in COMPILED:
        if rx.search(n):
            return name
    return 'other'


def main():
    cov_p, man_p, scratch = sys.argv[1], sys.argv[2], sys.argv[3]
    cov = json.load(io.open(cov_p, encoding='utf-8'))
    hits = set(cov['hits'])

    idx2 = {}
    for line in io.open(man_p, encoding='utf-8', errors='replace'):
        parts = line.rstrip('\n').split('\t')
        if len(parts) >= 3:
            try:
                idx2[int(parts[0])] = (parts[1], parts[2])
            except ValueError:
                pass

    def size_of(exp):
        try:
            return os.path.getsize(os.path.join(scratch, exp + '.o'))
        except OSError:
            return 0

    hot, cold = [], []
    for i, (exp, name) in idx2.items():
        s = size_of(exp)
        if s <= 0:
            continue
        (hot if i in hits else cold).append((name, s))

    h_n, c_n = len(hot), len(cold)
    h_b = sum(s for _, s in hot)
    c_b = sum(s for _, s in cold)
    tot_b = h_b + c_b
    print('manifest entries        : %d' % len(idx2))
    print('coverage slots set      : %d' % len(hits))
    print('')
    print('FUNCTIONS EXECUTED on first paint : %5d  (%5.1f%%)   %7.2f MB obj (%5.1f%%)'
          % (h_n, 100.0 * h_n / max(h_n + c_n, 1), h_b / 1e6, 100.0 * h_b / max(tot_b, 1)))
    print('functions never entered           : %5d  (%5.1f%%)   %7.2f MB obj (%5.1f%%)'
          % (c_n, 100.0 * c_n / max(h_n + c_n, 1), c_b / 1e6, 100.0 * c_b / max(tot_b, 1)))
    print('')
    print('=== NEVER ENTERED, by category (candidates for a lazy chunk) ===')
    agg = collections.defaultdict(lambda: [0, 0])
    for n, s in cold:
        agg[cat(n)][0] += s
        agg[cat(n)][1] += 1
    print('%-22s %7s %10s' % ('category', 'fns', 'obj MB'))
    print('-' * 42)
    for c, (s, k) in sorted(agg.items(), key=lambda kv: -kv[1][0]):
        print('%-22s %7d %10.2f' % (c, k, s / 1e6))
    print('')
    print('=== biggest never-entered functions ===')
    for n, s in sorted(cold, key=lambda x: -x[1])[:20]:
        print('  %7.1f KB  %s' % (s / 1e3, n[:92]))


main()
