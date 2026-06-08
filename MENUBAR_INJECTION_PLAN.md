# Software menu-bar injection on X11 (test window positioning)

Goal: when no native global menu exists, inject a software menu bar into the window;
clicking a top-level item opens its children as a dropdown **positioned below the item**,
exercising the menu/window-positioning code. Then add a `<select>`/`<option>` using the
same open-menu path.

---

# ✅ 2026-06-08 — BUG 1 FIXED: menubar dropdown opens + parented (X11, verified)

**The menubar dropdown now works.** Root cause was NOT the window-positioning code
(that was correct): the W3C dispatcher's `invoke_single_callback` (layout/window.rs)
hard-coded the callback hit node to `{root, null}`, so `info.get_hit_node()` returned
a null node for EVERY dispatched event → `open_menu_for_hit_node()` →
`open_menu_for_node()` → `get_node_rect(null)` = None → returned false → no `OpenMenu`
pushed → no popup. (The parent never actually moved in current code — the prior
session's "parent jumps" symptom was already gone.)

Fix — commit `66c343f36` `fix(callbacks): thread event-target node into get_hit_node()`:
- Extract `invoke_single_callback_at(hit_node, …)`; pass the real propagation target
  (`planned.dom_id/node_id`) from `dispatch_events_propagated` (dll/.../common/event.rs).
  `invoke_single_callback` stays a null-node wrapper for create/layout/timer/unmount.
- `open_menu_for_node` now prefers `get_node_hit_test_bounds` (display-list rect),
  falling back to `get_node_rect`.
- Verified on X11 (azul-paint, AZ_BACKEND=cpu): clicking "File" opens its dropdown
  (New/Open/Save/Quit) directly below the item at parent_origin+(2,26)=(644,362);
  main window stays put at (642,336). Screenshot /tmp/mb_popup_crop.png.
- This is a CROSS-PLATFORM fix: `get_hit_node()` was broken for all dispatched
  callbacks everywhere, not just the menubar.

## ✅ #10 double-popup — FIXED (W3C capture-phase double-fire)
One physical click fired `menubar_item_click` TWICE → two stacked Menu windows.
Root cause (general, not menu-specific): `matches_filter_phase` (core/events.rs)
IGNORED the propagation phase, so every `EventFilter` — all of which are bubble-phase
listeners (azul has no capture registration) — matched during the **Capture** walk
too. When the hit target is a descendant (a menubar item is hit via its **text
child**, target=NodeId(3), the item=NodeId(2) is an ancestor), the item's callback
was collected in BOTH the capture and bubble walks → fired twice. Confirmed by trace:
`COLLECT node=NodeId(2) phase=Capture` AND `phase=Bubble`. **Every** callback whose
hit target was a descendant double-fired (all buttons-with-text included).
Fix: `matches_filter_phase` returns false for the Capture phase. Verified: `[open]
menus=1` (was 2); core events tests 16/16, headless 23/23 green.

## ✅ Menu grab + dismissal — FIXED (commit `069a2b3e2`)
Two bugs made menus unusable: (1) the pointer grab silently failed — `XGrabPointer`
ran right after `XMapWindow` (XFlush, not synced) → `GrabNotViewable`, unchecked → no
grab → clicks fell through to the window BELOW (right-click context menu fired on a
"File>New" click) and the menu never got a click-outside. (2) Dismissal set
`is_open=false` directly, so the later `Drop→close()` skipped its `if self.is_open
{ XDestroyWindow }` body → the dismissed menu's X window LEAKED (stayed mapped +
grabbing). Fix: `XSync` + retry-until-`GrabSuccess`; dismissal/Escape call
`self.close()` (ungrabs + XDestroyWindow). Verified: grab returns 0, clicks route to
the menu (wtype=Menu), click-outside/re-click dismisses (menus 1→0), right-click opens
a single context menu that dismisses.

## ✅ BUG 3 "View"→"V" clip — FIXED (text3 kerning, NOT taffy)
The earlier "taffy under-allocates the widest flex item" diagnosis was **wrong**. The
flex item IS sized correctly — to its label's **max-content width**. The real bug was a
kerning inconsistency between two text3 code paths:
- `measure_intrinsic_widths` (sizing) summed only `c.advance` — **omitting per-glyph
  kerning** — so max-content under-measured the kerned text.
- `get_item_measure` (the line breaker) sums `c.advance + kerning`.

So for words with **positive total kerning** ("View", "Wiew", "Xiew", "AAAA", …) the
breaker's width exceeded the box that max-content sizing produced → the unbreakable word
"overflowed its own box" → with `overflow-wrap:normal` the breaker force-placed only the
**first cluster** ("V"), and the 1-line-tall box dropped the rest. Words with ≤0 kerning
("File"/"Edit"/"Help"/"Open") were unaffected — exactly the observed pattern (it is NOT
about width magnitude: "Vi" at 2 chars also clipped).

Fix (`layout/src/text3/cache.rs`, `measure_intrinsic_widths`): sum `advance + kerning`
per cluster, bit-identical to `get_item_measure`, so the shrink-to-fit box exactly fits
its kerned text. Verified: probe of 12 words (incl. View/Wiew/Xiew/Viww/AAAA/Vi/Vie) all
emit one glyph per char; real azul-paint headless snapshot shows "File Edit View Help"
with "View" spanning 29px / 4 glyph-clusters (was 7px / "V"). Regression test:
`layout/tests/menubar_item_clip.rs`. lib (112) + 9 width-sensitive integration suites green.
Also fixed (same investigation) the **paged/PDF path** (`getters.rs`): when no
`SystemStyle` is threaded in, `system:` fonts now resolve via `Platform::current()`
(matching the font-LOADING pass) instead of a bare "sans-serif" the loader never
registered — which had made system-font text measure as 0-width there.

## STILL OPEN
- **Context-menu position +2,+26 offset** (minor): right-click context menu opens at
  cursor + (2px WM-border, 26px menubar-height); should be exactly at the cursor. The
  dropdown is correct (verified at the item rect), so it's specific to how the
  right-click window-local position relates to `parent_pos` in `show_menu`.
- BUG 2 (light-on-dark menubar colors): open.

## 🟡 Tile worker (azul-maps) — IN PROGRESS
The demo used `MapWidget…dom()` (no fetch → Pending placeholders). Wired
`.dom_with_fetch(ThreadCallback::new(azul::desktop::extra::map::tile_fetch_worker))` +
enabled the `map-tiles` feature (pulls http + mvt-reader/geo-types/proj4rs/geojson).
The worker http-GETs each tile's MVT, decodes, renders SVG, writes back via
`map_tile_writeback`. Building + runtime-testing (does it fetch + render? pan in cpu?).

## Tooling added: `scripts/mb-test.sh`, `scripts/mb-test2.sh`, `scripts/menu-grab-test.sh`
(launch azul-paint CPU → xdotool click → diff window tree → screenshot → grep probes).

---

# ⏸ MONDAY HANDOFF — 2026-06-06 (end of Friday session)

Branch `mobile-ios-android`. Two commits landed this session; a third (menubar) is being
committed now. Read this block first, then the dated "CURRENT STATE" sections below.

## ✅ DONE + COMMITTED this session
1. **#27 unified window parenting** (`8c4dfd1e5`): `WindowPosition::RelativeToParentWindow`
   resolved in X11/macOS/Windows `position_window_on_monitor` via the window registry
   (`parent_origin + offset`, monitor-relative fallback); removed the Windows-only
   `WindowsWindowOptions.parent_window` HWND field + `HwndHandle`/`OptionHwndHandle` types;
   `menu.rs::show_menu` now emits `RelativeToParentWindow`. Fixed a real **autofix bug**
   (`are_types_equivalent` now strips `ManuallyDrop<…>` + accepts `*mut c_void`), renamed
   the test-local `ScrollState`→`ScrollTestState` (FFI name collision). `autofix` is now
   ZERO-DIFF + exit 0. Verified: linux headless 23/23, macOS+Windows cross-compile.
2. **api.json drift applied** (in the same commit): CssPathSelector::Root(CssScopeRange),
   CssScopeRange struct, IconProviderHandle Drop. All bindings regenerated.

## 🟡 DONE (compiles + renders), COMMITTING NOW — software menu bar
- New `layout/src/widgets/menubar.rs` (`build_menubar_dom`), injected at the **Dom level**
  (before `create_from_dom`) in `shell2::common::layout.rs` via `inject_software_menubar`;
  removed the old broken `csd.rs` menubar. Styled 100% with inline `with_css(&str)` +
  `system:` colors + `system:ui` font. **RENDERS HORIZONTALLY + themed on X11** (verified:
  /tmp/mb2_rest_crop.png). One unified menubar path now.

## 🔴 BUGS TO FIX MONDAY (priority order)
1. **Menubar click repositions the PARENT window instead of opening the child popup**
   (user diagnosis; main window jumped 50,50→54,122 on click, no dropdown). The open-menu
   path must CREATE + position a child popup, not move the current window. Trace
   `menubar_item_click → open_menu_for_hit_node → OpenMenu → show_menu_from_callback →
   show_fallback_menu → show_menu (RelativeToParentWindow + parent_window_id) → pending
   window create → child position_window_on_monitor`. Suspect: child window not actually
   created, OR the X11 store-back (`self.common.current_window_state.position = Initialized`
   in the relative arm) running against the parent; confirm `self` is the CHILD.
2. **Menubar colors look wrong** ("probably a system style problem" — user). `system:*`
   resolve to light-theme fallbacks → light bar on a dark app. Investigate `SystemColors`
   population on X11 (likely empty → fallbacks). Deferred.
3. **"View" (3rd menubar item) renders as "V"** — text clipped to one glyph (File/Edit/Help,
   also 4 chars, render full). Item box looks correctly sized → a text3 layout/measure quirk
   on that item. Not root-caused.
4. **contenteditable: text blank on FIRST draw, appears on second draw** (user-confirmed,
   reproduced /tmp/ce_shot2.png). First-frame render/relayout ordering bug. Cursor caret DOES
   render + typing works (saw "Hello WHi|orld" after click+type). Scroll + cursor-BLINK need
   a LIVE test (a screenshot can't show blinking).

## 🧪 TO TEST MONDAY
- **contenteditable live (X11, CPU)**: scrolling (mouse-wheel over the blue multi-line area —
  exercises the #13–#18 scroll-shift work) + cursor blink. LAUNCH COMMAND (see gotcha #1):
  `cd tests/e2e && LD_LIBRARY_PATH=$PWD/../../target/release AZ_BACKEND=cpu ./contenteditable_test`
  (rebuild first: `cc contenteditable.c -I../../target/codegen/ -L../../target/release/ -lazul
  -o contenteditable_test -Wl,-rpath,$PWD/../../target/release`).
- **menubar click → dropdown** once bug #1 is fixed.
- **Wayland** (NEXT MAJOR): run azul-paint + contenteditable under Wayland. RelativeToParentWindow
  is the enabler (no absolute coords there); verify menu/dropdown popups position via the
  parent-relative path. The Wayland backend's `xdg_popup` positioner is still a gap (task #6).

## ⚠️ ENVIRONMENT GOTCHAS (cost real time this session)
1. **STALE `/lib/libazul.so`** (root-owned, ~41MB, OLD ABI) shadows the fresh
   `target/release/libazul.so` (~276MB). C examples link a *relative* rpath, so from the wrong
   CWD they load the stale lib → immediate crash (exit 144). ALWAYS run with
   `LD_LIBRARY_PATH=<abs>/target/release`. Consider deleting/refreshing `/lib/libazul.so`.
2. **`libazul.so` is 276MB** (release). Almost certainly unstripped debug info
   (`[profile.release] debug=...` or split-debuginfo). Investigate stripping / `debug=0` for a
   smaller, faster-to-link build — slows every rebuild + bloats the C-example link.
3. **Harness exit 144** on long foreground commands / `sleep` (artifact, not an app crash —
   the verify script also exits 144 yet produces screenshots). Use `run_in_background` or have
   the USER launch persistent GUI apps via the `!` prefix.

## ▶ HOW TO REBUILD (the full chain — each ~3–4 min)
`cargo run -p azul-doc -- codegen all` (if api.json changed) → `cargo build --release -p
azul-dll --features build-dll` (→ libazul.so) → `cargo build --release -p azul-paint`
(rebuild examples against the new ABI — `WindowCreateOptions` layout changed this session).

## STILL TODO (feature backlog)
- Expose `Menubar` + `Titlebar` as api.json widgets (repr(C) config). Titlebar already in
  api.json; add `Menubar { menu: Menu }` + `create`/`dom`. (User asked; not yet done.)
- Refactor software **titlebar** injection to the same Dom-level pattern (currently still
  StyledDom-level `csd::wrap_user_dom_with_decorations`). (User asked.)
- Step 3 DropDown/`<select>`; Step 5 migrate azul-paint header buttons → menu callbacks.
- Consider removing `add_component_css` from api.json (folded into `with_css(&str)`).

---

## Live checklist (SOURCE OF TRUTH for the cron — tick items + rewrite the
## "CURRENT STATE" note at the very bottom after each verified increment)

### Foundation — make the EXISTING menu/popup system actually work at runtime (X11)
The popup system (a `WindowType::Menu` window per `show_menu`) was build-verified
last session but NEVER runtime-tested. First real test (this session) found it
fundamentally broken; fixing it IS the "solid fallback" the user asked for first.
- [x] **#47 leak fix** (`b663e141e`) — bare `with_css` is node-only.
- [x] **#47 follow-up: component-CSS descendant selectors** (`8f1914748`, this session) —
  `Root(range)` now matches by subtree containment; bare `*` stays node-only, but
  selector rules (`add_component_css`, e.g. `.menu-item`) match the owner's subtree.
  This was why menu popups collapsed to ~0×16 (the `.menu-item` rules never matched).
  Unit-tested (`core/tests/css_layout_prop_47.rs`); full css+core suites green.
- [x] **Step 1 — global-menu detection** (`865f8e193`).
- [x] **GPU→CPU fallback** (`e71e3fde6`).
- [ ] **Context-menu TRIGGER fix (X11)** — `get_first_hovered_node` returned the ROOT
  (BTreeMap `.keys().next()` = min id), and `try_show_context_menu` decoded the node id
  1-based (`from_usize`) vs the 0-based `index()` producer ⇒ context menus never fired.
  FIX (uncommitted): return the DEEPEST hit node (`next_back()`) + walk the ancestor
  chain for the nearest context menu + 0-based `NodeId::new` decode. Replicate to
  wayland (`mod.rs:2307/2330`) + macos after X11 is verified.
- [ ] **Runtime-verify the popup renders at the cursor** (X11 screenshot), then commit.

### Step 2 — inject software menubar (recipe in "Step 2 implementation" below)
- [ ] widget `layout/src/widgets/menubar.rs` + inject in dll `common/layout.rs`.

### Step 3 — `<select>` widget
- [ ] make `DropDown` open on click + update label + fire `onchange`; expose in api.json.

### Cross-cutting
- [ ] relative/edge-aligned popup positioning (BottomOfHitRect / RightOfHitRect),
  works on X11 AND Wayland (xdg_popup relative positioner — the Wayland path is a gap).
- [ ] migrate azul-paint header buttons → menu callbacks; elaborate menus.
  (NOTE: azul-paint's `HEADER` css is missing `display:flex` → buttons stack vertically;
  fix when migrating.)
- [ ] KDE/global app-menu DBus export (blind build-verify).
- [ ] context menu in azul-paint (metaballs/paint mode) — added on canvas+body (uncommitted).

## Key facts already gathered (so step 2 is mechanical)
- **Injection template**: `inject_software_titlebar` (dll `common/layout.rs:1175`):
  build a widget Dom → `StyledDom::create(&mut dom, Css::empty())`, then
  `container = StyledDom::create(Dom::create_html())`, `container.append_child(widget)`,
  `container.append_child(user_dom)`. Called in `regenerate_layout` (~line 262, the CSD
  block). **Inject the menubar BEFORE the CSD titlebar** so the bar ends up *below* the
  titlebar: order = titlebar / menubar / user content.
- **Read the menu_bar**: after `create_from_dom`, the root carries it:
  `styled_dom.node_data.as_container()[NodeId::ZERO].get_menu_bar().cloned()` →
  `Option<Box<Menu>>` (core `dom.rs:2460`; `copy_special` preserves `extra`).
- **Callback wiring template**: titlebar (`layout/src/widgets/titlebar.rs:376`):
  `.with_callbacks(vec![CoreCallbackData { event: EventFilter::Hover(HoverEventFilter::MouseUp),
   callback: CoreCallback { cb: callbacks::FN as usize, ctx: OptionRefAny::None },
   refany: RefAny::new(DATA) }].into())`.
- **Positioning API**: `CallbackInfo::open_menu_for_hit_node(menu) -> bool`
  (`layout/src/callbacks.rs:1781`) → positions the popup at the hit node's **bottom-left**
  (`open_menu_for_node:1757`: `rect.origin.x, rect.origin.y + rect.size.height`) = a dropdown
  under the bar item. This is the window-positioning path to test.
- **Menu API** (`core/src/menu.rs`): `Menu { items: MenuItemVec }`, `Menu::create(MenuItemVec)`;
  `MenuItem::String(StringMenuItem)|Separator|BreakLine`;
  `StringMenuItem { label: AzString, callback: OptionCoreMenuCallback, children: MenuItemVec, .. }`.
- azul-paint already declares the menu via `Dom::create_body().with_menu_bar(menu)`
  (`examples/azul-paint/src/lib.rs:733`) — File/Edit/View(+children) and a leaf Help.

## Step 2 implementation
1. **New widget** `layout/src/widgets/menubar.rs` (register in `widgets/mod.rs`):
   - `pub fn build_menubar_dom(menu: &Menu) -> Dom`: a flex-row container
     (`.with_css("display:flex; flex-direction:row; background:<system>; ...")`, class
     `__azul-native-menubar`) whose children are one `build_item` per top-level
     `MenuItem::String`.
   - `fn build_item(item: &StringMenuItem) -> Dom`: a div with the label text +
     `with_css("padding:4px 10px; cursor:pointer")` + a MouseUp `CoreCallbackData` whose
     `refany = RefAny::new(submenu)`, where
     `submenu = if item.children.is_empty() { Menu::create([MenuItem::String(item.clone())]) }
      else { Menu::create(item.children.clone()) }` (leaf items become a 1-item dropdown so
     their callback still fires; refine leaf UX later).
   - `mod callbacks { pub extern "C" fn menubar_item_click(data: RefAny, mut info: CallbackInfo)
     -> Update { if let Some(m) = data.downcast_ref::<Menu>() { info.open_menu_for_hit_node(m.clone()); } Update::DoNothing } }`
     (confirm the exact callback fn signature against `titlebar.rs` `callbacks::titlebar_drag_start`).
2. **Inject** in dll `common/layout.rs`: add `fn inject_software_menubar(user_dom: StyledDom,
   menu: &Menu) -> StyledDom` mirroring `inject_software_titlebar`, and in `regenerate_layout`
   BEFORE the CSD block:
   ```
   let user_styled_dom = match root_menu_bar(&user_styled_dom) {
       Some(menu) if !dbus::native_global_menu_available() => inject_software_menubar(user_styled_dom, &menu),
       _ => user_styled_dom, // native path: export via gnome_menu DBus (separate TODO)
   };
   ```
   Cache `native_global_menu_available()` (per-process) to avoid a DBus round-trip each frame.
3. **Build + test**: `cargo build -r -p azul-examples --example <azul-paint binary>` (or the
   dll + run azul-paint), then on X11 click File/Edit/View → a dropdown must appear directly
   under the item. That validates the window-positioning code.

## Step 3 — `<select>`/`<option>`
- A `<select>` element rendered as a closed box (current value). On click, build a Menu from
  the `<option>`s and `open_menu_for_hit_node` (same path, positioned below — no submenus).
- The select window holds a backreference (`RefAny` + Callback) to the user's `onchange`;
  picking an option closes the popup and invokes onchange with the chosen value.

## Notes
- #47 (node-scoped inline CSS) is done/committed (`b663e141e`) — `with_css` strings now apply
  node-only, so the menubar's inline CSS is safe to use.
- Re-run `cargo test -p azul-core` cleanly (no concurrent edits) once more to reconfirm #47
  has no regression (the in-session run thrashed due to concurrent rebuilds).

---

## CURRENT STATE — 2026-06-06 (late): SOFTWARE MENUBAR RENDERS ✓ (X11), Dom-level

MILESTONE: the software menu bar now renders as a HORIZONTAL bar (File / Edit / View /
Help) at the top of azul-paint on X11, themed from the `system:` CSS namespace.
Screenshots: /tmp/mb2_rest.png, /tmp/mb2_rest_crop.png (rest), /tmp/mb2_file.png (after
click). Built as one unified Dom-level widget.

### What was done (UNCOMMITTED on mobile-ios-android until this commit)
- **New widget** `layout/src/widgets/menubar.rs` (`Menubar` pending): `build_menubar_dom(menu)`
  → flex-row `.azul-menubar` of one `.azul-menubar-item` per top-level `MenuItem::String`.
  Each item's `MouseUp` callback `menubar_item_click` stores the submenu as a `RefAny`
  backreference and calls `info.open_menu_for_hit_node(menu)` (the unified
  `WindowPosition::RelativeToParentWindow` popup path). A top-level leaf opens a 1-item menu
  of itself so its callback still fires.
- **Styling is 100% inline `with_css(&str)`** with the `system:` color namespace
  (`system:window-background`, `system:text`, `system:selection-background`,
  `system:selection-text`) + `font-family: system:ui`, `:hover` via CSS nesting. NO
  `add_component_css` / `SystemStyle` threading.
- **Injected at the Dom level** in `shell2::common::layout::regenerate_layout`, BEFORE
  `StyledDom::create_from_dom`, via `inject_software_menubar(user_dom)` (Linux-only, gated on
  `!gnome_menu::should_use_gnome_menus()` + root `get_menu_bar()`). This is the KEY fix — see
  the CSS-model note below.
- **Unified / removed the old broken menubar**: deleted `csd.rs::create_menubar_styled_dom`
  + `csd_menubar_item_callback` (wrong `.csd-menubar` stylesheet that `create_menu_stylesheet`
  never styled → it rendered VERTICAL + unstyled; stale callback ABI). One menubar path now.

### CSS MODEL (researched — the reason the first attempt failed)
- `with_css(&str)` → `parse_inline` wraps in `* { … }`, tags rules `rule_priority::INLINE`
  (=30, the HIGHEST: UA 0 < SYSTEM 10 < AUTHOR 20 < INLINE 30), stores on the Dom's `.css` vec.
- `scope_inline_css` (core `styled_dom.rs`) runs ONLY inside `create_from_dom`: it walks the
  tree assigning flat NodeIds and `push_front_scope(start,end)` prepends a `Root(CssScopeRange)`
  — **node-only `[start,start]` for bare-`*` rules, subtree `[start,end]` for selector rules**.
  Re-synthesized every flatten ⇒ survives `append_child` ONLY when flattened together.
- FIRST ATTEMPT FAILED because the bar was built as a SEPARATE `StyledDom::create()` +
  `append_child` — `create` never runs `scope_inline_css`, so the rules were never scoped/applied
  (rendered vertical+unstyled). FIX = inject the raw `Dom` before `create_from_dom`.
- `system:` colors (css `props/basic/color.rs`) resolve against `SystemColors` at cascade time.

### KNOWN ISSUES (user-noted, deferred — finish-before-midnight)
1. **Colors look wrong** — "probably a system style problem" (user). The `system:*` values
   resolve to light-theme fallbacks; `SystemColors` likely not populated for this WM, so the
   bar is light-on-dark-app. Investigate `SystemColors` population on X11.
2. **"View" (3rd item) renders as "V"** — text truncated to one glyph; File/Edit/Help (also
   4 chars) render full. Item box looks correctly sized (big gap before Help), so it's a TEXT
   clip/measure quirk on that item, not flex sizing. NOT yet root-caused. Investigate text3
   layout of the menubar item text node.
3. **Click repositions the PARENT window instead of opening the child popup** (user diagnosis,
   matches observation: the MAIN window jumped 50,50 → 54,122 right after clicking File; no File
   dropdown appeared). The open-menu path is moving the parent (main) window to the computed
   position rather than creating + positioning a NEW child popup window there. NEXT-SESSION
   PRIORITY. Trace `menubar_item_click` → `open_menu_for_hit_node` → `OpenMenu{menu,position}`
   → backend `show_menu_from_callback` → `show_fallback_menu` → `show_menu` (sets
   `RelativeToParentWindow(menu_pos-parent_pos)` + `parent_window_id`) → pending window create →
   child `position_window_on_monitor`. Suspect: either the child window isn't actually created
   (so nothing new appears) and a position update lands on the parent, OR the X11 store-back
   (`self.common.current_window_state.position = Initialized(x,y)` for the relative arm) is
   running against the parent. Confirm `self` is the CHILD in `position_window_on_monitor`, and
   that `OpenMenu` routes to `create_window`/`pending_window_creates` (a real child) not a
   `set_window_position` on the current window.

### STILL TODO (this feature)
- Expose `Menubar` + `Titlebar` as widgets in api.json (repr(C) config). Titlebar already
  in api.json (module `widgets`); add a `Menubar { menu: Menu }` repr(C) struct + `create`/`dom`.
- Refactor the software **titlebar** injection to the SAME Dom-level pattern (currently
  StyledDom-level `csd::wrap_user_dom_with_decorations` + `inject_software_titlebar`).
- DropDown/`<select>` (step 3); migrate azul-paint header buttons → menu callbacks (step 5).

---

## CURRENT STATE (earlier) — 2026-06-06: CONTEXT MENU FUNCTIONAL ✓ (X11)

MILESTONE: the software context-menu fallback is FUNCTIONAL END-TO-END on X11 — right-click
the azul-paint canvas/body → a BORDERLESS popup opens at the cursor with visible items,
hover highlights an item (blue `:hover`), and clicking an item runs its callback + CLOSES
the menu (verified: Menu window count 1→0). Screenshots: /tmp/menu4_crop.png,
/tmp/clickfix_header.png.

Fix chain (all committed):
- `8f1914748` cascade: component-CSS descendant selectors (`.menu-item`) match the subtree.
- `13ac1413d` menu data via `layout_callback.ctx` (`info.get_ctx()`) + context-menu trigger
  (deepest hit node `.keys().next_back()` + ancestor walk + 0-based node-id decode).
- `9884f8e71` wrap menu item text in block divs (a bare text node isn't a flex item → used=None).
- `3e4683a38` borderless override-redirect popups via `x11_override_redirect` + WM_CLASS opts.
- `ab3297662` remove `overflow-y:auto` from `.menu-container` (it collapsed to 8px padding,
  clipping all items → blank). Container now fits content (160x80).
- `aa918c44a` honor `flags.close_requested` in the Linux run loop → menu items close the menu.

NEXT (priority):
1. CROSS-WINDOW REFRESH (IN PROGRESS): a menu item's callback mutates the SHARED app data,
   the menu closes, but the MAIN window doesn't re-layout (its "Effect:" label stays stale).
   PARTIAL FIX committed: `menu_item_click_callback` now escalates the action's RefreshDom →
   `RefreshDomAllWindows` (a menu action affects the parent app, and the menu window is
   closing). VERIFIED via eprintln probes that this ALONE is NOT enough: `on_set_brush` fires
   (data IS mutated) + the menu closes, but the main window's `layout()` NEVER re-runs — even
   after nudging the mouse over the main window. So `frame_needs_regeneration` is not being
   effectively set/acted-upon on the MAIN window from the menu's `ShouldRegenerateDomAllWindows`.
   NEXT: DLL-probe `x11/mod.rs:1736` (the `ShouldRegenerateDomAllWindows` arm) — is it reached
   from the menu window's poll_event? does the registry loop mark the main window
   (frame_needs_regeneration=true + request_redraw)? Suspect either (a) the menu window's
   close_requested short-circuits result handling before 1736, or (b) the active multi-window
   run loop (run.rs ~1120) has no regenerate step (the `frame_needs_regeneration → regenerate`
   at run.rs:926 may be a DIFFERENT, inactive loop path) so marking never triggers a relayout.
   Confirm where the 3 startup `layout()` calls come from vs. why none fire post-startup.
2. ARCHITECTURE (user directive): make `show_menu` = `create_window(options)` on ALL OSes (drop
   the per-platform `show_menu_from_callback` glue; route `OpenMenu` → `CreateNewWindow`). Add
   `WindowPosition::RelativeToParent { parent_rect: LogicalRect, anchor }` resolved per backend
   (X11/Win/macOS = absolute `parent_origin + anchor` + work-area clamp; Wayland = `xdg_popup`
   positioner anchored to the parent surface rect). Generalizes "align to the div edge/size".
   Note: `create_window` → `CallbackChange::CreateNewWindow` (common/event.rs:1233) → each
   backend's `pending_window_creates` IS the cross-platform multi-window path already.
2. Verify SUBMENU (hover an item with children → RightOfHitRect) + DROPDOWN render+position.
3. Step 2: software menubar injection (widget `layout/src/widgets/menubar.rs` + inject in
   `dll/src/desktop/shell2/common/layout.rs`, gated on root menu_bar && !native_global_menu_available()).
4. Step 3: DropDown/<select> open-on-CLICK + label update + onchange + expose in api.json.
5. azul-paint: HEADER lacks `display:flex` (buttons stack); canvas not hittable (only body is
   hit at 450,400) — fix when migrating header buttons → menu callbacks.
6. LAYOUT-ENGINE bug (deeper): `overflow-y:auto` on an auto-height container collapses to
   padding instead of fitting content up to a max — breaks scroll areas + over-tall-menu scroll.

Tooling: `scripts/verify-menu-x11.sh`; `xdotool search --name "^Menu$"`; `AZ_BACKEND=cpu` for
real windows; `pkill -x azul-paint`; logging needs AZ_DEBUG (NOT AZ_LOG) — use eprintln, strip
before commit. System colors are fine (text 238,238,236 on bg 48,48,48 = correct light-on-dark).
