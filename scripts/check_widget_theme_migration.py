#!/usr/bin/env python3
"""Exit check for the widget theme migration.

Answers one question: is the migration described in
`doc/WIDGET_THEME_MIGRATION.md` finished? Exits 0 when every phase passes and
non-zero with a report of what is left, so an unattended run can use it as its
stopping condition rather than a guess.

The checks are deliberately narrow and mechanical. Each one states a property
that is false today and true when that phase is done; none of them can be
satisfied by deleting code, because every count that must reach zero is paired
with one that must stay above zero.

    python3 scripts/check_widget_theme_migration.py          # all phases
    python3 scripts/check_widget_theme_migration.py --phase 1
    python3 scripts/check_widget_theme_migration.py --json
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
WIDGET_DIR = REPO / "layout" / "src" / "widgets"
THEME_DIR = WIDGET_DIR / "themes"
FLORA_CSS = REPO / "doc" / "templates" / "flora.css"

# Not widgets: the theme modules themselves, the module root, and the map's
# vendored MapCSS palettes (which are map tile styling, not widget styling).
NON_WIDGET = {"mod.rs", "map_themes.rs"}


@dataclass
class Result:
    name: str
    ok: bool
    detail: str = ""
    offenders: list[str] = field(default_factory=list)


def widget_files() -> list[Path]:
    return sorted(p for p in WIDGET_DIR.glob("*.rs") if p.name not in NON_WIDGET)


def theme_files() -> list[Path]:
    return sorted(THEME_DIR.glob("*.rs"))


# A test module's attribute is not always the bare `#[cfg(test)]`: this repo also
# uses `#[cfg(all(test, feature = "std"))]`, and some modules carry an
# `#[allow(..)]` between the cfg and the `mod`.
TEST_MOD = re.compile(
    r"#\[cfg\((?:test|all\(\s*test\b[^)]*\)[^)]*)\)\]"   # #[cfg(test)] / #[cfg(all(test, ..))]
    r"(?:[ \t]*//[^\n]*)?"                                    # ..with a trailing comment
    r"(?:\s*#\[[^\]]*\](?:[ \t]*//[^\n]*)?)*"                 # further attributes, each may
    r"(?:\s*//[^\n]*)*"                                        #   carry one; comment lines too
    r"\s*mod\s+\w+\s*\{"
)


def _module_end(src: str, open_brace: int) -> int | None:
    """Index just past the `}` closing the block that opens at `open_brace`.

    Counts braces while SKIPPING string literals, char literals, comments and
    raw strings. Doing this naively reads `"{ctx}: ..."` or `"\\u{0301}"` as real
    braces and closes the module hundreds of lines early, which silently leaks
    the rest of the test module into every check below (it did: label.rs).
    """
    depth = 0
    i = open_brace
    n = len(src)
    while i < n:
        c = src[i]
        if c == "/" and i + 1 < n and src[i + 1] == "/":
            j = src.find("\n", i)
            i = n if j == -1 else j + 1
            continue
        if c == "/" and i + 1 < n and src[i + 1] == "*":
            j = src.find("*/", i + 2)
            i = n if j == -1 else j + 2
            continue
        if c == "r" and i + 1 < n and src[i + 1] in '"#':
            m = re.match(r'r(#*)"', src[i:])
            if m:
                close = '"' + m.group(1)
                j = src.find(close, i + m.end())
                i = n if j == -1 else j + len(close)
                continue
        if c == '"':
            i += 1
            while i < n and src[i] != '"':
                i += 2 if src[i] == "\\" else 1
            i += 1
            continue
        if c == "'":
            # A lifetime (`'a`) is not a char literal; a char literal closes on
            # the next unescaped quote within a few bytes.
            m = re.match(r"'(?:\\.|[^\\'])'", src[i:])
            if m:
                i += m.end()
                continue
            i += 1
            continue
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                return i + 1
        i += 1
    return None


def strip_test_modules(src: str) -> str:
    """Drop every `#[cfg(test)] mod .. { .. }` so test code is not mistaken for API.

    Strips ALL such modules, not just the first: a file may carry more than one,
    and stopping at the first leaves the rest readable by the checks.
    """
    while True:
        marker = TEST_MOD.search(src)
        if not marker:
            return src
        end = _module_end(src, marker.end() - 1)
        src = src[: marker.start()] + (src[end:] if end is not None else "")


# ---------------------------------------------------------------------------
# Phase 1 — every widget style field is an Option, and no constructor pre-fills
# ---------------------------------------------------------------------------

BARE_FIELD = re.compile(r"^\s*pub\s+(\w*(?:style|css))\s*:\s*CssPropertyWithConditionsVec\s*,", re.M)
PREFILLED = re.compile(r"^\s*(\w*(?:style|css))\s*:\s*OptionCssPropertyWithConditionsVec::Some\(", re.M)


def phase1() -> list[Result]:
    bare: list[str] = []
    prefilled: list[str] = []
    option_fields = 0

    for f in widget_files():
        src = strip_test_modules(f.read_text())
        for m in BARE_FIELD.finditer(src):
            bare.append(f"{f.name}:{src[:m.start()].count(chr(10)) + 1} {m.group(1)}")
        for m in PREFILLED.finditer(src):
            prefilled.append(f"{f.name}:{src[:m.start()].count(chr(10)) + 1} {m.group(1)}")
        option_fields += len(
            re.findall(r"^\s*pub\s+\w*(?:style|css)\s*:\s*OptionCssPropertyWithConditionsVec", src, re.M)
        )

    out = [
        Result(
            "1a. no widget style field is a bare CssPropertyWithConditionsVec",
            not bare,
            f"{len(bare)} field(s) still bare",
            bare,
        ),
        Result(
            "1b. no constructor pre-fills a style field with Some(default)",
            not prefilled,
            "a pre-filled field cannot be told from one the caller chose; "
            f"{len(prefilled)} site(s) left",
            prefilled,
        ),
        # Guards 1a against being "passed" by deleting the fields outright.
        Result(
            "1c. the Option style fields still exist",
            option_fields >= 100,
            f"{option_fields} Option style field(s) (expected >= 100)",
        ),
    ]
    return out


# ---------------------------------------------------------------------------
# Phase 2 — interactive states live in the themes, and each has a dark twin
# ---------------------------------------------------------------------------

STATE_CALLS = ("on_hover(", "on_active(", "on_focus(")
DARK_STATE_CALLS = ("dark_on_hover(", "dark_on_active(", "dark_on_focus(")


def count_states(src: str) -> dict[str, int]:
    # `dark_on_hover(` also contains `on_hover(`, so the plain count subtracts
    # the dark one rather than double-counting it.
    out = {}
    for plain, dark in zip(STATE_CALLS, DARK_STATE_CALLS):
        out[plain] = src.count(plain) - src.count(dark)
        out[dark] = src.count(dark)
    return out


def phase2() -> list[Result]:
    stragglers: list[str] = []
    for f in widget_files():
        src = strip_test_modules(f.read_text())
        c = count_states(src)
        n = sum(c[k] for k in STATE_CALLS)
        if n:
            stragglers.append(f"{f.name} ({n} state rule(s))")

    theme_plain = 0
    theme_dark = 0
    per_theme: list[str] = []
    for f in theme_files():
        if f.name == "mod.rs":
            continue
        c = count_states(f.read_text())
        p = sum(c[k] for k in STATE_CALLS)
        d = sum(c[k] for k in DARK_STATE_CALLS)
        theme_plain += p
        theme_dark += d
        per_theme.append(f"{f.name}: {p} light, {d} dark")

    return [
        Result(
            "2a. no interactive-state styling is left in the widget files",
            not stragglers,
            "hover/active/focus belongs in the theme modules, where the dark "
            "variant is in scope",
            stragglers,
        ),
        Result(
            "2b. the theme modules declare interactive states",
            theme_plain >= 100,
            f"{theme_plain} state rule(s) across the themes (expected >= 100)",
            per_theme,
        ),
        Result(
            "2c. every interactive state has a dark twin",
            theme_dark >= theme_plain,
            f"{theme_dark} dark vs {theme_plain} light state rule(s)",
            per_theme,
        ),
    ]


# ---------------------------------------------------------------------------
# Phase 3 — flora actually uses flora.css's gradients
# ---------------------------------------------------------------------------

FLORA_STOP_TOKENS = ("_RT", "_RB", "_HT", "_HB", "_PT", "_PB")


def phase3() -> list[Result]:
    flora = (THEME_DIR / "flora.rs").read_text()
    css = FLORA_CSS.read_text() if FLORA_CSS.exists() else ""

    css_linear = css.count("linear-gradient")
    rs_linear = flora.count("LinearGradient")

    # The raised / hover / pressed pairs exist to BE gradient stops. A token
    # referenced once is referenced only by its own definition.
    unused = []
    for tok in FLORA_STOP_TOKENS:
        for mode in ("LIGHT", "DARK"):
            name = f"{mode}{tok}"
            if f"pub const {name}" in flora and flora.count(name) <= 1:
                unused.append(name)

    return [
        Result(
            "3a. flora declares linear gradients",
            rs_linear >= css_linear // 2,
            f"{rs_linear} in flora.rs vs {css_linear} in flora.css "
            f"(expected at least half)",
        ),
        Result(
            "3b. the raised/hover/pressed stop tokens are used, not just defined",
            not unused,
            f"{len(unused)} token(s) referenced only by their own definition",
            unused,
        ),
    ]


PHASES = {1: ("Option migration", phase1), 2: ("interactive states + dark", phase2), 3: ("flora gradients", phase3)}


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--phase", type=int, choices=sorted(PHASES), help="check one phase only")
    ap.add_argument("--json", action="store_true", help="machine-readable output")
    args = ap.parse_args()

    wanted = [args.phase] if args.phase else sorted(PHASES)
    results: list[tuple[int, str, Result]] = []
    for n in wanted:
        label, fn = PHASES[n]
        for r in fn():
            results.append((n, label, r))

    failed = [r for _, _, r in results if not r.ok]

    if args.json:
        print(
            json.dumps(
                {
                    "done": not failed,
                    "checks": [
                        {"phase": n, "name": r.name, "ok": r.ok, "detail": r.detail, "offenders": r.offenders}
                        for n, _, r in results
                    ],
                },
                indent=2,
            )
        )
        return 0 if not failed else 1

    current = None
    for n, label, r in results:
        if n != current:
            print(f"\n=== Phase {n}: {label}")
            current = n
        print(f"  [{'PASS' if r.ok else 'FAIL'}] {r.name}")
        if r.detail:
            print(f"         {r.detail}")
        if not r.ok and r.offenders:
            for o in r.offenders[:12]:
                print(f"         - {o}")
            if len(r.offenders) > 12:
                print(f"         ... and {len(r.offenders) - 12} more")

    print()
    if failed:
        print(f"MIGRATION INCOMPLETE: {len(failed)} of {len(results)} checks failing")
        return 1
    print(f"MIGRATION COMPLETE: all {len(results)} checks pass")
    return 0


if __name__ == "__main__":
    sys.exit(main())
