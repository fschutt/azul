#!/usr/bin/env python3
"""The table of the font-miss A/B (scripts/ci/startup_ab.sh): first present of the starts of two builds, A and B, on
one machine, for starts WITHOUT a font cache (miss) and starts WITH one (hit).

    startup_ab_table.py DIR [--json OUT]

DIR holds the files `ab-[<app>-]<A|B>-<miss|hit>-<n>.json` that `startup_measure.py --json` wrote. A miss file has one run; a hit
file has the warm-up run first (it makes the cache: dropped) and then the measured ones. For each block the table gives the
number of starts, the median, min and max of the first present, how the font cache answered (`fonts:cache`: hit, miss,
rebuilt) and what the first layout did about the font scan (`fonts:first_layout_wait`: skipped, waited). Std only."""
import json
import os
import re
import statistics
import sys

FILE = re.compile(r"^ab-(?:([A-Za-z0-9_]+)-)?(A|B)-(miss|hit)-(\d+)\.json$")


def first_word(text):
    """The first word of a mark's detail ('hit, 812 faces, ...' -> 'hit', 'skipped: ...' -> 'skipped')."""
    text = (text or "").strip()
    return re.split(r"[ ,:(]", text, maxsplit=1)[0] if text else "-"


def mark_detail(run, name):
    for mark in (run.get("timeline") or {}).get("marks", []):
        if mark.get("name") == name:
            return mark.get("detail", "")
    return None


def collect(directory):
    """{('AzDrive', 'A', 'miss'): [run, ...], ...} ('' for a file with no app in its name): every run of every file of the
    directory, the warm-up of a hit file left out."""
    found = {}
    for name in sorted(os.listdir(directory), key=lambda n: (n.split("-")[:3], int(re.sub(r"\D", "", n.split("-")[-1]) or 0))):
        match = FILE.match(name)
        if not match:
            continue
        app, which, kind, _ = match.groups()
        with open(os.path.join(directory, name)) as handle:
            runs = json.load(handle).get("runs", [])
        if kind == "hit":
            runs = runs[1:]
        found.setdefault((app or "", which, kind), []).extend(runs)
    return found


def summarize(runs):
    """The numbers of one block."""
    times = [r["timeline"]["first_present_ms"] for r in runs if (r.get("timeline") or {}).get("first_present_ms") is not None]
    cache = {}
    waits = {}
    for run in runs:
        word = first_word(mark_detail(run, "fonts:cache"))
        cache[word] = cache.get(word, 0) + 1
        wait = first_word(mark_detail(run, "fonts:first_layout_wait"))
        waits[wait] = waits.get(wait, 0) + 1
    return {
        "starts": len(runs),
        "with_a_timeline": len(times),
        "median_ms": statistics.median(times) if times else None,
        "min_ms": min(times) if times else None,
        "max_ms": max(times) if times else None,
        "font_cache": cache,
        "first_layout_wait": waits,
    }


def table(summary):
    apps = sorted({app for (app, _, _) in summary})
    with_apps = apps != [""]
    head = "| app | build |" if with_apps else "| build |"
    lines = [
        f"{head} font cache | starts | first present median ms | min | max | font cache answered | first layout's wait |",
        ("|---|---|" if with_apps else "|---|") + "---|---|---|---|---|---|---|",
    ]
    for app in apps:
        for which in ("A", "B"):
            for kind in ("miss", "hit"):
                block = summary.get((app, which, kind))
                if block is None:
                    continue
                fmt = lambda v: "-" if v is None else f"{v:.0f}"
                lead = f"| {app} | {which} |" if with_apps else f"| {which} |"
                lines.append(
                    f"{lead} {kind} | {block['with_a_timeline']}/{block['starts']} | {fmt(block['median_ms'])} | "
                    f"{fmt(block['min_ms'])} | {fmt(block['max_ms'])} | "
                    f"{', '.join(f'{k} {v}' for k, v in sorted(block['font_cache'].items()))} | "
                    f"{', '.join(f'{k} {v}' for k, v in sorted(block['first_layout_wait'].items()))} |"
                )
    return "\n".join(lines)


def main(argv):
    if len(argv) < 2:
        print(__doc__)
        return 2
    directory = argv[1]
    found = collect(directory)
    summary = {key: summarize(runs) for key, runs in found.items()}
    print(table(summary))
    if "--json" in argv:
        out = argv[argv.index("--json") + 1]
        with open(out, "w") as handle:
            json.dump({"-".join(part for part in key if part): v for key, v in summary.items()}, handle, indent=2)
    return 0 if summary else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
