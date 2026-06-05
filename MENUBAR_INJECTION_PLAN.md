# Software menu-bar injection on X11 (test window positioning)

Goal: when no native global menu exists, inject a software menu bar into the window;
clicking a top-level item opens its children as a dropdown **positioned below the item**,
exercising the menu/window-positioning code. Then add a `<select>`/`<option>` using the
same open-menu path.

## Status
- [x] **Step 1 — detection** (commit `865f8e193`): `dbus::native_global_menu_available()`
  checks `com.canonical.AppMenu.Registrar` on the session bus (KDE Global Menu/Unity/
  appmenu-gtk). Absent on this X11/XFCE box → inject. Compiles.
- [ ] **Step 2 — inject software menubar.**
- [ ] **Step 3 — `<select>`/`<option>` dropdown (no submenus yet).**

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
