#!/usr/bin/env python3
"""Consistency checker for scripts/RSS_MAP_2026_08_07.md.

WHY THIS EXISTS. That report deliberately keeps its own corrections rather
than editing them away, so it contains both current and superseded values
for the same quantities. A locale bug once truncated every filter total in
it, and purging that took SEVEN passes: each hand pass fixed the loud
occurrences (headings, headline tables) and missed the quiet ones (table
cells, subordinate clauses). The seventh pass was this check, and it found
nine live stale claims the six before it had walked past — five of them the
same value.

The check is mechanical and needs no knowledge of the numbers: section 0
already declares, per row, which value is CURRENT and which are SUPERSEDED.
Any superseded value appearing outside a retraction note is therefore a
defect by construction.

    scripts/rss-map-check.py [path]      prints a triage list; always exits 0

THIS IS A TRIAGE LIST, NOT A GATE, and the distinction is deliberate.

Matching on the bare number cannot be made precise here, because §0's
superseded column contains values that are simultaneously the CURRENT value
of some other quantity: "68.9" is superseded as a scaling law but current as
a marginal-cost cell; "267.8" is current for 3 839 repeated lines; "49.1" is
current for the document's retained cost. Roughly three quarters of what
this prints is that collision, or narrative legitimately quoting an old
figure.

It was tempting to keep tightening the heuristics until the output looked
clean. That would have been the wrong fix: a checker tuned until it passes
is a checker that has stopped checking. So it stays noisy and honest, and
exits 0 — a check that always "fails" gets ignored, which is how the
"(empty = clean)" line in an earlier script came to hide seven real hits.

Read the output as: ~40 candidates, of which the last real audit found 9
genuine stale claims. Reviewing 40 lines to catch 9 is a good trade;
believing a green tick would not have been.
"""
import re
import sys

DEFAULT = "scripts/RSS_MAP_2026_08_07.md"

# A line carrying any of these is talking ABOUT a correction, not asserting
# the old value as current.
RETRACTION = re.compile(
    r"originally read|first draft|earlier draft|earlier version|superseded"
    r"|locale-truncated|locale-corrected|truncat|retract|\(was |was \d"
    r"|->|corrected to|not the \d|rather than \d|supersedes",
    re.I,
)


def parse_section_0(lines):
    """Return (current_values, superseded_values) declared by section 0."""
    current, superseded = set(), set()
    inside = False
    for line in lines:
        if line.startswith("## 0."):
            inside = True
            continue
        if inside and line.startswith("## 1."):
            break
        if inside and line.startswith("|"):
            cols = [c.strip() for c in line.strip("|").split("|")]
            if len(cols) >= 4:
                for m in re.finditer(r"(?<![\d.])(\d+\.\d+)(?![\d])", cols[1]):
                    current.add(m.group(1))
                for m in re.finditer(r"(?<![\d.])(\d+\.\d+)(?![\d])", cols[3]):
                    if float(m.group(1)) >= 1.0:
                        superseded.add(m.group(1))
    return current, superseded


# Match the DIRECTIVE, not a whole comment — the marker in the report carries
# an explanation after it, and requiring the exact closing "-->" meant the
# guard silently never fired.
HISTORICAL = "rss-map-check: historical"


def skippable_lines(lines):
    """Line indices the checker must not judge.

    Three kinds: section 0 (whose whole job is to LIST superseded values), a
    section carrying a SUPERSEDED banner, and a section explicitly marked
    HISTORICAL because its subject IS a correction — those cite old values
    throughout by design, and guessing at that from prose produced a 78%
    false-positive rate. The report declares the intent; the checker obeys it.
    """
    # Track the TOP-LEVEL (##) section only. Keying on any heading meant a
    # "###" subheading inside a banner-marked section reset the tracker, so
    # everything after the first subheading stopped being skipped — which is
    # why banner'd sections were still producing candidates.
    banner, cur = {}, None
    for line in lines:
        if re.match(r"^## ", line):
            cur = line
            banner[cur] = False
        if cur and (("SUPERSEDED" in line and line.lstrip().startswith(">"))
                    or HISTORICAL in line):
            banner[cur] = True
    skip, cur = set(), None
    for i, line in enumerate(lines):
        if re.match(r"^## ", line):
            cur = line
        if cur and (banner.get(cur) or cur.startswith("## 0.") or "SUPERSEDED" in cur):
            skip.add(i)
    return skip


def main():
    path = sys.argv[1] if len(sys.argv) > 1 else DEFAULT
    try:
        lines = open(path).read().split("\n")
    except OSError as e:
        print(f"error: {e}", file=sys.stderr)
        return 2

    current, superseded = parse_section_0(lines)
    if not superseded:
        print("error: section 0 declared no superseded values — has its table moved?",
              file=sys.stderr)
        return 2
    # A value that is BOTH current and superseded somewhere cannot be judged
    # by number alone; excluding it avoids flagging every correct use.
    ambiguous = current & superseded
    checkable = superseded - ambiguous
    skip = skippable_lines(lines)

    hits = []
    for i, line in enumerate(lines):
        if i in skip or line.lstrip().startswith(">") or RETRACTION.search(line):
            continue
        for v in checkable:
            if re.search(r"(?<![\d.])" + re.escape(v) + r"(?![\d])", line):
                hits.append((i + 1, v, line.strip()[:96]))

    print(f"section 0 declares {len(current)} current / {len(superseded)} superseded values")
    if ambiguous:
        print(f"  {len(ambiguous)} appear as both and are not checkable by number: "
              f"{sorted(ambiguous, key=float)}")
    print(f"checked {len(checkable)} values against {len(lines)} lines "
          f"({len(skip)} skipped as section 0 / superseded sections)\n")

    if not hits:
        print("no candidates — either the report is clean or §0's table moved.")
        return 0
    print(f"{len(hits)} CANDIDATE(S) — review each; most will be legitimate:\n")
    for ln, v, text in hits:
        print(f"  line {ln:5d}  [{v}]  {text}")
    print("\nFor each: is this asserting the old value as CURRENT (fix it), or\n"
          "quoting it as history / using the same digits for a different quantity\n"
          "(leave it)? Exit code is always 0 — see the module docstring for why.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
