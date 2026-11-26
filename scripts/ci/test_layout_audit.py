#!/usr/bin/env python3
"""Tests of scripts/ci/layout_audit.py: pure functions on small dumps (python3 -I scripts/ci/test_layout_audit.py)."""
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import layout_audit as la  # noqa: E402
import audit_compare as ac  # noqa: E402

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

    def test_a_text_that_is_a_message_id_is_listed(self):
        tree = [a11y(1, 1, "Label", "azmeet-loading-key", 10, 100, 200, 20), a11y(2, 2, "Label", "Loading this device's key...", 10, 130, 200, 20),
                a11y(3, 3, "Button", "menu-file-open", 10, 160, 100, 48, clickable=True), a11y(4, 4, "Label", "e-mail", 10, 220, 50, 20),
                a11y(5, 5, "Label", "Save-as", 10, 250, 50, 20)]
        r = self.run_audit([node(i, "p", 0, 0, 1, 1) for i in range(1, 6)], tree)
        self.assertEqual(sorted(e["text"] for e in r["raw_keys"]["top"]), ["azmeet-loading-key", "menu-file-open"])
        self.assertIn("texts are raw keys", la.summary_line(r))

    def test_a_scroll_container_with_wider_content_is_the_page_that_scrolls_sideways(self):
        scroll = {"status": "ok", "data": {"type": "scroll_states", "value": {"scroll_states": [
            {"node_id": 27, "scroll_x": 0.0, "scroll_y": 4079.0, "content_width": 606.7, "content_height": 14189.0, "container_width": 411.4,
             "container_height": 702.5, "max_scroll_x": 195.3, "max_scroll_y": 13486.0},
            {"node_id": 28, "scroll_x": 0.0, "scroll_y": 0.0, "content_width": 327.4, "content_height": 15.0, "container_width": 327.4,
             "container_height": 15.0, "max_scroll_x": 0.0, "max_scroll_y": 0.0}]}}}
        r = la.audit(answer([node(27, "div", 0, 48, 411.4, 702.5, ident="page")]), answer([]), STATE, self.bars, "portrait", scroll)
        sw = r["sideways_scroll"]
        self.assertEqual((sw["count"], sw["scrollers"]), (1, 2))
        self.assertEqual(sw["top"][0]["node"], "div#page")
        self.assertEqual(sw["top"][0]["max_scroll_x"], 195.3)
        self.assertEqual([o["scroll_y"] for o in sw["offset_at_reading"]], [4079.0])      # the dump was not taken at the top
        self.assertIn("scroll containers are wider", la.summary_line(r))
        self.assertIn("the page scrolls sideways", la.report_markdown([r]))

    def test_without_scroll_states_the_list_is_empty_not_missing(self):
        r = self.run_audit([node(0, "div", 0, 0, 100, 100)], [])
        self.assertEqual(r["sideways_scroll"]["count"], 0)

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


class WordsTests(unittest.TestCase):
    def tree(self, names):
        return answer([a11y(i + 1, i, "Label", n, 0, i * 20, 100, 20) for i, n in enumerate(names)])

    def test_a_language_that_arrived_changes_the_texts_of_the_same_nodes(self):
        en = self.tree(["File", "Edit", "Azul Widget Showcase", "Save"])
        de = self.tree(["Datei", "Bearbeiten", "Azul Widget Showcase", "Speichern"])
        w = la.compare_words(en, de)
        self.assertEqual((w["named"], w["changed"]), (4, 3))
        self.assertEqual(w["sample"][0], ("File", "Datei"))
        self.assertEqual(w["raw_keys_b"], [])

    def test_message_ids_in_the_second_language_are_named(self):
        en = self.tree(["File", "Loading"])
        de = self.tree(["azpaint-file", "azpaint-loading-key"])
        w = la.compare_words(en, de)
        self.assertEqual(w["raw_keys_b"], ["azpaint-file", "azpaint-loading-key"])

    def test_nothing_in_common_is_not_a_change(self):
        w = la.compare_words(answer([]), self.tree(["a"]))
        self.assertEqual((w["named"], w["changed"], w["share"]), (0, 0, 0.0))


class CompareTests(unittest.TestCase):
    def audit_of(self, tree, layout=None):
        bars = la.parse_system_bars(WINDOW_DUMP)
        return la.audit(answer(layout or [node(1, "button", 10, 10, 100, 50)]), answer(tree), STATE, bars, "portrait")

    def test_a_box_is_the_same_box_after_it_moved_down_the_page(self):
        b = [{"node": "button#a", "text": "Save", "x": 10.0, "y": 100.0, "w": 30.0, "h": 30.0}]
        a = [{"node": "button#a", "text": "Save", "x": 10.0, "y": 140.0, "w": 30.4, "h": 30.2}]
        self.assertEqual(ac.diff_lists(b, a), ([], []))

    def test_what_is_gone_and_what_is_new_are_told_apart(self):
        b = [{"node": "button#a", "text": "Save", "w": 30.0, "h": 30.0}, {"node": "p", "text": "Old", "w": 50.0, "h": 10.0}]
        a = [{"node": "button#a", "text": "Save", "w": 48.0, "h": 48.0}, {"node": "p", "text": "Old", "w": 50.0, "h": 10.0}]
        gone, new = ac.diff_lists(b, a)
        self.assertEqual([k for k, _n in gone], [("button#a", "Save", 30, 30)])
        self.assertEqual([k for k, _n in new], [("button#a", "Save", 48, 48)])

    def test_the_comparison_names_the_lists_and_the_fixed_box(self):
        before = {"AzPaint": {"portrait": self.audit_of([a11y(1, 1, "Button", "Undo", 10, 10, 30, 30, clickable=True)])}}
        after = {"AzPaint": {"portrait": self.audit_of([a11y(1, 1, "Button", "Undo", 10, 80, 48, 48, clickable=True)], [node(1, "button", 10, 80, 48, 48)])}}
        text = ac.compare(before, after)
        self.assertIn("## AzPaint", text)
        self.assertIn("| tappable nodes under 48 dp | 1 | 0 |", text)
        self.assertIn("| tappable boxes under a bar | 1 | 0 |", text)
        self.assertIn("gone: ", text)


if __name__ == "__main__":
    unittest.main(verbosity=1)
