#!/usr/bin/env python3
"""audit_compare.py: two phone-fit audits side by side, per app and orientation (standard library only).

    audit_compare.py BEFORE AFTER

BEFORE and AFTER are the folders two runs of scripts/ci/qa_android_audit.sh produced (or anything above their
`audit/<App>-e2e-android-x86_64/layout-audit.json` files). Per app and orientation it prints the count of every list of
scripts/ci/layout_audit.py before and after, and the boxes that are new and the ones that are gone (a box is told by its
node, its text and its size, so a box that only moved down the page by a few dp is the same box).
"""
import json
import sys
from pathlib import Path

LISTS = (("edge_right", "boxes over the right edge (outermost)", lambda a: a["edge_right"]["outer"], lambda a: a["edge_right"]["top"]),
         ("edge_left", "boxes over the left edge (outermost)", lambda a: a["edge_left"]["outer"], lambda a: a["edge_left"]["top"]),
         ("bars_tappable", "tappable boxes under a bar", lambda a: a["under_bars"]["tappable"]["count"], lambda a: a["under_bars"]["tappable"]["top"]),
         ("bars_text", "text boxes under a bar", lambda a: a["under_bars"]["text"]["count"], lambda a: a["under_bars"]["text"]["top"]),
         ("small", "tappable nodes under 48 dp", lambda a: a["small_targets"]["small"], lambda a: a["small_targets"]["top"]),
         ("cut_text", "text boxes crossing an edge", lambda a: a["cut_text"]["count"], lambda a: a["cut_text"]["top"]),
         ("sideways", "scroll containers wider than they are", lambda a: a["sideways_scroll"]["count"], lambda a: a["sideways_scroll"]["top"]))


def app_name(path):
    return path.parent.name.replace("-e2e-android-x86_64", "")


def load_audits(root):
    """{app: {orientation: audit result}} of every layout-audit.json under `root`."""
    found = {}
    for f in sorted(Path(root).rglob("layout-audit.json")):
        try:
            audits = json.loads(f.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            continue
        for a in audits if isinstance(audits, list) else []:
            if a and "error" not in a:
                found.setdefault(app_name(f), {})[a["orientation"]] = a
    return found


def box_key(entry):
    """A box is told by what it is, not where on the page it sits: the node, its text and its size to the dp."""
    return (entry.get("node", ""), entry.get("text", ""), round(entry.get("w", 0.0)), round(entry.get("h", 0.0)))


def diff_lists(before, after):
    """-> (gone, new): the boxes of `before` that `after` no longer has, and the other way round (a count per key is kept)."""
    count_b, count_a = {}, {}
    for e in before:
        count_b[box_key(e)] = count_b.get(box_key(e), 0) + 1
    for e in after:
        count_a[box_key(e)] = count_a.get(box_key(e), 0) + 1
    gone = [(k, n - count_a.get(k, 0)) for k, n in count_b.items() if n > count_a.get(k, 0)]
    new = [(k, n - count_b.get(k, 0)) for k, n in count_a.items() if n > count_b.get(k, 0)]
    return gone, new


def _fmt(key):
    node, text, w, h = key
    return f"{node}" + (f" {text!r}" if text else "") + f" {w}x{h}"


def compare(before, after, show=12):
    """The Markdown of the comparison of two load_audits() results."""
    out = ["# Phone-fit audit, before and after", ""]
    for app in sorted(set(before) | set(after)):
        out += [f"## {app}", ""]
        b_app, a_app = before.get(app, {}), after.get(app, {})
        if not b_app or not a_app:
            out += [f"only in {'before' if b_app else 'after'}: nothing to compare", ""]
            continue
        for orient in ("portrait", "landscape"):
            b, a = b_app.get(orient), a_app.get(orient)
            if not b or not a:
                continue
            out += [f"### {orient}", "", "| list | before | after |", "|---|---|---|"]
            for _key, title, count, _items in LISTS:
                out.append(f"| {title} | {count(b)} | {count(a)} |")
            out.append("")
            for key, title, _count, items in LISTS:
                gone, new = diff_lists(items(b), items(a))
                if not gone and not new:
                    continue
                out.append(f"{title}:")
                out += [f"- gone: {_fmt(k)}" + (f" (x{n})" if n > 1 else "") for k, n in gone[:show]]
                out += [f"- new: {_fmt(k)}" + (f" (x{n})" if n > 1 else "") for k, n in new[:show]]
                hidden = max(0, len(gone) - show) + max(0, len(new) - show)
                if hidden:
                    out.append(f"- ... {hidden} more")
                out.append("")
    return "\n".join(out) + "\n"


def main(argv):
    if len(argv) != 2:
        print(__doc__)
        return 2
    before, after = load_audits(argv[0]), load_audits(argv[1])
    if not before or not after:
        print(f"no layout-audit.json under {argv[0] if not before else argv[1]}")
        return 1
    print(compare(before, after))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
