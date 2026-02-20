#!/usr/bin/env python3
"""Analyze per-line coverage of a source file from lcov data.

Usage:
    python3 analyze_fc_coverage.py <lcov_file> <source_file>

Example:
    python3 scripts/analyze_fc_coverage.py /tmp/fc_coverage.lcov layout/src/solver3/fc.rs
"""
import re
import sys


def parse_da_lines(lcov_path):
    """Parse DA (line coverage) entries from lcov file."""
    da = {}
    with open(lcov_path) as f:
        for line in f:
            line = line.strip()
            if line.startswith("DA:"):
                parts = line[3:].split(",")
                if len(parts) >= 2:
                    try:
                        da[int(parts[0])] = int(parts[1])
                    except ValueError:
                        pass
    return da


def find_fn_defs(src_lines):
    """Find all function definitions in source by regex."""
    fn_defs = []
    for i, sl in enumerate(src_lines, 1):
        m = re.match(r"\s*(pub\s+)?(fn|async\s+fn)\s+(\w+)", sl)
        if m:
            fn_defs.append((i, m.group(3)))
    return fn_defs


def find_function_at(fn_defs, lineno):
    """Find the nearest function definition at or before lineno."""
    best = None
    for fl, fn in fn_defs:
        if fl <= lineno:
            best = fn
    return best or "?"


def find_uncovered_ranges(da):
    """Find consecutive uncovered (count=0) line ranges."""
    sorted_lines = sorted(da.items())
    ranges = []
    start = None
    end = None
    for lineno, count in sorted_lines:
        if count == 0:
            if start is None:
                start = lineno
            end = lineno
        else:
            if start is not None:
                ranges.append((start, end))
                start = None
                end = None
    if start is not None:
        ranges.append((start, end))
    return ranges


def main():
    lcov_path = sys.argv[1] if len(sys.argv) > 1 else "/tmp/fc_coverage.lcov"
    src_path = sys.argv[2] if len(sys.argv) > 2 else "layout/src/solver3/fc.rs"

    da = parse_da_lines(lcov_path)
    src_lines = open(src_path).readlines()
    fn_defs = find_fn_defs(src_lines)

    total = len(da)
    covered = sum(1 for v in da.values() if v > 0)
    uncovered = total - covered

    print(f"Total instrumented lines: {total}")
    print(f"Covered: {covered} ({100 * covered // total}%)")
    print(f"Uncovered: {uncovered} ({100 * uncovered // total}%)")

    # Find uncovered ranges
    all_ranges = find_uncovered_ranges(da)
    big = [(s, e) for s, e in all_ranges if e - s >= 4]
    big.sort(key=lambda x: -(x[1] - x[0]))

    print(f"\nTotal uncovered ranges: {len(all_ranges)}")
    print(f"Significant uncovered blocks (>=5 lines): {len(big)}")

    # --- Function-level summary ---
    print()
    print("=" * 80)
    print("FUNCTION-LEVEL COVERAGE SUMMARY (sorted by uncovered lines)")
    print("=" * 80)

    fn_coverage = {}
    for lineno, count in sorted(da.items()):
        fn = find_function_at(fn_defs, lineno)
        if fn not in fn_coverage:
            fn_coverage[fn] = [0, 0, lineno]
        fn_coverage[fn][1] += 1
        if count > 0:
            fn_coverage[fn][0] += 1

    fn_items = sorted(fn_coverage.items(), key=lambda x: x[1][1] - x[1][0], reverse=True)
    for fn, (cov, tot, start) in fn_items:
        uncov = tot - cov
        pct = 100 * cov // tot if tot > 0 else 0
        if uncov > 0:
            print(f"  L{start:5d} {fn:55s} {cov:4d}/{tot:4d} ({pct:2d}%) - {uncov} uncovered")

    # --- Largest uncovered blocks ---
    print()
    print("=" * 80)
    print("LARGEST UNCOVERED CODE BLOCKS (top 40)")
    print("=" * 80)
    for s, e in big[:40]:
        size = e - s + 1
        fn = find_function_at(fn_defs, s)
        context = []
        for i in range(s, min(e + 1, len(src_lines) + 1)):
            if i <= len(src_lines):
                context.append(src_lines[i - 1].rstrip())
        print(f"\n  Lines {s}-{e} ({size} lines) in fn {fn}:")
        for cl in context[:5]:
            print(f"    | {cl}")
        if len(context) > 5:
            print(f"    | ... ({size - 5} more lines)")


if __name__ == "__main__":
    main()
