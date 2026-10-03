# Spatial navigation vs CSS Spatial Navigation Level 1: audit and fill-in (2026-09-28)

- Branch `wt/spatial-nav-properties`, based on `7e020dc49` (PR #476, `fix/input-bugs-2026-09-19`).
- **Nothing here was compiled or run.** Eight agents share one machine, so the parent compiles
  once at the end. §6 lists the spots most likely to fail to compile.
- Reference: W3C WD `css-nav-1` (https://www.w3.org/TR/css-nav-1/). `drafts.csswg.org/css-nav-1`
  returns 404 today, so every quote here is from the TR.
- User ruling, 2026-09-03: follow the spec. `spatial-navigation-contain: auto` makes every scroll
  container a container.

**Scope change during the task** (user ruling, relayed by the parent):
- The `nav-up/right/down/left` properties (CSS Basic UI 4) were DROPPED as outdated syntax. None
  of that work was written.
- This branch implements the css-nav-1 surface instead:
  - the `spatial-navigation-function` property;
  - the `spatial-navigation-action` semantics;
  - the JS-equivalent query API;
  - the navigation events only if small. They are not small; §8 has a design sketch.

---

## 1. Audit (as found at `7e020dc49`, before any change)

Line numbers refer to the base commit.

| # | Building block (css-nav-1) | Status at base | Where / what |
|---|---|---|---|
| 1 | Candidate set: "find focusable areas" (descendants, negative tabindex removed) | **present** | `collect_tab_order`, `layout/src/managers/focus_cursor.rs:475`. This is the Tab pool: tabindex −1, `<transient-window>` subtrees and out-of-scope DOMs are left out. It is in TAB order, not document order, which matters for ties. |
| 2 | Directional filter from the focused element's box | **partial** | `next_in_direction`, `focus_cursor.rs:2652`, compares centre to centre inside a 45° cone (`cross > along` ⇒ rejected). The spec filters by EDGES: *insiders* overlap the origin and are edge-tested; *beyond* candidates do not overlap and lie entirely past the far edge. A row that is below the focus but shifted sideways by more than it is below could not be reached. There was no insiders step and no single-candidate shortcut. |
| 3 | Distance function | **different** | `along + 3·cross` on centre deltas (`:2717-2728`). The comment says the 3 "is what the CSS spec's reference implementation uses"; it is not. The spec: `euclidean(P1,P2) + (orth + orthBias)·orthWeight − alignBias·alignWeight − √overlap`, with orthWeight 30 (left/right) or 2 (up/down), alignWeight 5, and orthBias = half the origin's cross size. Ties went to the first in tab order; the spec says document order, then painting order. |
| 4 | `spatial-navigation-contain` | **present** | Type and parser in `css/src/props/style/spatial_nav.rs`, getter at `layout/src/solver3/getters.rs:6375`. The chain `spatial_navigation_containers` (`focus_cursor.rs:2548`) takes `contain`, or `auto` plus a CSS scroll container; it includes the node itself and runs innermost first. The `Directional` arm (`:874-892`) narrows to each container outward, then to the whole pool. |
| 4a | Containers nested in scroll containers | **present, same DOM only** | The chain walks every ancestor, so a `contain` panel inside a scroller and a scroller inside a panel both chain. It never leaves the node's own DOM: `is_within` (`:2615`) rejects other DOMs, and nothing matches the spec's "nested browsing context" step. "Scroll container" is decided by CSS `overflow` (`hidden` included), not by whether layout gave the box a scroll frame. The parallel `overflow: hidden` scroll-frame change therefore does not move this. |
| 5 | `spatial-navigation-action` | **broken lookup** | Read at `layout/src/default_actions.rs:507-561` off "the nearest ancestor with `scrollbar_info`". But layout gives EVERY laid-out box a `scrollbar_info`: `solver3/cache.rs:3460` sets `Some(..)` unconditionally, all-false for boxes that do not scroll. So the walk always stopped at the focused node and read ITS value. As a result, a container's `focus` or `scroll` was never read unless the container itself had focus: `focus` on a scroll box still scrolled it. By accident, `scroll` on an ancestor was ignored, which §9.2 wants. Truth table: `resolve_arrow_action` `:461-491`. There was no "can be manually scrolled" test: the scroll fallback fired at the scroll boundary too, and with `overflow: hidden`. |
| 6 | `auto`: scroll the container when no candidate is VISIBLE in it | **missing** | The search ran over all candidates. A candidate scrolled out of view inside the container got focus, and focus's scroll-into-view then revealed it, instead of the container scrolling. A candidate OUTSIDE the container beat scrolling a container that still had content that way, so focus left a half-read list. |
| 7 | The scroll target | **partial** | `DefaultAction::ScrollFocusedContainer` names no target. The dll arm (`dll/src/desktop/shell2/common/event.rs:11706`) scrolls `find_scrollable_ancestor(focus)` (`layout/src/window.rs:13939`), the NEAREST overflowing ancestor. That is not always the container the algorithm would choose: it differs when the inner container is at its boundary or says `focus`. |
| 8 | `spatial-navigation-function: normal \| grid` | **missing** | No property, no grid rule. |
| 9 | `navbeforefocus` / `navnotarget` (cancelable, bubbling) | **missing, expressible** | There is no event type for them. They can be expressed: an `EventType` (`core/src/events.rs:753`) plus a synthetic event sent through `dispatch_events_propagated` (`event.rs:8409`), which returns a prevented flag. `Click`, `Submit` and `Invalid` already use that pattern from the default-action phase. Design in §8. |
| 10 | JS API `getSpatialNavigationContainer()`, `focusableAreas()`, `spatialNavigationSearch()` | **missing** | `window.navigate(dir)` has a partial counterpart, `CallbackInfo::set_focus(FocusTarget::Directional(dir))`: focus only, over static geometry. |
| 11 | Geometry: painted position (after scroll and transform) | **missing** | `rect_of` in `next_in_direction` (`:2672-2686`) read `calculated_positions`: static, unscrolled, untransformed and DOM-relative. A focused node in a scrolled list was compared at its unscrolled position, transforms were ignored, and a `VirtualView` node was compared at its 0-relative position. The right helpers already existed: `headless::node_rect_to_screen` (`layout/src/headless.rs:338`) and `headless::nested_dom_window_origin` (`:555`). |
| 12 | Across DOMs (`VirtualView`) | **partial** | The pool spans all DOMs, but the geometry was wrong (row 11) and containment stayed within one DOM (row 4a). |
| 13 | Across windows (transient popups) | **partial, one bug** | By design a popup is its own window, and the parent excludes its content (`focus_out_of_scope_doms`, `window.rs:10366`). But the DECISION (`default_actions.rs:299-304`) resolved with an EMPTY out-of-scope set, while the APPLICATION (`event.rs` ~11576, `layout/src/e2e/runner.rs:3746`) passed the real one. The decision could answer "focus found" for a node inside an open popup; the application then found nothing, and the arrow neither moved focus nor scrolled. Key routing into popups belongs to analysis §A3/B3. |
| 14 | Search origin / starting point | **partial** | The origin is the focused node only. There is no "spatial navigation starting point" (the click position). An unlaid or removed focus falls back to the first tab stop, not to its last known rect. |
| 15 | Headless / E2E | **partial** | `key_down Down` plus `get_focus_state` observes a focus move (runner arms `FocusUp..Right`, `runner.rs:3735`). The runner had NO arm for a scroll default action (`_ => DoNothing`, `:3863`), so the arrow's scroll fallback could not be seen headless. Every spatial test ran on an EMPTY layout tree, so the geometry had no test at all. An AZ_E2E script against the live dll can press an arrow and read the focus through the debug server. |
| — | Modifier gate (unmodified arrows only) | other agent | P2-10 in `default_actions.rs`. Not touched here. |
| — | Roving tabindex in composite widgets | other agent | Widgets are not touched here. |

---

## 2. What changed: the audit again, after this branch

| # | Building block | Now | How |
|---|---|---|---|
| 1 | Candidate set | **present** | Same Tab pool, re-sorted into DOCUMENT order (`spatial_candidate_pool`). |
| 2 | Directional filter | **spec** | `select_best_candidate`: insiders first (they overlap the origin's inside area and lie further along than its start edge; the nearest start edge wins). Otherwise only candidates entirely BEYOND the far edge count. Edges replace centres. Boxes that touch do not overlap. |
| 3 | Distance (`normal`) | **spec** | `normal_distance`: `euclidean + (crossGap + crossSize/2)·(30 or 2) − overlapRatio·5`. Beyond candidates never overlap, so √overlap is 0. Ties go to the earlier in document order; painting order is not used. |
| 4/4a | `-contain`, nesting | **present** | Unchanged chain. Containment still stays within one DOM, with a cross-DOM extension (row 12). |
| 5 | `-action` | **spec** | Read per container inside the steps (`action_of`). The legacy lookup is fixed to use CSS `overflow` (`default_actions.rs` `spatial_navigation_action`); it is only used when the steps find nothing at all. |
| 6 | `auto` scrolls when nothing is visible | **spec** | `spatial_navigation_steps` follows §8.3; see §3 below. |
| 7 | Scroll target | **fixed** | New `DefaultAction::ScrollContainer { container, .. }` names the container the steps chose. |
| 8 | `spatial-navigation-function` | **present** | New CSS property, read off the container being searched. `grid` follows §9.3. |
| 9 | Navigation events | **open** | Design in §8. |
| 10 | JS API | **present** | `CallbackInfo::{get_spatial_navigation_container, get_focusable_areas, spatial_navigation_search}`; details in §4. |
| 11 | Painted geometry | **present** | `SpatialGeometry::rect` = `node_rect_to_screen` (ancestor scroll offsets + transforms) + `nested_dom_window_origin` (the VirtualView lift). Visibility is clipped by every scroll-container scrollport above the node and by the window viewport. |
| 12 | Across DOMs | **better** | Geometry is now in window space. After the whole DOM is exhausted, the other DOMs are searched (an azul extension standing in for the nested-browsing-context step). |
| 13 | Across windows | **fixed** | The decision and the application both go through `LayoutWindow::with_spatial_navigation_env`, which carries the same `focus_out_of_scope_doms`. |
| 14 | Search origin | **unchanged** | Still open. |
| 15 | Headless | **better** | The runner uses the live env and has a `ScrollContainer` arm, so arrow-driven scrolling is visible headless. It still has NO `ScrollFocusedContainer` arm (open). |

---

## 3. The engine (one place, every caller)

All of it lives in `layout/src/managers/focus_cursor.rs`, section "CSS Spatial Navigation Level 1 -
the engine".

**`SpatialNavigationEnv<'a>`** holds everything a search reads besides layout:
- `layout_results`, `out_of_scope`;
- `scroll_info: Option<&dyn Fn(DomId, NodeId) -> Option<ScrollNodeInfo>>`;
- `transform: &dyn Fn(DomId, NodeId) -> Option<ComputedTransform3D>`.

There are two ways to get one:
- **Live:** `LayoutWindow::with_spatial_navigation_env(|env| ..)` passes the scroll manager's
  `get_scroll_node_info`, the GPU cache's `css_current_transform_values`, and
  `focus_out_of_scope_doms`.
- **Layout-only:** `SpatialNavigationEnv::layout_only(layout_results, out_of_scope)`. Every offset
  is 0 and there are no transforms. A box "can scroll" Down or Right exactly when layout gave it a
  scrollbar on that axis (`needs_vertical` / `needs_horizontal`), and never Up or Left. The
  public free functions `determine_keyboard_default_action[_with_editing]` and
  `resolve_focus_target` use this model, so their signatures are unchanged.

**`spatial_navigation_steps(env, origin, dir) -> SpatialNavigationOutcome { Focus(n) |
Scroll(container) | NoTarget }`** follows §8.3, the "spatial navigation steps":

1. If the origin is itself a scroll container, or its DOM's root (the document):
   - unless its `spatial-navigation-action` is `focus`, it SCROLLS while it "can be manually
     scrolled" (Appendix A: overflow on that axis is not `hidden`, and it is not at the boundary);
   - otherwise its own candidates are searched: the visible ones, or all of them under `focus`.

   This follows the TR text: a focused scroll container with `auto` scrolls BEFORE its children
   are searched.
2. Then every container, from the innermost ancestor outward, with the DOM root last. In each:
   - the best candidate wins (visible ones only, unless the container says `focus`);
   - with none, a container that can still scroll that way SCROLLS (unless it says `focus`);
   - otherwise the search moves outward. This is where `navnotarget` would fire.
3. azul extension: the window's OTHER DOMs.

"Candidates" means direction-filtered candidates: the TR's "if candidates is empty" is read the way
its §6.2.2 note reads it.

**`directional_focus_target(env, current, dir)`** is `FocusTarget::Directional` and the gamepad
D-pad. It runs the keyboard steps first. When those would scroll or find nothing, it searches
again as if every container said `focus`, because a focus move cannot scroll. A D-pad still
reaches an item that is scrolled out of view, and focus scrolls it in. With nothing focused, it
picks the first tab stop, as before.

**`default_actions`** maps the outcome of the steps as follows:

| Outcome of the steps | Default action |
|---|---|
| `Focus` | `FocusUp/Down/Left/Right` |
| `Scroll(c)` | `ScrollContainer { container: c, direction, amount: Line }` |
| `NoTarget` | Legacy `resolve_arrow_action(spatial_navigation_action(..), .., found=false, ..)`: `ScrollFocusedContainer` unless the nearest scroll container says `focus`. This is harmless: the consumer only scrolls an overflowing ancestor, and does nothing at a boundary. |

---

## 4. Commits (oldest first), each with its expected RED

| Commit | Kind | What / expected RED |
|---|---|---|
| `96da3481c` | docs | Audit first (the §1 table; this file's first version). Row 5 was corrected in the final report commit. |
| `f9b44670a` | **RED** css | `the_function_property_parses_from_its_css_name_and_prints_back` (`css/src/props/style/spatial_nav.rs`). Fails at runtime: panic "`spatial-navigation-function` must be a known property name". |
| `d662fd291` | fix | `feat(css): spatial-navigation-function: normal \| grid`: type, parser, errors, name table, every `CssProperty`/`CssPropertyType` arm, prop cache, solver getter. |
| `4fdcd593a` | **RED** focus | `layout/tests/spatial_navigation.rs` (registered in `all.rs`). Four runtime failures on a real layout: **(1)** `a_box_below_is_reachable_even_when_it_sits_further_to_the_side_than_below`: `None` vs `Some(node 2)`. **(2)** `grid_prefers_the_aligned_candidate_over_the_nearer_one`: node 3 vs node 4. **(3)** `an_arrow_with_nowhere_to_go_does_not_scroll_a_focus_container`: `ScrollFocusedContainer` vs `None`. **(4)** `auto_scrolls_the_container_instead_of_leaving_it_while_it_still_has_content_below`: got `FocusDown`. |
| `95faa5740` | fix | `fix(focus): spatial navigation runs the css-nav-1 steps over painted geometry`: the engine, `determine_keyboard_default_action_with_env`, and the fixed action lookup. Adds 3 integration guards and 7 pure `spatial_selection_tests`. |
| `bb7bd3caf` | **RED** e2e | `arrow_down_walks_a_scroll_box_scrolling_it_and_then_leaves_it` (`layout/src/e2e/runner.rs`, `--features e2e-server`): ten `key_down Down`. Fails at runtime: focus stuck on node 3 instead of node 6, because the runner cannot scroll and the layout-only model never lets the arrow escape. |
| `f84cfc7d0` | fix | `fix(focus): arrow keys decide and apply spatial navigation on the live window`: `with_spatial_navigation_env`, `keyboard_default_action`, `resolve_focus_target_live`, `scroll_container_by_keyboard`, `DefaultAction::ScrollContainer`. Wires the dll shell (keys and D-pad) and the runner. Adds 3 live GREEN tests. |
| `33a175bf0` | **RED** focus | `the_container_of_a_list_item_is_its_scroll_box_and_of_the_box_the_document`, `focusable_areas_are_the_visible_ones_or_all_of_them`, `spatial_navigation_search_searches_the_container_or_the_candidates_it_is_given`. Fails to COMPILE: no such methods on `LayoutWindow`, unresolved `FocusableAreaSearchMode` / `SpatialNavigationSearchOptions` / `OptionDomNodeIdVec`. |
| `90016760c` | fix | `feat(focus): the css-nav-1 query API on CallbackInfo, from the arrow-key engine`. |
| (last) | docs | This report. |

---

## 5. Public API, CSS and ABI additions (for `azul-doc autofix`; api.json NOT edited)

### CSS property: `spatial-navigation-function: normal | grid`

- It is not inherited. Its initial value is `normal`, and it applies to spatial navigation
  containers.
- It is `RelayoutScope::None`, and `can_trigger_relayout() == false`.

Everything below sits beside its `-contain` sibling.

**`azul_css::props::style::spatial_nav` (new types and fn):**
- `enum StyleSpatialNavigationFunction { Normal (default), Grid }`, `#[repr(C)]`, with a
  `PrintAsCssValue` impl and `impl_enum_fmt!` in `css/src/codegen/format.rs`;
- `enum CssSpatialNavigationFunctionParseError<'a> { InvalidValue(&'a str) }`;
- `enum CssSpatialNavigationFunctionParseErrorOwned { InvalidValue(AzString) }`,
  `#[repr(C, u8)]`, with `to_contained` / `to_shared`;
- `fn parse_style_spatial_navigation_function(&str)` (feature `parser`).

**`azul_css::props::property` (new variants and items):**
- `type StyleSpatialNavigationFunctionValue`;
- variants: `CssProperty::SpatialNavigationFunction(StyleSpatialNavigationFunctionValue)`,
  `CssPropertyType::SpatialNavigationFunction` (also listed in `ALL`),
  `CssParsingError::SpatialNavigationFunction(..)`,
  `CssParsingErrorOwned::SpatialNavigationFunction(..)`;
- `CssProperty::as_spatial_navigation_function()`;
- name-table entry `"spatial-navigation-function"` (the map is now 197 entries).

**Getters:**
- `azul_core::prop_cache::CssPropertyCache::get_spatial_navigation_function`;
- `azul_layout::solver3::getters::get_spatial_navigation_function`.

### Core types

- `azul_core::callbacks::FocusableAreaSearchMode { Visible (default), All }`, `#[repr(C)]`.
- `azul_core::callbacks::SpatialNavigationSearchOptions { candidates: OptionDomNodeIdVec,
  container: OptionDomNodeId }`, `#[repr(C)]`. It derives Default, Clone, PartialEq and
  PartialOrd.
- `azul_core::dom::OptionDomNodeIdVec`, via `impl_option!(DomNodeIdVec, .., copy = false, [Debug,
  Clone, PartialEq, PartialOrd])`.
- `azul_core::events::DefaultAction::ScrollContainer { container: DomNodeId, direction:
  ScrollDirection, amount: ScrollAmount }` is APPENDED at the enum tail. The C, C++ and other
  bindings need regenerating, and every exhaustive match needs an arm. The dll has one.

### `CallbackInfo` (FFI surface; api.json `fn_body` mirrors `get_parent`)

- `get_spatial_navigation_container(&self, node_id: DomNodeId) -> Option<DomNodeId>`, returning
  `OptionDomNodeId`. Suggested `fn_body`: `object.get_spatial_navigation_container(node_id).into()`.
- `get_focusable_areas(&self, node_id: DomNodeId, mode: FocusableAreaSearchMode) -> DomNodeIdVec`.
- `spatial_navigation_search(&self, node_id: DomNodeId, direction: FocusDirection, options:
  SpatialNavigationSearchOptions) -> Option<DomNodeId>`, returning `OptionDomNodeId`. The options
  are passed by value.

### `LayoutWindow` (Rust; in api.json only if autofix exposes it)

- `with_spatial_navigation_env<R>(&self, f: impl FnOnce(&SpatialNavigationEnv<'_>) -> R) -> R`
  (generic, Rust only);
- `keyboard_default_action(&self, &KeyboardState, Option<DomNodeId>, prevented: bool,
  Option<&EditingQueryState>) -> DefaultActionResult`;
- `resolve_focus_target_live(&self, &FocusTarget, Option<DomNodeId>) -> Result<FocusResolution,
  UpdateFocusWarning>`;
- `scroll_container_by_keyboard(&mut self, DomNodeId, ScrollDirection, ScrollAmount, Duration,
  Instant) -> bool`;
- `get_spatial_navigation_container(&self, DomNodeId) -> Option<DomNodeId>`;
- `get_focusable_areas(&self, DomNodeId, FocusableAreaSearchMode) -> Vec<DomNodeId>`;
- `spatial_navigation_search(&self, DomNodeId, FocusDirection, &SpatialNavigationSearchOptions) ->
  Option<DomNodeId>`.

### `azul_layout::managers::focus_cursor` (Rust; borrowing `&dyn Fn`, not FFI-shaped)

- `struct SpatialNavigationEnv<'a> { layout_results, out_of_scope, scroll_info, transform }` and
  `SpatialNavigationEnv::layout_only`;
- `enum SpatialNavigationOutcome { Focus(DomNodeId), Scroll(DomNodeId), NoTarget }`;
- fns: `spatial_navigation_steps`, `directional_focus_target`, `resolve_focus_target_in`,
  `get_spatial_navigation_container`, `focusable_areas`, `spatial_navigation_search`.
- `azul_layout::default_actions::determine_keyboard_default_action_with_env`.

**Removed:** nothing public. The private `next_in_direction` is gone.

---

## 6. Least sure to compile (check these first)

1. **`SpatialNavigationEnv::layout_only`** sets `transform: &no_transform`, relying on a fn item
   reference being promoted to `'static` and coerced to `&'a dyn Fn(..)`.
   - Fallback: `const NO_TRANSFORM: &dyn Fn(DomId, NodeId) -> Option<ComputedTransform3D> =
     &no_transform;`.
2. **dll `event.rs` default-action block.** `layout_window` (a `&LayoutWindow` borrowed from
   `self`) now spans the decision and the focus arm's `resolve_focus_target_live`, and
   `self.apply_system_change` and `self.get_layout_window_mut()` come after it in the arms.
   - This is the same NLL shape the old `layout_results` borrow had.
   - If borrowck complains, compute `resolve_result` before the `match` into a local.
3. **`impl_option!(DomNodeIdVec, OptionDomNodeIdVec, copy = false, [Debug, Clone, PartialEq,
   PartialOrd])`** in `core/src/dom.rs`, and `SpatialNavigationSearchOptions`'s
   `derive(Default, PartialOrd)` over it.
4. **`options.candidates.as_ref()`** in `focus_cursor::spatial_navigation_search` is meant to
   resolve to the inherent `OptionDomNodeIdVec::as_ref`, not `AsRef`.
5. **`with_spatial_navigation_env`** relies on the `&dyn Fn` locals (`scroll_info_dyn`,
   `transform_dyn`) and on the higher-ranked `impl FnOnce(&SpatialNavigationEnv<'_>) -> R`.
6. **The CSS property plumbing.** Every `SpatialNavigationContain` site in `property.rs`,
   `macros.rs`, `format.rs`, `prop_cache.rs` and `getters.rs` got a `SpatialNavigationFunction`
   sibling (31 = 31 occurrences in `property.rs`). A match elsewhere that is exhaustive with no
   `SpatialNavigationContain` arm would have broken before as well, so there should be none.
7. **`layout/tests/spatial_navigation.rs`** assumes:
   - `Dom::with_tab_index` / `with_css` chaining;
   - `azul_core::task::Instant: From<std::time::Instant>`;
   - `ScrollManager::set_scroll_position` clamping.
8. **The runner test** assumes `serde_json::json!` with a `Vec<Value>` interpolated.

---

## 7. Behaviour changes to watch (intended, but visible)

- **`normal` distance is the spec's.** Left/Right now weigh cross-axis drift 15× more than
  Up/Down do. A box that is "below" by its edges is reachable however far to the side it sits:
  the old 45° cone is gone. Right with nothing on the same row can therefore land far below.
  This is spec behaviour, and Chrome does the same.
- **A FOCUSED scroll container with `auto` scrolls on arrows until its boundary, and only then
  enters its children** (TR §8.3 step 4 as written).
  - The composite-widget agent's roving-tabindex work (ListView, TreeView) calls
    `prevent_default`, so it is unaffected.
  - A plain focusable scroller now pans before it navigates.
- **`spatial-navigation-action: focus` on a scroll container is honoured for the first time.**
  With nowhere to go the arrow does nothing, instead of scrolling.
- **Visibility matters under `auto`.** An item scrolled out of view is no longer focused directly;
  the box scrolls first. A D-pad or `FocusTarget::Directional` still reaches it, through the
  focus-only retry.
- **Headless runner.** `ScrollContainer` now scrolls, instantly, like the runner's `ScrollTo`. A
  scenario that pressed arrows inside an overflowing box now moves that box.
- **Cost.** Each arrow press builds the pool (the Tab walk), computes a painted rect per candidate
  (an ancestor walk), and runs an `is_within` walk per container. That is fine for hundreds of
  focusables. `directional_focus_target` may run the steps twice.

---

## 8. Still open

1. **`navbeforefocus` / `navnotarget`: not small enough.** Design sketch:
   - **Core types.** `EventType::NavBeforeFocus` and `EventType::NavNoTarget`, appended.
     `EventData::Navigation(NavigationEventData { dir: FocusDirection, related_target:
     OptionDomNodeId })`, appended. A filter: either new `FocusEventFilter::NavBeforeFocus` /
     `NavNoTarget` on the focused node, or a bubbling hover-style filter, since the spec says it
     bubbles. Plus an `event_type ↔ filter` row in `core/src/events.rs` (~1700), without which the
     filter never fires (the "46 dead filters" lesson), and `CallbackInfo::get_navigation_event()`.
   - **Engine.** `spatial_navigation_steps` returns the containers it passed, e.g.
     `SpatialNavigationOutcome::Focus { node, passed: Vec<DomNodeId> }`.
   - **Hosts.** In the dll focus arm and the runner focus arm, BEFORE `SetFocus`:
     - dispatch one `NavNoTarget` per passed container, then a `NavBeforeFocus` naming the target;
     - if `prevent_default`, stop, so focus does not move;
     - use `dispatch_events_propagated`, which already returns the prevented flag (the
       Click/Submit pattern).
   - **Where the event fires.** At the spec's cancel point: navnotarget fires before the search
     climbs; if it is cancelled, return. That means the decision must also run with a "stop at
     cancelled container" input, or be split into search-then-dispatch per container. The
     simplest correct shape re-runs the steps in the host with a veto callback.
   - **Cost.** About 250 lines across core, dll and runner, plus api.json and bindings.
2. **The runner has no `ScrollFocusedContainer` arm.** PgUp, PgDn, Space, Home and End, and an
   arrow with nowhere to go, still do nothing headless. The dll scrolls them. Porting the arm
   (anchored on the focus, through `scroll_container_by_keyboard`) is one arm, but it changes
   what existing headless scenarios observe.
3. **Programmatic directional focus uses STATIC geometry.** `CallbackInfo::set_focus(FocusTarget::Directional)`,
   `set_focus_for_seat` and window.rs's deferred focus still resolve through
   `resolve_focus_target` (layout-only env). Switching them means `resolve_focus_target_live` at
   `event.rs` `SetSeatFocusTarget` (~5373) and at the runner's `SetFocusTarget` arm. Until then
   apps can call `spatial_navigation_search`, then `set_focus(FocusTarget::Id)`.
4. **Search origin / starting point (§8.4).** There is no click-position starting point, and no
   "last known rect" for a focus that was removed, disabled or scrolled fully off-screen.
5. **Nested browsing contexts, done properly.** A VirtualView's content DOM does not chain to the
   host's containers: the host node of a child DOM is not reachable from `layout_results` alone.
   Only the cross-DOM extension (step 3) connects them. A nested DOM is also not clipped by its
   VirtualView host in the visibility test.
6. **Box fragments.** The spec says "each box fragment is considered separately". azul uses the
   first layout node of a DOM node, so an inline focusable split across lines is one box.
7. **Tie-break by painting order (§8.4).** Not applied; document order only.
8. **Scrollport precision.** The inside area of a scroll container is its border box, not its
   padding box minus the scrollbar.
9. **`overflow: hidden` boxes.** They count as containers under `auto`, which is correct per spec,
   but can never be "manually scrolled", so the steps move past them. This is independent of the
   parallel agent that gives `hidden` a real scroll frame.
10. **Dropped:** CSS Basic UI 4 `nav-up`/`nav-right`/`nav-down`/`nav-left` (user ruling).

### Notes for the parallel agents

- **Modifier gate (P2-10):** the arrow arm in `determine_keyboard_default_action_with_env` changed
  only below `anchor_is_live` (the old `spatial_navigation_action` / `resolve_focus_target` lines
  became one `match spatial_navigation_steps(..)`). The function `_with_editing` is now a thin
  wrapper, and its old body lives in `_with_env`, so a gate added to `_with_editing`'s body must
  move to `_with_env`.
- **Composite widgets:** nothing here touches widgets. Arrows that a widget `prevent_default`s never
  reach the steps.
- **`overflow: hidden` scroll frames:** containers are decided by CSS `overflow`, and
  "can be manually scrolled" requires the axis to allow USER scrolling (`hidden` does not), so a
  new scroll frame for `hidden` boxes changes neither answer. It does change what the scroll
  manager registers, and so the live `ScrollNodeInfo`: harmless, because `hidden` is rejected
  before that lookup.
