#!/usr/bin/env python3
"""Attribute lifted-wasm size to crates and categories, from a lift's own logs.

The shipped wasm is `--strip-all`'d, so it carries no names. But the server log
records, per function, `lifting <NAME> addr=<a> size=<bytes> export_as=<E>`, and
the scratch dir holds one object file per function named by `<E>.o`. Joining the
two gives per-function wasm-object size keyed by a demangled Rust name, which is
enough to answer "why is this 100 MB and what can be cut".

Usage:
  wasm-size-report.py <server.log> <scratch-dir> [--top N] [--csv out.csv]

Size proxy is the per-function `.o` (its wasm contribution before final LTO);
it tracks the shipped size closely enough to rank cut/merge candidates.
"""
import io, os, re, sys, collections

def demangle_crate(name):
    """Best-effort crate/category bucket from a mangled or plain Rust name."""
    n = name
    # Panic machinery. Nearly all of it is FnClass::NeverLift (it traps and is
    # never lifted), so a non-trivial number here means the classifier missed a
    # family - that is the signal to read, not a stub opportunity.
    if re.search(r'panicking|begin_panic|rust_begin_unwind|handle_alloc_error|'
                 r'capacity_overflow|assert_failed|expect_failed|unwrap_failed|'
                 r'panic_bounds_check|_fail$|_fail::', n):
        return 'PANIC (should be ~0)'
    # Formatting. `Display`/`Debug` are matched only in TRAIT position: bare
    # substring matching also hits TYPE names - `DisplayList`, `DisplayListItem`
    # - and silently books core layout code as stubbable formatting.
    if re.search(r'core::fmt|::fmt::|Formatter|write_fmt|format_args|::fmt$|'
                 r'Display>|Debug>|as core::fmt::(Display|Debug)', n):
        return 'FMT'
    for crate, pat in [
        ('allsorts (font)',      r'allsorts'),
        ('printpdf (pdf)',       r'printpdf|pdf::'),
        ('ooxml (docx)',         r'ooxml|docx'),
        ('azul_layout',          r'azul_layout|azul.layout'),
        ('azul_core',            r'azul_core|azul.core'),
        ('azul_css',             r'azul_css|azul.css'),
        ('hashbrown',            r'hashbrown'),
        ('alloc',                r'^alloc::|alloc::(vec|string|boxed|raw_vec|collections)'),
        ('core',                 r'^core::|core::(slice|iter|option|result|num|str|ptr)'),
        ('std',                  r'^std::|std::(sys|io|collections|thread)'),
        ('serde',               r'serde'),
        ('app (AzWriter/az)',    r'AzStartup|azwriter|AzWriter'),
        ('unicode/icu',          r'unicode|icu|bidi'),
        ('webrender',            r'webrender|wr_'),
    ]:
        if re.search(pat, n):
            return crate
    return 'other'

def main():
    if len(sys.argv) < 3:
        print(__doc__); sys.exit(2)
    log, scratch = sys.argv[1], sys.argv[2]
    top = 25
    csv_out = None
    if '--top' in sys.argv:
        top = int(sys.argv[sys.argv.index('--top') + 1])
    if '--csv' in sys.argv:
        csv_out = sys.argv[sys.argv.index('--csv') + 1]

# Rust names from MSVC/PDB CONTAIN SPACES - `Vec<Box<T> >` renders with a
# space before the closing angle bracket. Anchoring the name capture on
# `(\S+)` stops at that space and silently drops the function: measured, it
# lost 1197 of run39's 5251 lifting lines (23%) and 20.6 MB of wasm, which
# is more than most of the savings these scripts are used to evaluate. The
# capture must be non-greedy and anchored on the trailing ` addr=` field.
    lift_re = re.compile(
        r'lifting (.+?) addr=0x([0-9a-fA-F]+) size=(\d+) export_as=(\S+)')
    fns = {}  # export_as -> (name, native_size)
    for line in io.open(log, encoding='utf-8', errors='replace'):
        m = lift_re.search(line)
        if m:
            name, _addr, nsize, exp = m.group(1), m.group(2), int(m.group(3)), m.group(4)
            fns[exp] = (name, nsize)

    # object size per function
    rows = []
    missing = 0
    for exp, (name, nsize) in fns.items():
        o = os.path.join(scratch, exp + '.o')
        try:
            osize = os.path.getsize(o)
        except OSError:
            missing += 1
            osize = 0
        rows.append((osize, nsize, demangle_crate(name), name, exp))

    total_o = sum(r[0] for r in rows)
    total_n = sum(r[1] for r in rows)
    by_cat = collections.defaultdict(lambda: [0, 0, 0])  # cat -> [obj, native, count]
    for osize, nsize, cat, *_ in rows:
        c = by_cat[cat]
        c[0] += osize; c[1] += nsize; c[2] += 1

    print(f"functions lifted: {len(rows)}   objects on disk: {len(rows) - missing}"
          f"   ({missing} not yet compiled)")
    print(f"total .o size: {total_o/1e6:.1f} MB   native .text: {total_n/1e6:.1f} MB"
          f"   expansion {total_o/max(total_n,1):.1f}x")
    print()
    print(f"{'category':22} {'obj MB':>8} {'%':>5} {'count':>7} {'avg B':>8}")
    print('-' * 54)
    for cat, (o, n, c) in sorted(by_cat.items(), key=lambda kv: -kv[1][0]):
        print(f"{cat:22} {o/1e6:8.1f} {100*o/max(total_o,1):5.1f} {c:7} {o//max(c,1):8}")
    print()
    pan = by_cat.get('PANIC (should be ~0)', [0, 0, 0])
    fmt = by_cat.get('FMT', [0, 0, 0])
    print(f"*** PANIC: {pan[0]/1e6:.2f} MB across {pan[2]} functions. Expected ~0 - the"
          f" panic family is NeverLift. Anything large here is a classifier miss.")
    print(f"*** FMT:   {fmt[0]/1e6:.2f} MB / {100*fmt[0]/max(total_o,1):.0f}% across {fmt[2]}"
          f" functions - mostly live (markdown, CSS, PDF text), not free to stub.")
    print()
    print(f"top {top} functions by object size:")
    for osize, nsize, cat, name, exp in sorted(rows, reverse=True)[:top]:
        print(f"  {osize/1e3:7.0f} KB  [{cat:16}]  {name[:78]}")

    if csv_out:
        with io.open(csv_out, 'w', encoding='utf-8') as f:
            f.write("obj_bytes,native_bytes,category,name\n")
            for osize, nsize, cat, name, _ in sorted(rows, reverse=True):
                f.write(f'{osize},{nsize},{cat},"{name}"\n')
        print(f"\nwrote {csv_out}")

if __name__ == '__main__':
    main()
