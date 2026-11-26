#!/usr/bin/env python3
"""Tests of scripts/ci/layout_audit.py: pure functions on small dumps (python3 -I scripts/ci/test_layout_audit.py)."""
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import layout_audit as la  # noqa: E402

SCALE = 2.625

# what `dumpsys window | grep ...` printed on the API 34 emulator (the app's bounds are the screen without the bars)
WINDOW_DUMP = """\
  overrideConfig={1.0 310mcc260mnc [en_US] ldltr sw411dp w411dp h818dp 420dpi nrml long port finger qwerty/v/v -nav/h winConfig={ mBounds=Rect(0, 0 - 1080, 2400) mAppBounds=Rect(0, 128 - 1080, 2274) mMaxBounds=Rect(0, 0 - 1080, 2400) mDisplayRotation=ROTATION_0 mWindowingMode=fullscreen} as.3 s.977 fontWeightAdjustment=0}
      overrideConfig={0.0 ?mcc0mnc winConfig={ mBounds=Rect(0, 0 - 0, 0) mAppBounds=null mMaxBounds=Rect(0, 0 - 0, 0) mDisplayRotation=undefined} ?fontWeightAdjustment}
      InsetsSource id=3 type=statusBars frame=[0,0][1080,128] visible=true flags=
      InsetsSource id=4 type=navigationBars frame=[0,2274][1080,2400] visible=true flags=
      InsetsSource id=5 type=ime frame=[0,2400][1080,2400] visible=false flags=
      InsetsSource id=6 type=displayCutout frame=[0,0][0,0] visible=true flags=
"""


def node(i, tag, x, y, w, h, node_id=None, ident=None, classes=()):
    return {"node_id": i if node_id is None else node_id, "tag": tag, "id": ident, "classes": list(classes), "rect": {"x": x, "y": y, "width": w, "height": h}}


def a11y(i, dom, role, name, x, y, w, h, clickable=False, parent=None, clips=False, hidden=False, disabled=False):
    return {"id": i, "node_id": dom, "dom_id": 0, "role": role, "name": name, "clickable": clickable, "focusable": clickable, "hidden": hidden,
            "disabled": disabled, "clips_children": clips, "parent": parent, "children": [],
            "bounds": [x * SCALE, y * SCALE, (x + w) * SCALE, (y + h) * SCALE]}


def answer(nodes):
    return {"status": "ok", "data": {"type": "x", "value": {"nodes": nodes}}}


STATE = {"status": "ok", "window_state": {"logical_width": 411.4, "logical_height": 914.3, "hidpi_factor": SCALE}}


class BarsTests(unittest.TestCase):
    def test_the_inset_sources_are_the_bands_and_the_zero_ones_are_not(self):
        got = la.parse_system_bars(WINDOW_DUMP)
        self.assertEqual(got["screen"], (1080, 2400))
        self.assertEqual(got["app"], (0, 128, 1080, 2274))
        self.assertEqual([(b["name"], b["rect"]) for b in got["bands"]],
                         [("statusBars", (0, 0, 1080, 128)), ("navigationBars", (0, 2274, 1080, 2400))])   # the cutout is empty, the ime is not visible

    def test_without_inset_sources_the_bands_are_what_lies_outside_the_app_bounds(self):
        got = la.parse_system_bars("  winConfig={ mBounds=Rect(0, 0 - 1080, 2400) mAppBounds=Rect(0, 128 - 1080, 2274) mMaxBounds=Rect(0, 0 - 1080, 2400) }")
        self.assertEqual([(b["name"], b["rect"]) for b in got["bands"]], [("top", (0, 0, 1080, 128)), ("bottom", (0, 2274, 1080, 2400))])

    def test_a_dump_without_anything_gives_no_bands(self):
        self.assertEqual(la.parse_system_bars("")["bands"], [])


class AuditTests(unittest.TestCase):
    def setUp(self):
        self.bars = la.parse_system_bars(WINDOW_DUMP)

    def run_audit(self, layout, tree):
        return la.audit(answer(layout), answer(tree), STATE, self.bars, "portrait")

    def test_a_box_over_the_right_edge_is_listed_once_with_its_text(self):
        layout = [node(0, "body", 0, 0, 411.4, 900), node(1, "div", 10, 100, 500, 40, ident="bar"), node(2, "button", 20, 100, 200, 40),
                  node(3, "p", 0, 300, 411.4, 20)]
        tree = [a11y(1, 1, "Group", None, 10, 100, 500, 40), a11y(2, 2, "Button", "Wide", 20, 100, 200, 40, clickable=True, parent=1)]
        r = self.run_audit(layout, tree)
        self.assertEqual(r["edge_right"]["count"], 1)
        self.assertEqual(r["edge_right"]["top"][0]["node"], "div#bar")
        self.assertEqual(r["edge_left"]["count"], 0)

    def test_a_box_exactly_at_the_edge_is_not_over_it(self):
        r = self.run_audit([node(0, "div", 0, 0, 411.4, 50), node(1, "div", 0, 60, 412.0, 50)], [])
        self.assertEqual(r["edge_right"]["count"], 0)          # 0.6 dp of rounding

    def test_children_inside_an_offender_are_counted_but_not_listed(self):
        layout = [node(0, "div", 10, 100, 500, 100), node(1, "div", 20, 110, 480, 20), node(2, "div", 30, 140, 470, 20)]
        r = self.run_audit(layout, [])
        self.assertEqual((r["edge_right"]["count"], r["edge_right"]["outer"]), (3, 1))

    def test_a_tappable_node_under_the_status_bar_is_listed_and_a_node_below_it_is_not(self):
        tree = [a11y(1, 1, "Button", "Menu", 10, 10, 100, 50, clickable=True), a11y(2, 2, "Button", "Fine", 10, 80, 100, 50, clickable=True),
                a11y(3, 3, "Label", "Clock", 300, 5, 60, 20), a11y(4, 4, "Group", None, 0, 0, 411.4, 30)]
        r = self.run_audit([node(1, "button", 10, 10, 100, 50), node(2, "button", 10, 80, 100, 50)], tree)
        under = r["under_bars"]
        self.assertEqual([e["text"] for e in under["tappable"]["top"]], ["Menu"])
        self.assertEqual([e["text"] for e in under["text"]["top"]], ["Clock"])
        self.assertEqual(under["other_boxes"], 1)                # the empty group behind the bar

    def test_the_navigation_bar_catches_a_box_that_reaches_into_it(self):
        bottom = 2274 / SCALE
        tree = [a11y(1, 1, "Button", "Save", 10, bottom - 20, 100, 50, clickable=True)]
        r = self.run_audit([node(1, "button", 10, bottom - 20, 100, 50)], tree)
        self.assertEqual(len(r["under_bars"]["tappable"]["top"]), 1)
        self.assertIn("navigationBars", r["under_bars"]["tappable"]["top"][0]["why"])

    def test_a_touch_target_under_48_dp_is_listed_unless_its_parent_is_large_and_tappable(self):
        tree = [a11y(1, 1, "Group", None, 0, 100, 411, 60, clickable=True), a11y(2, 2, "Button", "Inside", 10, 110, 30, 30, clickable=True, parent=1),
                a11y(3, 3, "CheckBox", "Alone", 10, 300, 14, 14, clickable=True), a11y(4, 4, "Button", "Big", 10, 400, 120, 48, clickable=True),
                a11y(5, 5, "Button", "Off", 10, 500, 20, 20, clickable=True, hidden=True), a11y(6, 6, "Button", "Gone", 10, 520, 20, 20, clickable=True, disabled=True)]
        r = self.run_audit([node(i, "x", 0, 0, 1, 1) for i in range(1, 7)], tree)
        small = r["small_targets"]
        self.assertEqual([e["text"] for e in small["top"]], ["Alone"])
        self.assertEqual((small["tappable"], small["small"], small["inside_a_large_parent"]), (4, 1, 1))     # Group, Inside, Alone, Big
        self.assertEqual(small["by_role"], {"CheckBox": 1})

    def test_a_text_that_crosses_the_window_or_a_clipping_parent_is_listed(self):
        tree = [a11y(1, 1, "Group", None, 0, 100, 300, 40, clips=True), a11y(2, 2, "Label", "Long label", 250, 100, 100, 20, parent=1),
                a11y(3, 3, "Paragraph", "Over the edge", 380, 200, 100, 20), a11y(4, 4, "Label", "Fits", 10, 300, 100, 20)]
        r = self.run_audit([node(i, "p", 0, 0, 1, 1) for i in range(1, 5)], tree)
        why = {e["text"]: e["why"] for e in r["cut_text"]["top"]}
        self.assertEqual(sorted(why), ["Long label", "Over the edge"])
        self.assertIn("clipping parent", why["Long label"])
        self.assertIn("window's right edge", why["Over the edge"])

    def test_without_a_layout_dump_it_says_so_instead_of_an_empty_list(self):
        r = la.audit(None, None, STATE, self.bars, "portrait")
        self.assertIn("error", r)
        self.assertIn("nothing to audit", la.summary_line(r))

    def test_the_report_names_what_it_cannot_measure(self):
        r = self.run_audit([node(0, "div", 0, 0, 100, 100)], [])
        text = la.report_markdown([r], "AzDemo")
        self.assertIn("# Layout audit: AzDemo", text)
        self.assertIn("Not measured: a text clipped inside its own box", text)
        self.assertIn("portrait: window 411.4 x 914.3 dp", text)


if __name__ == "__main__":
    unittest.main(verbosity=1)
