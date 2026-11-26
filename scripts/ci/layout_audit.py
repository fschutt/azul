#!/usr/bin/env python3
"""layout_audit.py: what the debug server's layout and accessibility dumps say about how an app fits a phone (standard library only).

    layout_audit.py DIR [--orientation portrait|landscape ...]     re-reads the dumps mobile_e2e.py kept in DIR/logs and prints the report

The inputs are the dumps of ONE moment of ONE orientation (mobile_e2e.py writes them as logs/<name>-<orientation>.*):
    nodes-layout-<o>.json     `get_all_nodes_layout`: every DOM node's box in logical pixels (dp on Android)
    a11y-tree-<o>.json        `get_accessibility_tree`: role, name, clickable, bounds in DEVICE pixels, parent chain
    get_state-<o>.json        `get_state`: the window's logical size, its physical size, the pixel ratio
    dumpsys-window-<o>.txt    Android's `dumpsys window`: the status and navigation bars' frames, the app's bounds

What it lists, per orientation (every list names the node: tag#id.class, the role and the text the accessibility tree gives it, the box in dp):
    edge          boxes that cross the window's right or left edge (the page does not fit the width; scrolled strips show up too)
    bars          boxes of tappable or text nodes that lie under a system bar (a button no finger can reach, a text nobody can read)
    small         tappable nodes smaller than 48 dp in either direction (Android's touch target guideline), without those inside a
                  large enough tappable parent (the finger lands on the parent anyway)
    cut text      text-bearing boxes that cross the window's edge or the edge of an ancestor that clips its children
What it can NOT say: a text clipped inside its own box. The debug server reports boxes, not glyph extents; it is listed under
`not_measured` in every report so nobody reads an empty list as "nothing is clipped".
"""
import json
import re
import sys
from pathlib import Path

MIN_TARGET_DP = 48.0
SLACK_DP = 1.0                       # a box this far over an edge is rounding, not overflow
MIN_UNDER_BAR_DP = 2.0               # a box this deep under a bar counts
INTERACTIVE_ROLES = {"Button", "RadioButton", "CheckBox", "Switch", "Slider", "Link", "Tab", "ComboBox", "SpinButton",
                     "TextInput", "MultilineTextInput", "SearchInput", "MenuItem", "MenuItemCheckBox", "MenuItemRadio", "ToggleButton"}
TEXT_ROLES = {"Label", "Paragraph", "Heading", "StaticText", "Button", "Link", "Tab", "ListItem", "RadioButton", "CheckBox",
              "MenuItem", "Switch", "ComboBox"}
NOT_MEASURED = ["a text clipped inside its own box (a long word in a narrow button, an ellipsis): the debug server reports boxes, "
                "not glyph extents, so this needs an engine op that returns each text's laid-out size"]
LIST_LIMIT = 40


# ---- the system's own account of the bars -------------------------------------------------------------------------------------

INSET_LINE = re.compile(r"type=(statusBars|navigationBars|displayCutout)\b.*?frame=\[(-?\d+),(-?\d+)\]\[(-?\d+),(-?\d+)\](?:.*?visible=(true|false))?")
APP_BOUNDS = re.compile(r"mAppBounds=Rect\((-?\d+), (-?\d+) - (-?\d+), (-?\d+)\)")
MAX_BOUNDS = re.compile(r"mMaxBounds=Rect\((-?\d+), (-?\d+) - (-?\d+), (-?\d+)\)")


def parse_system_bars(text):
    """`dumpsys window` -> {"screen": (w, h) | None, "app": (l, t, r, b) | None, "bands": [{"name", "rect": (l, t, r, b)}]} in device
    pixels. The bands are the status and navigation bars the window system lists as visible inset sources; when it lists none
    (an older API), they are what lies outside the app's bounds (the system's `mAppBounds`) inside the screen."""
    screen = app = None
    for m in MAX_BOUNDS.finditer(text):
        l, t, r, b = (int(v) for v in m.groups())
        if r - l > 0 and b - t > 0:
            screen = (r - l, b - t)
            break
    for m in APP_BOUNDS.finditer(text):
        l, t, r, b = (int(v) for v in m.groups())
        if r - l > 0 and b - t > 0:
            app = (l, t, r, b)
            break
    bands, seen = [], set()
    for line in text.splitlines():
        m = INSET_LINE.search(line)
        if not m or m.group(6) == "false":
            continue
        name = m.group(1)
        rect = tuple(int(v) for v in m.groups()[1:5])
        if rect[2] - rect[0] <= 0 or rect[3] - rect[1] <= 0 or (name, rect) in seen:
            continue
        seen.add((name, rect))
        bands.append({"name": name, "rect": rect})
    if not bands and screen and app:
        w, h = screen
        l, t, r, b = app
        for name, rect in (("top", (0, 0, w, t)), ("bottom", (0, b, w, h)), ("left", (0, t, l, b)), ("right", (r, t, w, b))):
            if rect[2] - rect[0] > 0 and rect[3] - rect[1] > 0:
                bands.append({"name": name, "rect": rect})
    return {"screen": screen, "app": app, "bands": bands}


# ---- reading the dumps ----------------------------------------------------------------------------------------------------------

def layout_nodes(answer):
    """The node dicts of a `get_all_nodes_layout` answer (empty when it is not one)."""
    data = (answer or {}).get("data") or {}
    value = data.get("value") if isinstance(data, dict) else None
    return (value or {}).get("nodes") or []


def a11y_nodes(answer):
    data = (answer or {}).get("data") or {}
    value = data.get("value") if isinstance(data, dict) else None
    return (value or {}).get("nodes") or []


def window_state(answer):
    return (answer or {}).get("window_state") or {}


def _name(node):
    tag = node.get("tag") or "?"
    ident = f"#{node['id']}" if node.get("id") else ""
    classes = "".join("." + c for c in (node.get("classes") or [])[:3])
    return f"{tag}{ident}{classes}"


def _rect_dp(r):
    return (r["x"], r["y"], r["x"] + r["width"], r["y"] + r["height"])


def _inside(inner, outer, slack=SLACK_DP):
    return (inner[0] >= outer[0] - slack and inner[1] >= outer[1] - slack and inner[2] <= outer[2] + slack and inner[3] <= outer[3] + slack)


def _entry(layout, a11y, rect, why):
    l, t, r, b = rect
    out = {"node": _name(layout) if layout else (a11y or {}).get("html_tag") or (a11y or {}).get("role") or "?",
           "x": round(l, 1), "y": round(t, 1), "w": round(r - l, 1), "h": round(b - t, 1), "why": why}
    if a11y:
        out["role"] = a11y.get("role")
        if a11y.get("name"):
            out["text"] = str(a11y["name"])[:60]
    return out


def _outer(entries):
    """The entries that are not inside another entry's box: the offender, not every node inside it."""
    boxes = [(e, (e["x"], e["y"], e["x"] + e["w"], e["y"] + e["h"])) for e in entries]
    keep = []
    for e, box in boxes:
        area = (box[2] - box[0]) * (box[3] - box[1])
        contained = any(o is not e and _inside(box, ob) and (ob[2] - ob[0]) * (ob[3] - ob[1]) > area + 1.0 for o, ob in boxes)
        if not contained:
            keep.append(e)
    return keep


# ---- the audit --------------------------------------------------------------------------------------------------------------------

def audit(layout_answer, a11y_answer, state_answer, bars, orientation):
    """The lists described in the module's docstring for one moment. `bars` is parse_system_bars()'s answer (or None)."""
    nodes = layout_nodes(layout_answer)
    a11y = a11y_nodes(a11y_answer)
    state = window_state(state_answer)
    scale = float(state.get("hidpi_factor") or 0.0) or 1.0
    width_dp = float(state.get("logical_width") or 0.0)
    height_dp = float(state.get("logical_height") or 0.0)
    result = {"orientation": orientation, "window_dp": [round(width_dp, 1), round(height_dp, 1)], "pixels_per_dp": scale,
              "nodes": len(nodes), "a11y_nodes": len(a11y), "not_measured": NOT_MEASURED}
    if not nodes or width_dp <= 0:
        result["error"] = "no layout dump or no window size: nothing to audit"
        return result

    by_dom = {n.get("node_id"): n for n in a11y if n.get("node_id") is not None and n.get("dom_id") in (0, None)}
    by_id = {n.get("id"): n for n in a11y}
    layout_by_id = {n.get("node_id"): n for n in nodes}

    # -- the window's edges (every DOM node of the layout)
    right, left = [], []
    for n in nodes:
        r = n.get("rect")
        if not r or r.get("width", 0) <= 0 or r.get("height", 0) <= 0:
            continue
        box = _rect_dp(r)
        info = by_dom.get(n.get("node_id"))
        if info and info.get("hidden"):
            continue
        if box[0] < width_dp and box[2] > width_dp + SLACK_DP:
            right.append(_entry(n, info, box, f"right edge at {width_dp:.0f}: box ends at {box[2]:.0f}"))
        if box[2] > 0 and box[0] < -SLACK_DP:
            left.append(_entry(n, info, box, f"left edge at 0: box starts at {box[0]:.0f}"))
    for name, found in (("right", right), ("left", left)):
        found.sort(key=lambda e: (-e["w"], e["y"]))
        outer = _outer(found)
        result[f"edge_{name}"] = {"count": len(found), "outer": len(outer), "top": outer[:LIST_LIMIT]}

    # -- under the bars (the a11y tree: device pixels)
    bands = (bars or {}).get("bands") or []
    under = {"tappable": [], "text": [], "boxes": 0}
    for a in a11y:
        b = a.get("bounds")
        if not b or a.get("hidden") or a.get("dom_id") not in (0, None):
            continue
        box = tuple(v / scale for v in b)
        if box[2] - box[0] <= 0 or box[3] - box[1] <= 0:
            continue
        role = a.get("role")
        tappable = bool(a.get("clickable")) or role in INTERACTIVE_ROLES
        texty = bool(a.get("name")) and role in TEXT_ROLES
        for band in bands:
            bl, bt, br, bb = band["rect"]
            dx = (min(b[2], br) - max(b[0], bl)) / scale          # the overlap in dp, both boxes in device pixels
            dy = (min(b[3], bb) - max(b[1], bt)) / scale
            if dx >= MIN_UNDER_BAR_DP and dy >= MIN_UNDER_BAR_DP:
                depth = dy if (br - bl) >= (bb - bt) else dx          # a bar along the top or bottom is entered from the side of its height
                entry = _entry(layout_by_id.get(a.get("node_id")), a, box, f"{depth:.0f} dp under the {band['name']} bar")
                if tappable:
                    under["tappable"].append(entry)
                elif texty:
                    under["text"].append(entry)
                else:
                    under["boxes"] += 1
                break
    result["under_bars"] = {"bands_px": [{"name": b["name"], "rect": list(b["rect"])} for b in bands],
                            "tappable": {"count": len(under["tappable"]), "top": under["tappable"][:LIST_LIMIT]},
                            "text": {"count": len(under["text"]), "top": under["text"][:LIST_LIMIT]},
                            "other_boxes": under["boxes"]}

    # -- touch targets
    small, tappable_count, covered, unknown = [], 0, 0, 0
    for a in a11y:
        role = a.get("role")
        if a.get("hidden") or a.get("disabled") or a.get("dom_id") not in (0, None):
            continue
        if not (a.get("clickable") or role in INTERACTIVE_ROLES):
            continue
        b = a.get("bounds")
        if not b:
            unknown += 1
            continue
        tappable_count += 1
        box = tuple(v / scale for v in b)
        w, h = box[2] - box[0], box[3] - box[1]
        if w <= 0 or h <= 0 or min(w, h) >= MIN_TARGET_DP:
            continue
        # a larger tappable ancestor takes the finger anyway
        parent, big_parent = by_id.get(a.get("parent")), False
        while parent is not None:
            pb = parent.get("bounds")
            if pb and (parent.get("clickable") or parent.get("role") in INTERACTIVE_ROLES):
                if min((pb[2] - pb[0]) / scale, (pb[3] - pb[1]) / scale) >= MIN_TARGET_DP:
                    big_parent = True
                    break
            parent = by_id.get(parent.get("parent"))
        if big_parent:
            covered += 1
            continue
        small.append(_entry(layout_by_id.get(a.get("node_id")), a, box, f"{w:.0f} x {h:.0f} dp"))
    small.sort(key=lambda e: (min(e["w"], e["h"]), e["y"]))
    roles = {}
    for e in small:
        roles[e.get("role") or "?"] = roles.get(e.get("role") or "?", 0) + 1
    result["small_targets"] = {"tappable": tappable_count, "no_bounds": unknown, "small": len(small), "inside_a_large_parent": covered,
                               "by_role": roles, "top": small[:LIST_LIMIT]}

    # -- text-bearing boxes that cross the window or a clipping ancestor
    cut = []
    for a in a11y:
        b = a.get("bounds")
        if not b or a.get("hidden") or not a.get("name") or a.get("role") not in TEXT_ROLES or a.get("dom_id") not in (0, None):
            continue
        box = tuple(v / scale for v in b)
        if box[2] - box[0] <= 0:
            continue
        why = None
        if box[2] > width_dp + SLACK_DP and box[0] < width_dp:
            why = f"crosses the window's right edge ({box[2]:.0f} > {width_dp:.0f})"
        elif box[0] < -SLACK_DP and box[2] > 0:
            why = f"crosses the window's left edge ({box[0]:.0f} < 0)"
        else:
            parent = by_id.get(a.get("parent"))
            while parent is not None and why is None:
                pb = parent.get("bounds")
                if parent.get("clips_children") and pb:
                    pbox = tuple(v / scale for v in pb)
                    if box[2] > pbox[2] + SLACK_DP and box[0] < pbox[2]:
                        why = f"crosses the right edge of a clipping parent ({box[2]:.0f} > {pbox[2]:.0f})"
                    elif box[0] < pbox[0] - SLACK_DP and box[2] > pbox[0]:
                        why = f"crosses the left edge of a clipping parent ({box[0]:.0f} < {pbox[0]:.0f})"
                parent = by_id.get(parent.get("parent"))
        if why:
            cut.append(_entry(layout_by_id.get(a.get("node_id")), a, box, why))
    cut.sort(key=lambda e: (e["y"], e["x"]))
    result["cut_text"] = {"count": len(cut), "top": cut[:LIST_LIMIT]}
    return result


# ---- the report -------------------------------------------------------------------------------------------------------------------

def _table(entries, columns):
    if not entries:
        return ["(none)", ""]
    lines = ["| " + " | ".join(columns) + " |", "|" + "---|" * len(columns)]
    for e in entries:
        text = e.get("text", "")
        cells = {"node": e["node"], "role": e.get("role") or "", "text": text.replace("|", "/"), "x": e["x"], "y": e["y"], "w": e["w"], "h": e["h"], "why": e["why"]}
        lines.append("| " + " | ".join(str(cells[c]) for c in columns) + " |")
    return lines + [""]


def report_markdown(audits, label=""):
    """One Markdown document for the audits of the orientations (a list of audit() results)."""
    out = [f"# Layout audit{': ' + label if label else ''}", ""]
    for a in audits:
        out += [f"## {a['orientation']}: window {a['window_dp'][0]} x {a['window_dp'][1]} dp ({a['pixels_per_dp']} device px per dp), "
                f"{a['nodes']} layout nodes, {a['a11y_nodes']} accessibility nodes", ""]
        if a.get("error"):
            out += [a["error"], ""]
            continue
        for side in ("right", "left"):
            e = a[f"edge_{side}"]
            out += [f"### Boxes crossing the {side} edge: {e['count']} ({e['outer']} outermost)", ""]
            out += _table(e["top"], ["node", "role", "text", "x", "y", "w", "h", "why"])
        u = a["under_bars"]
        bands = ", ".join(f"{b['name']} {b['rect']}" for b in u["bands_px"]) or "none found"
        out += [f"### Under a system bar (bands in device px: {bands})", "",
                f"tappable: {u['tappable']['count']}, text: {u['text']['count']}, other boxes: {u['other_boxes']}", ""]
        out += _table(u["tappable"]["top"] + u["text"]["top"], ["node", "role", "text", "x", "y", "w", "h", "why"])
        s = a["small_targets"]
        out += [f"### Tappable nodes smaller than {MIN_TARGET_DP:.0f} dp: {s['small']} of {s['tappable']} "
                f"({s['inside_a_large_parent']} more sit inside a large tappable parent; {s['no_bounds']} have no box)",
                "", "by role: " + (", ".join(f"{r} {n}" for r, n in sorted(s["by_role"].items(), key=lambda kv: -kv[1])) or "none"), ""]
        out += _table(s["top"], ["node", "role", "text", "x", "y", "w", "h"])
        c = a["cut_text"]
        out += [f"### Text-bearing boxes that cross an edge: {c['count']}", ""]
        out += _table(c["top"], ["node", "role", "text", "x", "y", "w", "h", "why"])
        out += ["Not measured: " + "; ".join(a["not_measured"]), ""]
    return "\n".join(out) + "\n"


def summary_line(a):
    """One line for a step's note: the counts of the lists."""
    if a.get("error"):
        return f"{a['orientation']}: {a['error']}"
    u = a["under_bars"]
    return (f"{a['orientation']} {a['window_dp'][0]:.0f}x{a['window_dp'][1]:.0f} dp: {a['edge_right']['count']} boxes cross the right edge "
            f"({a['edge_right']['outer']} outermost), {a['edge_left']['count']} the left, {u['tappable']['count']} tappable and {u['text']['count']} text boxes "
            f"under a bar, {a['small_targets']['small']} of {a['small_targets']['tappable']} tappable nodes under {MIN_TARGET_DP:.0f} dp, "
            f"{a['cut_text']['count']} text boxes cross an edge")


def load_dir(logs_dir, orientation):
    """The four dumps of one orientation from a logs folder -> audit() input, or None when the layout dump is missing."""
    logs = Path(logs_dir)

    def js(name):
        p = logs / f"{name}-{orientation}.json"
        return json.loads(p.read_text(encoding="utf-8")) if p.exists() else None

    layout = js("nodes-layout")
    if layout is None:
        return None
    dump = logs / f"dumpsys-window-{orientation}.txt"
    bars = parse_system_bars(dump.read_text(encoding="utf-8", errors="replace")) if dump.exists() else None
    return audit(layout, js("a11y-tree"), js("get_state"), bars, orientation)


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    logs = Path(argv[0])
    logs = logs / "logs" if (logs / "logs").is_dir() else logs
    orients = [a for a in argv[1:] if a in ("portrait", "landscape")] or ["portrait", "landscape"]
    audits = [x for x in (load_dir(logs, o) for o in orients) if x]
    if not audits:
        print(f"no nodes-layout-<orientation>.json in {logs}")
        return 1
    print(report_markdown(audits, logs.parent.name))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
