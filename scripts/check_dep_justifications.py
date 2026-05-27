#!/usr/bin/env python3
"""
check_dep_justifications.py — supply-chain governance gate for azul's CI.

Every crate in azul's dependency tree must carry a human-written justification
in `scripts/dependency-justifications.toml` ("why does azul need this crate?").
A crate that appears in the tree with no justification FAILS the build. So
pulling in a new dependency — direct OR transitive — requires a deliberate,
reviewable one-line justification, raising the bar against quietly sneaking in
(or supply-chain-injecting) a dependency.

This is the post-processor for the `Dependency tree` CI job: that job runs
`cargo tree` per workspace member per OS and writes normalized crate lists to
`deptree/<member>.<os>.crates.txt` (one lowercase crate name per line). This
script reads those lists, diffs them against the justification file, and:

  * exits 0 if every crate present is justified (or under --report-only),
  * exits 1 (failing the build) listing the crates that need a justification.

Usage:
    check_dep_justifications.py [PATHS...]
        [--justifications FILE] [--os LABEL] [--report-only]

PATHS may be `*.crates.txt` files and/or directories containing them; default
is `./deptree`. --os is a label for the Summary heading. --report-only never
fails (for local inspection). If $GITHUB_STEP_SUMMARY is set, a markdown
report is appended to it.

The justification file is parsed with a tolerant line regex rather than a TOML
library, so this runs on the runners' stock `python3` (3.8+) without needing
`tomllib` (Python 3.11+). Recognized lines: `crate-name = "reason"`. Lines that
are blank, start with `#`, or are `[section]` headers are ignored. A crate with
an empty reason ("") does NOT count as justified.
"""
from __future__ import annotations

import argparse
import os
import re
import sys
from pathlib import Path

# `name = "reason"` — crate names are [a-z0-9_-]; reason is a double-quoted
# string (we only use plain double-quoted strings in the file). Trailing
# `# comment` after the value is allowed.
_ENTRY_RE = re.compile(r'^\s*([A-Za-z0-9_.-]+)\s*=\s*"((?:[^"\\]|\\.)*)"\s*(?:#.*)?$')


def load_justifications(path: Path) -> tuple[dict[str, str], list[str]]:
    """Return (name -> reason, [parse_warnings]). Empty reasons are kept (so we
    can report them as "present but empty" = not justified)."""
    if not path.is_file():
        sys.exit(f"error: justification file not found: {path}")
    out: dict[str, str] = {}
    warnings: list[str] = []
    for lineno, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#") or line.startswith("["):
            continue
        m = _ENTRY_RE.match(raw)
        if not m:
            warnings.append(f"{path}:{lineno}: unparseable line: {line!r}")
            continue
        name, reason = m.group(1), m.group(2).replace('\\"', '"').strip()
        if name in out:
            warnings.append(f"{path}:{lineno}: duplicate entry for {name!r}")
        out[name] = reason
    return out, warnings


def collect_crate_files(paths: list[str]) -> list[Path]:
    files: list[Path] = []
    for p in paths or ["deptree"]:
        pp = Path(p)
        if pp.is_dir():
            files.extend(sorted(pp.glob("*.crates.txt")))
        elif pp.is_file():
            files.append(pp)
        else:
            print(f"warning: path not found, skipping: {pp}", file=sys.stderr)
    return files


def load_present_crates(files: list[Path]) -> set[str]:
    present: set[str] = set()
    for f in files:
        for line in f.read_text(encoding="utf-8").splitlines():
            name = line.strip()
            if name and not name.startswith("#"):
                present.add(name)
    return present


def emit_summary(text: str) -> None:
    """Append markdown to the GitHub run Summary, if running under Actions."""
    path = os.environ.get("GITHUB_STEP_SUMMARY")
    if path:
        with open(path, "a", encoding="utf-8") as fh:
            fh.write(text + "\n")


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description="Fail the build on unjustified dependencies.")
    ap.add_argument("paths", nargs="*", help="*.crates.txt files or dirs (default: ./deptree)")
    ap.add_argument("--justifications", default=None,
                    help="path to dependency-justifications.toml (default: alongside this script)")
    ap.add_argument("--os", default=os.environ.get("RUNNER_OS", ""), help="OS label for the report heading")
    ap.add_argument("--report-only", action="store_true", help="never fail; just report")
    args = ap.parse_args(argv)

    just_path = Path(args.justifications) if args.justifications \
        else Path(__file__).resolve().parent / "dependency-justifications.toml"
    justified_map, parse_warnings = load_justifications(just_path)
    justified = {name for name, reason in justified_map.items() if reason}

    files = collect_crate_files(args.paths)
    if not files:
        # No crate lists means the dep-tree generation produced nothing — we
        # can't verify the gate, so fail loudly rather than pass vacuously.
        msg = "error: no *.crates.txt found — cannot verify dependency justifications."
        emit_summary(f"## 🧾 Dependency justifications{(' (' + args.os + ')') if args.os else ''}\n\n❌ {msg}")
        print(msg, file=sys.stderr)
        return 1

    present = load_present_crates(files)
    missing = sorted(present - justified)
    empty = sorted(n for n in present if n in justified_map and not justified_map[n])
    stale = sorted(justified - present)  # justified but not present on THIS OS — informational only

    label = f" ({args.os})" if args.os else ""
    lines = [f"## 🧾 Dependency justifications{label}", ""]
    lines.append(f"- crates in tree: **{len(present)}**")
    lines.append(f"- justified: **{len(present) - len(missing)}**")
    lines.append(f"- **unjustified: {len(missing)}**")
    if stale:
        lines.append(f"- justification entries not used on this OS: {len(stale)} (ok — may apply to another platform)")
    lines.append("")

    if missing:
        lines.append("❌ **These crates have no justification in "
                     "`scripts/dependency-justifications.toml`.** Add a line "
                     '`crate = "why azul needs it"` for each (a transitive dep '
                     "should note the parent that pulls it):")
        lines.append("")
        lines.append("| crate |")
        lines.append("|---|")
        lines.extend(f"| `{c}` |" for c in missing)
        lines.append("")
    else:
        lines.append("✅ Every dependency present on this OS is justified.")
        lines.append("")

    report = "\n".join(lines)
    emit_summary(report)
    # Also print to the job log (plain, no color — honoring NO_COLOR).
    print(report)
    for w in parse_warnings:
        print(f"::warning::{w}")
    if empty:
        print(f"::warning::empty (unjustified) reasons: {', '.join(empty)}")

    if missing and not args.report_only:
        print(f"::error::{len(missing)} dependency(ies) lack a justification "
              f"(see scripts/dependency-justifications.toml).")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
