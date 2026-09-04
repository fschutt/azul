#!/usr/bin/env python3
"""What would stubbing panics actually remove from the lifted wasm?

A backend can turn every panic into a trap, so any function reachable ONLY
through the panic / formatting machinery is dead weight. This walks the lift's
own dependency edges (the `dep: <callee> ... (pulled in by <caller>)` log lines)
and reports the set of functions reachable only via panic/fmt entry points -
the true size of the cut, which is larger than the panic functions themselves.

Usage:
  wasm-dep-report.py <server.log> [scratch-dir]

With a scratch dir it also sums the .o bytes of the dead set, so the saving is
in megabytes, not just a function count.
"""
import io, os, re, sys, collections

PANIC_RE = re.compile(
    r'panic|panicking|begin_panic|assert_failed|expect_failed|unwrap_failed|'
    r'core::fmt|::fmt::|Formatter|write_fmt|format_args|Display|Debug')

def main():
    if len(sys.argv) < 2:
        print(__doc__); sys.exit(2)
    log = sys.argv[1]
    scratch = sys.argv[2] if len(sys.argv) > 2 else None

    # edges: caller_name -> set(callee_name); also collect every function seen,
    # its export_as (for size), and whether it is a panic/fmt entry.
    edges = collections.defaultdict(set)
    rev = collections.defaultdict(set)
    export_of = {}
    all_fns = set()

    lift_re = re.compile(r'lifting (\S+) .*export_as=(\S+)')
    dep_re = re.compile(r'dep: \S+ .* resolved=([^@]+)@.*\(pulled in by ([^)]+)\)')

    for line in io.open(log, encoding='utf-8', errors='replace'):
        m = lift_re.search(line)
        if m:
            all_fns.add(m.group(1))
            export_of[m.group(1)] = m.group(2)
            continue
        d = dep_re.search(line)
        if d:
            callee, caller = d.group(1).strip(), d.group(2).strip()
            edges[caller].add(callee)
            rev[callee].add(caller)
            all_fns.add(callee); all_fns.add(caller)

    panic_fns = {f for f in all_fns if PANIC_RE.search(f)}

    # A function is "panic-only" if EVERY path that reaches it passes through a
    # panic/fmt function. Compute the complement: what is reachable from the
    # NON-panic roots without entering a panic function. Anything unreached is
    # panic-only (dead if panics trap).
    non_panic_roots = {f for f in all_fns if f not in panic_fns and not rev.get(f)}
    # also treat any non-panic fn called by a non-panic fn as a live root seed
    live = set()
    stack = list(non_panic_roots)
    # seed with every non-panic function that has a non-panic caller
    for f in all_fns:
        if f in panic_fns:
            continue
        if any(c not in panic_fns for c in rev.get(f, ())) or not rev.get(f):
            stack.append(f)
    while stack:
        f = stack.pop()
        if f in live or f in panic_fns:
            continue
        live.add(f)
        for callee in edges.get(f, ()):
            if callee not in panic_fns and callee not in live:
                stack.append(callee)

    dead = (all_fns - live) - non_panic_roots  # unreachable without panics
    dead_nonpanic = {f for f in dead if f not in panic_fns}

    def sz(fn):
        if not scratch:
            return 0
        e = export_of.get(fn)
        if not e:
            return 0
        try:
            return os.path.getsize(os.path.join(scratch, e + '.o'))
        except OSError:
            return 0

    panic_bytes = sum(sz(f) for f in panic_fns)
    dead_bytes = sum(sz(f) for f in dead_nonpanic)

    print(f"functions seen in edges: {len(all_fns)}")
    print(f"panic/fmt functions:     {len(panic_fns)}"
          + (f"  ({panic_bytes/1e6:.1f} MB)" if scratch else ""))
    print(f"reachable only via panic/fmt (dead if stubbed): {len(dead_nonpanic)}"
          + (f"  ({dead_bytes/1e6:.1f} MB)" if scratch else ""))
    print(f"TOTAL removable by stubbing panics: "
          f"{len(panic_fns) + len(dead_nonpanic)} functions"
          + (f"  ({(panic_bytes+dead_bytes)/1e6:.1f} MB)" if scratch else ""))
    print()
    print("biggest panic-only-reachable functions (cut candidates):")
    for f in sorted(dead_nonpanic, key=sz, reverse=True)[:20]:
        print(f"  {sz(f)/1e3:7.0f} KB  {f[:80]}")

if __name__ == '__main__':
    main()
