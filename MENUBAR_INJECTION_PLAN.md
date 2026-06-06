# Software menu-bar injection on X11 (test window positioning)

Goal: when no native global menu exists, inject a software menu bar into the window;
clicking a top-level item opens its children as a dropdown **positioned below the item**,
exercising the menu/window-positioning code. Then add a `<select>`/`<option>` using the
same open-menu path.

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

## CURRENT STATE (for next firing) — updated 2026-06-06, after the kickoff session

DONE + committed this session:
- `8f1914748` — #47 cascade follow-up: component-CSS descendant selectors (`.menu-item`)
  now match the owner's subtree (was why menu items were unstyled). Unit-tested; full
  azul-css + azul-core suites green.
- `13ac1413d` — menu popups build + context menus trigger on X11:
  (a) `MenuWindowData` is now carried in `layout_callback.ctx` and read via `info.get_ctx()`
  in `menu_layout_callback` — it was never attached, so menus rendered a 1-node empty body;
  (b) `get_first_hovered_node` returns the DEEPEST hit node (`.keys().next_back()`) and
  `try_show_context_menu` walks the ancestor chain + decodes the node id 0-based.
  Verified on X11: the context-menu window now builds 7 nodes and size_to_content-sizes to
  160x72 at the cursor (was 1 node / 0x16 / unmapped). azul-paint has a Metaballs/Normal-paint
  context menu (on canvas AND body — canvas isn't hittable yet, see NEXT #3);
  `scripts/verify-menu-x11.sh` is the X11 click+screenshot harness (WIN_NAME="Azul Window").

NEXT (priority order):
1. **Menu content paints BLANK** (dark grey, no item text) although the DOM is built + sized.
   The container bg IS painted (clean uniform grey → render works), so suspect DARK-ON-DARK
   system colors on XFCE. Check `cargo run -r -p azul-dll --example system_style --features
   link-static` → compare colors.window_background vs colors.text. If both dark (or text is
   black on a dark bg), fix XFCE detection (`css/src/system.rs`) OR make
   `create_menu_stylesheet` (`dll/src/desktop/menu_renderer.rs`) choose a text color with
   guaranteed contrast vs the menu bg. ALSO the label is a bare text node with `used=None` in
   the tiny measure pass — confirm it lays out at final size; if not, wrap it in a div (like
   `drop_down.rs` does with `create_p()`). Verify with the harness + Read the PNG: the 2 items
   must be visible.
2. **Menu windows get WM decorations** (a titlebar) — must be override-redirect / borderless.
   show_menu already sets decorations:None + is_always_on_top; the X11 backend
   (`shell2/linux/x11/mod.rs` window creation) must set `override_redirect` (or _MOTIF_WM_HINTS)
   for `WindowType::Menu`.
3. **azul-paint layout**: HEADER css lacks `display:flex` (buttons stack vertically); and the
   canvas isn't hit-testable at (450,400) — only the body/root is hit (total_rects=9), so the
   canvas context menu can't fire directly (the body one does). Likely height:100% + flex-grow
   not resolving. Fix when migrating buttons → menu callbacks.
4. Then Step 2 (software menubar injection) and Step 3 (`<select>` widget) per the checklist.

Tooling: `xdotool search --name "^Menu$"` (plain "Menu" also matches the terminal title which
contains "menubar"). AZ_BACKEND=cpu for real windows (GPU teardown segfaults). Logging is gated
behind the debug server (AZ_DEBUG), NOT AZ_LOG — use ad-hoc `eprintln!` probes for debugging and
strip them before committing.
