#!/usr/bin/env python3
"""A/B two lift runs by per-function wasm object size.

Compares a saved size manifest (`<bytes> <name>.o` per line) against a live
scratch dir, so the effect of an IR pass is measured per function rather than
guessed from a total. Functions present in only one run are reported
separately - a changed function set means the runs are not comparable and the
totals should not be read as a delta.

Usage:
  wasm-size-diff.py <baseline-manifest> <new-scratch-dir> [--top N]
"""
import io, os, sys

def main():
    if len(sys.argv) < 3:
        print(__doc__); sys.exit(2)
    manifest, scratch = sys.argv[1], sys.argv[2]
    top = int(sys.argv[sys.argv.index('--top') + 1]) if '--top' in sys.argv else 15

    base = {}
    for line in io.open(manifest, encoding='utf-8', errors='replace'):
        parts = line.split()
        if len(parts) == 2:
            base[parts[1]] = int(parts[0])

    new = {}
    for f in os.listdir(scratch):
        if f.endswith('.o'):
            try:
                new[f] = os.path.getsize(os.path.join(scratch, f))
            except OSError:
                pass

    common = set(base) & set(new)
    b_tot = sum(base[f] for f in common)
    n_tot = sum(new[f] for f in common)

    print(f"baseline objects: {len(base)}   new objects: {len(new)}   "
          f"in both: {len(common)}")
    if len(base) != len(new):
        print(f"  ! only in baseline: {len(set(base)-set(new))}   "
              f"only in new: {len(set(new)-set(base))}")
    print()
    print(f"over the {len(common)} functions present in BOTH runs:")
    print(f"  baseline : {b_tot/1e6:8.2f} MB")
    print(f"  new      : {n_tot/1e6:8.2f} MB")
    delta = b_tot - n_tot
    print(f"  delta    : {delta/1e6:+8.2f} MB   "
          f"({100*delta/max(b_tot,1):+.1f}%)")
    print()

    diffs = sorted(((base[f] - new[f], f) for f in common), reverse=True)
    grew = [d for d in diffs if d[0] < 0]
    print(f"functions that SHRANK: {sum(1 for d in diffs if d[0] > 0)}   "
          f"unchanged: {sum(1 for d in diffs if d[0] == 0)}   "
          f"GREW: {len(grew)}")
    if grew:
        print("  (a function growing after a DCE pass is a red flag - inspect)")
        for d, f in grew[:5]:
            print(f"    {-d/1e3:7.1f} KB larger  {f[:-2]}")
    print()
    print(f"top {top} reductions:")
    for d, f in diffs[:top]:
        pct = 100 * d / max(base[f], 1)
        print(f"  -{d/1e3:7.1f} KB  ({pct:5.1f}%)  {base[f]/1e3:7.1f} -> "
              f"{new[f]/1e3:7.1f} KB  {f[:-2]}")

if __name__ == '__main__':
    main()
