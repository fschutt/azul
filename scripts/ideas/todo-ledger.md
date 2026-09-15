# azul — nice-to-have ledger

Written 2026-09-14 against branch `fix/compilation-1009` @ `f01d2413c` (tag `0.2.0` = `2c29af263`, 2026-05-26; the *served* 0.2.0 is rebuilt from master on every deploy, last on 2026-09-08).
Every claim below was re-checked in the tree on that date; line numbers are from that tree.
Revised the same day after Felix's answers: F1 (font fix is deployed), E10 (auto-wrap: yes, with effort estimate), S3 (Homebrew audit), W2 (full ListView v2 spec: lazy rows, sort/filter/edit/expand/reorder callbacks, mobile kv layout, `Db` CRUD demo), E11 (`@container` unified with `@media`), E12 (`measure_dom` in CI), H5 (telemetry demo moves), W7 (select-all on Tab focus), W2 (h)/W3 (editable cells + pen icon), B2/B4/B16/H7/M1 closed. §9 has no open questions.

**Sources.** (N) my own notes · (G) gist `fschutt/62b86a71b915f23a05b059d2767e6652` (Linux Mint first-run, 2026-09-08) ·
(S) "A 2026 Survey of Rust GUI Libraries" (blog.wybxc.cc, 2026-08-22, macOS) · (L) lobste.rs `s/83yugk` discussion ·
(CI) scheduled `Post-release checks` run 34815401779 on master, 2026-09-14 (failing daily since at least 09-13).

**Status legend.** `BUG` = reproduces, is wrong · `GAP` = missing feature · `STALE` = docs/comments/files out of date ·
`FIXED` = already fixed in tree since the note was written · `NOT-REPRO` = note was wrong or is no longer true · `?` = need your input (see §9).

**Priority.** P1 = a user hits it on the documented happy path · P2 = feature users ask for · P3 = hygiene. Effort S/M/L is a guess.

**Existing docs this ledger deliberately does not restate** (cross-link only):
`scripts/ideas/SCRIPTS_AUDIT_2026_08_01.md` (the "4.1 functional gaps" list lives at its line 103),
`scripts/ideas/mobile/PLATFORM_INTEGRATION_AUDIT.md` (per-platform stub matrix),
`scripts/ideas/SITE_AND_EXAMPLES_PLAN_2026_08_20.md` (site/examples backlog, incl. the macOS font post-mortem R1),
`scripts/ideas/SPEC_CONFORMANCE_REVIEW.md` (CSS spec verdicts), `scripts/audits/QUICK_PASS_HACKS_2026_07_28.md` (vacuous gates).
There is no CHANGELOG / ROADMAP / TODO file anywhere in the repo; this is the first cross-cutting one. Conventional home would be `scripts/ideas/`.

---

## 1. Engine / core correctness

### E1. `gap` / `row-gap` / `column-gap` in em or % silently becomes 0px — `BUG` P1 S
- `core/src/compact.rs:1579-1601` only writes the gap into the compact cache when `g.inner.metric == SizeMetric::Px`; otherwise the slot keeps its default, which is a plain `0` (`css/src/compact_cache.rs:1553-1554`), not a sentinel.
- `layout/src/solver3/taffy_bridge.rs:929-967` reads gap *only* from the compact cache (`map_or_else`, slow path only when the cache is `None`) and maps the sentinel to `0.0` anyway. There is no cascade fallback.
- The behaviour is locked in by `core/src/compact_test.rs:798-805`, whose comment says "so the slow path can handle it" — but for gap there is no slow path.
- Same defect class (px-only write, no sentinel/fallback): `border-spacing` (`compact.rs:1678-1687, 363-367`) and the four `border-*-radius` (`compact.rs:1743-1772`; reader maps sentinel → `px(0.0)` at `getters.rs:1752-1756`).
- Contrast: padding/margin/border-width/insets do it right — non-px → `I16_SENTINEL` → reader walks the cascade (`css/src/compact_cache.rs:2453-2467`, `layout/src/solver3/getters.rs:644-649`).
- **Todo:** write `I16_SENTINEL` on the non-px branch, add the sentinel → slow-path fallback in `taffy_bridge.rs:952-966` (pattern exists at `getters.rs:1945-1949`), fix the test, do the same for border-spacing and radii. Add a reftest with `gap: 1em`.

### E2. Make the layout crates environment-free / deterministic — `GAP` P2 M
- The note said `system.rs` is in core. It is in **`azul-css`**: `css/src/system.rs:73` (`AZ_RICING`), `:1848` (`XDG_CURRENT_DESKTOP`), `:1893-1923` (`DESKTOP_SESSION`, `GNOME_DESKTOP_SESSION_ID`, `KDE_FULL_SESSION`, `HYPRLAND_INSTANCE_SIGNATURE`, `SWAYSOCK`, `I3SOCK`), `:1939` (`LANGUAGE`/`LC_ALL`/`LC_MESSAGES`/`LANG`), `css/src/dynamic_selector.rs:847` (`AZ_THEME`). All 12 are **ungated**; `css/src/lib.rs:64` has `// #![no_std]` commented out and `css/Cargo.toml` has no `std` feature.
- `azul-core` is actually disciplined: `core/src/lib.rs:14` is `no_std`-capable and all 9 env reads (`AZ_SUPPRESS`, `AZ_LOG`, `AZ_LOG_STDERR`, `AZ_PROFILE`, `AZ_PROFILE_OUT`, `RUST_BACKTRACE`, `AZ_CASCADE_TRACE`, `AZ_OVERLAY`) are `#[cfg(feature = "std")]` with `no_std` twins. They are diagnostics-only. But core depends on css unconditionally, so core's `no_std` build is nominal.
- `azul-layout` has ~100 ungated reads, and some **change layout/render output**: `AZ_DENSE_TEXT` (`layout/src/solver3/layout_tree.rs:374`, `window.rs:9868,10043,18431`), `AZ_TEXT_HINTING` / `AZ_HINT_LIGHT` / `AZ_TEXT_SUBPIXEL` (`glyph_cache.rs:718-749`), plus `request.rs:340` (`AZ_BACKEND`), `taffy_bridge.rs:2469`, `solver3/cache.rs:1618,1857`, `telemetry/config.rs:508-665`, `updater.rs`, `widgets/map.rs` (16 sites).
- **Todo:** (1) move the css/system.rs desktop-environment detection behind a `SystemEnvironment` value that `dll/` constructs and passes in; (2) turn output-affecting layout env vars into fields of an explicit config struct read once in `dll/`; (3) make `azul-css` genuinely `no_std`-capable so core's flag means something. Diagnostics-only reads can stay behind `std`.

### E3. `<html>{height:100%}` UA rule still commented out → body-level scrolling broken — `BUG` P1 L
- `core/src/ua_css.rs:930-936`: `// (NT::Html, PT::Height) => Some(&HEIGHT_100_PERCENT),` with the 2026-06-02 DIAG comment; second site at `:615` (`Body, Height`). `HEIGHT_100_PERCENT` (`:81`) has zero live producers. The block at `:920-929` explains the consequence: `<html>` grows to content, `container_size == content_size`, useless 100 % scrollbar.
- The named "real fix" is the remill/web-lift jump-table (table-mirror) dispatch. Not done; the tree instead carries jump-table *avoidance* rewrites at `core/src/prop_cache.rs:1467-1472`, `compact.rs:971`, `styled_dom.rs:2887`, `dom.rs:3818`, and the lifter side is still open at `dll/src/web/loader_js.rs:609-611`, `transpiler_remill.rs:967,1224,2647,3666-3680`.
- New since the audit: `core/src/ua_css_test.rs:444-457` (`html_has_no_default_height`) now *asserts* the arm stays off. Re-enabling is a two-file change plus the lifter fix.
- **Todo:** decide whether native builds should get the rule back (cfg on `target_arch != wasm32`) while the lifter fix is pending.

### E4. `overflow:hidden` on inline-only block loses its BFC; 219 spec non-conformances — `BUG` P2 M
- `layout/src/solver3/layout_tree.rs:4585-4626`: the `Block | ListItem` arm returns bare `FormattingContext::Inline` when `has_only_inline_children` without consulting `establishes_new_block_formatting_context`; `FlowRoot` (`:4582`) keeps its BFC. Structural: `core/src/dom.rs:1174-1175` `Inline` is a unit variant with nowhere to record the bit.
- `scripts/ideas/SPEC_CONFORMANCE_REVIEW.md` has **219** PARTIAL/MISSING/INCORRECT verdicts out of 685, not "~105". Its line 8 tooling blocker (`azul-doc spec show` does not exist; `spec annotations` reports 0 known / 1237 unknown) is still true.

### E5. `::selection` is not a pseudo-element — `GAP` P2 S
- `css/src/css.rs:2041-2072` `CssPathPseudoSelector` has no `Selection` (but has `Placeholder`, so the pseudo-element machinery exists); `css/src/parser2.rs:458-500` has no `"selection"` arm.
- Ad-hoc replacement: `-azul-selection-background-color`, `-azul-selection-color`, and a third the audit missed, `SelectionRadius` (`css/src/props/property.rs:264-267, :42`; `css/src/props/style/selection.rs:3`).

### E6. Skip-ink underline — `GAP` P2 M
- Zero hits for `skip_ink|text-decoration-skip|has_descender` in `core css layout dll doc examples`. One rect per run: `layout/src/solver3/display_list.rs:7713-7738` (and `:4317`). The DL variant `Underline { bounds, color, thickness }` (`:1269-1273`) cannot express segments, and offset/thickness are `APPROX_*_RATIO` constants rather than the font's `post` table.
- **Todo:** read `post.underlinePosition/Thickness`, add a segmented underline DL item, sample glyph outlines against the underline band.

### E7. Paged/PDF output still drops opacity, filters, backdrop-filters, reference frames — `BUG` P2 M
- Clips / stacking contexts / scroll frames / text-shadow / image-mask are now re-derived per page (`display_list.rs:9560-9704`, commit `d2fb22564`). But `is_push_marker`/`is_pop_marker` (`display_list.rs:1580-1615`) do not list `PushOpacity`, `PushFilter`, `PushBackdropFilter`, `PushReferenceFrame`, so those still hit the `=> None` arm at `display_list.rs:8612`. A printed `opacity: 0.5` renders opaque.

### E8. `ClipboardEventData` is untyped and dead — `STALE` P3 S
- `core/src/events.rs:542-547` is still `content: Option<String>`; `EventData::Clipboard` (`:724`) has exactly one constructor in the tree and it is a test (`core/src/events_test.rs:1945`). The live rich path bypasses it via `ClipboardContent`/`StyledTextRun` (`dll/src/desktop/shell2/common/clipboard.rs:193-274`). Either type it like `TextInputEventData` (`events.rs:726`) or delete the variant.

### E9. `AzString::from_utf8` returns empty on invalid UTF-8 and there is no way to tell — `GAP` P2 S
- `css/src/corety.rs:447-464`: null ptr, zero length and invalid bytes all yield `Self::default()`. `copy_from_bytes` (`:268-281`) and `from_c_str` (the paths bindings use) are lossy (U+FFFD). `as_str` is `from_utf8_unchecked` (`:294`).
- No `is_valid_utf8` / `try_from_utf8` anywhere in `core css dll api.json`. The api.json `String` surface is `copy_from_bytes, from_c_str, to_c_str, from_utf16_le, from_utf16_be, from_utf8_lossy, from_utf8`.
- **Todo:** add `String::isValidUtf8(ptr, len) -> bool` and `String::tryFromUtf8 -> OptionString` to api.json; the check is already computed and thrown away at `corety.rs:276`.

### E10. Bare text nodes: give them a box (decided: yes) — `GAP` P2, M for the 80 % fix, L to be spec-correct
- True today: `layout/src/solver3/layout_tree.rs:4674-4686` (also `:4553-4568`, `:3536-3540`) hard-codes `FormattingContext::Inline` before `display` is read; the node's position stays at the `f32::MIN` sentinel and `used_size` is 0×0 (`display_list.rs:5513-5535`), so no `TAG_TYPE_DOM_NODE` hit area is ever pushed for it (`:6417-6425`).
- Correction 1: only `Hover`/`Focus` callbacks are inert; `Window` and `Component`/lifecycle listeners fire by node identity (`layout/src/dom_lint.rs:134-149`). Correction 2: not silent — `dom_lint.rs` warns once per finding (`AZ_SUPPRESS=bare_text`). Correction 3: `background`, `border`, `padding`, `color`, `font-*` on a text node **already paint**, because the run's style is read from the text node itself (`fc.rs:8778-8790, 9275-9287`) and smuggled into `InlineBorderInfo` (`getters.rs:2394-2416`, painted at `display_list.rs:2623-2680`). What is missing is a box: `width/height/margin/overflow`, a hit rect, a DOM-node tag.
- **Key finding:** a `<span>` has no box either. Non-replaced inline boxes are "transparent" (`fc.rs:9667-9690`, `sizing.rs:700-737` case 3); only atomic inlines get a rect written back (`fc.rs:4309-4325`, replay at `:4064-4076`). So this is "implement non-replaced inline box geometry", not "special-case text".
- **Rejected:** (a) wrapping in `Dom::text()` / the C API — double-wraps the ~55 `create_*_with_text` helpers, renumbers nodes (788 call sites, 49 test files assert node ids, 274 `NodeType::Text` matches incl. contenteditable/document_edit/a11y adjacency assumptions, `xml.rs:4662` round-trip), breaks the C ABI's stable ids; (c) conditional wrap at `StyledDom` construction — node ids would depend on whether a node has a handler, breaking the reconciliation diff (`core/src/diff.rs:180-197`) and the deterministic tag scheme (`prop_cache.rs:2106`). azul already does the anonymous-*block* half (`layout_tree.rs:2404-2494, 2609-2657` `InlineWrapper`); the anonymous-*inline* half is what is missing.
- **Recommended (b), layout-level:** (1) drop the three early returns so `display` is honoured on text nodes; (2) after inline layout, group `PositionedItem`/`ShapedCluster` by `source_node_id`, union into a rect, write `used_size` + position — at `fc.rs:4309-4325` **and** the cache-reuse replay `:4064-4076`, and for the dense representation (`text3/dense.rs:61-73, 757`; `display_list.rs:7818-7826` shows layouts can be cluster-only); (3) extend `parent_paints_as_inline_shape` (`display_list.rs:5558-5583`) so the box does not repaint what the IFC already painted; (4) retire the `INERT_CSS/CALLBACKS/TAB_INDEX` lint predicates. Precedent for one DOM node ↔ several boxes: `create_marker_pseudo_element` (`layout_tree.rs:2733-2789`). ≈300-500 lines over `layout_tree.rs`, `fc.rs`, `display_list.rs`, `sizing.rs`, `dom_lint.rs`, `dense.rs`.
- **The L tail:** a wrapped run is N fragments, not one rect (the union over-covers the concave region and mis-fires hit tests); selection/caret anchor to the IFC root today (`display_list.rs:3822-3845`) and must not double-offset; the cursor/DOM tag spaces already collide for `DomId(0)` (`layout/src/window.rs:8236-8258`); 49 baseline reftests + 52 `.xht` files are inline-geometry-sensitive and will need re-baselining.

### E11. `@container`: make it one thing with `@media` (decided) — `GAP` P2 S for the root-only step
- Today: parsing `css/src/parser2.rs:1327-1390`, selectors `css/src/dynamic_selector.rs:75-108`, matching `:1347-1353`, setter `with_container` `:1213-1221`. `with_container` has **zero production callers** (only `#[cfg(test)]`), so `container_width`/`container_height` stay `NAN` (`:1082, 1128`) and every size condition is false. No `container-type` / `container-name` property exists. `@media` is end-to-end live: the context is built from window state at `core/src/styled_dom.rs:2400-2433`, consumed at `core/src/prop_cache.rs:3201`, and `css/src/css.rs:141` flags stylesheets whose `@media (min-/max-width/height)` rules can flip so a resize triggers a restyle.
- **Decision (2026-09-14):** `@media` *is* `@container` evaluated against node 0 (the window root). Step 1: where the `@media` context is built (`styled_dom.rs:2400-2433`), also call `with_container(viewport.width, viewport.height, None)`, so an unnamed `@container (min-width: …)` matches exactly like `@media` (S: a few lines plus tests). Step 2: extend the `css.rs:141` "can flip on resize" flag to `ContainerWidth`/`ContainerHeight` rules so a resize restyles them too. Step 3 (later, L): `container-type`/`container-name` properties, per-node container sizes fed from the node's laid-out box, which needs a second cascade+layout pass; named containers (`ContainerName`) then resolve to the nearest ancestor with that name, defaulting to node 0.

### E12. `measure_dom`: in the API, but not tested through the C ABI or any e2e — `GAP` P2 S
- In the API: `core/src/callbacks.rs:562` (`measure_dom`), `:592` (`measure_dom_shrink_to_fit`), impl `layout/src/window.rs:4526-4681` (literal union of node bounds; hook injected at exactly one site, `window.rs:8019`). api.json exposes both on `VirtualViewCallbackInfo` (`:11739, 11770`) and `CallbackInfo` (`:15484, 15510`); generated `target/codegen/azul.h:50916-50921, 51351-51356` has `AzVirtualViewCallbackInfo_measureDom`, `AzCallbackInfo_measureDom` and the `…Byref` variants. (`dll/azul.h` is gitignored, `.gitignore:25`, and just a stale local file from May — nothing ships or reads it.)
- Tested today, all in required jobs: 4 plumbing-only unit tests with a fake hook (`core/src/callbacks_test.rs:424-560`), **one** real measurement test (`layout/tests/icon_pipeline.rs:210-239`), indirect coverage via the statusbar's content-sized VirtualView (`layout/tests/statusbar_live_label.rs:182-295`) and `dll/tests/transient_window_layout.rs:211,638`. Job: "Workspace Unit Tests (dev profile)" `rust.yml:1234-1272`, not experimental.
- **Not tested:** no call through the C ABI symbols anywhere (`examples/c/hello-world.c` is compiled + ASan-run in CI at `rust.yml:1496-1509` but never measures; no language in the e2e matrix calls it; `examples/go/main.go` does not either); no e2e JSON scenario (`scripts/gen_e2e_cases.py:1321-1323` deliberately classes `measure_dom` as geometry that "azul-doc reftest owns", so the generator emits no numeric assertion).
- **Todo:** (1) a C test in `examples/c/` (or a `dll/tests/*` headless test through the exported symbol) that builds a known DOM, calls `AzCallbackInfo_measureDom` with a fixed available size and asserts the exact `LogicalSize`; (2) one e2e scenario for a VirtualView whose item height comes from `measure_dom`, asserting the resulting virtual scroll size; (3) let `gen_e2e_cases.py` keep a numeric assertion for this one function.

### E13. Software titlebar "exclusively on macOS" — `NOT-REPRO`
- Full CSD (title + buttons) injects on every desktop OS: `dll/src/desktop/csd.rs:53-63` excludes only iOS/Android; drag is wired on macOS (`macos/mod.rs:4208`), Windows (`WM_NCLBUTTONDOWN`, `windows/mod.rs:2400`), X11 (`_NET_WM_MOVERESIZE`, `x11/mod.rs:7288`), Wayland (`xdg_toplevel_move`, `wayland/mod.rs:1675`).
- Only `WindowDecorations::NoTitleAutoInject` (title-only bar, no buttons) is macOS-only, deliberately, to avoid the double-titlebar on Win/Linux (`dll/src/desktop/shell2/common/layout.rs:812-844`). The "macOS ONLY" phrase exists only in a stale worktree audit.

### E14. Already fixed since the 2026-08-01 audit (record only)
- Mobile text input: iOS `UIKeyInput` + full `UITextInput` (`dll/src/desktop/shell2/ios/mod.rs:1269-1322`, `ios/text_input.rs`, commit `68cb24761`), iOS `UIPasteboard` (`ios/clipboard.rs:38`), Android IME via `scripts/android/NativeTextBridge.java` + 14 JNI fns (`android/mod.rs:3495-3850`). The class the audit grepped for (`NativeInputConnection`) was never the shipped name.
- macOS swallowed `RegenerateLayoutIncremental`: handled at `macos/events.rs:886-894` (commit `c70c25a04`), parity on all shells (`common/layout.rs:1606`).
- Multi-node icon replacement: whole subtree spliced at `core/src/icon.rs:869-917` (commit `27a971742`).
- Rich clipboard `styled_runs`: wired end to end (`layout/src/managers/selection.rs:6-14`); X11/XWayland still plain-text only by design.

---

## 2. Widgets — tree view, list view, table (lobste.rs)

The engine has the primitives (VirtualView with a 1M-row proof, keyed reconciliation + FLIP spring animation, drag auto-scroll, AccessKit on 5 platforms). `TreeView` (566 code lines, `layout/src/widgets/tree_view.rs`) and `ListView` (1171 lines, `list_view.rs`) use almost none of them. There is **no** `TableView`/`DataGrid`; `layout/src/widgets/mod.rs:334` has `// pub mod spreadsheet;` commented out.

### W1. TreeView against yokljo's criteria — `GAP` P2 L
| # | criterion | now | evidence |
|---|---|---|---|
| 1 | unlimited items, virtualized | MISSING | `tree_view.rs:399-524` eagerly emits one Dom per expanded node; zero hits for `virtual|viewport|overscan` |
| 2 | unopinionated model | MISSING | owned `TreeViewNode { label, children, is_expanded, is_selected }` (`:285-295`), deep-cloned per frame; guide prescribes rebuild-the-world (`doc/guide/en/widgets/structural.md:115`) |
| 3 | custom row delegate | MISSING | row = icon-or-spacer + label (`:441-458`); ListView *does* take `DomVec` cells (`list_view.rs:829-835`) |
| 4 | DnD with insertion line | MISSING | no drag code in widget; framework has `DragStart…Drop` (`core/src/events.rs:822-834`, `core/src/drag.rs`) but **no insertion indicator anywhere** |
| 5 | auto-scroll while dragging near edge | MET (framework) | `dll/src/desktop/shell2/common/event.rs:194-195, 310, 10538-10552` — works for node DnD, not just text |
| 6 | multi/range selection, stable across collapse | MISSING | `is_selected` is an app-set bool; callback delivers a positional `usize` that shifts when siblings change (`:45`, `:1296`) |
| 7 | keyboard navigation | PARTIAL | rows are focusable (`:462`); generic arrows/PgUp/Home via `layout/src/default_actions.rs:285-344`; **no** Left/Right expand-collapse, no type-ahead (zero hits repo-wide), Ctrl+A is text select-all |
| 8 | interactive items in a row | MISSING (tree) / MET (list) | ListView wraps arbitrary cells and puts the click on the row (`list_view.rs:1090-1119`) |
| 9 | granular model notifications | MISSING | single `on_node_click`; engine *recovers* moves by keyed diff (`core/src/diff.rs:361, 495, 595-599, 1074, 1226`) |
| 10 | animated expand/collapse/move | MISSING (widget) / MET (engine) | FLIP + springs already run on every reconciliation (`core/src/animation.rs:294-652`, `layout/src/window.rs:11160-11192`); rows just need stable `.with_key()` |
- **Decided (2026-09-14):** move the content onto `VirtualView` (flattened proxy model, yokljo's design; the list virtualization is the shared core), and add user function pointers for **sorting** and **searching/filtering** — see W2 for the API shape, which TreeView mirrors (`TreeViewState`, `TreeViewOnSortCallbackType(parent, direction)`, `TreeViewOnFilterCallbackType(query)`, `TreeViewNodeRangeCallbackType(first_visible, count) -> TreeViewNodeVec`). Widening `TreeViewOnNodeClickCallbackType` to carry a `TreeViewState` is an ABI break; all bindings regenerate.
- Still on the list after that: `DomVec` row delegate, selection model with stable ids (`.with_key()`), tree keyboard semantics (Left/Right, type-ahead, Ctrl+A over rows), DnD with an insertion indicator, FLIP via keys.
- **a11y bug found on the way:** `tree_view.rs:466-470` puts `Outline` (→ `Role::Tree`) on **every row** instead of `OutlineItem`; `list_view.rs:1082-1088` puts `Role::List` on every row instead of `ListItem`. No `Expanded` state or set-size/pos-in-set emitted. A screen reader announces a tree of trees.

### W2. ListView v2 (spec decided 2026-09-14) — `GAP` P2 L
**Current state (mqudsi's criteria):**
- Renders **every** row it is handed (`list_view.rs:1075-1076`); `ListView::visible_row_range` (`:896-903`, fixed-height only) is correct, tested, and **never called**.
- `on_lazy_load_scroll` (`:799`) and `column_context_menu` (`:796`) are declared, have thunks (`:709`), and are **never attached** in `dom()` (`:1009-1123`, which wires only `on_column_click` and `on_row_click`).
- Variable heights: `ListViewRow.height` exists (`:833`) but nothing measures on demand; the primitive is there (`VirtualViewCallbackInfo::measure_dom`, `core/src/callbacks.rs:560-575`) — no estimated-height cache.
- Scroll anchoring on insert-above: MISSING (only text-selection anchors exist; no `overflow-anchor`). `managers/virtual_view.rs:74-81` covers estimate growth only.
- Drag-to-scroll: touch panning only reaches scroll on Android/iOS (`android/mod.rs:1694`, `ios/mod.rs:685`); no desktop touch pan, no mouse-drag pan. Momentum/acceleration is MET (`managers/scroll_state.rs:80-140`).
- Docs stale: `doc/guide/en/widgets/structural.md:35, 140-147` say row/column callbacks are unwired; they are now. The lazy-load/context-menu half of the warning is still true.
**Spec (Felix, 2026-09-14).** The widget owns the hard parts — virtualization over `VirtualView`, row-height measurement and estimation, lazy querying of the host's data — and the host supplies rows, sorting, filtering and edits through function pointers. Rows may be infinite. `on_sort` **replaces** `on_column_click`.

- **(a) Data model and callbacks.** Follow `impl_widget_callback!` + `impl_managed_callback!` (`list_view.rs:717-735`); api.json routes `*CallbackType` → `callbacks`, `*Callback` → `dom`, `Option*` → `option`, rest → `widgets` (`doc/src/autofix/function_diff.rs:1074-1091`, "api.json drift" job `rust.yml:522`).
  `enum SortDirection { Ascending, Descending }`, `enum ListViewFindBarPosition { Hidden, Top, Bottom }`, `enum ListViewLayoutMode { Table, KeyValue, Auto }`;
  `ListViewState` gains `sort_column: OptionUsize` (today `sorted_by`), `sort_direction`, `filter_query: AzString`, `find_bar_position`, `column_order: UsizeVec`, `expanded_rows: UsizeVec`, and `current_row_count` becomes the post-filter count;
  `ListViewRowRangeCallbackType = extern "C" fn(RefAny, VirtualViewCallbackInfo, ListViewState, first_row: usize, row_count: usize) -> ListViewRowVec` — the lazy row supplier, called by the widget for the visible band (+ overscan) only; the host applies `sort_column/sort_direction/filter_query` from the state when producing the band;
  `ListViewOnSortCallbackType = extern "C" fn(RefAny, CallbackInfo, ListViewState, column: usize, direction: SortDirection) -> Update` — the host runs its own sort (or re-issues its query) and the widget re-fetches lazily;
  `ListViewOnFilterCallbackType = extern "C" fn(RefAny, CallbackInfo, ListViewState, query: AzString) -> Update`;
  `ListViewOnEditCallbackType = extern "C" fn(RefAny, CallbackInfo, ListViewState, row: usize, column: usize, new_value: AzString) -> Update`;
  `ListViewOnRowExpandCallbackType = extern "C" fn(RefAny, CallbackInfo, ListViewState, row: usize, expanded: bool) -> Update` and `ListViewRowDetailCallbackType = extern "C" fn(RefAny, CallbackInfo, ListViewState, row: usize) -> Dom` for the detail content;
  `ListViewOnColumnReorderCallbackType = extern "C" fn(RefAny, CallbackInfo, ListViewState, from: usize, to: usize) -> Update`;
  builders `with_on_row_range`, `with_on_sort`, `with_on_filter`, `with_on_edit`, `with_on_row_expand`, `with_row_detail`, `with_on_column_reorder`, `with_find_bar(pos)`, `with_layout_mode(mode)`, `with_total_row_count(n)`, `with_estimated_row_height(px)`.
- **(b) Virtualization + row height (widget-owned).** Build on `Dom::create_virtual_view` (`core/src/dom.rs:4768`); the render callback gets `reason/bounds/materialized/virtual_rect/scroll_offset` (`core/src/callbacks.rs:436-470`) and returns `VirtualViewReturn { dom, materialized, virtual_rect }` (`:643-680`) whose `virtual_rect` is *designed* to be refined over time ("only the scrollbar reads this", `:670-675`). Measure with `measure_dom` / `measure_dom_shrink_to_fit` (`:560-604`; statusbar precedent `statusbar.rs:1590-1618`, fall back to `bounds` when it returns zero). **Missing and to be written:** a per-row measured-height map, a prefix-sum/running-average estimator that feeds `virtual_rect`, scroll anchoring on insert-above (no `overflow-anchor` anywhere), and `EdgeScrolled`/`ScrollBeyondContent` (`managers/virtual_view.rs:588-640`, `EDGE_THRESHOLD = 200`) as the "fetch the next band" trigger. Delete the dead `on_lazy_load_scroll` (`:799`) and the unused `visible_row_range`.
- **(c) Sorting.** Today `on_column_click` (`:715`) carries no direction, `sort_arrow()` (`:670-691`) is one `arrow_drop_up` glyph (add `arrow_drop_down`), the doc at `:1158` says `MouseUp` while the filter is `Click`. Header click → widget toggles `sort_direction`, fires `on_sort`, then re-fetches the band lazily. Rename touch points: `api.json:11476, 27026, 56902, 57104, 57121, 57231`; `examples/c/widgets.c:25, 276, 376`; `examples/azul-go/*` regenerates; `doc/guide/en/widgets/structural.md:35` is stale anyway.
- **(d) Filter bar (Ctrl+F).** Nothing exists (zero hits for `find_bar|search_bar|filter_query` in widgets, azul-writer, azul-review, dll; azul-writer's ribbon has only the icons, `ribbon_ui.rs:397-398`). No widget-scoped accelerator API: menus have accelerators (`core/src/menu.rs:47`); widgets match keys in a `Focus(VirtualKeyDown)` handler via `get_current_keyboard_state()` + `ctrl_down()` (`text_input.rs:1216-1218`, `combobox.rs:943-947`); `VirtualKeyCode::F` appears nowhere. Bar = a `TextInput` at top or bottom; on query change write it into the shared `RefAny`, fire `on_filter`, then `trigger_virtual_view_rerender(dom_id, node_id)` (`api.json:12719`, `layout/src/callbacks.rs:2003-2015`) so the callback re-runs with `DomRecreated` and returns the shrunken `virtual_rect`. `update_virtual_view` (`api.json:16888`) alone is wrong for a filter. Reach the view node with `Dom::with_marker` + `get_node_id_by_marker` (`layout/src/callbacks.rs:3595`, as StatusBar does).
- **(e) Expandable rows.** Disclosure icon pair from `tree_view.rs:441-455` (`expand_more` / `chevron_right`, `LEAF_SPACER_STYLE` for alignment); toggle logic from `accordion.rs:518-580` — body is `get_next_sibling(header)`, and the ownership rule at `:541-565` applies: if the host returns `RefreshDom*`, write `CssProperty::initial(Display)` to clear the `set_css_property` user override, otherwise the override outranks every later rebuild; restate `AccessibilityState::Expanded/Collapsed` afterwards (`:567-578`). In a virtualized list the detail row must be part of the band the row-range callback returns (height measured like any row). No animation (accordion's own TODO at `:13-17`).
- **(f) Drag-reorderable headers.** Framework DnD is complete: `DragStart/Drag/DragEnd/DragEnter/DragOver/DragLeave/Drop` (`core/src/events.rs:820-834`), `DragContext`/`NodeDrag`/`DragData` (`core/src/drag.rs:104-421`), `CallbackInfo::{set_drag_data, get_drag_data, accept_drop, set_drop_effect}` (`layout/src/callbacks.rs:6189-6232`), edge auto-scroll during node drags. A working reorder already exists **in the demo, not a widget**: tab reorder in `examples/azul-widgets/src/lib.rs:401-442, 460-488` (`AttributeType::draggable(true)`, `set_drag_data("application/x-azul-tab", idx)`, `accept_drop()` in `DragOver`, remove/insert on `Drop`). Copy it for headers. **Missing:** a drop-indicator primitive (zero hits for `drop_indicator|insertion_line` in core/layout/dll; the demo even has an unread `drag_over` field at `:82`), and before/after discrimination (`DragOver` gives the node, not the half; compute from `get_node_size`/`get_node_position` `layout/src/callbacks.rs:3293-3299` + cursor).
- **(g) Mobile two-column key/value layout.** CSS-driven, like the only precedent `ribbon.rs:484-533`: emit both the table row and a kv block per row and gate them with `DynamicSelector::ViewportWidth` conditions (`MOBILE_MAX_PX = 720`, `mobile_only_visibility` / `desktop_only_visibility`), so a resize flips them with no rebuild. The kv block repeats the column header as the key cell. Alternatives if the band should not double: the host picks `ListViewLayoutMode` from `LayoutCallbackInfo::window_width_less_than` (`core/src/callbacks.rs:1618`, recorded so `layout()` re-runs only when the answer flips), or the VirtualView callback branches on `info.bounds`. E11's `@container` unification would let the widget's own width drive this instead of the viewport.
- **(h) Inline editing of the actual fields, with an optional pen icon (Felix, 2026-09-14).** Per-column configuration: today `columns: StringVec` (`:762-777`); either a `ListViewColumn { title: AzString, editable: bool, show_edit_icon: bool }` vec (ABI change, same regen as the state fields) or additive builders `with_editable_columns(UsizeVec)` + `with_edit_icons(bool)`. An editable cell renders its value plus, when configured, a trailing pen icon via `Dom::create_icon("edit")` — the name is already used by `examples/rust/src/icons.rs:84` and resolves through the icon-provider packs (`core/src/icon.rs:10-25`), so no new asset. Entering edit mode: click the pen, double-click the cell, or Enter/F2 on a focused cell; the cell swaps its label for a `TextInput` (a `TextInput` already fits `cells: DomVec`, `:829-835`, wrapped verbatim at `:1088-1100`). Commit via Enter (`TextInputOnVirtualKeyDownCallbackType`, `text_input.rs:604-623`) or focus loss (`on_focus_lost`, `:626-644`) → `on_edit(row, column, new_value)`; Escape cancels. The cell handler must `stop_propagation()` (`layout/src/callbacks.rs:1689`) or the row's `Hover(Click)` handler (`:1102-1119`) co-fires as a row select. Do **not** `RefreshDom` per keystroke: the controlled-widget hazard is documented at `examples/azul-widgets/src/lib.rs:1101-1112` (rebuild eats a half-typed `"3."`). Editable cells get `tab_index` in row-major order so Tab walks the grid; with W7 (select-all on keyboard focus) that gives the data-entry flow tab → type → tab → type; an `edit_on_focus` option opens the next cell's editor directly when focus arrives by Tab. Same configuration applies to the TableView (W3). No `spreadsheet` module exists (`widgets/mod.rs:334` is a commented-out line with no file).
- **(i) Demo: CRUD list over the `Db` API with a filter bar.** Home: `examples/azul-widgets/src/lib.rs` — a new `crud_section()` next to `tabs_section` (`:446`), called from `layout()` (`:521`), appended to the column at `:1017-1035`. `link-dynamic` reaches `azul::db` without any feature (proof: `examples/rust/src/resume.rs:16, 213-260`, e2e `tests/e2e/resume_db_sync.json`); on iOS/Android the demo is `link-static`, which has no `db-sqlite` (`dll/Cargo.toml:733-738`), so degrade gracefully. **Constraint:** `Db` is a key/value + index store — `Db::{open, get, set, remove, iterate(store, range, limit), query_index(store, index, range, limit), subscribe, sync_now}` over `DbValue`/`DbRows`/`DbKeyRange` (api.json module `db`, `:128144`) — and raw SQL is "never available through the API" (`api.json:9238`, `doc/guide/en/data/database.md:37-39`). So "sort by column" = `query_index` on that column's index, "filter" = key range or client-side over the band, "infinite rows" = `iterate` with `limit` per band. **Trap:** desktop reads run inline via `block_on` (`dll/src/desktop/extra/sqlite/mod.rs:19, 898-921`) and only the callback is deferred to the next pump (`layout/src/request.rs:121-124`), so a big band query blocks the UI thread; the `*_blocking` variants are Rust-only. For off-thread paging use the map widget's `Thread` + `ThreadWriteBackMsg` + `trigger_all_virtual_view_rerender` pattern (`map.rs:1846-2040`); `Db` is `Send + Sync` and `request::complete` queues through a global mutex, but nothing calls `Db` off-thread today, so this is unproven.
- **ABI / migration:** `ListViewState` crosses the C ABI by value in three typedefs (`:693, 715, 736`), so every field addition regenerates all bindings; remove `on_column_click` in the same release; update `examples/c/widgets.c:260-277, 355-376` and `structural.md:118-147`.
- **Effort:** (a)+(b)+(c)+(d) ≈ the core, M–L; (e)+(h) S each on top; (f) M because of the missing drop indicator; (g) S; (i) M including the off-thread question. Total L.

### W3. TableView / DataGrid (david_chisnall's 100M-row test) — `GAP` P2 L
- Nothing exists. The 1M-row `examples/c/infinity.c` and `examples/rust/src/infinity.rs` are hand-written VirtualView callbacks, not a widget.
- Shares W2's contract: lazy row range, `on_sort`, filter bar, `on_edit` with per-column `editable` + pen icon (W2 (h)), header reorder, and the row-major Tab order for data entry (W7). The difference is cell-level focus/selection (a focused cell, not a focused row) and fixed column widths with horizontal virtualization.
- At 1e8 rows × 22 px the virtual height is 2.2e9 logical px; all geometry is `f32`, so far-end row addressing degrades. A table widget needs an f64 or row-indexed scroll offset.
- Async background updates are MET at framework level (`layout/src/widgets/map.rs` is the worked example; targeted re-render API at `api.json:12721, 15749, 16890`).
- No stress benchmark in CI (no criterion, no frame-time assertion).

### W4. Sorting with minimal moves (LIS) — `GAP` P3 S
- No `longest_increasing` anywhere. Keyed diff + FLIP gives animated reorder but not minimal-move optimality. ListView only draws a sort arrow (`list_view.rs:1044-1069`) and never keys rows.

### W5. Text engine hooks (david_chisnall) — `GAP` P3 M
- Hyphenation: real (Knuth–Plass + `hyphenation` crate, `layout/src/text3/knuth_plass.rs:4-19`, `-azul-hyphenation-language` `css/src/props/style/exclusion.rs:134-142`, soft hyphen honoured) but **not pluggable**.
- Kerning: GPOS always on (`text3/default.rs:864`), `letter-spacing` exists; no `font-kerning` property to disable, no per-pair override.
- Non-rectangular flow: MET (`css/src/shape.rs`, `shape-outside`, exclusion margin).
- Pluggable text layout engine: MISSING — only public trait in `text3/` is `FontRefExt` (`default.rs:230`).

### W7. TextInput: select all when focus arrives via Tab (default on, disable-able) — `GAP` P2 S–M
- Goal (Felix, 2026-09-14): a data-entry worker does tab → type → tab → type; each Tab must land in the next field with its whole value selected so typing replaces it. Click focus keeps today's behaviour (caret at the click position).
- Today: `default_on_focus_received` (`layout/src/widgets/text_input.rs:1011-1042`) adopts the engine text and puts the caret at the end (`cursor_pos = engine_caret(...).unwrap_or(end_of_text)`); nothing selects. The engine already exposes what the widget needs: `CallbackInfo::set_selection(dom_id, node_id, Selection)` (`layout/src/callbacks.rs:3068`) and `get_node_selection_ranges` (mirrored by `engine_selection`, `text_input.rs:965-973`). Tab is `DefaultAction::FocusNext/FocusPrevious` (`layout/src/default_actions.rs:119-127`) → `FocusTarget::Next/Previous` (`:605-606`).
- **Missing piece: the focus event does not say why it arrived.** `FocusTarget` (`core/src/callbacks.rs:1728-1735`: `Id`, `Path`, `Previous`, `Next`, `First`, `Last`, `NoFocus`, `Directional`) is the request, but `FocusReceived` carries no cause; nothing named `focus_reason`/`focus_cause` exists in `focus_cursor.rs`, `window.rs`, or `callbacks.rs`.
- **Todo:** (1) record a `FocusCause { Pointer, Keyboard, Programmatic }` in the focus manager when a `FocusTarget` is applied (`Next/Previous/First/Last/Directional` → Keyboard; `Id` from a hit-test click → Pointer; `Id`/`Path` from `set_focus` inside a callback → Programmatic) and expose `CallbackInfo::get_focus_cause()`; (2) in `default_on_focus_received`, when the new `TextInput` flag `select_all_on_keyboard_focus` (default `true`; builder `with_select_all_on_keyboard_focus(bool)` next to `with_text/with_placeholder` at `:762-796`) is set and the cause is Keyboard, call `set_selection` over the whole value and mirror it into `inner.selection`; (3) same flag on `NumberInput` (default on) and `TextArea` (`text_area.rs:2170` wires `FocusReceived`; default **off**, a multi-line field should not be replaced by accident); (4) an e2e scenario: two inputs, Tab, type, Tab, type, assert both values replaced. Cheaper but fragile alternative for (1): read `get_current_keyboard_state().current_virtual_keycode == Some(Tab)` inside the focus handler — only valid if the key is still latched when the focus event dispatches in the same pass; verify before relying on it.
- Interacts with W2 (h): an editable list cell entered via Tab should open its editor with the value selected.

### W6. wareya's checklist — mostly MET
- AccessKit on Windows/macOS/iOS/Android/X11 (`dll/src/desktop/shell2/*/accessibility.rs`, `layout/src/managers/a11y.rs`), IME on Windows (IMM32, `windows/mod.rs:5744-5897`), macOS (`setMarkedText`), Wayland (`text_input_v3`, `wayland/{events,defines,mod}.rs`), X11 (GTK3 IM context, confirmed by the first-run log in G), custom widgets via VirtualView / `ImageRef::callback` / GL, TrueType + light hinting + LCD (`layout/src/glyph_cache.rs:583-755`), MIT, 4/4 desktop shells + android/ios/headless.
- Only the survey's "Accessibility" and "IME" cells for azul are blank because the author never got past the font bug (§3).

---

## 3. Fonts (survey verdict "😭 cannot read fonts" + Linux first-run warnings)

### F1. macOS "no text renders at all" — `FIXED` and deployed; survey predates the fix
- Root cause recorded in `scripts/ideas/SITE_AND_EXAMPLES_PLAN_2026_08_20.md:260-296` (R1) and fixed 2026-08-29 (`3042d9474`/`edfeb9e64`): the first-layout guard `cache_empty || build_complete` was false on both sides on macOS (2 cached patterns, scan of ~370 fonts still running), so `request_fonts()` never ran and every UI family missed. Fix: `dll/src/desktop/shell2/common/layout.rs:216-233` `should_request_fonts`; regression test `dll/tests/font_cache_regression.rs:1-86`.
- **Verified shipped (2026-09-14):** the live `https://azul.rs/ui/release/0.2.0/libazul.dylib` (Last-Modified 2026-09-08 15:38 GMT, sha in the tap formula commit `666f897`) contains the log string `[regenerate_layout] Font cache in sync with the completed build`, which only exists since `edfeb9e64`. The gist's Linux log from 2026-09-08 prints the same line. The survey ran on 2026-08-22, one week before the fix. "0.2.0" is a moving version; nothing to release, but the survey row will only change if the author re-tests.
- Not a scan-path bug: rust-fontconfig 5.0 scans `~/Library/Fonts`, `/System/Library/Fonts`, `/Library/Fonts`, `/System/Library/AssetsV2` recursively, `.ttc` accepted; CoreText is used only on iOS.
- **Todo:** none for the bug itself. Consider asking the survey author (blog.wybxc.cc) to re-test, and consider a visible build date/commit in the dylib (`strings` finds no git sha today) so "which 0.2.0 do you have" is answerable.

### F2. No embedded last-resort font on desktop → empty cache = zero glyphs — `GAP` P1 S
- `layout/src/solver3/getters.rs:4874-4909` `ensure_chains_nonempty` returns early on an empty cache; `layout/src/text3/cache.rs:9087, 9153-9165` then emits **no segment at all**, not even `.notdef`. Embedded fallback bytes exist only for web (`dll/src/web/eventloop.rs:69`) and the headless harness (`headless/mod.rs:3240`).
- **Todo:** embed one small face in the desktop dylib as the true last resort so text never disappears.

### F3. Linux `system:ui` chain is hardcoded Cantarell-first and ignores the desktop environment — `BUG` P2 S
- `css/src/system.rs:1348-1365`: `Cantarell → Ubuntu → Noto Sans → DejaVu Sans → Liberation Sans → "Sans"`, asserted DE-blind by the test `linux_fallback_chain_ignores_the_desktop_environment` (`:3306-3312`). The detected KDE `ui_font` ("Noto Sans" in G) is only used to pre-warm the registry (`dll/src/desktop/shell2/common/layout.rs:305-321`) and is never spliced into the CSS stack. Defaults on detection failure are GNOME's (`system.rs:794, 2521, 2571`).
- **Todo:** put the detected `ui_font` first in the `Ui` chain and drop the test.

### F4. `"Sans"` is not a generic and triggers UNRESOLVED noise — `BUG` P3 S
- `css/src/system.rs:1277` `SANS_SERIF = "Sans"` is azul's own injection; `GenericFamily::from_css` recognises `sansserif` but not `sans`, so `!is_generic_family("Sans")` lets it reach `report_unresolved_families` (`getters.rs:5123, 5202-5231`). fontconfig aliases (`Sans`/`Serif`/`Monospace`) should map to the CSS generics.

### F5. `[azul][font] UNRESOLVED font-family …` is worded as an error for a by-design situation — `STALE` P3 S
- `layout/tests/unresolved_family_render.rs:4-8` says the warning "is correct behaviour" for the shipped demos. The survey author and the gist both read it as a failure. Warn once, at debug level, and only escalate when the *whole* stack fails (which is F2's real error).

### F6. Variable font (`Ubuntu[wdth,wght].ttf`) renders as `.notdef` after the async registry finishes — `BUG` P2 M
- Documented as a workaround in `examples/azul-writer/src/fonts.rs:3-10` referencing `ENGINE-ISSUES.md #1`; that file does not exist (also referenced from `backstage_ui.rs:7`, `lib.rs:1232`).

---

## 4. Bindings, install routes and the language matrix

### B1. `link-static`-only examples: what is really outside the C API — `GAP` P2 M
- `examples/rust/Cargo.toml:92-117`: `icu_demo`, `fluent_demo`, `http_zip_demo` need `link-static` because `azul::desktop` exists only under `cabi_internal` (`dll/src/lib.rs:184-186`; `dll/Cargo.toml:664-770`).
- api.json **does** have `icu`, `fluent`, `http`, `zip` modules. What is missing is the free-function layer and some types:
  - zip: `zip_create`, `zip_create_from_files`, `zip_extract_all`, `zip_list_contents` (`layout/src/zip.rs:541-583`) and `ZipReadConfig/WriteConfig/PathEntry/FileEntry/File/ReadError/WriteError`.
  - fluent: `create_fluent_zip`, `create_fluent_zip_from_strings`, `export_to_zip` (`layout/src/fluent.rs:893-909`), `FluentError`, `FluentLanguageInfo`, `FluentLocalizerInner`.
  - icu: `IcuLocalizer` (non-handle), `CollationStrength`, `DateTimeFieldSet`; `ListType`/`IcuTime` collide with same-named api.json types in other modules.
  - http: bare `http_get/put/request_with_config`, `download_bytes_with_config`.
- Whole subsystems present only in `build-dll`, absent from `link-static`: `http`, `db-sqlite`, `pdf`, `video-native`, `map-tiles`, `telemetry`, `crash-mail`, `e2e-scripting` (`dll/Cargo.toml:670-743, 817-823`).
- **Todo:** decide per item: expose through api.json (then the examples can go back to `link-dynamic`) or document as static-only.

### B2. Go — idiomatic binding done; the documented install path does not build — `BUG` P1 S
- The idiomatic binding is in (closed 2026-09-14): one layered package `azul.go` → `types.go` → `functions.go` → `wrappers.go` → `callbacks.go` + `callbacks_export.go` (`doc/src/codegen/v2/lang_go/mod.rs:44-56, 120-126`); `examples/go/main.go` imports `github.com/azul/azul-go` with no cgo, and the e2e matrix gates it as SHIPPED tier (`scripts/e2e_language_matrix.sh:117-133, 1132-1146`).
- **Docs bug, three independent failures:** (1) all seven api.json Go tabs download only `main.go` and run `go mod init hello-world` with no `require`/`replace`, and the three `*-manual` descriptions still say "main.go calls the C API directly through cgo; no generated Go package needed" → `go build` fails with "no required module provides package github.com/azul/azul-go"; (2) the flat release dir copies `azul.go, types.go, functions.go, wrappers.go, go.mod` (`deploy.rs:918-938`) but **not** `callbacks.go` / `callbacks_export.go`, and those files are `package azul` while `main.go` is `package main`, so even hand-assembling one directory fails; (3) `azul-go-$VERSION.tar.gz` contains exactly `azul.h` + `main.go` (bundle map `{"azul.h","main.go"}`, `bundles.rs:386-388`, `deploy.rs:486-511`).
- **Fix:** ship the 7 generated files under an `azul-go/` subdirectory in both the bundle and the flat dir, ship a ready `go.mod` (or add `go mod edit -require=github.com/azul/azul-go@v0.0.0 -replace=github.com/azul/azul-go=./azul-go` to the steps), delete the "no generated Go package needed" sentence.
- Historical, fixed: the gist's `#cgo linux,darwin` (AND, matched nothing) from `363ee9775`; generator now emits `#cgo LDFLAGS: -lazul` (`lang_go/mod.rs:188`). Still open: the wrappers pass structs by value and never use the `*Byref` entry points (0 hits), which is what the gist's Go-GC crash (`0x8` enum tags in pointer-typed cgo fields during stack growth) argues for.

### B3. Kotlin uses JNA interface mapping (~10× slower per call than Java) — `BUG` P2 S
- Java: `Native.register` + `public static native` (`doc/src/codegen/v2/lang_java/functions.rs:127, 247`). Kotlin: `interface X : Library` + `Native.load` proxy (`lang_kotlin/mod.rs:273-278`, `managed.rs:35-40`), deliberately split per module to stay under the 64 KB `<clinit>` limit (`mod.rs:237-241`). Artifacts differ (`rs.azul:azul` vs `rs.azul:azul-kotlin`). The cost is undocumented. Switch the Kotlin template to `@JvmStatic external` direct mapping.

### B4. Scala — `BUG` P2 S
- No Scala codegen; Scala uses the Java jar. Docs say `scala run … --dep --repository` (`api.json:5979-6010`), which only works on Scala 3.5+ / scala-cli; no version prerequisite, no toolchain install step; CI gates on `scalac` so the documented command is never exercised. (CI) both scala jobs fail (Linux: 30 s `TimeoutException`; Windows: exit 127). The "refactor comments" note is dropped (2026-09-14).

### B5. OCaml — `BUG` P1 S
- (CI) both OCaml routes fail with `No switch is currently set`; docs omit `opam init` / `eval $(opam env)` (the e2e harness knows: `scripts/e2e_language_matrix.sh:477,1709`). The `hello_world.exe` underscore is now correct in api.json.
- `azul.ml` is 8.1 MB / 120k lines; `ocamlopt` overflows an 8 MB stack (`ulimit -s unlimited` needed, 3 min 20 s build). Split into `azul_dom.ml`, `azul_css.ml`, … with a facade module; also removes the ulimit.
- Tarball ships no native library; binding dlopens `libazul.{dylib,so}`/`azul.dll` by name (`target/codegen/azul.ml:9-25`, `AZ_DYLIB` override). Document as a hard prerequisite.

### B6. Haskell — `GAP` P2 M
- Single-module `Azul.hs` (3.4 MB) + `Azul/Types.hs` (4.2 MB) + `Internal/FFI.hs` (3.2 MB) + 2.5 MB C shim; GHC peaks at 3.5 GB RSS on one core. Split into `Azul.Types.{Dom,Css,Window,…}` behind a re-exporting `Azul` facade (unlocks `-j8`, incremental builds). Docs do not warn that distro `cabal-install` < 3.10 fails Hackage TUF root-key verification on first use.

### B7. Fortran — `BUG` P1 S
- `examples/fortran/hello_world.f90:2, 57` use bare `use azul` over a 10 MB / 244k-line module: **416 s** vs **7 s** with `use azul, only: …` (59×). Emit `only:` lists in the example template (`lang_fortran/mod.rs:147-148` currently promises "a single `use azul`"). Then split modules.

### B8. Zig — `GAP` P3 S
- `@cImport(@cInclude("azul.h"))` over the 5.6 MB header (`lang_zig/mod.rs:117-118`): 91 s cold, 5 s warm, `ReleaseSafe` 28 s. Ship a pre-translated `azul_c.zig`.

### B9. C++ standard mismatch on the documented path — `BUG` P1 S
- The single shipped `hello-world.cpp` is the **C++20** variant (`deploy.rs:1316-1317` ← `examples/cpp/cpp20/hello-world.cpp`, `#include "azul20.hpp"`, `string_view` literals). The apt route and `scripts/verify_install_commands.sh:493-500` compile it with `-std=c++17` and check for `azul17.hpp`; api.json's cpp03 tab says `-std=c++03`; the frontpage default dialect is `cpp23`. (CI) `install: apt` fails on exactly that line. Ship one `hello-world.cpp` per dialect (or name the file `hello-world-cpp20.cpp`) and make the verify script use the dialect's flag.

### B10. C# `hello-world.cs` is 404 on the live site — `BUG` P1 S (one-line fix)
- `curl -sI https://azul.rs/ui/release/0.2.0/hello-world.cs` → 404 today; `Hello.csproj` next to it is 200, so it is not a stale deploy.
- **Root cause (from the 2026-09-08 "Deploy to GitHub Pages" job log, run 34233011017):** the "LARGE assets → GitHub Release" step at `.github/workflows/rust.yml:5228-5291` builds its prune list with `find "$REL" -maxdepth 1 -type f \( -name '*.a' … -o -name '*.cs' -o -name '*.psm1' … \)`. The `*.cs` glob is there for the 9 MB generated `Azul.cs` binding but also matches the 1.4 KB `hello-world.cs`; the step uploads both to the GitHub Release (`gh release view 0.2.0` lists `Azul.cs Azul.psm1 hello-world.cs`) and then `rm -f`s them from the Pages artifact (`removed website/ui/release/0.2.0/hello-world.cs` at 15:35:56Z).
- **Fix:** add `! -name 'hello-world.cs'` next to the existing `! -name 'libazul.so' …` exclusions (or match `Azul.cs` by name instead of `*.cs`). Until then the file is at `https://github.com/fschutt/azul/releases/download/0.2.0/hello-world.cs` (200).
- (CI) csharp jobs on macOS/Linux fail compiling the HTML 404 page. Also switch every documented `curl -o` to `curl -fsSL -o` so a 404 fails instead of writing HTML into `Program.cs`.

### B11. Rust consumer route — `BUG` P1 M
- (CI) `install: rust`, `install: cargo`, `install: cargo (macOS)`, `steps: rust` all fail linking the documented consumer: `undefined symbol: AzApp_create …` on Linux, `symbol(s) not found for architecture arm64` on macOS — the crate's `build.rs` is not finding `libazul` (`AZ_LINK_PATH` / next-to-project / system dirs, per api.json). This is the gist's "missing export for setting library link path".
- rust-analyzer: the survey's complaint is real for the in-repo crate (`dll/src/lib.rs:211-262` `include!`s `../target/codegen/*.rs`, outside the crate, excluded from r-a's VFS) and fixed for users by the pre-rendered `azul` crate (`doc/src/dllgen/bundles.rs:180-315`, 2026-09-07) with a CI regression test (`scripts/verify_install_commands.sh:619-642`). Contributors building from source still hit it.
- crates.io: `azul-css/core/layout` 0.0.16 are published (`scripts/publish-crates.sh:29`); the user-facing `azul` bundle publishes only when `CARGO_REGISTRY_TOKEN` is set (`scripts/publish_upstream.sh:35`). `doc/guide/en/hello-world/rust.md:104` still says "azul is not on crates.io"; `README.md:19-45` still says "NOT usable yet" and documents only clone + `azul-doc codegen all` (the 1.9 GB checkout the survey complained about).

### B12. Ruby / Java / Node — `STALE` P3 S
- Ruby: `--clear-sources` is anticipated by installing `ffi` first (api.json ruby `all-gem`). Options: `--user-install` in docs (no sudo, works because `ffi` ships a prebuilt platform gem), a scoped `source` block in a Gemfile, or claim/rename the `azul` gem on rubygems.org (an unrelated 2010 `azul` 0.0.1 exists).
- Java: JNA searches `jna.library.path` and the system path before the jar resource, so an apt-installed `/usr/lib/libazul.so` shadows the jar's copy; version mismatch after upgrading one but not the other. Document or pin.
- (CI) `install: bindings`: `azul-java-0.2.0.tar.gz` lacks `HelloWorld.java` promised by api.json's bundle map; `install: maven`: jar not resolved from the mirror; `install: nuget`: `dotnet add package azul.net` fails against `azul.rs/ui/nuget`; `install: choco`: `libazul` not found although `scripts/build_registry_mirrors.sh:16` says the v3 feed serves it; `steps: c (windows-2022)`: all three Windows routes fail.

### B13. PHP — not done — `GAP` P2 L
- Two paths: pure `ext-ffi` `Azul.php` (9.5 MB; POD-only, cannot do callbacks: `scripts/e2e_language_matrix.sh:1904-1907`) and a Zend extension (`dll/src/php_extension.rs`, feature `php-extension`, `dll/Cargo.toml:1099`) that builds on Linux/macOS with special flags, not on Windows (`:1923`), and is `experimental: true` in CI (`rust.yml:2885, 3219`). Not published anywhere (no PECL/Packagist hits). api.json's three PHP blocks end with "See examples/php/README.md" — file does not exist.

### B14. Smalltalk — `STALE` P3 S
- Generator targets **Pharo** UnifiedFFI (`lang_smalltalk/mod.rs:1-26`), not GNU Smalltalk; `gst` is only the smoke tier (`e2e_language_matrix.sh:2121`). It emits one flat 8.5 MB `Azul.st` that it *calls* Tonel but is not a Tonel package directory. `examples/smalltalk/README.md` (which held the "Tonel layout blocker" text) was deleted, yet `e2e_language_matrix.sh:2108,2129` still cite it.

### B15. VB6 — record only
- 32-bit-only, audience "essentially zero" (`lang_vb6/mod.rs:3-20`, `api.json:6325`), experimental in CI. README with the "out of scope" text was deleted with the rest. Leave as is.

### B16. Docs / release generics — `STALE` P2 S
- 41 languages in api.json, 17 hello-world guide pages (`doc/guide/en/hello-world/*.md`), 11 deep-linked from the release page (`deploy.rs:2109-2126`). Per-language API reference pages are **not wanted** (decided 2026-09-14): the hello-world guides are the per-language entry point, and the language-neutral API reference (`doc/src/docgen/apidocs.rs`, `api/{version}.html`) is already pre-rendered and linked from every page.
- `azul-scala-0.2.0.tar.gz` is built but no install step ever downloads it.
- Version the downloadable hello-world files (the gist's first comment): `hello-world.cpp` needs to say which dialect it is (see B9).

---

## 5. Mobile / Android

### M1. Android keeps its Java classes for now (decided 2026-09-14) — record only, P3
- 12 Java files in `scripts/android/` (`AzulActivity`, `NativeTextBridge`, `NativeGestureBridge`, `AzulAccessibilityBridge`, `AzulGamepad`, `AzulFilePicker`, `AzulMediaSession`, `AzulBiometric`, `AzulKeyring`, `AzulGeolocation`, `AzulSensors`, `AzulPermissions`), compiled by `scripts/build-android.sh:110-122` (javac + d8 → `classes.dex`). Each exists because the corresponding Android API (`InputMethodManager`/`InputConnection`, TalkBack node provider, `onRequestPermissionsResult`, SAF picker, …) has no NDK entry point; you can *call* framework Java from Rust via JNI, but you cannot *implement* the callback classes without bytecode.
- `.so`-only already works: `AZ_ANDROID_NO_JAVA=1` skips the dex and keeps `android:hasCode="false"`, but the script does not swap `com.azul.app.AzulActivity` back to `android.app.NativeActivity` in the manifest (`build-android.sh:145-198, 235-237`), and you lose IME/a11y/permissions/pickers/sensors/biometric/keyring/geolocation/media session.
- Options: (a) prebuild `classes.dex` once in CI and ship it as a binary asset next to the `.so` (no javac on the user's machine, still no `.java` in the SDK); (b) generate DEX at codegen time; (c) NativeActivity-only tier. (a) is the cheap one.
- Stale: `dll/Cargo.toml:394` and `SCRIPTS_AUDIT_2026_08_01.md:106` still talk about `NativeInputConnection`.

### M2. Platform stubs (from `PLATFORM_INTEGRATION_AUDIT.md`) — record only
- Windows geolocation/sensors/biometric stubs, mic/audio still a 440 Hz test tone on macOS/Windows/mobile, no desktop pen/tablet path. See that file, lines 18-81.

---

## 6. Site, homepage, docs

### S1. Azlin vs Azul on the homepage — `BUG` P1 S
- `/` is `doc/templates/azlin-index.template.html`; nav = workspace / ui toolkit / operating system (`doc/src/docgen/mod.rs:1216-1223`, "the workspace IS the front page"). 16 "Coming soon" markers across `azlin-{index.template,ws,os}.html` (five product pills on the homepage itself); hero CTA is "Get notified"; the docs are reachable only via the "ui toolkit" tab and FAQ prose (`azlin-index.template.html:102,113,135,168`).
- `/ui/` still titles itself "Azul GUI Framework" (`doc/templates/index.template.html:4,18`); the homepage says "Azlin UI Toolkit" everywhere except one stray "Azul UI toolkit" (`azlin-index.template.html:32`). The relationship is never stated. This is the survey's first paragraph.
- **Todo:** one sentence on `/` explaining Azlin = product family, Azul = the toolkit, and an above-the-fold "Docs / Get started" link.

### S2. Guide pages on mobile — `NOT-REPRO` (verify on device)
- Viewport meta is on every page (`doc/src/docgen/mod.rs:911, 1146`); breakpoints exist in `azul-docs.css:391-413`, `flora.css`, `ui-landing.css`; the specific overflow bug was fixed in `be00c32b1` (`minmax(0,1fr)` post-mortem at `azul-docs.css:391-400`). Residual risk: `docs-guide.css` has only `@media print` (`:289`) and fixed 21/26 px sizes (`:43,112,144`). If it still looks wrong on a phone it is element-level, not infra.

### S3. Prefer Homebrew over raw `curl` in the install steps — `STALE` P1 M
- Route selection: first route whose `os[]` matches the detected OS (`doc/templates/index.template.html:128-139`); array order in `api.json → installation.languages.<lang>.install[]` is what the visitor sees. The tap is real (`scripts/build_registry_mirrors.sh:602-707` → `brew tap fschutt/azul https://azul.rs/ui/brew.git`), installs `lib/libazul.dylib`, `include/azul.h`, `include/azul03…23.hpp`, `lib/pkgconfig/azul.pc`, and is CI-verified for the C route only (`rust.yml:5508-5538`, currently failing on GNU `timeout` at `scripts/verify_install_commands.sh:264`, which macOS lacks).
- **Where macOS visitors still get a raw dylib curl as the default** (17 languages, none run in CI): `ada, algol68, cobol, crystal, d, freebasic, julia, lisp, nim, odin, perl, powershell, racket, red, smalltalk, swift, v`. `algol68` already runs `brew install algol68g` and then curls `libazul.dylib` anyway. **No brew route at all** for those 17 plus `csharp, java, node, php, python, ruby, scala` (package managers that bundle the lib, fine) and `vb6` (no macOS route).
- **Ordering slips:** `kotlin` lists `macos-maven` before `macos-brew` and `linux-maven` before `linux-apt` (the only frontpage language affected; CI-covered by the post-release `manual` job). `node/macos-bun` and `macos-deno` curl the dylib behind `all-npm`.
- **Same request on the other OSes:** `rust/linux-cargo` curls `libazul.so` although `apt install azul` exists, and `rust/windows-cargo` curls `azul.dll` + `azul.dll.lib` although `choco install libazul` exists, while `rust/macos-cargo` correctly uses brew. No route puts `linux-manual` or `windows-manual` ahead of a package route except kotlin.
- **Prose that contradicts its own tab:** `doc/guide/en/hello-world/zig.md:49` says "There is no package-manager story for Zig yet" (false: brew, apt, choco, scoop routes exist); `go.md`, `haskell.md`, `kotlin.md`, `lua.md`, `ocaml.md`, `pascal.md`, `fortran.md` have a `macos-brew` route but no brew paragraph and lead with `curl libazul.dylib`; `c.md:113-123` and `cpp.md:104-120` re-list the curls as peers instead of fallbacks; `README.md` has no install section at all (clone + build only). `rust.md:63-145` and the generated `azul-rust` README (`doc/src/dllgen/bundles.rs:330-336`) are the model shape: package manager first, curl as "or download it next to your project".
- **Brew cannot replace everything:** it ships only the C/C++ headers and the dylib. Every other language still needs its binding source (`azul.lua`, `azul.pas`, `Azul.kt`, `azul.ml`, …) via curl or a language package manager; that curl is fine and stays.
- **Todo, in order:** (1) reorder kotlin; (2) add a brew-first route cloned from `lua/macos-brew` (tap, install, curl binding file, run with `DYLD_LIBRARY_PATH="$(brew --prefix)/lib"`) for the 17 languages and demote their `macos` route to `macos-manual`; (3) apt/choco lines for `rust/linux-cargo` and `rust/windows-cargo`; (4) the seven guide pages + zig.md sentence + README install section; (5) fix `scripts/verify_install_commands.sh:685-687`, whose `extract_urls.py` reads `cfg.get("methods")`/`"platforms"` keys that api.json no longer has, so it link-checks zero api.json URLs today and would not notice a typo in any new route.

### S4. Post-release checks fail every day — `BUG` P1 M
- 20+ failing jobs in run 34815401779. Causes already isolated: B9 (c++17 vs cpp20), B10 (404), B11 (link path), B5 (opam switch), B4 (scala run), B12 (bindings/maven/nuget/choco/windows C), S3 (`timeout` on macOS), plus two CI-config bugs: `post-release.yml:248` iterates `lang: cpp` but api.json only has `cpp03…cpp23` keys ("no install steps for language 'cpp'"), and Alpine containers on `ubuntu-24.04-arm` cannot run JS actions (`post-release.yml:429-430`).

### S5. README — `STALE` P1 S
- "This repository is currently under heavy development. Azul is NOT usable yet." (`README.md:19-21`) and clone-only install (`:31-45`) contradict the release page. Update before anyone else surveys it.

---

## 7. Repo hygiene

| id | item | status | evidence / action |
|---|---|---|---|
| H1 | `doc/probes/fontblend-probe.xht` | STALE | Only file in `doc/probes/`; one backlink (`doc/src/reftest/mod.rs:1022`); not in the reftest glob (`doc/working/*.xht`). Move under `doc/working/` as a reftest or delete. |
| H2 | `scripts/api-json-additions/` | gone | Deleted in `63efaa2e8`; content merged into `api.json` (41 languages inline). Dangling comment: `doc/src/codegen/v2/lang_cobol/mod.rs:47`. |
| H3 | `scripts/audits/QUICK_PASS_HACKS_2026_07_28.md` | mostly fixed | 6/8 spot-checks fixed. Still live: D3 `tests/src/layout.rs` has no `mod` entry in `tests/src/lib.rs`; E3 `focus.pending_focus_request` has no production caller (`layout/src/managers/focus_cursor.rs:237-242`); D2 `DISABLED_hint_vs_freetype` feature still in `layout/Cargo.toml:415`. Fix those three and archive the doc to `scripts/ideas/`. |
| H4 | `scripts/e2e-web` "mini puppeteer" | undocumented outside its folder | Real and shipped: bundled into every dll by `dll/build.rs:107,737-794`, run via `AZ_BACKEND=<url> AZ_E2E=<dir>` (`dll/src/e2e_web_runner.rs:3-11`, `desktop/app.rs:319`). Documented only in `scripts/e2e-web/README.md`; not in `doc/guide/en/debugging/e2e-testing.md`; not in any workflow. Add a guide section and a CI job. |
| H5 | Telemetry demo → `examples/` | decided 2026-09-14 | Move `layout/examples/telemetry_grafana.rs` + `layout/examples/telemetry-grafana/` (compose stack: otel-collector → VictoriaMetrics + Loki → Grafana) to `examples/telemetry-grafana/` as a workspace member (add to the root `Cargo.toml` `members`) depending on `azul-layout` with feature `telemetry`; drop the `[[example]]` block at `layout/Cargo.toml:417-428`; keep the README's run line (`AZ_TELEMETRY=metrics AZ_TELEMETRY_ENDPOINT=… cargo run -p <new crate> --features telemetry,probe`). Add a guide page, since none covers telemetry. |
| H6 | Stale references | STALE | `examples/php/README.md` (api.json ×3), `examples/smalltalk/README.md` (e2e script ×2), `ENGINE-ISSUES.md` (azul-writer ×3), `dll/azul.h` (E12), `doc/guide/en/widgets/structural.md:35,140-147` (W2), `SCRIPTS_AUDIT_2026_08_01.md:106` + `dll/Cargo.toml:394` (`NativeInputConnection`). |
| H7 | Windows GUI | manual verification (Felix) | 16-file Win32 shell (`dll/src/desktop/shell2/windows/`, `mod.rs` 7.9k lines), 0 `todo!`, 2 TODOs (`mod.rs:656` menu bar from window state, `:660` `size_to_content`). CI builds and unit-tests it on `windows-2022` with an anti-vacuity gate (`rust.yml:785`); no workflow on any OS runs `AZ_BACKEND=native`, so the manual check is the only GUI evidence. Checklist for the manual pass: window opens, CSD titlebar drag (`WM_NCLBUTTONDOWN` path), IME composition (IMM32 path at `mod.rs:5744-5897`), text input relayout, menu bar, DPI change, and the three documented Windows install routes (all failing in CI, B12). |
| H8 | `.claude/worktrees/*` | noise | ~10 stale full-tree copies; several notes in this ledger were true only there. Consider gitignoring or pruning. |

---

## 8. Suggested order

1. One-line and doc fixes on the documented happy path: B10 (`*.cs` glob), B9 (C++ dialect), B2 (Go bundle), B5 (opam), B7 (`only:`), S4's two CI-config bugs, the GNU `timeout` in the brew verify job, S5 (README), S1 (Azlin sentence + docs link).
2. B11 (Rust consumer link path) and S3 (Homebrew-first routes + guide pages), which together are most of the daily post-release red.
3. Engine bugs with small fixes: E1 (gap), F2 (embedded last-resort font), F3/F4/F5 (Linux font chain + warning), E9 (UTF-8 check), E12 (`measure_dom` C-ABI test).
4. Widgets: W1 + W2 as one project on VirtualView with the sort/filter/find-bar callbacks (fixes the a11y role bug on the way), then W3.
5. E10 (inline box geometry, M for the 80 %), then bindings ergonomics: B3, B6/B8 splits, B1 API coverage decision.
6. Everything else in §7.

---

## 9. Decisions recorded 2026-09-14

- Go idiomatic binding is done; B2 keeps only the docs bug.
- Windows GUI: manual check by Felix; H7 has the checklist.
- "Homebrew first" = install steps prefer brew over raw curl; S3 has the per-language list.
- Bare text nodes get a box (E10, M for the 80 %, L to be spec-correct).
- Per-language API reference pages are not wanted (B16); the hello-world guides are the per-language entry point.
- The served 0.2.0 already has the font fix (F1).
- Scala comment note dropped (B4 keeps the `scala run` docs/CI problem).
- Telemetry demo moves to `examples/` as a workspace member (H5).
- `@container` becomes one mechanism with `@media`: unnamed container queries evaluate against node 0 / the window root first (E11 step 1), per-node containers later.
- ListView v2: `on_sort` replaces `on_column_click`; widget owns virtualization, row measurement and estimation, lazy row fetching; plus filter bar, expandable rows, drag-reorderable headers, mobile key/value layout, `on_edit` with per-column editability and an optional pen icon; demo = CRUD over the `Db` key/value API with a filter bar in `examples/azul-widgets` (W2, W1 mirrors it for TreeView, W3 for the table).
- TextInput selects its whole value when focus arrives via Tab, default on and disable-able, so data entry is tab → type → tab → type (W7; needs a focus-cause signal the engine does not have yet).
- Android keeps its Java classes for now (M1).

Nothing is waiting on an answer. One constraint worth knowing before the ListView demo starts: azul's `Db` API is key/value + index by design and never exposes SQL (`api.json:9238`), so the "SQL CRUD" demo is `iterate` / `query_index` / `set` / `remove` over a store, not `SELECT … ORDER BY`.
