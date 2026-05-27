#!/usr/bin/env python3
"""
check_dep_justifications.py — supply-chain governance gate + annotated tree.

Every crate in azul's dependency tree must carry a human-written justification
in `scripts/dependency-justifications.toml` ("why does azul need this crate?").
A crate that appears in the tree with no justification FAILS the build. So
pulling in a new dependency — direct OR transitive — requires a deliberate,
reviewable one-line justification, raising the bar against quietly sneaking in
(or supply-chain-injecting) a dependency.

This is the post-processor for the `Dependency tree` CI job, which runs
`cargo tree` per workspace member per OS and writes normalized crate lists to
`deptree/<member>.<os>.crates.txt` (one lowercase crate name per line). This
script reads those lists and:

  * renders, into the GitHub run Summary, a per-member collapsible table of
    `crate | justification` so the justifications can be reviewed inline
    against the actual dependency set (unjustified crates flagged ❌),
  * exits 0 if every crate present is justified (or under --report-only),
  * exits 1 (failing the build) when any crate lacks a justification.

Usage:
    check_dep_justifications.py [PATHS...]
        [--justifications FILE] [--os LABEL] [--report-only]

PATHS may be `*.crates.txt` files and/or directories containing them; default
is `./deptree`. --os is a label for the Summary heading. --report-only never
fails (for local inspection). If $GITHUB_STEP_SUMMARY is set, the annotated
report is appended to it; stdout gets a concise result (counts + any offenders).

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

# `name = "reason"` — crate names are [a-z0-9_.-]; reason is a double-quoted
# string. A trailing `# comment` after the value is allowed.
_ENTRY_RE = re.compile(r'^\s*([A-Za-z0-9_.-]+)\s*=\s*"((?:[^"\\]|\\.)*)"\s*(?:#.*)?$')

# Render members smallest-to-superset; unknown members fall after, alphabetical.
_MEMBER_ORDER = {"azul-css": 0, "azul-core": 1, "azul-layout": 2, "azul-dll": 3}


def load_justifications(path: Path) -> tuple[dict[str, str], list[str]]:
    """Return (name -> reason, [parse_warnings]). Empty reasons are kept (so we
    can report them as present-but-empty = not justified)."""
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


def member_of(path: Path) -> str:
    """`azul-dll.ubuntu-22.04.crates.txt` -> `azul-dll` (member names have no
    dots; the OS label may, so split on the first dot after stripping suffix)."""
    name = path.name
    if name.endswith(".crates.txt"):
        name = name[: -len(".crates.txt")]
    return name.split(".", 1)[0]


def collect_member_files(paths: list[str]) -> list[tuple[str, Path]]:
    files: list[tuple[str, Path]] = []
    for p in paths or ["deptree"]:
        pp = Path(p)
        if pp.is_dir():
            for f in sorted(pp.glob("*.crates.txt")):
                files.append((member_of(f), f))
        elif pp.is_file():
            files.append((member_of(pp), pp))
        else:
            print(f"warning: path not found, skipping: {pp}", file=sys.stderr)
    files.sort(key=lambda mf: (_MEMBER_ORDER.get(mf[0], len(_MEMBER_ORDER)), mf[0]))
    return files


def read_crates(path: Path) -> list[str]:
    out = []
    for line in path.read_text(encoding="utf-8").splitlines():
        name = line.strip()
        if name and not name.startswith("#"):
            out.append(name)
    return sorted(set(out))


def emit_summary(text: str) -> None:
    """Append markdown to the GitHub run Summary, if running under Actions."""
    path = os.environ.get("GITHUB_STEP_SUMMARY")
    if path:
        with open(path, "a", encoding="utf-8") as fh:
            fh.write(text + "\n")


def _cell(s: str) -> str:
    """Escape a string for a markdown table cell."""
    return s.replace("\\", "\\\\").replace("|", "\\|")


def main(argv: list[str]) -> int:
    # Windows runners default stdout/stderr to cp1252, which can't encode the
    # emoji below (UnicodeEncodeError would crash the gate on its own output).
    for _stream in (sys.stdout, sys.stderr):
        try:
            _stream.reconfigure(encoding="utf-8")
        except (AttributeError, ValueError):
            pass

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

    members = collect_member_files(args.paths)
    if not members:
        # No crate lists means the dep-tree generation produced nothing — we
        # can't verify the gate, so fail loudly rather than pass vacuously.
        msg = "error: no *.crates.txt found — cannot verify dependency justifications."
        emit_summary(f"## 🧾 Dependency justifications{(' (' + args.os + ')') if args.os else ''}\n\n❌ {msg}")
        print(msg, file=sys.stderr)
        return 1

    present: set[str] = set()
    for _m, path in members:
        present.update(read_crates(path))
    missing = sorted(present - justified)
    empty = sorted(n for n in present if n in justified_map and not justified_map[n])

    label = f" ({args.os})" if args.os else ""
    out: list[str] = [f"## 🧾 Dependency tree + justifications{label}", ""]
    out.append(f"**{len(present)}** unique crates · "
               f"**{len(present) - len(missing)}** justified · "
               + (f"**❌ {len(missing)} unjustified**" if missing else "**✅ 0 unjustified**"))
    out.append("")
    out.append("Every crate must carry a justification in "
               "`scripts/dependency-justifications.toml`. Tables below pair each "
               "crate with its justification for review; an unjustified crate "
               "fails the build.")
    out.append("")

    if missing:
        out.append(f"### ❌ {len(missing)} crate(s) need a justification")
        out.append("")
        out.append('Add `crate = "why azul needs it"` (a transitive dep should name the parent that pulls it):')
        out.append("")
        out.append("| crate |")
        out.append("|---|")
        out.extend(f"| `{c}` |" for c in missing)
        out.append("")

    # Per-member annotated tables (collapsed; azul-dll is the superset).
    for member, path in members:
        crates = read_crates(path)
        n_missing = sum(1 for c in crates if c not in justified)
        status = "✅ all justified" if n_missing == 0 else f"❌ {n_missing} unjustified"
        out.append(f"<details><summary><b>{member}</b> — {len(crates)} crates · {status}</summary>")
        out.append("")
        out.append("| crate | justification |")
        out.append("|---|---|")
        for c in crates:
            if c in justified:
                out.append(f"| `{c}` | {_cell(justified_map[c])} |")
            else:
                out.append(f"| `{c}` | ❌ **no justification** |")
        out.append("")
        out.append("</details>")
        out.append("")

    emit_summary("\n".join(out))

    # stdout: concise result only (the full annotated tree lives in the Summary).
    print(f"dependency justifications{label}: {len(present)} crates, "
          f"{len(present) - len(missing)} justified, {len(missing)} unjustified")
    for w in parse_warnings:
        print(f"::warning::{w}")
    if empty:
        print(f"::warning::empty (unjustified) reasons: {', '.join(empty)}")
    if missing:
        print("unjustified crates: " + ", ".join(missing))

    if missing and not args.report_only:
        print(f"::error::{len(missing)} dependency(ies) lack a justification "
              f"(see scripts/dependency-justifications.toml).")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
