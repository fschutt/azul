# Spatial navigation vs CSS Spatial Navigation Level 1 — audit and fill-in (2026-09-28)

Branch `wt/spatial-nav-properties`, based on `7e020dc49` (PR #476, `fix/input-bugs-2026-09-19`).
Nothing in this branch was compiled or run (8 agents on one machine, the parent compiles once).

Reference: W3C WD `css-nav-1` (https://www.w3.org/TR/css-nav-1/). `drafts.csswg.org/css-nav-1`
answers 404 today; the TR text is the one quoted here. The user ruling of 2026-09-03 is
"follow the spec; `spatial-navigation-contain: auto` makes every scroll container a container".

Scope change during the task (user ruling relayed by the parent): the `nav-up/right/down/left`
properties (CSS Basic UI 4) are DROPPED as outdated syntax. Nothing of them was written. The
work is the css-nav-1 surface instead: `spatial-navigation-function`, `spatial-navigation-action`
semantics, the JS-equivalent API, and (if small) the two navigation events.

---

## 1. Audit (as found at `7e020dc49`, before any change)

Line numbers are at the base commit.

| # | Building block (css-nav-1) | Status | Where / what |
|---|---|---|---|
| 1 | Candidate set: "find focusable areas" (descendants, negative tabindex removed) | **present** | `collect_tab_order` `layout/src/managers/focus_cursor.rs:475` — the Tab pool: tabindex −1 out, `<transient-window>` subtrees out, out-of-scope DOMs out. Ordered in TAB order, not document order (matters for ties). |
| 2 | Directional filter from the focused element's box | **partial** | `next_in_direction` `focus_cursor.rs:2652`. Centre-to-centre deltas with a 45° cone (`cross > along` ⇒ rejected). The spec filters by EDGES: *insiders* (candidates overlapping the origin, edge-tested) and *beyond* (not overlapping, entirely past the far edge). A row that is below but shifted sideways by more than it is below is unreachable today. No insiders step, no single-candidate shortcut. |
| 3 | Distance function | **different** | `along + 3·cross` on centre deltas (`:2717-2728`). The comment claims 3 "is what the CSS spec's reference implementation uses" — it is not. Spec: `euclidean(P1,P2) + (orth + orthBias)·orthWeight − alignBias·alignWeight − √overlap`, orthWeight 30 (left/right) / 2 (up/down), alignWeight 5, orthBias = half the origin's cross size. Ties: first in tab order (spec: document order, then painting order). |
| 4 | `spatial-navigation-contain` | **present** | Type/parser `css/src/props/style/spatial_nav.rs`, getter `layout/src/solver3/getters.rs:6375`, chain `spatial_navigation_containers` `focus_cursor.rs:2548` (`contain`, or `auto` + CSS scroll container, self-inclusive, innermost first), used by the `Directional` arm `:874-892` (narrow to each container outward, then the whole pool). |
| 4a | Containers nested in scroll containers | **present, same DOM only** | The chain walks every ancestor, so a `contain` panel inside a scroller and a scroller inside a panel both chain. It never leaves the node's DOM: `is_within` `:2615` rejects cross-DOM, and the spec's "nested browsing context" step (continue from the iframe in the parent document) has no counterpart; the whole-pool fallback is the only cross-DOM path. "Scroll container" here is CSS `overflow` (`hidden` included), independent of whether layout gave the box a scroll frame, so the parallel `overflow: hidden` scroll-frame change does not move this. |
| 5 | `spatial-navigation-action` | **partial, two deviations** | Read at `layout/src/default_actions.rs:507-561` off the nearest `scrollbar_info` ancestor; truth table `resolve_arrow_action` `:461-491`. (a) `scroll` on an ANCESTOR scroll container forces a scroll — per §9.2 it "has the same effect as auto" unless the FOCUSED element is the scroll container, so every control inside a `scroll` container was unreachable by arrow keys. (b) `auto` never looks at visibility (next row). (c) no "can be manually scrolled" test: the scroll fallback is issued at the scroll boundary too, and with `overflow: hidden`. (d) outer containers' own values are never read. |
| 6 | `auto`: scroll the container when no candidate is VISIBLE in it | **missing** | The search is over all candidates. A candidate scrolled out of view inside the container is focused (then revealed by focus's scroll-into-view) instead of the container scrolling; a candidate OUTSIDE the container beats scrolling a container that still has content that way — focus leaves a half-read list. |
| 7 | The scroll target | **partial** | `DefaultAction::ScrollFocusedContainer` has no target; the dll arm (`dll/src/desktop/shell2/common/event.rs:11706`) scrolls `find_scrollable_ancestor(focus)` (`layout/src/window.rs:13939`), the NEAREST overflowing ancestor — not the container the algorithm chose (differs when the inner one is at its boundary or says `focus`). |
| 8 | `spatial-navigation-function: normal \| grid` | **missing** | No property, no grid rule. |
| 9 | `navbeforefocus` / `navnotarget` (cancelable, bubbling) | **missing, expressible** | No event type. Expressible: `EventType` (`core/src/events.rs:753`) + a synthetic event through `dispatch_events_propagated` (`event.rs:8409`, which returns a prevented flag), the pattern `Click`/`Submit`/`Invalid` already use from the default-action phase. See §5 for the sketch. |
| 10 | JS API `getSpatialNavigationContainer()`, `focusableAreas()`, `spatialNavigationSearch()` | **missing** | `window.navigate(dir)` has a partial counterpart: `CallbackInfo::set_focus(FocusTarget::Directional(dir))` (focus only, static geometry). |
| 11 | Geometry: painted position (after scroll/transform) | **missing** | `rect_of` in `next_in_direction` (`:2672-2686`) reads `calculated_positions` = STATIC, unscrolled, untransformed, DOM-relative. A focused node in a scrolled list is compared at its unscrolled position; transforms are ignored; a node in a `VirtualView` DOM is compared at its 0-relative position. The right helpers exist: `headless::node_rect_to_screen` (`layout/src/headless.rs:338`, scroll + transforms, what a11y and menus use) and `headless::nested_dom_window_origin` (`:555`, the cross-DOM lift). |
| 12 | Across DOMs (`VirtualView`) | **partial** | Pool spans all DOMs; geometry wrong (row 11); containment same-DOM only (row 4a). |
| 13 | Across windows (transient popups) | **partial, one bug** | By design a popup is its own window and the parent excludes its content (`focus_out_of_scope_doms` `window.rs:10366`). But the DECISION (`default_actions.rs:299-304`) resolved with an EMPTY out-of-scope set while the APPLICATION (`event.rs` ~11576, `layout/src/e2e/runner.rs:3746`) passes the real one: the decision can answer "focus found" for a node in an open popup, the application then finds nothing, and the arrow neither moves focus nor scrolls. Key routing into popups is analysis §A3/B3, not this file. |
| 14 | Search origin / starting point | **partial** | Focused node only. No "spatial navigation starting point" (click position); an unlaid/removed focus falls back to the FIRST tab stop, not to the last known rect. |
| 15 | Headless / E2E | **partial** | `key_down Down` + `get_focus_state` observes a focus move (runner arms `FocusUp..Right` `runner.rs:3735`). The runner has NO `ScrollFocusedContainer` arm (`_ => DoNothing`, `:3863`), so the arrow→scroll fallback is invisible headless. Every existing spatial test runs on an EMPTY layout tree, so `next_in_direction`'s geometry had no test at all. |
| — | Modifier gate (unmodified arrows only) | other agent | P2-10, `default_actions.rs`, not touched here. |
| — | Roving tabindex in composite widgets | other agent | Widgets not touched here. |

---

## 2. What this branch changes

(filled in at the end of the work; see the commit list below)
