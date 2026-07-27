#!/usr/bin/env python3
"""
gen_e2e_cases.py — the reproducible expander for scripts/E2E_TESTS.txt.

Reads the REAL surface out of the repo (never hand-typed lists):

  * Callback API      -> api.json  ["0.2.0"]["api"]["callbacks"]["classes"]["CallbackInfo"]["functions"]
                         UNION the `impl CallbackInfo` blocks in layout/src/callbacks.rs
  * Mock input / debug ops -> `pub enum DebugEvent` in layout/src/e2e/full.rs,
                         MINUS everything `OP_POLICY` (doc/src/gene2e.rs) denies and
                         MINUS every zombie (declared but with no match arm)
  * Managers          -> layout/src/managers/*.rs
  * Assertion families-> scripts/E2E_PLAN.md  §B  (a..f, g1..g5)

then crosses them combinatorially (interaction x target x mutation x phase x
assertion-family) and emits ONE natural-language e2e case per line, ordered
SIMPLE -> COMPLEX.

Scope rule (E2E_PLAN.md §0.1): BEHAVIOUR ONLY. No geometry assertions, ever.
`azul-doc reftest` owns layout/CSS/pixel correctness.

COHERENCE IS ENFORCED, NOT HOPED FOR
------------------------------------
Every emitted line is consumed by an LLM that turns it into a JSON test. If a
line says "…then undo the typing…" but no earlier step typed anything, the model
will silently INVENT the missing step: the resulting test passes or fails on
something the line never asked for, and nobody ever finds out. So the crossing is
not a raw cartesian product. Every axis element declares:

    REQUIRES  — capabilities that must already be live when it runs
    PROVIDES  — capabilities it establishes for everything after it

and a combination is emitted only when each element's REQUIRES is a subset of the
union of the PROVIDES of the elements BEFORE it. The capability vocabulary is
`CAPS` below. Combinations that cannot satisfy this are DROPPED, never softened:
a vaguer line is worse than no line.

Two further rules of the same kind:
  * an assertion is only attached to a combination that can actually satisfy it
    (no "assert the pixels changed" for an interaction defined as changing
    nothing, no "assert the scroll animation terminates" for an instant scroll);
  * a line may only name an op that `OP_POLICY` allows and that is not a zombie.

`self_audit()` at the bottom re-checks the finished corpus from the outside, with
regexes rather than with the tables above, and hard-fails on any violation.

Uniqueness is guaranteed BY CONSTRUCTION: every emitted line is normalized to a
key (lowercased, punctuation-stripped, whitespace-collapsed) and inserted into a
set; a collision is a hard assert, not a silent drop.

Usage:  python3 scripts/gen_e2e_cases.py            # writes scripts/E2E_TESTS.txt + coverage table
        python3 scripts/gen_e2e_cases.py --check    # verify the checked-in file is up to date
"""

import json
import os
import re
import sys
from collections import Counter, OrderedDict

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "scripts", "E2E_TESTS.txt")

FULL_RS = os.path.join(ROOT, "layout/src/e2e/full.rs")
GENE2E_RS = os.path.join(ROOT, "doc/src/gene2e.rs")
CALLBACKS_RS = os.path.join(ROOT, "layout/src/callbacks.rs")
API_JSON = os.path.join(ROOT, "api.json")
MANAGERS_DIR = os.path.join(ROOT, "layout/src/managers")


# --------------------------------------------------------------------------
# 1. ENUMERATE THE REAL SURFACE
# --------------------------------------------------------------------------
def read(p):
    with open(p, "r", encoding="utf-8") as f:
        return f.read()


# Every combination deliberately NOT emitted, and why. Printed at the end, so a
# shrinking corpus is always accounted for rather than mysterious.
DROPPED = Counter()


def drop(reason, n=1):
    DROPPED[reason] += n


def callback_api_functions():
    """Every function on CallbackInfo (the Callback API)."""
    fns = set()
    api = json.loads(read(API_JSON))["0.2.0"]["api"]["callbacks"]["classes"]["CallbackInfo"]
    fns |= set(api.get("functions", {}).keys())

    # UNION with the Rust impl blocks (api.json omits a few internal/inspect fns)
    src = read(CALLBACKS_RS)
    for m in re.finditer(r"\nimpl(?:<[^>]*>)? CallbackInfo(?:<[^>]*>)? \{", src):
        i, depth = m.end(), 1
        while i < len(src) and depth:
            if src[i] == "{":
                depth += 1
            elif src[i] == "}":
                depth -= 1
            i += 1
        fns |= set(re.findall(r"\n    pub fn ([a-z_0-9]+)", src[m.end():i]))
    fns -= {"new", "create", "from_ptr"}  # constructors, not behaviour surface
    return sorted(fns)


def snake(name):
    return re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower()


def debug_ops():
    """
    The ops a generated test may actually drive.

    `DebugEvent` is the DEBUG / VISUAL-EDITOR protocol, not a test surface. Two
    filters, both read out of the repo rather than hand-maintained here:

      1. OP_POLICY (doc/src/gene2e.rs) — the law the generator's validation gate
         enforces. Emitting a corpus line for a DENIED op produces a test that
         can never be written; the old corpus had 232 of them.
      2. ZOMBIES — a variant declared in the enum but with no match arm in the
         dispatch falls through to the `_ => Unhandled` catch-all, which answers
         `ok` without doing anything. A test using one is vacuously green.

    Returns (usable, all_variants, denied, zombies) so the coverage table can be
    honest about what was excluded and why.
    """
    src = read(FULL_RS)
    start = src.index("pub enum DebugEvent {")
    i, depth = src.index("{", start) + 1, 1
    while i < len(src) and depth:
        if src[i] == "{":
            depth += 1
        elif src[i] == "}":
            depth -= 1
        i += 1
    variants = sorted({snake(v) for v in re.findall(r"^    ([A-Z][A-Za-z0-9]*)", src[start:i], re.M)})

    handled = {snake(m) for m in re.findall(r"^\s*DebugEvent::([A-Za-z0-9_]+)", src, re.M)}
    # ops the E2E step loop dispatches itself (mirrors gene2e.rs::parse_schema)
    handled |= {o for o in ("commit_undo_snapshot", "undo_app_state", "redo_app_state",
                            "assert_response") if '"%s"' % o in src}

    pol_src = read(GENE2E_RS)
    pol = pol_src[pol_src.index("const OP_POLICY"):pol_src.index("/// The verdict for one op.")]
    allowed = set(re.findall(r'\("([a-z_0-9]+)",\s*None\)', pol))
    denied = set(re.findall(r'\("([a-z_0-9]+)",\s*Some\(', pol))
    unclassified = [o for o in variants if o not in allowed and o not in denied
                    and not o.startswith("assert_")]
    assert not unclassified, (
        "gen_e2e_cases: %d DebugEvent variant(s) are not in OP_POLICY: %s\n"
        "Classify them in doc/src/gene2e.rs before regenerating the corpus."
        % (len(unclassified), unclassified))

    zombies = [o for o in variants if o not in handled]
    usable = [o for o in variants if o in allowed and o in handled]
    return usable, variants, sorted(o for o in variants if o in denied), zombies


def relayout_exempt():
    """
    The CssPropertyType variants for which `can_trigger_relayout()` is FALSE.

    Read out of css/src/props/property.rs rather than hand-classified, because
    hand-classifying it got `z-index` and `visibility` wrong: both were tagged
    paint-only, so 48 lines asserted "it does not trigger a relayout" about two
    properties the engine explicitly relayouts for. A test that asserts the
    opposite of the engine's own declared behaviour is not adversarial, it is
    just wrong.
    """
    src = read(os.path.join(ROOT, "css/src/props/property.rs"))
    i = src.index("pub const fn can_trigger_relayout")
    m = re.search(r"!matches!\(\s*self,(.*?)\n\s*\)\n", src[i:i + 4000], re.S)
    assert m, "gen_e2e_cases: can_trigger_relayout no longer has the `!matches!` shape"
    return set(re.findall(r"\b([A-Z][A-Za-z0-9]*)\b", m.group(1)))


def cross_invariants():
    """(implemented, unimplemented) X-invariants, read out of the assertion itself.

    `eval_assert_manager_invariants` hard-FAILS on X1/X4/X7/X8 ("never a silent
    pass"), so a corpus line naming one is an always-red test that proves
    nothing. X4 is additionally unfalsifiable: LayoutWindow has no second
    drag-drop manager for the pair to disagree.
    """
    src = read(FULL_RS)
    known = re.search(r"const KNOWN_CROSS: &\[&str\] = &\[(.*?)\];", src, re.S)
    unimpl = re.search(r"const UNIMPLEMENTED_CROSS: &\[\(&str, &str\)\] = &\[(.*?)\n    \];",
                       src, re.S)
    assert known and unimpl, "gen_e2e_cases: KNOWN_CROSS/UNIMPLEMENTED_CROSS not found in full.rs"
    return (set(re.findall(r'"(X\d+)"', known.group(1))),
            set(re.findall(r'"(X\d+)"', unimpl.group(1))))


def key_surface_managers():
    """Managers `assert_manager_invariants` can actually sweep for dangling keys.

    Its KNOWN_MANAGERS list has 8 entries and it REFUSES any other name
    ("Refusing to pass an unchecked manager"), so a key-set lifecycle assertion
    aimed at one of the other modules cannot be written at all.
    """
    src = read(FULL_RS)
    m = re.search(r"const KNOWN_MANAGERS: &\[&str\] = &\[(.*?)\];", src, re.S)
    assert m, "gen_e2e_cases: KNOWN_MANAGERS not found in full.rs"
    return set(re.findall(r'"([a-z_]+)"', m.group(1)))


# the assertion's short names vs. the module file names
MGR_ALIAS = {"scroll": "scroll_state", "focus": "focus_cursor"}


def managers():
    """
    Modules that hold real manager STATE.

    Not "every .rs file in managers/": `changeset.rs` defines TextChangeset and
    the TextOp* payloads and no manager at all, and `a11y` / `drag_drop` /
    `scroll_into_view` hold no state either (which is exactly why X1 and X4 are
    unimplementable). Naming them "the X manager" asks for a lifecycle nobody
    can observe. A module qualifies if it declares a `*Manager` struct, or if
    the assertion's own KNOWN_MANAGERS list names it.
    """
    keyed = {MGR_ALIAS.get(m, m) for m in key_surface_managers()}
    out = []
    for f in sorted(os.listdir(MANAGERS_DIR)):
        if not f.endswith(".rs") or f == "mod.rs":
            continue
        name = f[:-3]
        if re.search(r"pub struct [A-Za-z]*Manager\b", read(os.path.join(MANAGERS_DIR, f))) \
                or name in keyed:
            out.append(name)
        else:
            drop("manager/%s: module holds no manager state (no *Manager struct)" % name, 15)
    return out


CALLBACK_FNS = callback_api_functions()
DEBUG_OPS, ALL_OPS, DENIED_OPS, ZOMBIE_OPS = debug_ops()
RELAYOUT_EXEMPT = relayout_exempt()
CROSS_OK, CROSS_UNIMPL = cross_invariants()
KEYED_MANAGERS = {MGR_ALIAS.get(m, m) for m in key_surface_managers()}
MANAGERS = managers()

# Assertion families, verbatim from E2E_PLAN.md §B
FAMILIES = OrderedDict([
    ("a", "idle stability (frame N+1 identical to frame N, damage drains to None)"),
    ("b", "liveness (damage non-empty AND pixels actually differ)"),
    ("c", "damage soundness (coverage + pixel identity vs the full-repaint oracle + tightness)"),
    ("d", "bounded work (relayout_iterations / dom_regenerations capped, hit_depth_cap false)"),
    ("e", "resource leak (counters return to baseline)"),
    ("f", "no panic / no unbounded growth"),
    ("g1", "composition (managers fire in order and reach a fixpoint)"),
    ("g2", "cross-manager consistency (X1..X10)"),
    ("g3", "state-machine leak (the interaction ended, the manager did not)"),
    ("g4", "dangling NodeId under mutation"),
    ("g5", "incrementality (the repaint is a patch, never Full)"),
])

# ══════════════════════════════════════════════════════════════════════════
# THE CAPABILITY VOCABULARY
#
# The one shared currency of the whole file. An axis element PROVIDES some of
# these and REQUIRES some of these; a combination is legal iff every element's
# requirements are already provided when it runs. Keep this list closed — an
# unknown capability name is a hard error, not a silently unsatisfiable
# requirement.
# ══════════════════════════════════════════════════════════════════════════
CAPS = {
    "press",      # a pointer button is currently held down
    "drag",       # a pointer drag is in flight (implies press)
    "scroll",     # some container's scroll offset has moved
    "anim",       # a time-driven animation is currently ticking
    "selection",  # a text selection exists
    "focus",      # some node holds keyboard focus
    "caret",      # a caret is blinking
    "editable",   # a contenteditable node exists and is engaged
    "typing",     # text was typed, so an undo record exists
    "hover",      # the pointer is resting over a node
    "gesture",    # a multi-touch / long-press gesture session is open
    "stroke",     # a pen stroke is in progress
    "domregen",   # the DOM was regenerated
}


def _chk(name, caps):
    bad = set(caps) - CAPS
    assert not bad, "%s: unknown capability %s" % (name, sorted(bad))
    return frozenset(caps)


# ══════════════════════════════════════════════════════════════════════════
# THE AXES
# ══════════════════════════════════════════════════════════════════════════
#
# WIDGET CAPABILITIES — what a mounted document can actually DO. Used to decide
# whether an event/property can visibly change it, so a liveness assertion is
# only ever attached where liveness must hold.
#
#   text        renders glyphs                selectable  text can be selected
#   children    has element children          editable    contains a contenteditable
#   focusable   can take keyboard focus       clickable   a click visibly changes it
#   hoverstyle  has a :hover rule             scrollv/h   scrolls on that axis
#   image       contains an image node        virtual     virtual-view list
#   overlap     two nodes overlap in z        uniquefont  its own font family
#
WIDGETS = [
    ("a red stretched flexbox filling the window", "flexbox", {"children"}),
    ("a single grey button with a :hover rule", "button",
     {"text", "children", "focusable", "clickable", "hoverstyle"}),
    ("a paragraph of selectable text", "text", {"text", "children", "selectable"}),
    ("a contenteditable single-line input", "input",
     {"text", "editable", "focusable", "clickable", "selectable"}),
    ("a contenteditable multi-line textarea holding 40 lines of text, twice its own height",
     "textarea", {"text", "editable", "focusable", "clickable", "selectable", "scrollv"}),
    ("a vertically scrollable list of 40 rows", "list", {"text", "children", "scrollv"}),
    ("a horizontally scrollable strip of 40 cells", "hstrip", {"children", "scrollh"}),
    ("a scroll container of 40 rows nested inside another scroll container of 40 rows",
     "nested-scroll", {"children", "scrollv"}),
    ("a 10x10 grid of coloured boxes", "grid", {"children"}),
    ("an image node inside a flex row", "image", {"children", "image"}),
    ("a virtual-view list with 5000 virtual rows", "virtual-list",
     {"text", "children", "scrollv", "virtual"}),
    ("a table with 20 rows and 5 columns", "table", {"text", "children", "scrollv"}),
    ("an absolutely positioned overlay above the content", "overlay", {"children", "overlap"}),
    ("two z-index stacked translucent boxes", "zstack", {"children", "overlap"}),
    ("a box with a CSS transform applied", "transformed", {"children"}),
    ("a clipped overflow:hidden box with content larger than itself", "clipped", {"children"}),
    ("a sticky header above a scrolling body", "sticky", {"text", "children", "scrollv"}),
    ("a form with three focusable fields and a submit button", "form",
     {"text", "children", "editable", "focusable", "clickable", "selectable"}),
    ("a tab strip with three switchable panels", "tabs",
     {"text", "children", "focusable"}),
    ("a tree view with expandable nodes", "tree",
     {"text", "children", "focusable"}),
    ("a text node whose font-family is unique to it", "unique-font", {"text", "uniquefont"}),
    ("a container of 40 rows with a custom scrollbar that fades out", "fading-scrollbar",
     {"children", "scrollv"}),
    ("a deeply nested 12-level div chain with one leaf", "deep-nest", {"children"}),
    ("an empty body with no content at all", "empty", set()),
]
WCAP = {k: c for _, k, c in WIDGETS}

# CSS properties. Each row states the CONCRETE change (so the generator is never
# left to invent the value) and which widget capability the property needs in
# order to change anything at all.
#   (property, kind, "from X to Y", needs)
# `needs` empty  => applies to every widget, including the empty body.
# `needs` {"*"}  => any widget except the empty body (needs something painted).
#
# The CssPropertyType each row maps onto, so `kind` can be CHECKED against the
# engine's own `can_trigger_relayout()` instead of trusted.
CSS_TYPE = {
    "background-color": "BackgroundContent", "color": "TextColor", "opacity": "Opacity",
    "border-color": "BorderTopColor", "border-width": "BorderTopWidth",
    "box-shadow": "BoxShadowLeft", "width": "Width", "height": "Height",
    "padding": "PaddingLeft", "margin": "MarginLeft", "display": "Display",
    "flex-direction": "FlexDirection", "flex-grow": "FlexGrow",
    "justify-content": "JustifyContent", "align-items": "AlignItems",
    "font-size": "FontSize", "font-family": "FontFamily", "font-weight": "FontWeight",
    "line-height": "LineHeight", "letter-spacing": "LetterSpacing", "text-align": "TextAlign",
    "overflow": "OverflowX", "position": "Position", "z-index": "ZIndex",
    "transform": "Transform", "visibility": "Visibility", "cursor": "Cursor",
    "border-radius": "BorderTopLeftRadius", "background-image": "BackgroundContent",
    "white-space": "WhiteSpace",
}
# EVERY value is pinned in ONE canonical form. `0` vs `0px`, `bold` vs `700`,
# `1em` vs `16px` and an unquoted vs quoted family all PARSE DIFFERENTLY
# (`SetNodeCssOverride` parses the string fresh), so a "re-set it to the value it
# already has" line written against the other spelling is a REAL mutation wearing
# a no-op's clothes, and the no-op families below silently test the opposite of
# what they say. Never write a bare `0` here.
CSS_PROPS = [
    ("background-color", "paint-only", "from #ff0000 to #0000ff", set()),
    ("color", "paint-only", "from #000000 to #ff0000", {"text"}),
    ("opacity", "paint-only", "from 1.0 to 0.4", {"*"}),
    ("border-color", "paint-only", "from red to blue on the node's existing 2px border", {"*"}),
    ("border-width", "layout", "from 2px to 10px", {"*"}),
    ("box-shadow", "paint-only", "from none to 0 0 8px black", {"*"}),
    ("width", "layout", "from 200px to 320px", {"*"}),
    ("height", "layout", "from 100px to 240px", {"*"}),
    ("padding", "layout", "from 0px to 16px", {"*"}),
    ("margin", "layout", "from 0px to 24px", {"*"}),
    ("display", "structural", "from block to none", {"*"}),
    ("flex-direction", "layout", "from row to column", {"children"}),
    ("flex-grow", "layout", "from 0 to 1", {"children"}),
    ("justify-content", "layout", "from flex-start to space-between", {"children"}),
    ("align-items", "layout", "from stretch to center", {"children"}),
    ("font-size", "layout", "from 16px to 28px", {"text"}),
    ("font-family", "layout", 'from "Azul Mock Mono" to "Azul Mock Wide"', {"text"}),
    ("font-weight", "layout", "from 400 to 700", {"text"}),
    ("line-height", "layout", "from 1.0 to 2.0", {"text"}),
    ("letter-spacing", "layout", "from 0px to 4px", {"text"}),
    ("text-align", "layout", "from left to right", {"text"}),
    ("overflow", "structural", "from visible to hidden", {"children"}),
    ("position", "structural", "from static to absolute", {"children"}),
    # z-index and visibility LOOK paint-only and are not: can_trigger_relayout()
    # returns true for both, so the paint-only assertion ("does not trigger a
    # relayout") contradicted the engine. Classified from the source below.
    ("z-index", "layout", "from 0 to 5", {"overlap"}),
    ("transform", "paint-only", "from none to translate(20px, 0)", {"*"}),
    ("visibility", "layout", "from visible to hidden", {"*"}),
    ("cursor", "none", "from default to pointer", set()),
    ("border-radius", "paint-only", "from 0px to 12px", {"*"}),
    ("background-image", "paint-only", "from none to a 4x4 embedded PNG", set()),
    ("white-space", "layout", "from normal to nowrap", {"text"}),
]


def css_values(change):
    """('from A to B', …) -> (A, B). Used by the no-op families, which must name
    BOTH the value the stylesheet already declares and the different value the
    positive control writes. A row whose text does not have that shape cannot be
    used for a no-op line and says so loudly rather than being approximated."""
    m = re.match(r"^from (.+?) to (.+)$", change)
    return (m.group(1), m.group(2)) if m else (None, None)


# HARD CHECK: a "paint-only" row promises the generator it may assert "no
# relayout". Only the engine gets to decide that.
for _p, _kind, _c, _n in CSS_PROPS:
    _ty = CSS_TYPE[_p]
    _engine_paint_only = _ty in RELAYOUT_EXEMPT
    _claimed_paint_only = _kind in ("paint-only", "none")
    assert _engine_paint_only == _claimed_paint_only, (
        "gen_e2e_cases: '%s' is classified %r but can_trigger_relayout(%s) says relayout=%s. "
        "Fix the row, not the engine — a corpus line that asserts the opposite of the engine's "
        "declared behaviour is a broken test, not a strict one."
        % (_p, _kind, _ty, not _engine_paint_only))


def css_applies(needs, wk):
    if not needs:
        return True
    if needs == {"*"}:
        return wk != "empty"
    return bool(needs & WCAP[wk])


MUTATIONS = [
    ("delete the node itself", "delete-self"),
    ("delete the node's parent", "delete-parent"),
    ("delete a PRECEDING sibling so every following NodeId is renumbered", "delete-prev-sibling"),
    ("insert a new PRECEDING sibling so every following NodeId is renumbered", "insert-prev-sibling"),
    ("replace the whole subtree under the node", "replace-subtree"),
    ("reorder the node's siblings", "reorder-siblings"),
    ("rebuild the entire DOM from scratch via set_app_state", "full-rebuild"),
    # NOT "so its style changes": `CallbackChange::SetNodeIdsAndClasses` writes
    # `node_data` and never re-runs CSS matching, so a class swap changes no
    # style at all today. The mutation is still worth having — it is a real
    # `ShouldIncrementalRelayout` against a live node — but the line must not
    # promise a restyle nobody performs.
    ("rewrite the node's id and class list via set_node_classes, keeping the node itself alive",
     "reclass"),
]

# PHASES — a mid-interaction MOMENT. Each names state the interaction must
# actually be in, so a phase is only offered to an interaction that provides it.
# The old corpus offered all five to all twelve interactions: 1,720 of 2,402
# mutate/* lines named a mouse button, a drag or a scroll animation the
# interaction never had.
#   (text, key, requires)
PHASES = [
    ("immediately after mouse_down, before any movement", "after-down", {"press"}),
    ("mid-drag, after several mouse_move steps", "mid-drag", {"drag"}),
    ("while the resulting scroll animation is still ticking", "mid-animation", {"anim"}),
    ("exactly one frame before mouse_up", "pre-up", {"press"}),
    ("immediately after mouse_up, in the same frame", "post-up", {"press"}),
    ("while the scroll offset is non-zero and the scrollbar is still faded in", "mid-scroll",
     {"scroll"}),
    ("while the caret blink timer is still armed", "caret-armed", {"caret"}),
    ("between two keystrokes, before the undo snapshot coalesces", "between-keys", {"typing"}),
    ("mid-gesture, between two touch_move steps", "mid-gesture", {"gesture"}),
    ("mid-stroke, between two pen_move steps", "mid-stroke", {"stroke"}),
    ("while the pointer is still resting on the node", "mid-hover", {"hover"}),
]

# INTERACTIONS — a whole interaction, run from a cold start. `provides` is what
# is live WHILE it runs (which is what PHASES and the cross/* invariants key
# off). `abortable` says whether "abort it halfway with Escape" is meaningful.
#   (text, key, provides, abortable)
INTERACTIONS = [
    ("a text-selection drag across the node", "text-select-drag",
     {"press", "drag", "selection"}, True),
    ("a node drag on the node", "node-drag", {"press", "drag"}, True),
    ("a scrollbar-thumb drag on the node's scroll container", "thumb-drag",
     {"press", "drag", "scroll"}, True),
    ("a momentum scroll of the node's container", "momentum-scroll", {"scroll", "anim"}, True),
    ("a scroll_into_view animation targeting the node", "scroll-into-view",
     {"scroll", "anim"}, True),
    ("keyboard focus plus a blinking caret on the node", "focus-caret",
     {"focus", "caret", "editable"}, True),
    ("a hover held over the node", "hover", {"hover"}, False),
    ("a virtual-view scroll that has the node in its over-scan window", "virtual-scroll",
     {"scroll"}, True),
    ("typing into the node until an undo stack is recorded against it", "undo-stack",
     {"focus", "editable", "typing"}, False),
    ("a long-press gesture on the node", "long-press", {"press", "gesture"}, True),
    ("a pinch gesture centred on the node", "pinch", {"gesture"}, True),
    ("a pen stroke drawn over the node", "pen-stroke", {"stroke"}, True),
]

# INPUT EVENTS — every one is self-contained (the old "a mouse_up delivered
# outside the window bounds" had no matching mouse_down, and "a mouse_move that
# leaves the node" was never inside it). `visible` is the set of widget
# capabilities that make the event visibly change something; a liveness
# assertion is attached ONLY when the widget has one of them, and the whole
# (widget x event) cell is asserted the OTHER way round when it has none.
# `ops` is the debug op(s) the event drives, which is what the wave-1 split
# consults so a cell whose op the runner refuses is never generated first.
#
# Two structural rules learned the hard way, encoded here rather than in prose:
#
#   * NAME THE KEY. "a key_down followed by the matching key_up" left the choice
#     to the model, and `Space` / `PageDown` / `Home` / `End` / the arrows all
#     hit a `ScrollFocusedContainer` default action (layout/src/default_actions.rs
#     :74-199) and MOVE PIXELS. The same corpus line then produced a green test
#     one run and a red one the next. `F12` is parseable by
#     `parse_virtual_keycode` and has no arm in `determine_keyboard_default_action`.
#
#   * NO VIEWPORT-CHANGING EVENTS HERE. `resize` / `dpi_changed` change the
#     screenshot's DIMENSIONS, and every pixel-diff assertion in the harness
#     (`assert_changed`, `assert_damage_covers_changes`, `assert_damage_sound`,
#     `assert_idle_stable`) either hard-fails or auto-passes across a dimension
#     change — so a (widget x resize) cell can be neither sound nor tight nor
#     stable. They have their own families at rank 35/37, which drive them with
#     a stated before/after size instead of "to it".
#
INPUT_EVENTS = [
    ("a single left click", "click", {"clickable", "hoverstyle"}, ("click",)),
    ("a double click", "double-click", {"clickable", "selectable"}, ("double_click",)),
    ("a right click", "right-click", set(), ("mouse_down", "mouse_up")),
    ("a middle click", "middle-click", set(), ("mouse_down", "mouse_up")),
    ("a mouse_down without a matching mouse_up", "down-only", {"clickable", "hoverstyle"},
     ("mouse_down",)),
    ("a mouse_move that enters the node", "move-in", {"hoverstyle"}, ("mouse_move",)),
    ("a mouse_move that enters the node followed by one that leaves it again", "move-out",
     {"hoverstyle"}, ("mouse_move",)),
    ("a mouse_move that stays inside the node and changes nothing", "move-noop", set(),
     ("mouse_move",)),
    ("a mouse_down on the node followed by a mouse_up delivered outside the window bounds",
     "up-outside", {"clickable", "hoverstyle"}, ("mouse_down", "mouse_up")),
    ("a vertical wheel scroll", "wheel-v", {"scrollv"}, ("scroll",)),
    ("a horizontal wheel scroll", "wheel-h", {"scrollh"}, ("scroll",)),
    ("a diagonal wheel scroll", "wheel-d", {"scrollv", "scrollh"}, ("scroll",)),
    ("a key_down of the Tab key", "key-tab", set(), ("key_down",)),
    ("a key_down of the Escape key", "key-esc", set(), ("key_down",)),
    # ArrowDown is NOT editable-only: with no focused node (and with a focused
    # node that is not a text input) `determine_keyboard_default_action` returns
    # ScrollFocusedContainer{Down, Line} — so it moves every scrollable widget,
    # and a focused text input is the one case where it does nothing.
    ("a key_down of the ArrowDown key", "key-down", {"scrollv"}, ("key_down",)),
    ("a key_down of the F12 key followed by the matching key_up", "key-updown", set(),
     ("key_down", "key_up")),
    ("a text_input of a single character", "text-1", {"editable"}, ("focus_node", "text_input")),
    ("a text_input of a 200 character paragraph", "text-200", {"editable"},
     ("focus_node", "text_input")),
    ("a touch_start / touch_move / touch_end sequence", "touch", {"scrollv", "clickable"},
     ("touch_start", "touch_move", "touch_end")),
    # A swipe reaches ONLY `inject_native_gesture`, whose payload is observable
    # exclusively by a user callback that calls `get_swipe_direction` — and a
    # `mount`ed document has no user callbacks. So no widget can consume it and
    # `visible` is empty BY CONSTRUCTION: the honest property is that a gesture
    # nothing can consume must not repaint anything.
    ("a swipe gesture", "swipe", set(), ("swipe",)),
    ("a pen_down / pen_move / pen_up sequence", "pen", {"clickable"},
     ("pen_down", "pen_move", "pen_up")),
]

# `text_input` hard-errors ("No focused node") unless a node holds DOM focus, and
# `focus_node` is the only op that gives it any. Events that need the precondition
# say so IN THE LINE, so the model cannot leave it out and cannot invent a
# coordinate-guessing click instead.
EVENT_PREFIX = {
    "text-1": "focus its contenteditable node with focus_node, then deliver ",
    "text-200": "focus its contenteditable node with focus_node, then deliver ",
}

# `relayout_iterations` counts EVENT PASSES, and an event pass runs only for an
# op that goes through `modify_window_state` AND actually changes the state the
# runner diffs (`runner.rs::apply_user_change`: size, dpi, mouse, keyboard, touch,
# window focus, position). `text_input` pushes `CallbackChange::CreateTextInput`
# and `scroll` writes the ScrollManager directly — neither enters the pass at all.
EVENT_RUNS_PASS = {"text-1": False, "text-200": False}
# A click / Tab that MOVES KEYBOARD FOCUS legitimately re-enters the pass once
# (runner.rs, the `default_action_focus_changed || mouse_click_focus_changed`
# arm); nothing else may. `>1` is the engine's own invalidation-loop signal, so
# the bound is DERIVED, never a constant.
FOCUS_MOVING = {"click", "down-only", "up-outside", "double-click", "key-tab"}
# Sequences whose NET effect on screen is zero: the first half applies a style
# and the second half removes it again.
ROUND_TRIP_EVENTS = {"move-out", "up-outside"}


def runs_event_pass(ik):
    return EVENT_RUNS_PASS.get(ik, True)


def relayout_bound(ik, wk):
    if not runs_event_pass(ik):
        return 0
    return 2 if ik in FOCUS_MOVING and "focusable" in WCAP[wk] else 1


def event_visible(vis, wk):
    return bool(vis & WCAP[wk])


# SCROLL MODES — `animates` decides whether an animation-termination assertion
# is honest; `fades` whether a scrollbar-fade assertion is.
SCROLL_MODES = [
    ("an instant scroll", "instant", False),
    ("a smooth animated scroll", "smooth", True),
    ("a wheel scroll with momentum", "momentum", True),
    ("a scrollbar-thumb drag", "thumb", False),
    ("a programmatic scroll_node_to", "programmatic", False),
]

# SCROLL TARGETS — `scrolls` says whether the container can move at all. The two
# that cannot are kept (they are excellent no-op tests) but get the CORRECT
# assertion instead of one that presumes movement.
SCROLL_TARGETS = [
    ("a 40-row vertical list", "list", True),
    ("a horizontally scrolling strip", "hstrip", True),
    ("a scroll container nested inside another scroll container", "nested", True),
    ("a virtual-view list of 5000 rows", "virtual", True),
    ("a scroll container with a sticky header", "sticky", True),
    ("a scroll container inside a CSS transform", "transformed", True),
    ("a scroll container whose content exactly fits (no overflow)", "nooverflow", False),
    ("a scroll container already pinned at its bottom edge, scrolled further down", "at-end",
     False),
]

# RESOURCES — and the cycle that actually PRODUCES each one. The old corpus
# asserted "gesture input sessions returned to baseline" after a bare
# mount/unmount, which is 0 -> 0: vacuously green, 144 lines of it.
#   (name, key, "the cycle that fills it", widget capability the cycle needs)
RESOURCES = [
    ("registered fonts", "fonts", "mount and unmount", {"text"}),
    ("font hash map entries", "font_hash_map", "mount and unmount", {"text"}),
    ("parsed font bytes in the FontManager", "parsed_fonts", "mount and unmount", {"text"}),
    ("the font chain cache", "font_chain_cache", "mount and unmount", {"text"}),
    ("registered images", "images", "mount and unmount", {"image"}),
    ("image key map entries", "image_key_map", "mount and unmount", {"image"}),
    ("scroll manager states", "scroll_states",
     "scroll to a non-zero offset in, then unmount", {"scrollv", "scrollh"}),
    ("hover hit-test histories", "hover_histories",
     "sweep the pointer across, then unmount", {"children"}),
    ("gesture input sessions", "input_sessions",
     "run a short press-drag-release on, then unmount", {"children"}),
    ("virtual view states", "virtual_view_states",
     "scroll past the over-scan window of, then unmount", {"virtual"}),
    ("undo/redo node stacks", "undo_stacks",
     "type a character into, then unmount", {"editable"}),
    ("GPU value caches", "gpu_caches",
     "scroll so the scrollbar fades in and out over, then unmount", {"scrollv", "scrollh"}),
]

# X1..X10 straight out of E2E_PLAN.md §B(g2). `needs` is the capability the
# interaction must supply for the invariant to say anything at all: asserting
# "the active drag's source node still exists" about a hover is vacuous.
CROSS = [
    ("X1", "scroll_into_view and ScrollManager agree on which container scrolled and by how much",
     {"scroll"}),
    ("X2", "has_active_animations() is true exactly when some AnimatedScrollState has an animation "
           "and tick() asks for a repaint", {"anim"}),
    ("X3", "the active drag's source node still exists and the hover manager resolves it against "
           "the same DOM", {"drag"}),
    ("X4", "GestureAndDragManager.active_drag and the legacy DragDropManager.active_drag never "
           "disagree about whether a drag is live", {"drag"}),
    ("X5", "the selection anchor's node exists for the whole drag, or the selection is cleared "
           "outright", {"selection"}),
    ("X6", "multi_cursor is Some only while a contenteditable node that exists has focus",
     {"editable"}),
    ("X7", "no scroll adjustment stays pending for a caret whose focus was cleared", {"caret"}),
    ("X8", "selection focus and the autoscrolled container stay mutually consistent frame to "
           "frame", {"selection", "scroll"}),
    ("X9", "scrollbar_fade_active returns to false within the fade duration of the last scroll",
     {"scroll"}),
    ("X10", "no manager key refers to a node that no longer exists", set()),
]

# Split by WHAT IT TAKES TO OBSERVE. The first group needs a per-node key set,
# which only `assert_manager_invariants` can read and only for the 8 managers in
# its KNOWN_MANAGERS list — it refuses any other name outright. The second group
# is observable through damage / idle-state / counters for any manager.
MANAGER_LIFECYCLE_KEYED = [
    ("acquires state on the first relevant event", "acquire"),
    ("keeps exactly one entry per live node, never two", "no-dupes"),
    ("drops its entry when the node is unmounted", "gc-on-unmount"),
    ("remaps its key when a preceding sibling is inserted", "remap-insert"),
    ("remaps its key when a preceding sibling is deleted", "remap-delete"),
    ("survives a full DOM rebuild without holding a dead key", "survive-rebuild"),
    ("holds exactly one key per live node and no key for a node the DOM does not contain, as "
     "assert_manager_invariants sweeps it", "counts-match-dom"),
    ("is unaffected by a mutation in an unrelated subtree", "isolation"),
]
MANAGER_LIFECYCLE_GENERIC = [
    ("returns to an empty/idle state once the interaction ends", "idle-after"),
    ("does not grow across 200 idle frames", "no-growth"),
    ("does not latch a permanently-dirty flag", "no-latched-dirty"),
    ("does not force a full redraw when only it changed", "patch-only"),
]

# --------------------------------------------------------------------------
# 2. EMITTER (uniqueness by construction)
# --------------------------------------------------------------------------
LINES = []
SEEN = {}
COLLISIONS = []
COVER_CB = {}
COVER_OP = {}
COVER_MGR = {}
COVER_FAM = {}
# line text -> the ops it drives. The wave-1 split reads this, so a line that
# drives an op the headless runner refuses is deferred by DERIVATION rather than
# by a hand-kept list that rots the moment the runner grows the missing arm.
LINE_OPS = {}


def norm(s):
    return re.sub(r"\s+", " ", re.sub(r"[^a-z0-9 ]", " ", s.lower())).strip()


def _as_list(x):
    return list(x) if isinstance(x, (list, tuple, set)) else [x] if x else []


def emit(tag, text, *, cb=None, op=None, mgr=None, fam=None, rank=0):
    """Emit one test line. `rank` orders simple -> complex."""
    line = "[%s] %s" % (tag, text)
    k = norm(line)
    if k in SEEN:
        COLLISIONS.append((line, SEEN[k]))
        return
    SEEN[k] = line
    LINES.append((rank, len(LINES), line))
    LINE_OPS[line] = set(_as_list(op))
    for name, bucket in ((cb, COVER_CB), (op, COVER_OP), (mgr, COVER_MGR), (fam, COVER_FAM)):
        for x in _as_list(name):
            bucket.setdefault(x, 0)
            bucket[x] += 1


# ══════════════════════════════════════════════════════════════════════════
# THE SHARED CLAUSES
#
# Two things every family that asserts an ABSENCE got wrong, factored out so
# they cannot be got wrong one family at a time:
#
#  1. A POSITIVE CONTROL. "no damage" / "no relayout" / "the counter did not
#     move" is ALSO what a completely dead engine reports. `assert_work_bounded`
#     grew `min_*` / `exact_*` and `assert_resource_counts` has `gt` / `lt`
#     precisely so a test can prove the work DID happen; before this, 2,268
#     corpus lines asserted only absence and exactly 72 carried a control.
#
#  2. AN ENFORCEABLE SETTLE BUDGET. "within 5 ticks" was unenforceable prose:
#     `assert_idle_stable` reported the frame count and never constrained it, so
#     a scenario that took 20 frames satisfied a line that said 5. It now takes
#     `max_frames`, counted from the `reset_frame_counters` the recipe puts
#     immediately before the interaction — so the number must be stated as a
#     FRAME BUDGET, in the harness's own vocabulary.
# ══════════════════════════════════════════════════════════════════════════
CONTROL_DAMAGE = (
    "then, in the same timeline, change the node's background-color via set_node_css_override as a "
    "POSITIVE CONTROL and assert THAT does produce damage and does change pixels, so the absence "
    "above is a real result and not a dead engine")
CONTROL_LAYOUT = (
    "then, in the same timeline, override the node's width to a different px value as a POSITIVE "
    "CONTROL and assert THAT does spend at least one layout pass and does produce damage")


def settle(frames=5, hold=3):
    """The settle clause WITHOUT a leading "assert", so a caller can chain it."""
    return ("the window is back to zero paint damage within %d frames of the change — a "
            "max_frames budget the harness enforces, not prose — and still reports zero damage "
            "%d frames later" % (frames, hold))


# ─────────────────────────────────────────────────────────────────────────
# RANK 10 — idle stability: the simplest possible test, one widget, one assert
# ─────────────────────────────────────────────────────────────────────────
# AN IDLE TIMELINE CAN ONLY BE DAMAGED THREE WAYS.
# `CpuBackend::render_frame` (dll/src/desktop/shell2/headless/mod.rs) takes the
# "nothing changed -> FrameDamage::None" fast arm unless `dl_damage` is non-empty,
# or `resize_damage`, `has_scroll`, `has_vview_damage` or `has_gpu_damage`. With
# no input and no resize only the last two are reachable. So a fixture with no
# path to a VirtualView re-invocation, a GPU value-cache change or an async
# resource landing late is BYTE-FOR-BYTE the same test as the flexbox baseline:
# 18 of the 24 widgets funnelled into one code path and differed only in layout
# COST, which neither idle assertion measures. They are dropped, not softened —
# every widget noun still appears 130-240x elsewhere in the corpus.
#   (widget key, why it survives, ticks, ms per tick)
IDLE_FIXTURES = [
    ("flexbox", "the canonical nothing-animates baseline", 5, 16),
    ("empty", "the degenerate case: a zero-item display list through the damage diff", 5, 16),
    ("image", "an async image decode/upload can land on a LATER frame and dirty it", 5, 16),
    ("virtual-list", "a VirtualView that re-queues itself every frame is a first-class idle "
                     "damage source and nothing else in the corpus catches it", 5, 16),
    ("button", "the hover manager can latch state for a pointer that never moved", 5, 16),
    # A time-driven fixture must (i) have the machinery ARMED by an event and
    # (ii) advance the clock past that machinery's OWN threshold. `tick_ms 16` x5
    # is 80ms of engine clock; the scrollbar fade is 500ms+200ms on the profiles
    # that have one and the caret blink is 530ms (managers/text_edit.rs), so the
    # old "tick 5 frames" could not reach a single toggle of either.
    ("fading-scrollbar", "the scrollbar fade is the one GPU-value-cache path an idle window has",
     6, 150),
]
IDLE_KEEP = {k for k, _, _, _ in IDLE_FIXTURES}
WDESC = {k: d for d, k, _ in WIDGETS}
for _, wk, _ in WIDGETS:
    if wk not in IDLE_KEEP:
        drop("idle/*: fixture has no path to idle damage — identical code path to the baseline", 2)

# The fixture whose whole point is a time-driven effect gets the event that ARMS
# it prepended, and the CSS that turns it on: the unconditional UA fallback (the
# one a Linux CI box evaluates) declares `-azul-scrollbar-fade-delay: 0`, so a
# fixture that does not declare its own fade has no fade to observe.
IDLE_ARM = {
    "fading-scrollbar":
        ("wheel-scroll it down by 120px so the scrollbar is faded in and the GPU value cache is "
         "live, then ", " (the widget must declare `-azul-scrollbar-fade-delay: 200ms` and "
                        "`-azul-scrollbar-fade-duration: 100ms` so there is a fade at all)"),
}

for wk, why, ticks, ms in IDLE_FIXTURES:
    wd = WDESC[wk]
    arm, note = IDLE_ARM.get(wk, ("", ""))
    emit("idle/stability",
         "mount %s, %sadvance the clock in %d steps of %dms with no input at all%s, assert every "
         "frame is byte-identical to the PREVIOUS one and the accumulated paint damage drains to "
         "None and stays there, %s"
         % (wd, arm, ticks, ms, note, CONTROL_DAMAGE),
         fam="a", op=["wait_frame", "tick_ms"], rank=10)
    # `assert_work_bounded` measures the work an EVENT caused. Attached to a
    # timeline with NO event it is vacuous by dispatch structure: `tick_ms` /
    # `wait_frame` re-push the current window state, `anything_changed` is false,
    # the event pass is skipped and every counter is 0 no matter how broken the
    # engine is — a manager that rebuilds the display list on every single frame
    # reported `relayout_iterations = 0, dom_regenerations = 0` and went green.
    # Retargeted at CONVERGENCE, which is what these counters actually measure.
    hover = "hoverstyle" in WCAP[wk]
    emit("idle/bounded",
         "mount %s, move the mouse ONCE to the centre of the window and then advance the clock in "
         "%d steps of %dms with no further input, assert the event converged and did not re-enter: "
         "at most one event-processing pass, zero DOM regenerations, %s, "
         "MAX_EVENT_RECURSION_DEPTH was never hit, and the last frame reports zero paint damage; %s"
         % (wd, ticks, ms,
            "at most one layout pass (the hover restyle)" if hover else "zero layout passes",
            CONTROL_LAYOUT),
         fam="d", op=["mouse_move", "tick_ms"], rank=10)
    # `debug_counts` is unreadable by any assertion (the 9 keys of
    # `collect_resource_counts` are the whole counter surface) and there is NO
    # RSS probe anywhere in the workspace, so the old line named two observables
    # that do not exist and a statistic — slope — nothing computes. Four explicit
    # sample points is the strongest form the harness actually supports: a
    # sawtooth that allocates during a frame and sweeps at the end of it is
    # invisible to a two-point diff.
    emit("idle/growth",
         "mount %s, snapshot the resource counters, then drive 40 idle frames as explicit "
         "tick_ms(16) + wait_frame pairs, re-snapshotting and comparing the font and image "
         "counters at frames 1, 10, 20 and 40, and assert all four samples equal the baseline — "
         "four samples, not two, so a counter that rises and is swept cannot hide between them"
         % wd,
         fam="f", op=["wait_frame", "tick_ms", "snapshot_resources"], rank=11)

# ── the NO-OP MUTATION family ────────────────────────────────────────────
# The sharpest idea in the corpus, previously wired to a counter the op cannot
# move: `set_node_css_override` never enters `process_window_events`, so
# `relayout_iterations` stays 0 and "assert the engine schedules no relayout" was
# green while `relayout_only()` re-laid-out the whole root DOM. The counter that
# SEES it is `layout_passes`, incremented in `layout_and_generate_display_list`,
# the one funnel both paths go through. The property is also named now: "a
# property" left the choice to the model, and a paint-only choice makes the
# assertion vacuous while a re-spelled length (`1em` for `16px`) makes the
# "no-op" premise false.
NOOP_CSS_PROPS = [(p, c, n) for p, kind, c, n in CSS_PROPS
                  if kind in ("layout", "structural") and all(css_values(c))]
assert len(NOOP_CSS_PROPS) >= 18, "gen_e2e_cases: the no-op CSS axis collapsed to %d rows" \
                                  % len(NOOP_CSS_PROPS)
for wk, _why, _t, _m in IDLE_FIXTURES:
    wd = WDESC[wk]
    for prop, change, needs in NOOP_CSS_PROPS:
        if not css_applies(needs, wk):
            drop("idle/noop-css: '%s' cannot affect this widget" % prop)
            continue
        old, new = css_values(change)
        emit("idle/noop-css",
             "mount %s then apply a set_node_css_override that re-sets %s to %s, the exact value "
             "its own stylesheet already declares, assert the engine recognises the no-op: ZERO "
             "layout passes, the accumulated paint damage stays kind \"none\" and the frame is "
             "pixel-identical to the snapshot taken before the override; then override %s to %s in "
             "the same timeline as a POSITIVE CONTROL and assert THAT does spend a layout pass and "
             "does produce damage"
             % (wd, prop, old, prop, new),
             fam=["a", "d"], op="set_node_css_override", rank=11)
    if "text" in WCAP[wk]:
        # "the glyph count is unchanged" was not expressible by ANY assertion
        # (`glyph_count` lives on a display-list-item response and no evaluator
        # reads it) and "schedules no repaint" was the same assertion as "the
        # damage stays None" written twice. What IS measurable — and is the real
        # waste — is that `ChangeNodeText` calls `layout_cache.reset_incremental()`
        # and re-lays-out unconditionally, even for a byte-identical string.
        emit("idle/noop-text",
             "mount %s then call set_node_text with the exact text the node already contains, "
             "assert the engine recognises the no-op: ZERO layout passes, the accumulated paint "
             "damage stays kind \"none\" and the frame is pixel-identical to the snapshot taken "
             "before the call; then set the text to a DIFFERENT string in the same timeline as a "
             "POSITIVE CONTROL and assert THAT does spend a layout pass and does produce damage"
             % wd,
             fam=["a", "d"], op="set_node_text", rank=11)
    else:
        drop("idle/noop-text: widget has no text")

# `idle/noop-class` is HELD, not reworded. `CallbackChange::SetNodeIdsAndClasses`
# (layout/src/e2e/runner.rs) writes `styled_dom.node_data` and returns
# `ShouldIncrementalRelayout` — it never calls `StyledDom::restyle`, which is
# exactly what the CSS-override arm one screen up DOES call ("STALE-SCREEN FIX").
# So setting the SAME classes and setting COMPLETELY DIFFERENT ones are
# indistinguishable: zero style change, zero damage, zero regenerations. The
# family cannot even be repaired by adding a positive control, because no control
# is constructible with the op. Held until the class path re-cascades.
CLASS_RESTYLES = "restyle" in read(os.path.join(ROOT, "layout/src/e2e/runner.rs")).split(
    "CallbackChange::SetNodeIdsAndClasses")[1][:600]
if CLASS_RESTYLES:
    for wk, _why, _t, _m in IDLE_FIXTURES:
        emit("idle/noop-class",
             "mount %s then call set_node_classes with exactly the class list AND the exact id the "
             "node already has, assert the accumulated paint damage stays kind \"none\" and no "
             "pixel differs from the pre-call snapshot; then set a class the stylesheet styles "
             "differently as a POSITIVE CONTROL and assert THAT does produce damage"
             % WDESC[wk],
             fam=["a", "d"], op="set_node_classes", rank=11)
else:
    drop("idle/noop-class: SetNodeIdsAndClasses writes node_data without re-running CSS matching, "
         "so identical and different class lists are indistinguishable and no positive control is "
         "constructible", len(IDLE_FIXTURES))

# ─────────────────────────────────────────────────────────────────────────
# RANK 20 — one input event, one widget, one assertion family
# ─────────────────────────────────────────────────────────────────────────
# EVERY assertion here is CONDITIONAL on `event_visible`. The old comment claimed
# damage / bounded / settle "hold for every pair (including zero-damage ones), so
# they are emitted unconditionally" — and that justification is precisely the
# bug. On the 423 pairs where the event cannot change the widget:
#   * the DAMAGE line demanded a bounded, non-full repaint of damage that is
#     correctly EMPTY (`assert_damage_incremental` fails outright on empty
#     damage) three lines away from an `over-invalidation` line on the SAME pair
#     demanding the damage stay None — two lines making opposite demands;
#   * the SETTLE line's "returns to idle with zero damage" was the INITIAL
#     condition, never an achievement;
#   * the BOUNDED line's `<= 2` was satisfied by 0, i.e. by an engine that
#     dropped the input entirely.
# So the invisible pairs belong to `over-invalidation` EXCLUSIVELY, and the
# visible pairs get damage/bounded/settle/liveness as a matched set of four.
for ie, ik, vis, iops in INPUT_EVENTS:
    pre = EVENT_PREFIX.get(ik, "deliver ")
    for wd, wk, _ in WIDGETS:
        if not event_visible(vis, wk):
            # Do not drop the cell — FLIP IT. The pair cannot change anything on
            # screen, so the property to assert is the opposite one, and it
            # covers a bug class the corpus otherwise has none of:
            # OVER-invalidation, where the engine repaints for an event that
            # changed nothing and burns a frame forever after.
            #
            # "no relayout iteration is spent" was FALSE for every input event by
            # the counter's own definition: `relayout_iterations` counts EVENT
            # PASSES, and any input that changes window state runs exactly one.
            # The counter that answers "did the engine needlessly re-lay-out?" is
            # `layout_passes`, which the documented table in
            # `FrameReport` puts at 0 for an inert pointer event.
            emit("input/over-invalidation",
                 "mount %s and %s%s to it — nothing this widget renders can change as a result — "
                 "and assert the engine does not over-invalidate: the accumulated paint damage "
                 "stays kind \"none\", the frame is pixel-identical to the snapshot taken before "
                 "the event, and the event costs at most one event-processing pass, zero DOM "
                 "regenerations and ZERO layout passes; %s"
                 % (wd, pre, ie, CONTROL_DAMAGE),
                 fam=["a", "d"], op=iops, rank=20)
            drop("input/damage+bounded+settle: the event cannot change this widget, so "
                 "over-invalidation owns the cell exclusively", 3)
            continue

        # SOUNDNESS *and* TIGHTNESS in one assertion. The old wording ("the
        # damage-driven buffer is pixel-identical to a full repaint") had no
        # implementing assertion at all — `assert_screenshot` compares against a
        # PNG on disk, not against another in-timeline buffer — so the model had
        # to approximate it. `assert_damage_sound` does coverage, overpaint and
        # `forbid_full` in one call and is named here in the harness's vocabulary.
        emit("input/damage",
             "mount %s and %s%s to it, assert the repaint is SOUND and TIGHT against the snapshot "
             "taken before the event: every pixel that changed lies inside the reported damage "
             "rects, the damage-driven buffer matches the full-repaint oracle, and the repaint is "
             "a bounded patch rather than a whole-window redraw" % (wd, pre, ie),
             fam="c", op=iops, rank=20)
        # `assert_work_bounded`'s bounds were `at most 2 relayout iterations, 0
        # DOM regenerations` for all 576 cells — one constant Python string, not
        # derived from the event, the widget or the engine. `<= 2` admits exactly
        # one spurious re-entry, i.e. the smallest instance of the invalidation
        # loop this family exists to detect, and an upper bound alone is
        # satisfied by an engine that did nothing.
        rb = relayout_bound(ik, wk)
        emit("input/bounded",
             "mount %s and %s%s to it, assert the work is EXACTLY what the event requires — %s — "
             "with zero DOM regenerations, at least one layout pass, and MAX_EVENT_RECURSION_DEPTH "
             "never tripped"
             % (wd, pre, ie,
                "no event-processing pass at all, because this op never enters one" if rb == 0 else
                "exactly %s event-processing pass%s, no more and no FEWER, so an engine that "
                "dropped the input fails too" % ("one" if rb == 1 else "two", "" if rb == 1 else "es")),
             fam="d", op=iops, rank=20)
        # The settle line never established that damage was ever non-zero, so
        # "returns to idle with zero damage" was true from the first frame
        # onwards. The beat that makes it falsifiable is the damage-non-zero
        # assertion BEFORE the drain.
        emit("input/settle",
             "mount %s and %s%s to it, assert the event FIRST produces a non-empty damage set — "
             "so an engine that dropped the input cannot pass — and that %s"
             % (wd, pre, ie, settle(frames=5, hold=3)),
             fam="a", op=iops, rank=20)
        # A SEQUENCE THAT ENDS WHERE IT STARTED IS NOT A LIVENESS TEST.
        # `move-out` (enter then leave) and `up-outside` (press then release
        # away) apply a hover/active style and then REMOVE it, so the final
        # frame equals the `before` snapshot and "the pixels actually changed"
        # is red against a perfectly correct engine. The property those two
        # sequences really assert is that the style was both APPLIED and UNDONE,
        # which needs a snapshot between the halves.
        if ik in ROUND_TRIP_EVENTS:
            emit("input/liveness",
                 "mount %s, snapshot the frame, %s%s to it one half at a time — asserting the "
                 "pixels change after the FIRST half — and assert that after the SECOND half the "
                 "frame is pixel-identical to the first snapshot again: the style must be both "
                 "applied and undone, and comparing only the end state to the start state proves "
                 "neither" % (wd, pre, ie),
                 fam="b", op=iops, rank=20)
        else:
            emit("input/liveness",
                 "mount %s and %s%s to it, assert the damage set is non-empty, the pixels actually "
                 "changed, and the repaint is a rects patch that does not cover the whole window"
                 % (wd, pre, ie),
                 fam="b", op=iops, rank=20)

# every usable debug op gets its own dedicated line set, split by what the op IS
DRIVE_TEMPLATES = [
    ("smoke", "assert the op is accepted, no panic occurs, and the process stays alive", "f", 21),
    ("settle", "assert " + settle(frames=5, hold=3) + ", " + CONTROL_DAMAGE, "a", 22),
    ("bounded", "assert the op costs a bounded number of relayout iterations and never trips the "
                "MAX_EVENT_RECURSION_DEPTH cap", "d", 22),
    ("incremental", "assert any repaint it causes is a Rects patch and never FrameDamage::Full "
                    "unless the op is structural by declaration", "g5", 23),
    ("resources", "assert every resource counter returns to its pre-op baseline after 3 GC frames",
     "e", 23),
    ("managers", "assert no manager key points at a node that does not exist afterwards", "g2", 23),
    ("repeat", "issue the op 50 times in a row and assert no counter grows without bound and the "
               "frame still settles", "f", 24),
    ("interleave", "issue the op while a smooth scroll animation started earlier in the same "
                   "timeline is still running, and assert both the animation and the op complete "
                   "and the window settles", "g1", 25),
]
# An observation op drives nothing, so "any repaint it causes" is vacuous for it.
# What it MUST do is not perturb the engine at all.
OBSERVE_TEMPLATES = [
    ("smoke", "assert the op is accepted, no panic occurs, and the process stays alive", "f", 21),
    ("nodamage", "assert merely issuing it produces NO damage, spends ZERO layout passes, and "
                 "leaves the next frame's incremental render state undisturbed, " + CONTROL_DAMAGE,
     "a", 22),
    ("repeat", "issue the op 50 times in a row and assert no counter grows without bound and the "
               "frame still settles", "f", 24),
    ("resources", "assert every resource counter returns to its pre-op baseline after 3 GC frames",
     "e", 23),
]
HARNESS_TEMPLATES = [
    ("smoke", "assert the op is accepted, no panic occurs, and the process stays alive", "f", 21),
    ("settle", "assert " + settle(frames=5, hold=3) + ", " + CONTROL_DAMAGE, "a", 22),
    ("repeat", "issue the op 50 times in a row and assert no counter grows without bound and the "
               "frame still settles", "f", 24),
]
HARNESS_OPS = {"mount", "unmount", "tick_ms", "wait", "wait_frame", "reset_frame_counters"}


def op_kind(o):
    if o in HARNESS_OPS:
        return "harness"
    if o.startswith(("get_", "dump_", "find_", "take_", "snapshot_", "capture_")) or o == "hit_test":
        return "observe"
    return "drive"


OP_TEMPLATES = {"drive": DRIVE_TEMPLATES, "observe": OBSERVE_TEMPLATES,
                "harness": HARNESS_TEMPLATES}
for o in DEBUG_OPS:
    for suffix, atxt, afam, rank in OP_TEMPLATES[op_kind(o)]:
        emit("op/%s" % suffix,
             "drive the %s debug op against a mounted DOM, %s" % (o, atxt),
             op=o, fam=afam, rank=rank)
drop("op/*: op denied by OP_POLICY", len(DENIED_OPS) * len(DRIVE_TEMPLATES))
drop("op/*: zombie op (declared, no match arm)", len(ZOMBIE_OPS) * len(DRIVE_TEMPLATES))

# ─────────────────────────────────────────────────────────────────────────
# RANK 30 — CSS-override damage matrix (behavioural: refresh + patch + settle)
# ─────────────────────────────────────────────────────────────────────────
# WHICH COUNTER. A `set_node_css_override` is a CALLBACK-API mutation: it never
# enters `process_window_events`, so `relayout_iterations` (an EVENT-PASS
# counter) stays 0 for it no matter what the engine does, and every "assert no
# relayout" written against that counter was green while `relayout_only()`
# re-laid-out the whole root DOM. `layout_passes` is the counter that sees it.
PROP_ASSERTS = {
    "paint-only": [
        ("assert the pixels change and the damage is a bounded patch, and that ZERO layout passes "
         "are spent — a paint-only property must not re-run layout", "c", 30),
        ("assert the damage-driven render is pixel-identical to the full repaint of the same frame", "c", 31),
        ("assert the change FIRST produces a non-empty damage set and the window THEN settles back to "
         "zero damage immediately afterwards — the settle half alone is satisfied by an engine "
         "that ignored the override", "a", 31),
    ],
    "layout": [
        ("assert the change is repainted, the damage covers every pixel that actually differs, and "
         "no under-paint leaves a stale region on screen", "c", 32),
        ("assert exactly one layout pass is spent — at least one, because the property changes "
         "geometry, and no more than one — with zero DOM regenerations and no event-processing "
         "pass at all", "d", 32),
        ("assert the change FIRST produces a non-empty damage set and the window THEN settles back to "
         "zero damage — the settle half alone is satisfied by an engine that ignored the "
         "override", "a", 32),
    ],
    "structural": [
        ("assert the repaint covers every changed pixel, is declared structural if it goes Full, and "
         "the window settles afterwards", "c", 33),
        ("assert every manager key still points at a live node afterwards", "g2", 33),
        ("assert the change costs exactly one layout pass and ZERO DOM regenerations — a CSS "
         "override re-lays-out, it does not re-run the layout callback", "d", 33),
    ],
    # A property with no visual effect can only be tested against a POSITIVE
    # CONTROL: "nothing was repainted" is also what a completely dead engine
    # reports, so every one of these pairs the inert change with a change that
    # MUST repaint, in the same timeline.
    "none": [
        ("assert nothing is repainted at all and the damage stays kind \"none\", then change the "
         "node's background-color in the same timeline as a POSITIVE CONTROL and assert THAT does "
         "produce damage (so the none above is a real result and not a dead engine)", "a", 30),
        ("assert ZERO layout passes are spent and the frame is pixel-identical to the snapshot "
         "before the override, then override the node's width in the same timeline as a POSITIVE "
         "CONTROL and assert THAT does spend a layout pass", "a", 30),
        ("assert no resource counter moves, then add an image node as a POSITIVE CONTROL and "
         "assert the image counter is strictly greater than the baseline snapshot", "e", 31),
    ],
}
for prop, kind, change, needs in CSS_PROPS:
    for wd, wk, _ in WIDGETS:
        if not css_applies(needs, wk):
            drop("css/%s: '%s' cannot affect this widget" % (kind, prop), len(PROP_ASSERTS[kind]))
            continue
        for atxt, afam, rank in PROP_ASSERTS[kind]:
            emit("css/%s" % kind,
                 "mount %s and change its %s %s via set_node_css_override, %s"
                 % (wd, prop, change, atxt),
                 op="set_node_css_override", fam=afam, rank=rank)

# ─────────────────────────────────────────────────────────────────────────
# RANK 35 — resize / dpi damage (the user's own example lives here)
# ─────────────────────────────────────────────────────────────────────────
#   (from, to, description, moved) — `moved` false means the geometry did not
#   change, so a refresh assertion would be flatly wrong: the correct property is
#   that the engine RECOGNISES the no-op.
RESIZES = [
    ("500x600", "800x900", "grow both axes", True),
    ("800x900", "500x600", "shrink both axes", True),
    ("400x300", "400x900", "grow only the height", True),
    ("400x300", "1200x300", "grow only the width", True),
    ("800x600", "801x600", "grow the width by a single pixel", True),
    ("800x600", "800x600", "resize to the exact same size", False),
    ("300x200", "60x40", "shrink below the content's minimum", True),
    ("640x480", "1920x1080", "grow by more than 4x in area", True),
]
for wd, wk, _ in WIDGETS:
    for a, b, desc, moved in RESIZES:
        if moved:
            # "assert the output refreshes" was previously read as a pixel diff
            # vs a before-snapshot — and `assert_changed` AUTO-PASSES whenever
            # the frame size changed ("pixels necessarily differ"), so the
            # liveness half of every resize line was decided by the harness
            # before the engine was consulted. It is asserted on the WORK
            # COUNTERS here, which a resize genuinely moves and which do not
            # care about the buffer's dimensions. The tightness half is kept
            # exactly as the user posed it and is deliberately adversarial: a
            # grow that repaints the untouched interior is over-invalidation.
            emit("resize/damage",
                 "window starts at %s with %s and is resized to %s (%s) via a window resize event, "
                 "assert the resize is HONOURED — at least one layout pass and at least one DOM "
                 "regeneration are spent — and that the repaint it schedules is a bounded patch "
                 "rather than a whole-window redraw: only the newly exposed region needs painting, "
                 "and repainting an unchanged interior is the over-invalidation this line exists "
                 "to catch" % (a, wd, b, desc),
                 op="resize", fam=["b", "c", "g5"], rank=35)
        else:
            emit("resize/noop",
                 "window starts at %s with %s and is resized to %s (%s), assert the engine "
                 "recognises the no-op: no event-processing pass runs at all, ZERO layout passes "
                 "are spent, the accumulated paint damage stays kind \"none\" and the frame is "
                 "pixel-identical to the one before the event, %s"
                 % (a, wd, b, desc, CONTROL_DAMAGE),
                 op="resize", fam=["a", "d"], rank=35)
        emit("resize/settle",
             "window starts at %s with %s and is resized to %s (%s), assert the resize DID cost "
             "work — at least one layout pass and at least one DOM regeneration, so an engine "
             "that ignored the resize fails too — and that %s, and does not re-enter an "
             "invalidation loop (MAX_EVENT_RECURSION_DEPTH is never tripped)"
             % (a, wd, b, desc, settle(frames=6, hold=3)),
             op="resize", fam=["a", "d"], rank=36)
    # THE DIMENSION-SAFE FORM. Every pixel-diff assertion in the harness is
    # undefined across a size change: `assert_changed` AUTO-PASSES on a dimension
    # mismatch ("frame size changed (pixels necessarily differ)") while
    # `assert_damage_covers_changes` and `assert_damage_sound` hard-FAIL on the
    # same situation. So a one-way resize can prove nothing about content
    # correctness. Resizing BACK puts both snapshots at the same size, which is
    # where the comparison is real — and "content vanished after a resize" is
    # exactly the bug that shows up there.
    for a, b, desc, _moved in RESIZES[:2]:
        emit("resize/roundtrip",
             "window starts at %s with %s, snapshot the frame, resize to %s (%s), let it settle, "
             "then resize back to %s and assert the frame is pixel-identical to that first "
             "snapshot — a resize that loses, clips or corrupts content shows up as a diff at the "
             "ORIGINAL size, which is the only size at which a pixel comparison across a resize "
             "means anything" % (a, wd, b, desc, a),
             op="resize", fam=["b", "c"], rank=36)
for wd, wk, _ in WIDGETS:
    for scale in ("1.0 to 2.0", "2.0 to 1.0", "1.0 to 1.5", "1.5 to 1.25"):
        emit("dpi/damage",
             "mount %s and change the DPI scale from %s via dpi_changed, assert the engine treats "
             "it as the geometry change it is: at least one layout pass and at least one DOM "
             "regeneration are spent, the reported damage kind is FULL rather than a stale patch "
             "of the old scale, and the window then settles to zero damage" % (wd, scale),
             op="dpi_changed", fam=["b", "a"], rank=37)
    emit("dpi/roundtrip",
         "mount %s, snapshot the frame, change the DPI scale from 1.0 to 2.0 via dpi_changed, let "
         "it settle, then change it back to 1.0 and assert the frame is pixel-identical to that "
         "first snapshot — the doubled scale changes the SCREENSHOT DIMENSIONS, so a one-way DPI "
         "change auto-passes every pixel assertion in the harness and only the round trip is a "
         "real comparison" % wd,
         op="dpi_changed", fam=["b", "c"], rank=37)

# ─────────────────────────────────────────────────────────────────────────
# RANK 40 — scroll matrix
# ─────────────────────────────────────────────────────────────────────────
#   (text, family, rank, needs) — `needs` gates the assertion on what the
#   mode/target can actually do: "moves" (the offset changes at all) and
#   "animates" (there is an animation to terminate).
SCROLL_ASSERTS = [
    ("assert the exposed strip is the only region PAINTED while the present damage covers the whole "
     "memmoved clip, and the damage-driven buffer matches a full repaint exactly", "g5", 40,
     {"moves"}),
    ("assert the ScrollManager offset and the rendered content agree and no stale row is left "
     "behind at the old offset", "b", 41, {"moves"}),
    ("assert has_active_animations() is FIRST true while the animation runs — so an engine that "
     "never started one fails too — and that the animation then terminates: "
     "has_active_animations() goes false, scroll_dirty clears and the window reaches zero "
     "damage", "g3", 42, {"moves", "animates"}),
    ("assert the scrollbar fade completes and gpu_state.scrollbar_fade_active returns to false so "
     "the window stops generating frames", "g3", 42, {"moves"}),
    # `scroll` writes the ScrollManager and marks the frame for regeneration; it
    # never goes through `modify_window_state`, so it runs NO event pass and
    # `relayout_iterations` is 0 for it whatever the engine does. The bound that
    # means something is on `layout_passes`.
    ("assert the scroll costs zero DOM regenerations, no event-processing pass at all, and at most "
     "one layout pass — moving an offset is not a reason to re-lay-out the tree", "d", 41,
     {"moves"}),
]
for st, sk, can_scroll in SCROLL_TARGETS:
    for sm, smk, animates in SCROLL_MODES:
        have = set()
        if can_scroll:
            have.add("moves")
        if animates:
            have.add("animates")
        for atxt, afam, rank, needs in SCROLL_ASSERTS:
            if needs - have:
                drop("scroll/%s: target/mode provides no %s" % (smk, ",".join(sorted(needs - have))))
                continue
            emit("scroll/%s" % smk,
                 "perform %s on %s, %s" % (sm, st, atxt),
                 op="scroll", mgr="scroll_state", fam=afam, rank=rank)
        if not can_scroll:
            # kept, with the assertion the situation actually supports
            emit("scroll/noop",
                 "perform %s on %s, assert the engine recognises there is nothing to scroll: the "
                 "ScrollManager offset does not move, the accumulated paint damage stays kind "
                 "\"none\", ZERO layout passes are spent and no scrollbar fade animation is armed; "
                 "%s" % (sm, st, CONTROL_DAMAGE),
                 op="scroll", mgr="scroll_state", fam=["a", "g3"], rank=40)

# ─────────────────────────────────────────────────────────────────────────
# RANK 45 — every Callback API function
# ─────────────────────────────────────────────────────────────────────────
# The old classifier was three prefix tests, which put `pluralize` and
# `format_date` in the "query a node" bucket (so 94 lines asked what happens to
# them "one frame after the node they refer to was deleted" — they refer to no
# node) and `add_timer`, `stop_propagation`, `keyring_store` in the "mutator"
# bucket (so 46 lines asserted their repaint is pixel-identical to a full
# repaint — they paint nothing). Classify by what the function DOES.

# refers to no node and touches no DOM: a pure/ICU/formatting helper
CB_PURE = set("""compare_strings format_date format_datetime format_decimal format_integer
format_list format_time pluralize sort_strings strings_equal get_plural_category
get_icu_localizer get_string_contents get_system_time_fn get_current_time""".split())

# reads process/OS/window-global state, not a node
CB_ENV = set("""get_monitors get_current_monitor get_gl_context get_gamepad_state
get_primary_gamepad get_location_fix get_biometric_kind get_biometric_result get_keyring_result
get_sensor_reading get_wacom_pad get_pen_pressure get_pen_state get_pen_tilt get_permission_status
get_thread get_thread_ids get_timer get_timer_ids get_ctx get_current_event_id
get_current_keyboard_state get_previous_keyboard_state get_current_mouse_state
get_previous_mouse_state get_current_window_state get_previous_window_state
get_current_window_flags get_previous_window_flags get_current_window_handle get_safe_area_insets
get_system_style get_clipboard_content get_dropped_file get_dropped_files get_hovered_file
get_hovered_files get_route_pattern get_route_param get_swipe_direction get_rotation get_pinch
get_long_press get_scroll_delta get_drag_types get_drag_data get_dom_ids get_loaded_fonts
get_loaded_font_bytes""".split())

# returns a rect/size/offset: `azul-doc reftest` owns whether the NUMBER is right,
# so a behaviour test may only assert that calling it is free and degrades safely
CB_GEOMETRY = set("""get_computed_height get_computed_width get_node_rect get_node_size
get_node_position get_hit_node_rect get_hit_node_layout_rect get_node_hit_test_bounds measure_dom
get_cursor_position get_cursor_position_screen get_cursor_relative_to_node
get_cursor_relative_to_viewport get_scroll_offset get_scroll_offset_for_node""".split())

# mutates real state but paints NOTHING by itself
CB_INVISIBLE = set("""stop_propagation stop_immediate_propagation prevent_default keyring_get
keyring_store keyring_delete request_biometric_auth set_clipboard_content set_copy_content
set_cut_content set_drag_data set_drop_effect accept_drop add_timer remove_timer add_thread
remove_thread commit_undo_snapshot find_scroll_parent set_route_param push_change
set_text_changeset queue_window_state_sequence inject_native_gesture add_image_to_cache
remove_image_from_cache set_node_ids_and_classes""".split())

# out of scope for a single-window headless timeline
CB_OUT_OF_SCOPE = {"close_window", "create_window"}

CB_SCREENSHOT_PREFIX = ("take_screenshot", "take_native_screenshot")
CB_QUERY_PREFIX = ("get_", "has_", "is_", "was_", "can_", "node_has", "had_", "inspect_",
                   "measure_", "compare_", "strings_", "format_", "pluralize", "sort_")


def cb_kind(fn):
    if fn in CB_OUT_OF_SCOPE:
        return "skip"
    if fn in CB_PURE:
        return "pure"
    if fn in CB_GEOMETRY:
        return "geometry"
    if fn in CB_ENV:
        return "env"
    if fn.startswith(CB_SCREENSHOT_PREFIX):
        return "screenshot"
    if fn == "take_changes":
        return "node-query"          # drains a changeset; it is a read, not a paint
    if fn.startswith(CB_QUERY_PREFIX):
        return "node-query"
    if fn in CB_INVISIBLE:
        return "invisible-mutator"
    return "mutator"


CB_TEMPLATES = {
    # reads a node -> the node-renumbering / dead-node templates are meaningful
    "node-query": [
        ("read", "call CallbackInfo::%s from inside a callback fired by a click and assert it "
                 "returns a value consistent with the current DOM and that merely calling it "
                 "produces NO damage and spends ZERO layout passes, " + CONTROL_DAMAGE, "a", 45),
        ("idle", "call CallbackInfo::%s on every one of 20 idle frames and assert the window still "
                 "reaches FrameDamage::None and no counter grows", "f", 46),
        ("stale", "call CallbackInfo::%s from a callback that runs one frame AFTER the node it "
                  "refers to was deleted, assert it fails gracefully (None/empty) instead of "
                  "panicking or returning a dangling id", "g4", 47),
        ("mutation", "call CallbackInfo::%s immediately after a preceding sibling was inserted so "
                     "every following NodeId shifted, assert the value it returns refers to the "
                     "same LOGICAL node as before the shift", "g4", 48),
        ("leak", "call CallbackInfo::%s 500 times in a row from one click callback and assert every "
                 "resource counter is back at the snapshot taken before the loop, sampling at "
                 "iteration 1, 100 and 500 so a sawtooth cannot hide between two samples", "e", 46),
        ("bounded", "call CallbackInfo::%s from a callback and assert the event it runs in still "
                    "terminates within the relayout iteration budget and never trips the recursion "
                    "depth cap", "d", 46),
    ],
    # returns a rect/size: assert only that reading it is free and degrades safely
    "geometry": [
        ("free", "call CallbackInfo::%s from inside a click callback and assert that merely "
                 "reading it produces NO damage, spends ZERO layout passes and does not mark "
                 "the display list dirty (its NUMERIC result is reftest territory, not this "
                 "suite's), " + CONTROL_DAMAGE, "a", 45),
        ("stale", "call CallbackInfo::%s from a callback that runs one frame AFTER the node it "
                  "refers to was deleted, assert it returns None/zero rather than panicking or "
                  "reading a stale layout entry", "g4", 47),
        ("idle", "call CallbackInfo::%s on every one of 20 idle frames and assert the window still "
                 "reaches FrameDamage::None and ZERO layout passes are spent by the read", "f", 46),
    ],
    # reads process/OS/window state: no node, so no renumbering template
    "env": [
        ("read", "call CallbackInfo::%s from inside a callback fired by a click and assert merely "
                 "calling it produces NO damage and spends ZERO layout passes, " + CONTROL_DAMAGE, "a", 45),
        ("idle", "call CallbackInfo::%s on every one of 20 idle frames and assert the window still "
                 "reaches FrameDamage::None and no counter grows", "f", 46),
        ("leak", "call CallbackInfo::%s 500 times in a row from one click callback and assert every "
                 "resource counter is back at the snapshot taken before the loop, sampling at "
                 "iteration 1, 100 and 500 so a sawtooth cannot hide between two samples", "e", 46),
        ("headless", "call CallbackInfo::%s under AZ_BACKEND=headless where the underlying "
                     "platform resource is absent, and assert it degrades cleanly (None/default, "
                     "no panic, no abort)", "f", 45),
    ],
    # a pure helper: it must be inert, and that is the whole test
    "pure": [
        ("inert", "call CallbackInfo::%s from inside a click callback and assert it is completely "
                  "inert with respect to the engine: no damage, ZERO layout passes, no DOM "
                  "regeneration and no resource counter moves, " + CONTROL_DAMAGE, "a", 45),
        ("leak", "call CallbackInfo::%s 500 times in a row from one click callback and assert every "
                 "resource counter is back at the snapshot taken before the loop, sampling at "
                 "iteration 1, 100 and 500 so a sawtooth cannot hide between two samples", "e", 46),
    ],
    # mutates state that IS visible
    "mutator": [
        ("effect", "call CallbackInfo::%s from a click callback and assert the resulting frame "
                   "actually refreshes: damage is non-empty and the pixels differ", "b", 45),
        ("patch", "call CallbackInfo::%s from a click callback and assert the repaint it causes is "
                  "an incremental Rects patch, never a full redraw, and matches the full-repaint "
                  "oracle pixel for pixel", "g5", 46),
        ("settle", "call CallbackInfo::%s from a click callback and assert the call FIRST "
                   "produces a non-empty damage set, then " + settle(frames=5, hold=3)
                   + " rather than re-invalidating forever", "a", 46),
        ("bounded", "call CallbackInfo::%s from a callback and assert the event costs a bounded "
                    "number of relayout iterations, DOM regenerations, and never trips the "
                    "recursion depth cap", "d", 46),
        ("loop", "call CallbackInfo::%s unconditionally from a callback that is itself re-fired by "
                 "the update it causes, and assert the engine reaches a fixpoint instead of "
                 "looping forever", "d", 49),
        ("leak", "call CallbackInfo::%s then undo its effect, force 3 GC frames, and assert every "
                 "resource counter returns to baseline", "e", 47),
        ("stale", "call CallbackInfo::%s targeting a node that was deleted in the same frame, "
                  "assert no panic, no dangling manager key, and the window still settles", "g4", 48),
        ("managers", "call CallbackInfo::%s and assert every manager's key set still matches the "
                     "live DOM afterwards (X10)", "g2", 47),
    ],
    # mutates state that is NOT visible: the repaint assertions are replaced by
    # their opposite, which is the real (and much sharper) property
    "invisible-mutator": [
        ("nodamage", "call CallbackInfo::%s from a click callback and assert it schedules NO "
                     "repaint of its own: the accumulated damage stays kind \"none\" and ZERO "
                     "layout passes are spent, because nothing it changes is on screen, "
                     + CONTROL_DAMAGE, "a", 45),
        ("settle", "call CallbackInfo::%s from a click callback and assert " + settle(frames=5, hold=3)
                   + " rather than re-invalidating forever, " + CONTROL_DAMAGE, "a", 46),
        ("bounded", "call CallbackInfo::%s from a callback and assert the event costs a bounded "
                    "number of relayout iterations, DOM regenerations, and never trips the "
                    "recursion depth cap", "d", 46),
        ("loop", "call CallbackInfo::%s unconditionally from a callback that is itself re-fired by "
                 "the update it causes, and assert the engine reaches a fixpoint instead of "
                 "looping forever", "d", 49),
        ("leak", "call CallbackInfo::%s then undo its effect, force 3 GC frames, and assert every "
                 "resource counter returns to baseline", "e", 47),
        ("stale", "call CallbackInfo::%s targeting a node that was deleted in the same frame, "
                  "assert no panic, no dangling manager key, and the window still settles", "g4", 48),
        ("managers", "call CallbackInfo::%s and assert every manager's key set still matches the "
                     "live DOM afterwards (X10)", "g2", 47),
    ],
    "screenshot": [
        ("oracle", "call CallbackInfo::%s from an assertion callback and assert it renders a full "
                   "repaint that is pixel-identical to the incremental buffer for the same frame", "c", 45),
        ("nodamage", "call CallbackInfo::%s and assert the act of taking it produces NO damage and "
                     "does not disturb the incremental render state of the next frame, "
                     + CONTROL_DAMAGE, "a", 46),
        ("leak", "call CallbackInfo::%s 100 times and assert the registered-font, parsed-font and "
                 "font-chain-cache counters are back at the pre-loop snapshot exactly", "e", 47),
        ("headless", "call CallbackInfo::%s under AZ_BACKEND=headless and assert it either succeeds "
                     "or fails cleanly (no panic, no abort) given the headless RawWindowHandle is "
                     "Unsupported", "f", 45),
    ],
}
for fn in CALLBACK_FNS:
    k = cb_kind(fn)
    if k == "skip":
        drop("callback/*: fn is out of scope for a single-window headless timeline",
             len(CB_TEMPLATES["mutator"]))
        continue
    for suffix, tmpl, afam, rank in CB_TEMPLATES[k]:
        emit("callback/%s" % suffix, tmpl % fn, cb=fn, fam=afam, rank=rank)

# ─────────────────────────────────────────────────────────────────────────
# RANK 50 — per-manager lifecycle
# ─────────────────────────────────────────────────────────────────────────
MGR_TRIGGER = {
    "scroll_state": "scrolling a container",
    "scroll_into_view": "calling scroll_node_into_view on an off-screen node",
    "gesture": "starting a drag with mouse_down + mouse_move",
    "drag_drop": "starting a legacy drag-and-drop",
    "hover": "moving the mouse over a node",
    "focus_cursor": "focusing a contenteditable node",
    "text_edit": "typing into a contenteditable node",
    "selection": "selecting a run of text",
    "gpu_state": "scrolling so the scrollbar fades in and out",
    "virtual_view": "scrolling a virtual-view list past its over-scan window",
    "undo_redo": "typing and then undoing in a contenteditable node",
    "a11y": "focusing a node so the accessibility tree updates",
    "clipboard": "copying a text selection",
    "changeset": "applying a text changeset",
    "text_input": "typing into a text input",
    "file_drop": "dragging a file over the window",
    "permission": "requesting a permission from a callback",
    "geolocation": "requesting a location fix from a callback",
    "biometric": "requesting biometric auth from a callback",
    "keyring": "storing and reading a keyring secret from a callback",
    "sensors": "reading a sensor value from a callback",
    "gamepad": "reading gamepad state from a callback",
}
# A manager with no named trigger used to fall back to "exercising it from a
# callback", which tells the generator nothing. Fail loudly instead.
_missing = [m for m in MANAGERS if m not in MGR_TRIGGER]
assert not _missing, (
    "gen_e2e_cases: no concrete trigger for manager(s) %s — add one to MGR_TRIGGER "
    "rather than emitting a line the generator has to invent an interaction for." % _missing)
for mgr in MANAGERS:
    trig = MGR_TRIGGER[mgr]
    keyed = mgr in KEYED_MANAGERS
    for desc, key in MANAGER_LIFECYCLE_KEYED:
        if not keyed:
            drop("manager/%s: assert_manager_invariants has no key surface for this manager" % mgr)
            continue
        emit("manager/%s" % mgr,
             "exercise the %s manager by %s, then assert it %s" % (mgr, trig, desc),
             mgr=mgr, fam="g2", rank=50)
    for desc, key in MANAGER_LIFECYCLE_GENERIC:
        emit("manager/%s" % mgr,
             "exercise the %s manager by %s, then assert it %s" % (mgr, trig, desc),
             mgr=mgr, fam="g3", rank=50)
    emit("manager/%s" % mgr,
         "exercise the %s manager by %s and assert the state it produces does not force "
         "FrameDamage::Full when only that manager's state changed" % (mgr, trig),
         mgr=mgr, fam="g5", rank=51)
    if keyed:
        emit("manager/%s" % mgr,
             "exercise the %s manager by %s, then delete the node it keyed and assert no dangling key "
             "survives and nothing panics" % (mgr, trig),
             mgr=mgr, fam="g4", rank=52)
    else:
        drop("manager/%s: assert_manager_invariants has no key surface for this manager" % mgr)
    emit("manager/%s" % mgr,
         "exercise the %s manager by %s, end the interaction, then assert assert_state_machines_idle "
         "holds and the window reaches FrameDamage::None" % (mgr, trig),
         mgr=mgr, fam=["g3", "a"], rank=52)

# ─────────────────────────────────────────────────────────────────────────
# RANK 55 — resource leak matrix
# ─────────────────────────────────────────────────────────────────────────
# Every line now runs the cycle that actually FILLS the counter it asserts on.
LEAK_CYCLES = [("once", 1), ("10 times in a row", 10), ("200 times in a row", 200)]
# THE CONTROL THE WHOLE FAMILY WAS MISSING. `eq` against a baseline is `0 == 0`
# on every widget that cannot host the resource, and `0 == 0` holds on an engine
# that has been deleted. `eval_assert_resource_counts` implements `gt` and `lt`
# for exactly this ("lets a test assert the LIVENESS half — the font really was
# loaded — so the leak half cannot go green for the wrong reason") and not one
# corpus line used them. One control per (resource, host) pair, so every counter
# the leak family reads is PROVEN LIVE before anything asserts it returns.
for rname, rk, cycle, needs in RESOURCES:
    hosts = [(wd, wk) for wd, wk, c in WIDGETS if needs & c]
    if not hosts:
        drop("leak/%s: no widget can host this resource" % rk, len(LEAK_CYCLES))
        continue
    for wd, wk in hosts:
        for cdesc, n in LEAK_CYCLES:
            emit("leak/%s" % rk,
                 "snapshot the resource counters, %s %s %s, force 3 GC frames, then "
                 "assert the count of %s has returned exactly to the baseline"
                 % (cycle, wd, cdesc, rname),
                 op="set_app_state", fam="e", rank=55)
        fill = cycle.split(", then unmount")[0].replace(" and unmount", "")
        emit("leak/%s-control" % rk,
             "snapshot the resource counters on an empty window, then %s %s and, WITHOUT "
             "unmounting it, assert the count of %s is STRICTLY GREATER than that snapshot — the "
             "positive control for the whole %s family: if this counter never moves, every "
             "\"returned to baseline\" assertion is 0 == 0 and holds on an engine that has been "
             "deleted" % (fill, wd, rname, rk),
             op="set_app_state", fam="e", rank=54)
        emit("leak/%s-control" % rk,
             "%s %s, snapshot the resource counters while it is still live, then unmount it and "
             "force 3 GC frames, and assert the count of %s is STRICTLY LESS than that snapshot — "
             "proving the RELEASE path runs at all, which \"returned to baseline\" cannot show"
             % (fill.capitalize(), wd, rname),
             op="set_app_state", fam="e", rank=54)
drop("leak/*: widget cannot produce the asserted resource",
     len(RESOURCES) * len(WIDGETS) * len(LEAK_CYCLES)
     - sum(len(LEAK_CYCLES) * len([1 for _, _, c in WIDGETS if needs & c])
           for _, _, _, needs in RESOURCES))

# ─────────────────────────────────────────────────────────────────────────
# RANK 58 — invalidation-loop / bounded-work traps
# ─────────────────────────────────────────────────────────────────────────
# Every action names a real Callback API function (checked below), so the
# generator never has to reach for a denied `relayout` / `redraw` op to express
# "the callback asks for more work".
LOOP_ACTIONS = [
    ("returning Update::RefreshDom from the callback that the DOM regeneration itself re-fires",
     None),
    ("calling set_css_property with the value it already has", "set_css_property"),
    ("calling scroll_to with the offset the container is already at", "scroll_to"),
    ("calling set_focus_to_node on the node that is already focused", "set_focus_to_node"),
    ("calling change_node_text with the text the node already contains", "change_node_text"),
    ("calling modify_window_state without changing anything", "modify_window_state"),
    ("calling request_hit_test_update on every frame", "request_hit_test_update"),
    ("calling trigger_virtual_view_rerender from inside the virtual-view render callback",
     "trigger_virtual_view_rerender"),
    ("calling commit_undo_snapshot on every keystroke callback", "commit_undo_snapshot"),
    ("calling scroll_node_into_view on a node that is already fully visible",
     "scroll_node_into_view"),
    ("toggling a class on and off in the same callback", "set_node_ids_and_classes"),
    ("calling take_screenshot from inside a layout callback", "take_screenshot"),
    ("returning ShouldRegenerateDom unconditionally from an event handler", None),
    ("mutating the DOM from a timer that fires every frame", "add_timer"),
    ("calling reload_system_fonts from a hover callback", "reload_system_fonts"),
]
_bogus = [f for _, f in LOOP_ACTIONS if f and f not in CALLBACK_FNS]
assert not _bogus, "gen_e2e_cases: loop actions name non-existent callbacks: %s" % _bogus
LOOP_TRIGGERS = [
    "a click", "a hover", "a wheel scroll", "a keypress", "a window resize",
    "a text_input", "a focus change", "an idle tick", "a timer firing", "a DOM mutation",
]
for act, _fn in LOOP_ACTIONS:
    for trg in LOOP_TRIGGERS:
        emit("loop/bounded",
             "trigger %s that responds by %s, assert the engine reaches a fixpoint: "
             "relayout_iterations stays under the cap, hit_depth_cap is false, and the window "
             "reaches FrameDamage::None instead of redrawing forever" % (trg, act),
             fam=["d", "a"], rank=58)

# ─────────────────────────────────────────────────────────────────────────
# RANK 60 — cross-manager consistency (X1..X10)
# ─────────────────────────────────────────────────────────────────────────
for xid, xdesc, xneeds in CROSS:
    # X1/X4/X7/X8 are declared UNIMPLEMENTED by eval_assert_manager_invariants,
    # which hard-fails when asked for one ("never a silent pass"). A corpus line
    # naming one is an always-red test that proves nothing about the engine.
    if xid in CROSS_UNIMPL:
        drop("cross/%s: the assertion declares this invariant UNIMPLEMENTED and hard-fails on it"
             % xid, 2 * len(INTERACTIONS))
        continue
    assert xid in CROSS_OK, "gen_e2e_cases: %s is neither implemented nor declared unimplemented" % xid
    for ie, ik, prov, abortable in INTERACTIONS:
        if xneeds - prov:
            drop("cross/%s: the interaction never engages %s"
                 % (xid, ",".join(sorted(xneeds - prov))), 2)
            continue
        emit("cross/%s" % xid,
             "run %s to completion and assert invariant %s: %s" % (ie, xid, xdesc),
             fam="g2", rank=60)
        if abortable:
            emit("cross/%s" % xid,
                 "run %s and abort it halfway with an Escape keypress, then assert invariant %s still "
                 "holds: %s" % (ie, xid, xdesc),
                 fam=["g2", "g3"], rank=61)
        else:
            drop("cross/%s: the interaction has no abortable midpoint" % xid)

# ─────────────────────────────────────────────────────────────────────────
# RANK 70 — COMPOSITIONS: two- and three-stage manager chains
# ─────────────────────────────────────────────────────────────────────────
# THE headline fix. Each stage declares REQUIRES/PROVIDES; a sequence is emitted
# only if every stage's requirements are met by the stages BEFORE it. In the old
# corpus 1,372 of 2,040 compose lines (67%) named state no earlier stage created
# — "press and drag to start a text selection, then undo the typing" and so on.
#   (text, key, requires, provides)
STAGES = [
    ("press and drag to start a text selection", "select-drag",
     set(), {"press", "drag", "selection"}),
    ("drag past the container edge so it autoscrolls", "autoscroll",
     {"drag"}, {"scroll", "anim"}),
    ("let the scroll animation run to a stop", "anim-stop", {"anim"}, set()),
    ("hover a sibling node while the drag is live", "hover-in-drag", {"drag"}, {"hover"}),
    ("focus a contenteditable node and blink the caret", "focus-caret",
     set(), {"focus", "caret", "editable"}),
    ("type a character into the focused node", "type", {"focus"}, {"typing"}),
    ("undo the typing", "undo", {"typing"}, set()),
    ("copy the selection to the clipboard", "copy", {"selection"}, set()),
    ("scroll a virtual-view list past its over-scan window", "virtual-scroll",
     set(), {"scroll"}),
    ("drag the scrollbar thumb", "thumb-drag", set(), {"press", "drag", "scroll"}),
    ("resize the window", "resize", set(), set()),
    ("change the DPI", "dpi", set(), set()),
    ("switch a tab so a whole subtree is replaced", "tab-switch", set(), {"domregen"}),
    ("long-press to open a context menu", "long-press", set(), {"press", "gesture"}),
    ("pinch-zoom the content", "pinch", set(), {"gesture"}),
    # ── self-starting providers, so the DEPENDENT stages above get real
    #    coverage instead of the fake coverage they had. Each is a distinct
    #    interaction in its own right, not a rephrasing of an existing one.
    ("wheel-scroll the container hard enough that momentum keeps it moving", "flick",
     set(), {"scroll", "anim"}),
    ("click into a text node to give it focus and select the whole paragraph with ctrl+a",
     "select-all", set(), {"focus", "editable", "selection"}),
    ("start a smooth scroll_into_view towards an off-screen node", "smooth-sciv",
     set(), {"scroll", "anim"}),
    # ── a consumer for the one PROVIDES nothing else consumed
    ("re-run the interaction against the subtree that replaced the old one", "post-regen",
     {"domregen"}, set()),
]
STAGE_REQ = {k: _chk("stage " + k, r) for _, k, r, _ in STAGES}
STAGE_PROV = {k: _chk("stage " + k, p) for _, k, _, p in STAGES}
STAGE_TXT = {k: t for t, k, _, _ in STAGES}
STAGE_KEYS = [k for _, k, _, _ in STAGES]

COMPOSE_ASSERTS = [
    ("every stage is entered in the listed order and the whole thing reaches a fixpoint", "g1"),
    ("every repaint along the way is an incremental patch and never a full redraw", "g5"),
    ("the damage-driven buffer is pixel-identical to the full-repaint oracle at each checkpoint", "c"),
    ("no manager key points at a dead node at any checkpoint", "g2"),
    ("after the last step every state machine is idle and the window reaches FrameDamage::None", "g3"),
    ("every resource counter returns to the pre-composition baseline", "e"),
]


def coherent(seq):
    """True iff every stage's REQUIRES is met by the PROVIDES of its predecessors."""
    have = set()
    for k in seq:
        if STAGE_REQ[k] - have:
            return False
        have |= STAGE_PROV[k]
    return True


def has_dependency(seq):
    """True iff at least one stage in the sequence consumes something an earlier
    stage produced. A sequence of three mutually independent stages is just three
    tests concatenated; the point of compose/3 is the CHAIN."""
    return any(STAGE_REQ[k] for k in seq)


n2 = n3 = 0
for i in STAGE_KEYS:
    for j in STAGE_KEYS:
        if i == j:
            continue
        if not coherent((i, j)):
            drop("compose/2: stage 2 requires state stage 1 does not provide",
                 len(COMPOSE_ASSERTS))
            continue
        n2 += 1
        for atxt, afam in COMPOSE_ASSERTS:
            emit("compose/2",
                 "in one timeline, %s, then %s, and assert %s"
                 % (STAGE_TXT[i], STAGE_TXT[j], atxt),
                 fam=afam, rank=70)
for i in STAGE_KEYS:
    for j in STAGE_KEYS:
        for k in STAGE_KEYS:
            if len({i, j, k}) != 3:
                continue
            if not coherent((i, j, k)):
                drop("compose/3: a stage requires state no earlier stage provides",
                     len(COMPOSE_ASSERTS[:4]))
                continue
            if not has_dependency((i, j, k)):
                drop("compose/3: three mutually independent stages, no chain to test",
                     len(COMPOSE_ASSERTS[:4]))
                continue
            n3 += 1
            for atxt, afam in COMPOSE_ASSERTS[:4]:
                emit("compose/3",
                     "in one timeline, %s, then %s, then %s, and assert %s"
                     % (STAGE_TXT[i], STAGE_TXT[j], STAGE_TXT[k], atxt),
                     fam=afam, rank=72)

# ─────────────────────────────────────────────────────────────────────────
# RANK 80 — (g4) DANGLING INDICES UNDER MID-INTERACTION MUTATION (the payload)
# ─────────────────────────────────────────────────────────────────────────
G4_ASSERTS = [
    ("assert nothing panics and every manager key still points at the same LOGICAL node it did "
     "before the mutation", "g4"),
    ("assert no manager retains a key for a node that no longer exists (X10)", "g2"),
    ("assert the interaction either completes or is cleanly cancelled, and every state machine "
     "ends idle", "g3"),
    ("assert the mutation FIRST produces a non-empty damage set, and then " + settle(frames=5, hold=3),
     "a"),
    ("assert the frame that follows the mutation is repainted correctly with no stale pixels left "
     "from the removed content", "b"),
]
for ie, ik, prov, _ab in INTERACTIONS:
    for pd, pk, preq in PHASES:
        if _chk("phase " + pk, preq) - _chk("interaction " + ik, prov):
            drop("mutate/*: the interaction is never in the '%s' state" % pk,
                 len(MUTATIONS) * len(G4_ASSERTS))
            continue
        for md, mk in MUTATIONS:
            for atxt, afam in G4_ASSERTS:
                emit("mutate/%s" % mk,
                     "start %s, then %s %s, %s" % (ie, md, pd, atxt),
                     fam=afam, rank=80)

# ─────────────────────────────────────────────────────────────────────────
# RANK 90 — hand-authored rich seeds (what combinatorics cannot express)
# ─────────────────────────────────────────────────────────────────────────
SEEDS = [
    ("drag/nodeid", "start a node drag on the third row of a list, delete a PRECEDING sibling mid-drag, assert every manager key still points at the same logical node and nothing panics"),
    ("drag/compose", "drag from node A across B and C while the list autoscrolls, delete B mid-drag, assert the selection either drops B or clears entirely but never spans a dead node"),
    ("drag/compose", "press in the first list row, drag down past the bottom edge so the list autoscrolls, tick until the scroll animation stops, assert the selection grew across rows, every repaint was a patch and never a full redraw, and on mouse_up everything settles to zero damage"),
    ("drag/compose", "start a text-selection drag inside a scroll container, scroll the container with the wheel using the other hand mid-drag, assert the selection focus follows the content and not the screen position, and the container and selection agree at every frame"),
    ("drag/abort", "start a node drag, then rebuild the entire DOM via set_app_state mid-drag, assert the drag is cancelled rather than left dangling and both active_drag fields end as None"),
    ("drag/abort", "start a scrollbar-thumb drag, then delete the scroll container mid-drag, assert no panic, the drag context clears, and the scrollbar fade flag returns to false"),
    ("drag/dual", "start a drag through GestureAndDragManager, delete the drag source node mid-drag, and assert GestureAndDragManager.active_drag ends as None rather than holding a key to a node that no longer exists"),
    ("selection/anchor", "select text across three paragraphs, delete the paragraph containing the selection ANCHOR, assert the selection is cleared outright rather than left with a dangling anchor"),
    ("selection/anchor", "select text across three paragraphs, delete the paragraph containing the selection FOCUS, assert the selection collapses to the anchor and no dead key remains"),
    ("selection/scroll", "drag-select downward past the bottom edge of a scroll container until it autoscrolls three screens, then assert at the end of the drag that the selection anchor and focus both resolve to live nodes, no manager holds a key to a recycled row, and the window settles to zero damage"),
    ("focus/leak", "focus a contenteditable node, start the caret blink, then delete the node, assert focus is cleared, multi_cursor becomes None, the blink timer stops, and no frame is generated for a caret that no longer exists"),
    ("focus/leak", "focus a contenteditable node, blur it, and assert display_list_dirty does not stay latched true (a permanently dirty flag is a permanent repaint)"),
    ("focus/tab", "tab through a form of three fields to the submit button and back with shift-tab, assert focus lands on each field exactly once, each focus change repaints only the two affected fields, and the window settles"),
    ("scroll/idle", "mount a window with a visible scrollbar and leave it completely idle for 200 frames, assert the damage reaches None and the scrollbar does not produce false per-frame damage that burns CPU forever"),
    ("scroll/fade", "wheel-scroll once and then stop, tick until the scrollbar fade completes, assert gpu_state.scrollbar_fade_active goes false and the window stops generating frames"),
    ("scroll/momentum", "flick-scroll a list to start momentum, then click to stop it mid-flight, assert the animation is cancelled, has_active_animations goes false, and the window settles"),
    ("scroll/nested", "scroll an inner container to its end then keep scrolling, assert the scroll chains to the outer container exactly once and both containers' offsets and the rendered pixels agree"),
    ("scroll/damage", "scroll a long list by one row, assert the fast path memmoves the clip (large present damage) but PAINTS only the newly exposed strip (small paint damage), and the result is pixel-identical to a full repaint"),
    ("virtualview/mutate", "scroll a virtual-view list until rows are recycled, then delete a row that is currently inside the over-scan window, assert no VirtualViewState keys a dead node and the list still renders"),
    ("virtualview/leak", "scroll a 5000-row virtual list end to end and back, assert virtual_view state count returns to its steady-state bound rather than growing with distance scrolled"),
    ("undo/mutate", "type into a node to build an undo stack, delete the node, then trigger undo, assert no panic and the undo stack for the dead node does not resurrect a NodeId that now belongs to a different node"),
    ("undo/renumber", "type into node B to build an undo stack, insert a new node BEFORE B so every following NodeId shifts, then undo, assert the undo applies to B and not to whatever node now holds B's old NodeId"),
    ("hover/stale", "hover a button, delete it while still hovering, assert the hover history is purged, no hover state keys the dead node, and the button's pixels are cleanly repainted away"),
    ("hover/history", "sweep the mouse across 40 nodes and back, assert hover_histories stays bounded (does not accumulate one entry per node visited) and the window settles"),
    ("leak/font", "add a text node with a font family used nowhere else, remove it, force 3 GC frames, assert the registered-font and parsed-font counts return to baseline (images are the control group and MUST return to baseline)"),
    ("leak/font", "cycle a text node through 50 different font families one at a time, assert font table growth is bounded rather than monotonic across the whole run"),
    ("leak/image", "add and remove an image node 100 times, assert currently_registered_images and image_key_map return to their baseline after the GC grace window"),
    ("loop/invalidation", "install a callback that requests a DOM regeneration on every frame, assert the engine trips the bounded-work assertion rather than silently capping at recursion depth 7 and quietly failing to converge"),
    ("loop/invalidation", "make a resize handler that itself resizes the window by one pixel, assert the resize does not oscillate forever and bounded work fails loudly if it does"),
    ("mutate/renumber", "hover node 7 in a flat list, insert a node at index 0 so every NodeId shifts by one, assert the hover state follows the same LOGICAL node and does not silently point at a different, live node"),
    ("mutate/renumber", "scroll container C to a non-zero offset, insert a preceding sibling so C is renumbered, assert the scroll offset stays with C and not with whatever node inherited C's old NodeId"),
    ("mutate/midanim", "start a scroll_into_view animation towards node X, delete X while the animation is still ticking, assert the animation terminates cleanly and does not scroll to a dead target forever"),
    ("mutate/midanim", "start a smooth scroll, then rebuild the DOM mid-animation, assert either the animation is cancelled or it re-targets a live node, and in both cases the window reaches zero damage"),
    ("compose/canonical", "press in the first list row, drag down across siblings so a multi-node selection grows, keep dragging past the bottom edge so the list autoscrolls, tick so the scroll animates, assert every stage was entered in order, the repaint stayed an incremental patch, and mouse_up leaves everything idle with zero damage"),
    ("compose/stress", "run a click, a hover, a scroll, a keypress and a resize back to back with no waiting frames between them, assert the coalesced result is still correct, work stays bounded, and the window settles"),
    ("resize/damage", "window with content of a red stretched flexbox starts at 500x600 and gets resized to 800x900 via window resize, verify the output correctly refreshes, the red box still covers the entire screen, and only a partial redraw region is generated"),
    ("resize/reentrant", "resize the window while a drag is in flight, assert the drag's source node survives the relayout and the drag continues against the same logical node"),
    ("resize/reentrant", "resize the window while a scroll animation is running, assert the animation retargets against the new viewport and still terminates"),
    ("a11y/stale", "focus a node so the accessibility tree updates, delete the node, assert the a11y tree drops it and no a11y state keys a dead node"),
    ("text/stale", "set a text node to AAA then to BBBBBBBB, assert the damage is non-empty AND the pixels actually differ (a repaint that reports damage while the pixels are unchanged is the stale-text signature the harness DOES detect)"),
    ("text/ime", "start an IME preedit, then delete the node being edited mid-preedit, assert the preedit is discarded, no panic occurs, and no state machine stays armed"),
    ("clipboard/leak", "copy a large selection 100 times, assert the clipboard manager's stored content does not accumulate one copy per operation"),
    ("gesture/sessions", "perform 200 short drags in a row, assert gesture input_sessions does not accumulate and old sessions are actually cleared"),
    ("gesture/longpress", "long-press a node, then delete it before the long-press timer fires, assert the callback is not invoked against a dead node and nothing panics"),
    ("damage/disjoint", "recolour two boxes that are far apart in one frame, assert the content BETWEEN them is not erased and the damage is two patches rather than one giant bounding box"),
    ("damage/tight", "recolour exactly one cell in a 10x10 grid, assert the paint region stays local to that cell and does not balloon to the whole window"),
    ("damage/structural", "insert a new node into a flex row, assert the damage covers the new node AND every sibling that shifted to make room for it, with no under-paint"),
    ("damage/scrolled", "change the colour of a node inside a scrolled frame, assert the repaint happens at the node's VIEWPORT position, not at its unscrolled document position"),
    ("damage/overlay", "move an absolutely positioned overlay by 10px, assert both the region it vacated and the region it now occupies are repainted, and the content underneath is restored correctly"),
    ("damage/transform", "animate a CSS transform on a box for 30 frames, assert every frame's damage covers both the old and new transformed bounds and the final frame settles to None"),
    ("damage/zorder", "change the colour of a box UNDER a translucent overlay, assert the overlay is recomposited over the patch and the result matches a full repaint pixel for pixel"),
]
for tag, txt in SEEDS:
    emit(tag, txt, fam="g1", rank=90)

# a few hundred more rich seeds, derived by crossing the seed shapes with widgets
#
# The old shapes all began "start an interaction" / "run an interaction" — 120
# lines in which the thing under test was never named, so the generator picked
# one. Each shape now takes a CONCRETE interaction, chosen per widget from the
# ones that widget can actually support, and a widget that supports none of them
# is skipped rather than handed a drag it has no children to drag across.
SEED_INTERACTIONS = [
    ("a text-selection drag across two of its nodes", {"selectable"}),
    ("a wheel scroll of its scroll container", {"scrollv", "scrollh"}),
    ("a hover held over one of its nodes", {"hoverstyle"}),
    ("a caret blink in its focused contenteditable node", {"editable"}),
    ("a click on its focusable node", {"clickable"}),
]
SEED_SHAPES = [
    ("drag from the first child across three siblings while the container autoscrolls, then delete "
     "the middle sibling mid-drag, and assert the interaction survives, the selection never spans a "
     "dead node, and everything settles", {"children", "scrollv"}, None),
    ("start %s, resize the window mid-interaction, and assert the interaction continues "
     "against the same logical nodes and the repaint stays incremental", None, "one"),
    ("start %s, change the DPI mid-interaction, and assert nothing panics and the window "
     "still reaches zero damage", None, "one"),
    ("run %s to completion, then run it again immediately, and assert the second run's "
     "resource counters match the first run's (no per-interaction leak)", None, "one"),
    ("run %s while a timer mutates an unrelated subtree on every frame, and assert the "
     "two do not interfere and both settle", None, "one"),
    ("run %s, then rebuild the entire DOM via set_app_state, and assert no manager "
     "carries a key across the rebuild that does not resolve to a live node", None, "one"),
]
for wd, wk, caps in WIDGETS:
    supported = [t for t, need in SEED_INTERACTIONS if need & caps]
    for shape, need, mode in SEED_SHAPES:
        if need is not None:
            if need - caps:
                drop("seed/%s: the widget cannot support the shape" % wk)
                continue
            emit("seed/%s" % wk, "with %s mounted, %s" % (wd, shape), fam=["g1", "g4"], rank=92)
            continue
        if not supported:
            drop("seed/%s: the widget supports no interaction to run" % wk)
            continue
        for ia in supported:
            emit("seed/%s" % wk, "with %s mounted, %s" % (wd, shape % ia),
                 fam=["g1", "g4"], rank=92)
# the empty body deserves its own degenerate cases rather than a drag across
# children it does not have
for shape in [
    "resize the window five times in a row and assert every frame still settles to zero damage "
    "with ZERO layout passes wasted on the absent content",
    "deliver a click, a wheel scroll, a keypress and a text_input into the void and assert every "
    "one is absorbed without a panic and without scheduling a repaint",
    "rebuild the DOM via set_app_state 50 times and assert no manager accumulates a key and every "
    "resource counter stays at zero",
]:
    emit("seed/empty", "with an empty body with no content at all mounted, %s" % shape,
         fam=["f", "e"], rank=92)


# ─────────────────────────────────────────────────────────────────────────
# 3. SELF-AUDIT — an OUTSIDE check on the finished text
# ─────────────────────────────────────────────────────────────────────────
# Deliberately NOT expressed with the tables above: this scans the emitted
# STRINGS for referring phrases and for the phrase that would have to establish
# them. If the two ever drift apart, this fails and the corpus is not written.
REFERENTS = [
    ("live-drag",
     r"while the drag is live|drag past the container edge"
     r"|mid-drag, after several mouse_move steps|the active drag\b|the drag's source node",
     r"press and drag|a text-selection drag|a node drag|scrollbar-thumb drag"
     r"|drag the scrollbar thumb|start a node drag|start a drag|starting a drag"
     r"|starting a legacy drag|drag from node|drag from the first|press in the first"
     r"|drag-select|mouse_down|drag through GestureAndDragManager|resize the window while a drag"),
    ("typing",
     r"undo the typing|undo the edit|redo the typing",
     r"typ(e|ing|ed)\b|text_input|insert_text|a character into|keystroke"),
    ("scroll-animation",
     r"the resulting scroll animation|let the scroll animation run to a stop"
     r"|tick until the scroll animation stops",
     r"autoscroll|momentum|smooth|scroll_into_view|scroll_node_into_view|flick"
     r"|scroll a virtual-view list|past the bottom edge|animated scroll"),
    ("selection",
     r"copy the selection|the live selection|the selection anchor",
     r"select"),
    ("focus",
     r"into the focused node|the focused node",
     r"focus|contenteditable|text input"),
]

# UNIMPLEMENTED CROSS-INVARIANTS, guarded from the OUTSIDE.
# `eval_assert_manager_invariants` hard-FAILS on X1/X4/X7/X8 ("never a silent
# pass"), so any line that describes one is an always-red test that proves
# nothing about the engine — and the tag-level filter in the cross/* block above
# cannot see a HAND-WRITTEN seed that describes the same invariant in prose. The
# guard is keyed on the SAME set the assertion declares, so the day an invariant
# is implemented the guard stops applying by itself.
UNIMPL_GUARD = {
    "X1": r"scroll_into_view and ScrollManager agree",
    "X4": r"DragDropManager",
    "X7": r"scroll adjustment stays pending",
    "X8": r"selection focus and the (autoscrolled|scrolled) container",
}
_ungarded = CROSS_UNIMPL - set(UNIMPL_GUARD)
assert not _ungarded, (
    "gen_e2e_cases: full.rs declares %s UNIMPLEMENTED but no UNIMPL_GUARD pattern covers it — add "
    "one, or a hand-written seed will smuggle an always-red line into the corpus." % sorted(_ungarded))

# Families whose whole claim is an ABSENCE. A line in one of these that does not
# also prove the mechanism FIRES is indistinguishable from the same line run
# against a dead engine, so carrying a positive control is a hard requirement,
# not a style preference.
CONTROL_REQUIRED_TAGS = ("idle/stability", "idle/bounded", "idle/noop-css", "idle/noop-text",
                         "idle/noop-class", "input/over-invalidation", "resize/noop",
                         "scroll/noop", "css/none")


def self_audit(lines):
    bad = []
    for line in lines:
        tag = line[1:line.index("]")]
        b = line[line.index("] ") + 2:]
        for name, ref, est in REFERENTS:
            m = re.search(ref, b)
            if m and not re.search(est, b[:m.start()]):
                bad.append((name, line))
        for xid, pat in UNIMPL_GUARD.items():
            if xid in CROSS_UNIMPL and re.search(pat, b):
                bad.append(("unimplemented-invariant/" + xid, line))
        if tag in CONTROL_REQUIRED_TAGS and "POSITIVE CONTROL" not in b \
                and "positive control" not in b:
            bad.append(("absence-without-positive-control", line))
    return bad


# --------------------------------------------------------------------------
# 4. WRITE + COVERAGE
# --------------------------------------------------------------------------
assert not COLLISIONS, "COLLISIONS: %d, e.g. %r" % (len(COLLISIONS), COLLISIONS[:3])

LINES.sort(key=lambda t: (t[0], t[1]))
_text = [l for _, _, l in LINES]

_bad = self_audit(_text)
assert not _bad, (
    "gen_e2e_cases: SELF-AUDIT FAILED — %d line(s) refer to state no earlier step establishes.\n"
    "The first three:\n  %s" % (len(_bad), "\n  ".join("[%s] %s" % b for b in _bad[:3])))

body = "\n".join(_text) + "\n"

# ─────────────────────────────────────────────────────────────────────────
# 3b. WAVE 1 — the subset that the HEADLESS RUNNER can execute faithfully today
# ─────────────────────────────────────────────────────────────────────────
# The corpus is a specification and keeps every line, including the ones the
# runner cannot yet drive: those lines are correct and the runner is what is
# deficient (it now records each such `CallbackChange` via `unsupported()` and
# FAILS the scenario, rather than reporting a vacuous pass).
#
# But there is no point spending LLM budget generating a test that is guaranteed
# to red on the harness rather than on the engine. So a second file carries the
# subset worth generating first. Same lines, byte for byte, so the artifacts are
# shared with the full corpus and nothing is regenerated later.
#
# Membership is DERIVED, not hand-listed: when the missing runner arms land, the
# `unsupported(...)` call sites disappear and re-running this script promotes
# those lines into Wave 1 automatically.
WAVE1 = os.path.join(ROOT, "scripts", "E2E_TESTS_WAVE1.txt")


def refused_change_variants():
    """The `CallbackChange` variants `Runner::apply_change` refuses by name."""
    src = read(os.path.join(ROOT, "layout/src/e2e/runner.rs"))
    variants = set(re.findall(r'unsupported\(\s*"([A-Za-z0-9]+)"', src))
    assert variants, "gen_e2e_cases: no `unsupported(\"...\")` call sites in runner.rs"
    return variants


REFUSED_CHANGES = refused_change_variants()


def unsupported_callback_fns():
    """CallbackInfo fns whose CallbackChange the headless runner cannot apply."""
    # a CallbackChange variant can be raised by several API fns
    alias = {"CreateNewWindow": ["create_window"],
             "OpenMenu": ["open_menu", "open_menu_at", "open_menu_for_hit_node",
                          "open_menu_for_node"],
             "ShowTooltip": ["show_tooltip", "show_tooltip_at"]}
    fns, unmapped = set(), []
    for v in sorted(REFUSED_CHANGES):
        cands = alias.get(v, [snake(v)])
        hit = [c for c in cands if c in CALLBACK_FNS]
        if hit:
            fns |= set(hit)
        else:
            unmapped.append(v)
    assert not unmapped, (
        "gen_e2e_cases: runner.rs marks CallbackChange::%s unsupported but no CallbackInfo fn "
        "maps to it — add an alias so the wave split cannot rot." % unmapped)
    return fns


UNSUPPORTED_FNS = unsupported_callback_fns()

# THE SECOND LAYER OF THE SAME GAP. `OP_POLICY` ALLOWS `swipe` / `pinch` /
# `rotate` / `long_press` — they are not denied and they are not zombies (they
# have real match arms) — and every one of them funnels into
# `CallbackChange::InjectNativeGesture`, which the runner refuses by name. The
# zombie check looks at the op DISPATCH; the policy looks at the op TABLE;
# neither can see the CallbackChange in between. So map the refusals down to the
# DEBUG OPS that raise them and let the wave split read that. The mapping is
# asserted TOTAL against the runner's call sites, so a new refusal cannot be
# silently ignored, and a refusal that disappears auto-promotes its lines.
REFUSED_CHANGE_OPS = {
    "InjectNativeGesture": ["swipe", "pinch", "rotate", "long_press"],
    "AddTimer": ["add_timer"], "RemoveTimer": ["remove_timer"],
    "StartCursorBlinkTimer": [], "StopCursorBlinkTimer": [],
    "AddThread": ["add_thread"], "RemoveThread": ["remove_thread"],
    "SetCopyContent": [], "SetCutContent": [],
    "CreateNewWindow": ["create_window"], "BeginInteractiveMove": [],
    "OpenMenu": [], "ShowTooltip": [], "HideTooltip": [],
    "AddImageToCache": ["add_image_to_cache"],
    "RemoveImageFromCache": ["remove_image_from_cache"],
    "CommitUndoSnapshot": ["commit_undo_snapshot"],
    "UndoAppState": ["undo_app_state"], "RedoAppState": ["redo_app_state"],
    "SwitchRoute": [],
}
_unmapped = REFUSED_CHANGES - set(REFUSED_CHANGE_OPS)
assert not _unmapped, (
    "gen_e2e_cases: runner.rs refuses CallbackChange::%s but REFUSED_CHANGE_OPS does not say which "
    "debug op(s) raise it — add a row (an empty list is fine when no op reaches it) so wave 1 "
    "cannot silently include a red-on-arrival line." % sorted(_unmapped))
REFUSED_OPS = {o for v in REFUSED_CHANGES for o in REFUSED_CHANGE_OPS[v]}
# The caret blink is armed by a FOCUS CHANGE, not by an op, so no op name
# identifies it; the phrase does.
REFUSED_PHRASES = []
if "StartCursorBlinkTimer" in REFUSED_CHANGES:
    REFUSED_PHRASES.append((r"caret blink|blinking caret|blink the caret|start the caret",
                            "the runner has no timer driver, so the caret blink never fires"))
# managers whose state is filled only by an OS facility the headless runner has
# no mock for; a `manager/<x>` line for one of them cannot be driven at all.
HOSTLESS_MANAGERS = {"biometric", "clipboard", "file_drop", "gamepad", "geolocation",
                     "keyring", "permission", "sensors"}


def wave1_defer(line):
    """Reason this line is NOT in wave 1, or None if it is."""
    tag = line[1:line.index("]")]
    if tag.startswith("compose/"):
        return "multi-stage chain: needs text editing + gesture injection end to end"
    if tag.startswith("manager/") and tag.split("/", 1)[1] in HOSTLESS_MANAGERS:
        return "manager needs an OS facility the headless runner does not mock"
    m = re.search(r"CallbackInfo::([a-z_0-9]+)", line)
    if m and m.group(1) in UNSUPPORTED_FNS:
        return "runner.rs marks this CallbackChange unsupported (it fails the scenario)"
    if LINE_OPS.get(line, ()) & REFUSED_OPS:
        return ("the op raises a CallbackChange the runner refuses by name — allowed by OP_POLICY "
                "and not a zombie, so only the runner's own refusal list can see it")
    body = line[line.index("] ") + 2:]
    for pat, why in REFUSED_PHRASES:
        if re.search(pat, body):
            return why
    for o in sorted(REFUSED_OPS):
        if re.search(r"\b%s\b" % re.escape(o), body):
            return ("the op raises a CallbackChange the runner refuses by name — allowed by "
                    "OP_POLICY and not a zombie, so only the runner's own refusal list can see it")
    return None


_wave1, _defer = [], Counter()
for l in _text:
    why = wave1_defer(l)
    if why:
        _defer[why] += 1
    else:
        _wave1.append(l)
wave1_body = "\n".join(_wave1) + "\n"

if "--check" in sys.argv:
    ok = (read(OUT) if os.path.exists(OUT) else "") == body
    ok = ok and (read(WAVE1) if os.path.exists(WAVE1) else "") == wave1_body
    sys.exit(0 if ok else 1)

with open(OUT, "w", encoding="utf-8") as f:
    f.write(body)
with open(WAVE1, "w", encoding="utf-8") as f:
    f.write(wave1_body)


def table(title, universe, cover, excluded=()):
    miss = [x for x in universe if not cover.get(x) and x not in excluded]
    note = "OK" if not miss else "MISSING: " + ", ".join(miss)
    if excluded:
        note += "   (deliberately excluded: %s)" % ", ".join(sorted(excluded))
    print("%-28s %5d / %-5d covered   %s"
          % (title, len(universe) - len(miss) - len(excluded), len(universe), note))
    return miss


print("wrote %s: %d lines (0 duplicates, uniqueness enforced on the normalized key)"
      % (OUT, len(LINES)))
print("self-audit: 0 dangling referents over %d lines" % len(LINES))
print("wrote %s: %d lines (the subset the headless runner can drive today)"
      % (WAVE1, len(_wave1)))
for k, v in _defer.most_common():
    print("    deferred %5d  %s" % (v, k))
print()
print("COVERAGE")
table("Callback API functions", CALLBACK_FNS, COVER_CB, excluded=CB_OUT_OF_SCOPE)
table("Debug ops (usable)", DEBUG_OPS, COVER_OP)
table("Managers", MANAGERS, COVER_MGR)
table("Assertion families", list(FAMILIES), COVER_FAM)
print("%-28s %5d denied by OP_POLICY, %d zombie(s), %d of %d usable"
      % ("Debug ops (excluded)", len(DENIED_OPS), len(ZOMBIE_OPS), len(DEBUG_OPS), len(ALL_OPS)))
print("%-28s compose/2 %d of %d orderings, compose/3 %d chains"
      % ("Coherent compositions", n2, len(STAGE_KEYS) * (len(STAGE_KEYS) - 1), n3))
print()
c = Counter(l.split("]")[0][1:].split("/")[0] for l in _text)
print("BY CATEGORY")
for k, v in sorted(c.items(), key=lambda kv: -kv[1]):
    print("  %-14s %6d" % (k, v))
print()
print("DROPPED (combinations deliberately NOT emitted; total %d)" % sum(DROPPED.values()))
for k, v in sorted(DROPPED.items(), key=lambda kv: -kv[1])[:20]:
    print("  %6d  %s" % (v, k))
