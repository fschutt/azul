#!/usr/bin/env python3
"""
gen_e2e_cases.py — the reproducible expander for scripts/E2E_TESTS.txt.

Reads the REAL surface out of the repo (never hand-typed lists):

  * Callback API      -> api.json  ["0.2.0"]["api"]["callbacks"]["classes"]["CallbackInfo"]["functions"]
                         UNION the `impl CallbackInfo` blocks in layout/src/callbacks.rs
  * Mock input / debug ops -> `pub enum DebugEvent` in
                         dll/src/desktop/shell2/common/debug_server/full.rs:1526
  * Managers          -> layout/src/managers/*.rs
  * Assertion families-> scripts/E2E_PLAN.md  §B  (a..f, g1..g5)

then crosses them combinatorially (interaction x target x mutation x phase x
assertion-family) and emits ONE natural-language e2e case per line, ordered
SIMPLE -> COMPLEX.

Scope rule (E2E_PLAN.md §0.1): BEHAVIOUR ONLY. No geometry assertions, ever.
`azul-doc reftest` owns layout/CSS/pixel correctness.

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
from collections import OrderedDict

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "scripts", "E2E_TESTS.txt")

FULL_RS = os.path.join(ROOT, "dll/src/desktop/shell2/common/debug_server/full.rs")
CALLBACKS_RS = os.path.join(ROOT, "layout/src/callbacks.rs")
API_JSON = os.path.join(ROOT, "api.json")
MANAGERS_DIR = os.path.join(ROOT, "layout/src/managers")


# --------------------------------------------------------------------------
# 1. ENUMERATE THE REAL SURFACE
# --------------------------------------------------------------------------
def read(p):
    with open(p, "r", encoding="utf-8") as f:
        return f.read()


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


def debug_ops():
    """Every DebugEvent variant = every mock input event / debug op the harness drives."""
    src = read(FULL_RS)
    start = src.index("pub enum DebugEvent {")
    i, depth = src.index("{", start) + 1, 1
    while i < len(src) and depth:
        if src[i] == "{":
            depth += 1
        elif src[i] == "}":
            depth -= 1
        i += 1
    body = src[start:i]
    variants = re.findall(r"^    ([A-Z][A-Za-z0-9]*)", body, re.M)
    return sorted(set(variants))


def snake(name):
    return re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower()


def managers():
    out = []
    for f in sorted(os.listdir(MANAGERS_DIR)):
        if f.endswith(".rs") and f != "mod.rs":
            out.append(f[:-3])
    return out


CALLBACK_FNS = callback_api_functions()
DEBUG_OPS = debug_ops()
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

# ── the axes ──────────────────────────────────────────────────────────────
WIDGETS = [
    ("a red stretched flexbox filling the window", "flexbox"),
    ("a single grey button with a :hover rule", "button"),
    ("a paragraph of selectable text", "text"),
    ("a contenteditable single-line input", "input"),
    ("a contenteditable multi-line textarea", "textarea"),
    ("a vertically scrollable list of 40 rows", "list"),
    ("a horizontally scrollable strip of 40 cells", "hstrip"),
    ("a scroll container nested inside another scroll container", "nested-scroll"),
    ("a 10x10 grid of coloured boxes", "grid"),
    ("an image node inside a flex row", "image"),
    ("a virtual-view list with 5000 virtual rows", "virtual-list"),
    ("a table with 20 rows and 5 columns", "table"),
    ("an absolutely positioned overlay above the content", "overlay"),
    ("two z-index stacked translucent boxes", "zstack"),
    ("a box with a CSS transform applied", "transformed"),
    ("a clipped overflow:hidden box with content larger than itself", "clipped"),
    ("a sticky header above a scrolling body", "sticky"),
    ("a form with three focusable fields and a submit button", "form"),
    ("a tab strip with three switchable panels", "tabs"),
    ("a tree view with expandable nodes", "tree"),
    ("a text node whose font-family is unique to it", "unique-font"),
    ("a container with a custom scrollbar that fades out", "fading-scrollbar"),
    ("a deeply nested 12-level div chain with one leaf", "deep-nest"),
    ("an empty body with no content at all", "empty"),
]

CSS_PROPS = [
    ("background-color", "paint-only"), ("color", "paint-only"),
    ("opacity", "paint-only"), ("border-color", "paint-only"),
    ("border-width", "layout"), ("box-shadow", "paint-only"),
    ("width", "layout"), ("height", "layout"),
    ("padding", "layout"), ("margin", "layout"),
    ("display", "structural"), ("flex-direction", "layout"),
    ("flex-grow", "layout"), ("justify-content", "layout"),
    ("align-items", "layout"), ("font-size", "layout"),
    ("font-family", "layout"), ("font-weight", "layout"),
    ("line-height", "layout"), ("letter-spacing", "layout"),
    ("text-align", "layout"), ("overflow", "structural"),
    ("position", "structural"), ("z-index", "paint-only"),
    ("transform", "paint-only"), ("visibility", "paint-only"),
    ("cursor", "none"), ("border-radius", "paint-only"),
    ("background-image", "paint-only"), ("white-space", "layout"),
]

MUTATIONS = [
    ("delete the node itself", "delete-self"),
    ("delete the node's parent", "delete-parent"),
    ("delete a PRECEDING sibling so every following NodeId is renumbered", "delete-prev-sibling"),
    ("insert a new PRECEDING sibling so every following NodeId is renumbered", "insert-prev-sibling"),
    ("replace the whole subtree under the node", "replace-subtree"),
    ("reorder the node's siblings", "reorder-siblings"),
    ("rebuild the entire DOM from scratch via set_app_state", "full-rebuild"),
    ("change the node's classes so its style but not its identity changes", "reclass"),
]

PHASES = [
    ("immediately after mouse_down, before any movement", "after-down"),
    ("mid-drag, after several mouse_move steps", "mid-drag"),
    ("while the resulting scroll animation is still ticking", "mid-animation"),
    ("exactly one frame before mouse_up", "pre-up"),
    ("immediately after mouse_up, in the same frame", "post-up"),
]

INTERACTIONS = [
    ("a text-selection drag across the node", "text-select-drag"),
    ("a node drag on the node", "node-drag"),
    ("a scrollbar-thumb drag on the node's scroll container", "thumb-drag"),
    ("a momentum scroll of the node's container", "momentum-scroll"),
    ("a scroll_into_view animation targeting the node", "scroll-into-view"),
    ("keyboard focus plus a blinking caret on the node", "focus-caret"),
    ("a hover held over the node", "hover"),
    ("a virtual-view scroll that has the node in its over-scan window", "virtual-scroll"),
    ("an undo stack recorded against the node", "undo-stack"),
    ("a long-press gesture on the node", "long-press"),
    ("a pinch gesture centred on the node", "pinch"),
    ("a pen stroke drawn over the node", "pen-stroke"),
]

INPUT_EVENTS = [
    ("a single left click", "click"),
    ("a double click", "double-click"),
    ("a right click", "right-click"),
    ("a middle click", "middle-click"),
    ("a mouse_down without a matching mouse_up", "down-only"),
    ("a mouse_move that enters the node", "move-in"),
    ("a mouse_move that leaves the node", "move-out"),
    ("a mouse_move that stays inside the node and changes nothing", "move-noop"),
    ("a mouse_up delivered outside the window bounds", "up-outside"),
    ("a vertical wheel scroll", "wheel-v"),
    ("a horizontal wheel scroll", "wheel-h"),
    ("a diagonal wheel scroll", "wheel-d"),
    ("a key_down of the Tab key", "key-tab"),
    ("a key_down of the Escape key", "key-esc"),
    ("a key_down of the ArrowDown key", "key-down"),
    ("a key_down followed by the matching key_up", "key-updown"),
    ("a text_input of a single character", "text-1"),
    ("a text_input of a 200 character paragraph", "text-200"),
    ("a window resize that grows both axes", "resize-grow"),
    ("a window resize that shrinks both axes", "resize-shrink"),
    ("a dpi_changed event doubling the scale factor", "dpi"),
    ("a touch_start / touch_move / touch_end sequence", "touch"),
    ("a swipe gesture", "swipe"),
    ("a pen_down / pen_move / pen_up sequence", "pen"),
]

SCROLL_MODES = [
    ("an instant scroll", "instant"),
    ("a smooth animated scroll", "smooth"),
    ("a wheel scroll with momentum", "momentum"),
    ("a scrollbar-thumb drag", "thumb"),
    ("a programmatic scroll_node_to", "programmatic"),
]

RESOURCES = [
    ("registered fonts", "fonts"),
    ("font hash map entries", "font_hash_map"),
    ("parsed font bytes in the FontManager", "parsed_fonts"),
    ("the font chain cache", "font_chain_cache"),
    ("registered images", "images"),
    ("image key map entries", "image_key_map"),
    ("scroll manager states", "scroll_states"),
    ("hover hit-test histories", "hover_histories"),
    ("gesture input sessions", "input_sessions"),
    ("virtual view states", "virtual_view_states"),
    ("undo/redo node stacks", "undo_stacks"),
    ("GPU value caches", "gpu_caches"),
]

# X1..X10 straight out of E2E_PLAN.md §B(g2)
CROSS = [
    ("X1", "scroll_into_view and ScrollManager agree on which container scrolled and by how much"),
    ("X2", "has_active_animations() is true exactly when some AnimatedScrollState has an animation and tick() asks for a repaint"),
    ("X3", "the active drag's source node still exists and the hover manager resolves it against the same DOM"),
    ("X4", "GestureAndDragManager.active_drag and the legacy DragDropManager.active_drag never disagree about whether a drag is live"),
    ("X5", "the selection anchor's node exists for the whole drag, or the selection is cleared outright"),
    ("X6", "multi_cursor is Some only while a contenteditable node that exists has focus"),
    ("X7", "no scroll adjustment stays pending for a caret whose focus was cleared"),
    ("X8", "selection focus and the autoscrolled container stay mutually consistent frame to frame"),
    ("X9", "scrollbar_fade_active returns to false within the fade duration of the last scroll"),
    ("X10", "no manager key refers to a node that no longer exists"),
]

MANAGER_LIFECYCLE = [
    ("acquires state on the first relevant event", "acquire"),
    ("keeps exactly one entry per live node, never two", "no-dupes"),
    ("drops its entry when the node is unmounted", "gc-on-unmount"),
    ("remaps its key when a preceding sibling is inserted", "remap-insert"),
    ("remaps its key when a preceding sibling is deleted", "remap-delete"),
    ("survives a full DOM rebuild without holding a dead key", "survive-rebuild"),
    ("returns to an empty/idle state once the interaction ends", "idle-after"),
    ("does not grow across 200 idle frames", "no-growth"),
    ("does not latch a permanently-dirty flag", "no-latched-dirty"),
    ("does not force a full redraw when only it changed", "patch-only"),
    ("reports the same key set through debug_counts as the DOM actually contains", "counts-match-dom"),
    ("is unaffected by a mutation in an unrelated subtree", "isolation"),
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


def norm(s):
    return re.sub(r"\s+", " ", re.sub(r"[^a-z0-9 ]", " ", s.lower())).strip()


def emit(tag, text, *, cb=None, op=None, mgr=None, fam=None, rank=0):
    """Emit one test line. `rank` orders simple -> complex."""
    line = "[%s] %s" % (tag, text)
    k = norm(line)
    if k in SEEN:
        COLLISIONS.append((line, SEEN[k]))
        return
    SEEN[k] = line
    LINES.append((rank, len(LINES), line))
    for name, bucket in ((cb, COVER_CB), (op, COVER_OP), (mgr, COVER_MGR), (fam, COVER_FAM)):
        for x in (name if isinstance(name, (list, tuple, set)) else [name] if name else []):
            bucket.setdefault(x, 0)
            bucket[x] += 1


# ─────────────────────────────────────────────────────────────────────────
# RANK 10 — idle stability: the simplest possible test, one widget, one assert
# ─────────────────────────────────────────────────────────────────────────
for wd, wk in WIDGETS:
    emit("idle/stability",
         "mount %s, tick 5 frames with no input at all, assert every frame is byte-identical to "
         "the previous one and the paint damage drains to None and stays there" % wd,
         fam="a", op="WaitFrame", rank=10)
    emit("idle/bounded",
         "mount %s, tick 5 frames with no input, assert relayout_iterations stays at 0 and "
         "dom_regenerations stays at 0 for every idle frame" % wd,
         fam="d", op="WaitFrame", rank=10)
    emit("idle/growth",
         "mount %s and run 200 idle frames, assert every manager debug_counts counter has zero "
         "slope and RSS does not climb" % wd,
         fam="f", op="Wait", rank=11)
    emit("idle/relayout-noop",
         "mount %s then issue a relayout op with no state change, assert the resulting damage is "
         "None and no pixel differs from the previous frame" % wd,
         fam=["a", "c"], op="Relayout", rank=11)
    emit("idle/redraw-noop",
         "mount %s then issue a redraw op with no state change, assert the redraw produces no "
         "paint damage and the frame is unchanged" % wd,
         fam="a", op="Redraw", rank=11)

# ─────────────────────────────────────────────────────────────────────────
# RANK 20 — one input event, one widget, one assertion family
# ─────────────────────────────────────────────────────────────────────────
INPUT_ASSERTS = [
    ("liveness", "assert the damage set is non-empty and the pixels actually changed", "b"),
    ("damage", "assert the damage-driven buffer is pixel-identical to a full repaint and the "
               "paint region does not balloon to the whole window", "c"),
    ("bounded", "assert the event costs at most 2 relayout iterations, 0 DOM regenerations and "
                "never trips the recursion depth cap", "d"),
    ("settle", "assert that after the event the window returns to idle with zero damage within "
               "5 ticks", "a"),
]
for ie, ik in INPUT_EVENTS:
    for wd, wk in WIDGETS:
        for aname, atxt, afam in INPUT_ASSERTS:
            emit("input/%s" % aname,
                 "mount %s and deliver %s to it, %s" % (wd, ie, atxt),
                 fam=afam, rank=20)

# every raw debug op gets its own dedicated line set
OP_TEMPLATES = [
    ("smoke", "assert the op is accepted, no panic occurs, and the process stays alive", "f", 21),
    ("settle", "assert the window returns to a zero-damage idle state within 5 ticks afterwards", "a", 22),
    ("bounded", "assert the op costs a bounded number of relayout iterations and never trips the "
                "MAX_EVENT_RECURSION_DEPTH cap", "d", 22),
    ("incremental", "assert any repaint it causes is a Rects patch and never FrameDamage::Full "
                    "unless the op is structural by declaration", "g5", 23),
    ("resources", "assert every resource counter returns to its pre-op baseline after 3 GC frames",
     "e", 23),
    ("managers", "assert no manager key points at a node that does not exist afterwards", "g2", 23),
    ("repeat", "issue the op 50 times in a row and assert no counter grows without bound and the "
               "frame still settles", "f", 24),
    ("interleave", "issue the op while a scroll animation is already running and assert both the "
                   "animation and the op complete and the window settles", "g1", 25),
]
for op in DEBUG_OPS:
    o = snake(op)
    for suffix, atxt, afam, rank in OP_TEMPLATES:
        emit("op/%s" % suffix,
             "drive the %s debug op against a mounted DOM, %s" % (o, atxt),
             op=op, fam=afam, rank=rank)

# ─────────────────────────────────────────────────────────────────────────
# RANK 30 — CSS-override damage matrix (behavioural: refresh + patch + settle)
# ─────────────────────────────────────────────────────────────────────────
PROP_ASSERTS = {
    "paint-only": [
        ("assert the pixels change, the damage is a patch, and it does not trigger a relayout", "c", 30),
        ("assert the damage-driven render is pixel-identical to the full repaint of the same frame", "c", 31),
        ("assert the window settles back to zero damage immediately afterwards", "a", 31),
    ],
    "layout": [
        ("assert the change is repainted, the damage covers every pixel that actually differs, and "
         "no under-paint leaves a stale region on screen", "c", 32),
        ("assert at most one relayout iteration is spent and no DOM regeneration happens", "d", 32),
        ("assert the window settles back to zero damage afterwards", "a", 32),
    ],
    "structural": [
        ("assert the repaint covers every changed pixel, is declared structural if it goes Full, and "
         "the window settles afterwards", "c", 33),
        ("assert every manager key still points at a live node afterwards", "g2", 33),
        ("assert the change costs at most one DOM regeneration", "d", 33),
    ],
    "none": [
        ("assert nothing is repainted at all and the damage stays None", "a", 30),
        ("assert no relayout is triggered and the frame is byte-identical", "a", 30),
        ("assert no resource counter moves", "e", 31),
    ],
}
for prop, kind in CSS_PROPS:
    for wd, wk in WIDGETS:
        for atxt, afam, rank in PROP_ASSERTS[kind]:
            emit("css/%s" % kind,
                 "mount %s and change its %s via set_node_css_override, %s" % (wd, prop, atxt),
                 op="SetNodeCssOverride", fam=afam, rank=rank)

# ─────────────────────────────────────────────────────────────────────────
# RANK 35 — resize / dpi damage (the user's own example lives here)
# ─────────────────────────────────────────────────────────────────────────
RESIZES = [
    ("500x600", "800x900", "grow both axes"),
    ("800x900", "500x600", "shrink both axes"),
    ("400x300", "400x900", "grow only the height"),
    ("400x300", "1200x300", "grow only the width"),
    ("800x600", "801x600", "grow the width by a single pixel"),
    ("800x600", "800x600", "resize to the exact same size"),
    ("300x200", "60x40", "shrink below the content's minimum"),
    ("640x480", "1920x1080", "grow by more than 4x in area"),
]
for wd, wk in WIDGETS:
    for a, b, desc in RESIZES:
        emit("resize/damage",
             "window starts at %s with %s and is resized to %s (%s) via a window resize event, "
             "assert the output refreshes, the content still covers the viewport it is supposed to, "
             "and only a partial redraw region is generated rather than a full redraw"
             % (a, wd, b, desc),
             op="Resize", fam=["b", "c", "g5"], rank=35)
        emit("resize/settle",
             "window starts at %s with %s and is resized to %s (%s), assert the window reaches "
             "zero damage within 5 ticks and does not re-enter an invalidation loop"
             % (a, wd, b, desc),
             op="Resize", fam=["a", "d"], rank=36)
for wd, wk in WIDGETS:
    for scale in ("1.0 to 2.0", "2.0 to 1.0", "1.0 to 1.5", "1.5 to 1.25"):
        emit("dpi/damage",
             "mount %s and change the DPI scale from %s via dpi_changed, assert the whole content "
             "is repainted correctly, no stale region survives, and the window settles to zero "
             "damage afterwards" % (wd, scale),
             op="DpiChanged", fam=["b", "a"], rank=37)

# ─────────────────────────────────────────────────────────────────────────
# RANK 40 — scroll matrix
# ─────────────────────────────────────────────────────────────────────────
SCROLL_TARGETS = [
    ("a 40-row vertical list", "list"),
    ("a horizontally scrolling strip", "hstrip"),
    ("a scroll container nested inside another scroll container", "nested"),
    ("a virtual-view list of 5000 rows", "virtual"),
    ("a scroll container with a sticky header", "sticky"),
    ("a scroll container whose content exactly fits (no overflow)", "nooverflow"),
    ("a scroll container already pinned at its bottom edge", "at-end"),
    ("a scroll container inside a CSS transform", "transformed"),
]
SCROLL_ASSERTS = [
    ("assert the exposed strip is the only region PAINTED while the present damage covers the whole "
     "memmoved clip, and the damage-driven buffer matches a full repaint exactly", "g5", 40),
    ("assert the ScrollManager offset and the rendered content agree and no stale row is left "
     "behind at the old offset", "b", 41),
    ("assert the scroll animation terminates, has_active_animations() goes false, scroll_dirty "
     "clears, and the window reaches zero damage", "g3", 42),
    ("assert the scrollbar fade completes and gpu_state.scrollbar_fade_active returns to false so "
     "the window stops generating frames", "g3", 42),
    ("assert the scroll costs no DOM regeneration and at most one relayout iteration", "d", 41),
]
for st, sk in SCROLL_TARGETS:
    for sm, smk in SCROLL_MODES:
        for atxt, afam, rank in SCROLL_ASSERTS:
            emit("scroll/%s" % smk,
                 "perform %s on %s, %s" % (sm, st, atxt),
                 op="Scroll", mgr="scroll_state", fam=afam, rank=rank)

# ─────────────────────────────────────────────────────────────────────────
# RANK 45 — every Callback API function
# ─────────────────────────────────────────────────────────────────────────
# classify so the generated assertion is honest about what the fn does
def cb_kind(fn):
    if fn.startswith(("get_", "has_", "is_", "was_", "can_", "node_has", "had_", "inspect_", "measure_", "compare_", "strings_", "format_", "pluralize", "sort_")):
        return "query"
    if fn.startswith("take_") or fn.startswith("take_native"):
        return "screenshot"
    return "mutator"


CB_TEMPLATES = {
    "query": [
        ("read", "call CallbackInfo::%s from inside a callback fired by a click and assert it "
                 "returns a value consistent with the current DOM and that merely calling it "
                 "produces NO damage and NO relayout", "a", 45),
        ("idle", "call CallbackInfo::%s on every one of 20 idle frames and assert the window still "
                 "reaches FrameDamage::None and no counter grows", "f", 46),
        ("stale", "call CallbackInfo::%s from a callback that runs one frame AFTER the node it "
                  "refers to was deleted, assert it fails gracefully (None/empty) instead of "
                  "panicking or returning a dangling id", "g4", 47),
        ("mutation", "call CallbackInfo::%s immediately after a preceding sibling was inserted so "
                     "every following NodeId shifted, assert the value it returns refers to the "
                     "same LOGICAL node as before the shift", "g4", 48),
        ("leak", "call CallbackInfo::%s 500 times in a loop from a timer callback and assert every "
                 "resource counter returns to its baseline and RSS does not climb", "e", 46),
        ("bounded", "call CallbackInfo::%s from a callback and assert the event it runs in still "
                    "terminates within the relayout iteration budget and never trips the recursion "
                    "depth cap", "d", 46),
    ],
    "mutator": [
        ("effect", "call CallbackInfo::%s from a click callback and assert the resulting frame "
                   "actually refreshes: damage is non-empty and the pixels differ", "b", 45),
        ("patch", "call CallbackInfo::%s from a click callback and assert the repaint it causes is "
                  "an incremental Rects patch, never a full redraw, and matches the full-repaint "
                  "oracle pixel for pixel", "g5", 46),
        ("settle", "call CallbackInfo::%s from a click callback and assert the window returns to "
                   "zero damage within 5 ticks afterwards rather than re-invalidating forever", "a", 46),
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
                     "does not disturb the incremental render state of the next frame", "a", 46),
        ("leak", "call CallbackInfo::%s 100 times and assert the glyph cache and font tables return "
                 "to baseline and RSS does not climb", "e", 47),
        ("headless", "call CallbackInfo::%s under AZ_BACKEND=headless and assert it either succeeds "
                     "or fails cleanly (no panic, no abort) given the headless RawWindowHandle is "
                     "Unsupported", "f", 45),
    ],
}
for fn in CALLBACK_FNS:
    k = cb_kind(fn)
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
for mgr in MANAGERS:
    trig = MGR_TRIGGER.get(mgr, "exercising it from a callback")
    for desc, key in MANAGER_LIFECYCLE:
        emit("manager/%s" % mgr,
             "exercise the %s manager by %s, then assert it %s" % (mgr, trig, desc),
             mgr=mgr, fam="g2" if "key" in desc or "entry" in desc else "g3", rank=50)
    emit("manager/%s" % mgr,
         "exercise the %s manager by %s and assert the state it produces does not force "
         "FrameDamage::Full when only that manager's state changed" % (mgr, trig),
         mgr=mgr, fam="g5", rank=51)
    emit("manager/%s" % mgr,
         "exercise the %s manager by %s, then delete the node it keyed and assert no dangling key "
         "survives and nothing panics" % (mgr, trig),
         mgr=mgr, fam="g4", rank=52)
    emit("manager/%s" % mgr,
         "exercise the %s manager by %s, end the interaction, then assert assert_state_machines_idle "
         "holds and the window reaches FrameDamage::None" % (mgr, trig),
         mgr=mgr, fam=["g3", "a"], rank=52)

# ─────────────────────────────────────────────────────────────────────────
# RANK 55 — resource leak matrix
# ─────────────────────────────────────────────────────────────────────────
LEAK_CYCLES = [("once", 1), ("10 times in a row", 10), ("200 times in a row", 200)]
LEAK_WIDGETS = [w for w in WIDGETS if w[1] in
                ("unique-font", "image", "text", "list", "virtual-list", "input", "grid", "table")]
for rname, rk in RESOURCES:
    for wd, wk in LEAK_WIDGETS:
        for cdesc, n in LEAK_CYCLES:
            emit("leak/%s" % rk,
                 "snapshot the resource counters, mount and unmount %s %s, force 3 GC frames, then "
                 "assert the count of %s has returned exactly to the baseline"
                 % (wd, cdesc, rname),
                 op="SetAppState", fam="e", rank=55)

# ─────────────────────────────────────────────────────────────────────────
# RANK 58 — invalidation-loop / bounded-work traps
# ─────────────────────────────────────────────────────────────────────────
LOOP_ACTIONS = [
    "requesting a relayout from inside the callback that the relayout itself re-fires",
    "calling set_css_property with the value it already has",
    "calling scroll_to with the offset the container is already at",
    "calling set_focus_to_node on the node that is already focused",
    "calling change_node_text with the text the node already contains",
    "calling modify_window_state without changing anything",
    "calling request_hit_test_update on every frame",
    "calling trigger_virtual_view_rerender from inside the virtual-view render callback",
    "calling commit_undo_snapshot on every keystroke callback",
    "calling scroll_node_into_view on a node that is already fully visible",
    "toggling a class on and off in the same callback",
    "calling take_screenshot from inside a layout callback",
    "returning ShouldRegenerateDom unconditionally from an event handler",
    "mutating the DOM from a timer that fires every frame",
    "calling reload_system_fonts from a hover callback",
]
LOOP_TRIGGERS = [
    "a click", "a hover", "a wheel scroll", "a keypress", "a window resize",
    "a text_input", "a focus change", "an idle tick", "a timer firing", "a DOM mutation",
]
for act in LOOP_ACTIONS:
    for trg in LOOP_TRIGGERS:
        emit("loop/bounded",
             "trigger %s that responds by %s, assert the engine reaches a fixpoint: "
             "relayout_iterations stays under the cap, hit_depth_cap is false, and the window "
             "reaches FrameDamage::None instead of redrawing forever" % (trg, act),
             fam=["d", "a"], rank=58)

# ─────────────────────────────────────────────────────────────────────────
# RANK 60 — cross-manager consistency (X1..X10)
# ─────────────────────────────────────────────────────────────────────────
for xid, xdesc in CROSS:
    for ie, ik in INTERACTIONS:
        emit("cross/%s" % xid,
             "run %s to completion and assert invariant %s: %s" % (ie, xid, xdesc),
             fam="g2", rank=60)
        emit("cross/%s" % xid,
             "run %s and abort it halfway with an Escape keypress, then assert invariant %s still "
             "holds: %s" % (ie, xid, xdesc),
             fam=["g2", "g3"], rank=61)

# ─────────────────────────────────────────────────────────────────────────
# RANK 70 — COMPOSITIONS: two- and three-stage manager chains
# ─────────────────────────────────────────────────────────────────────────
STAGES = [
    ("press and drag to start a text selection", "gesture + selection"),
    ("drag past the container edge so it autoscrolls", "scroll_into_view + scroll_state"),
    ("let the scroll animation run to a stop", "scroll animation"),
    ("hover a sibling node while the drag is live", "hover"),
    ("focus a contenteditable node and blink the caret", "focus_cursor + text_edit"),
    ("type a character into the focused node", "text_edit + changeset"),
    ("undo the typing", "undo_redo"),
    ("copy the selection to the clipboard", "clipboard"),
    ("scroll a virtual-view list past its over-scan window", "virtual_view"),
    ("drag the scrollbar thumb", "gesture + gpu_state"),
    ("resize the window", "layout + damage"),
    ("change the DPI", "layout + damage"),
    ("switch a tab so a whole subtree is replaced", "DOM regeneration"),
    ("long-press to open a context menu", "gesture"),
    ("pinch-zoom the content", "gesture"),
]
COMPOSE_ASSERTS = [
    ("every stage is entered in the listed order and the whole thing reaches a fixpoint", "g1"),
    ("every repaint along the way is an incremental patch and never a full redraw", "g5"),
    ("the damage-driven buffer is pixel-identical to the full-repaint oracle at each checkpoint", "c"),
    ("no manager key points at a dead node at any checkpoint", "g2"),
    ("after the last step every state machine is idle and the window reaches FrameDamage::None", "g3"),
    ("every resource counter returns to the pre-composition baseline", "e"),
]
n = len(STAGES)
for i in range(n):
    for j in range(n):
        if i == j:
            continue
        for atxt, afam in COMPOSE_ASSERTS:
            emit("compose/2",
                 "in one timeline, %s, then %s, and assert %s"
                 % (STAGES[i][0], STAGES[j][0], atxt),
                 fam=afam, rank=70)
for i in range(n):
    j = (i + 3) % n
    for k in range(n):
        if k in (i, j):
            continue
        for atxt, afam in COMPOSE_ASSERTS[:4]:
            emit("compose/3",
                 "in one timeline, %s, then %s, then %s, and assert %s"
                 % (STAGES[i][0], STAGES[j][0], STAGES[k][0], atxt),
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
    ("assert the window still reaches FrameDamage::None within 5 ticks afterwards", "a"),
    ("assert the frame that follows the mutation is repainted correctly with no stale pixels left "
     "from the removed content", "b"),
]
for ie, ik in INTERACTIONS:
    for md, mk in MUTATIONS:
        for pd, pk in PHASES:
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
    ("drag/dual", "start a drag through GestureAndDragManager and assert the deprecated DragDropManager.active_drag never disagrees about whether a drag is live, at every frame of the drag"),
    ("selection/anchor", "select text across three paragraphs, delete the paragraph containing the selection ANCHOR, assert the selection is cleared outright rather than left with a dangling anchor"),
    ("selection/anchor", "select text across three paragraphs, delete the paragraph containing the selection FOCUS, assert the selection collapses to the anchor and no dead key remains"),
    ("selection/scroll", "drag-select downward past the bottom edge of a scroll container until it autoscrolls three screens, assert the selection focus and the scrolled container stay mutually consistent on every single frame"),
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
    ("text/stale", "set a text node to AAA then to BBBBBBBB, assert the damage is non-empty AND the pixels actually differ (a repaint that leaves the glyph count at 3 is the stale-text bug)"),
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
SEED_SHAPES = [
    ("drag from the first child across three siblings while the container autoscrolls, then delete "
     "the middle sibling mid-drag, and assert the interaction survives, the selection never spans a "
     "dead node, and everything settles"),
    ("start an interaction, resize the window mid-interaction, and assert the interaction continues "
     "against the same logical nodes and the repaint stays incremental"),
    ("start an interaction, change the DPI mid-interaction, and assert nothing panics and the window "
     "still reaches zero damage"),
    ("run an interaction to completion, then run it again immediately, and assert the second run's "
     "resource counters match the first run's (no per-interaction leak)"),
    ("run an interaction while a timer mutates an unrelated subtree on every frame, and assert the "
     "two do not interfere and both settle"),
    ("run an interaction, then rebuild the entire DOM via set_app_state, and assert no manager "
     "carries a key across the rebuild that does not resolve to a live node"),
]
for wd, wk in WIDGETS:
    for shape in SEED_SHAPES:
        emit("seed/%s" % wk, "with %s mounted, %s" % (wd, shape), fam=["g1", "g4"], rank=92)

# ─────────────────────────────────────────────────────────────────────────
# 3. WRITE + COVERAGE
# ─────────────────────────────────────────────────────────────────────────
assert not COLLISIONS, "COLLISIONS: %d, e.g. %r" % (len(COLLISIONS), COLLISIONS[:3])

LINES.sort(key=lambda t: (t[0], t[1]))
body = "\n".join(l for _, _, l in LINES) + "\n"

if "--check" in sys.argv:
    cur = read(OUT) if os.path.exists(OUT) else ""
    sys.exit(0 if cur == body else 1)

with open(OUT, "w", encoding="utf-8") as f:
    f.write(body)


def table(title, universe, cover):
    miss = [x for x in universe if not cover.get(x)]
    print("%-28s %5d / %-5d covered   %s"
          % (title, len(universe) - len(miss), len(universe),
             "OK" if not miss else "MISSING: " + ", ".join(miss)))
    return miss


print("wrote %s: %d lines (0 duplicates, uniqueness enforced on the normalized key)"
      % (OUT, len(LINES)))
print()
print("COVERAGE")
table("Callback API functions", CALLBACK_FNS, COVER_CB)
table("Debug ops / mock inputs", DEBUG_OPS, COVER_OP)
table("Managers", MANAGERS, COVER_MGR)
table("Assertion families", list(FAMILIES), COVER_FAM)
print()
from collections import Counter
c = Counter(l.split("]")[0][1:].split("/")[0] for _, _, l in LINES)
print("BY CATEGORY")
for k, v in sorted(c.items(), key=lambda kv: -kv[1]):
    print("  %-14s %6d" % (k, v))
