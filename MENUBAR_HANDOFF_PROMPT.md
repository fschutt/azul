# Handoff prompt — azul software menubar / window-parenting (resume 2026-06-09)

> Paste everything below the line into a fresh agent. It is self-contained. The human
> will do runtime testing later — **your job is the theory/code: read, reason, fix, commit.
> Do NOT block on running the GUI.**

---

You are continuing work on **azul** (Rust GUI toolkit) at `/home/fs/Development/azul`,
branch **`mobile-ios-android`** (NOT master; master is 2400+ commits behind and only has 3
CI-only commits we don't need). The user is Felix (felixschuettatoutlook@gmail.com).

## Mission
Finish the **software menu bar** + **window-parenting** feature. The previous session landed
the unified positioning model and a rendering-correct menu bar; what remains is one functional
bug, a couple of cosmetic bugs, two API-exposure tasks, and a refactor. **Do the
theory/implementation now; the human runs the GUI tests afterward — do not block on launching
the app.**

## Standing rules (HARD)
- Commit each verified step on `mobile-ios-android`; **do NOT push**. End commit messages with:
  `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`
- **Never hand-edit `api.json` type internals** (fields/variants/derives) — regenerate via the
  pipeline: `cargo run -p azul-doc -- autofix` → `autofix apply <patch_file>` → `normalize` →
  `codegen all`. Authoring a NEW widget's `constructors`/`functions` (the `fn_body` strings) IS
  done by hand in api.json (autofix can't synth those) — then run `autofix` to validate the
  hand-authored struct_fields match the Rust struct (zero drift). CI requires `autofix` to be
  **zero-diff + exit 0**.
- Always run cargo under `timeout` (600s normal, 1200s heavy), cap `-j 4`, `--test-threads=1`,
  never overlap builds. Never end with a broken build. Tests must be honest.
- Read memory first: `~/.claude/projects/-home-fs-Development-azul/memory/`
  (azul-codegen-pipeline, azul-css-cascade-model, azul-this-machine, azul-runtime-debug-knobs).

## What is COMMITTED (3 commits, newest first)
- `7f95e36f7` feat(menubar): software menu bar widget + unified Dom-level chrome injection
- `8c4dfd1e5` feat(window): unified RelativeToParentWindow positioning across all backends
- `f2fb0b694` build(camera): Windows camera pure-Rust stub behind `camera-native`

## Models you MUST understand before touching anything

**1. Unified window parenting (`WindowPosition`)** — `core/src/window.rs`:
```
enum WindowPosition { Uninitialized, Initialized(PhysicalPositionI32),
                      RelativeToParentWindow(PhysicalPositionI32) }
```
A child window (menu/popup) sets `RelativeToParentWindow(offset)` + `WindowCreateOptions.parent_window_id`
(cross-platform parent registry key: X Window id / NSWindow ptr / HWND as `u64`). Each backend's
`position_window_on_monitor` resolves the parent's absolute origin from its window registry and
places the child at `parent_origin + offset` (monitor-relative fallback if no parent). The old
Windows-only `WindowsWindowOptions.parent_window` HWND field + `HwndHandle`/`OptionHwndHandle`
types were REMOVED. `menu.rs::show_menu` emits `RelativeToParentWindow(menu_pos - parent_window_position)`.

**2. The CSS model (critical — the first menubar attempt failed on this)**
- `rule_priority` (`css/src/css.rs`): UA 0 < SYSTEM 10 < AUTHOR 20 < **INLINE 30** (highest).
- `Dom::with_css(&str)` is the **single unified CSS entry point** (`core/src/dom.rs:5842`):
  `parse_inline` wraps the string in `* { … }`, tags rules INLINE, pushes onto the Dom's `.css`
  vec. Supports `:hover`, `@os(...)`, nesting. `add_component_css(Css)` is the low-level primitive
  it calls (candidate to drop from api.json later, folded into `with_css`).
- `scope_inline_css` (`core/src/styled_dom.rs:~2129`) runs **only inside `create_from_dom`**:
  walks the tree and `CssPath::push_front_scope(start,end)` prepends a `Root(CssScopeRange)` to each
  `.css` rule — **node-only `[start,start]` for bare-`*` rules, subtree `[start,end]` for selector
  rules**. Re-synthesized every flatten ⇒ survives `append_child` **only when flattened together**
  (`create_from_dom`). `StyledDom::create()` does NOT run `scope_inline_css`, so a
  separately-created-then-`append_child`'d StyledDom never gets its `.css` scoped → renders
  UNSTYLED. THIS is why chrome must be injected at the **Dom level** before `create_from_dom`, not
  as a separate StyledDom + append.
- `system:` colors (`css/src/props/basic/color.rs`): `system:text`, `system:window-background`,
  `system:selection-background`, `system:selection-text`, `system:accent`, `system:button-face`,
  … — resolved against `SystemColors` at cascade. Font: `font-family: system:ui`.

**3. Menu bar widget** — `layout/src/widgets/menubar.rs`: `build_menubar_dom(menu: &Menu) -> Dom`
= flex-row `.azul-menubar` of one `.azul-menubar-item` per top-level `MenuItem::String`, styled
100% via `with_css(&str)` (system: colors + `:hover`). Each item's MouseUp callback
`menubar_item_click(data, info)` downcasts `data` (a `Menu` backreference) and calls
`info.open_menu_for_hit_node(menu.clone())`. A top-level leaf opens a 1-item menu of itself so its
callback fires. Injected at the Dom level by `inject_software_menubar(user_dom)` in
`dll/src/desktop/shell2/common/layout.rs::regenerate_layout` (Linux-only, gated on
`!gnome_menu::should_use_gnome_menus()` + root `get_menu_bar()`). The old broken csd.rs menu bar
was removed → one path.

**Backreference pattern** (`doc/guide/en/architecture.md`): menu items carry the user's
`(RefAny, Callback)` via `StringMenuItem::with_callback(data, cb)` → `CoreMenuCallback{refany,
callback}`; the popup fires these on click. Keep it — it's how user code receives menu events.

## STATUS: menu bar RENDERS horizontally + themed on X11 (screenshot-verified). Bugs remain.

## TASKS (priority order — theory/code; defer GUI testing)

### BUG 1 (top priority): clicking a menubar item repositions the PARENT window instead of
opening the child popup. User: "it should position the child window, not the parent window."
(Observed: main window jumped 50,50 → 54,122 on click; no dropdown.) The flow IS meant to create
a NEW child window — trace it and find where the parent moves / the child fails:
- `menubar_item_click` (`layout/src/widgets/menubar.rs`) → `CallbackInfo::open_menu_for_hit_node`
  → `open_menu_for_node(menu, hit_node)` (`layout/src/callbacks.rs:~1755`): gets hit node rect,
  sets `position = node bottom-left` (window-LOCAL), pushes `CallbackChange::OpenMenu{menu, position}`.
- `OpenMenu` handler (`dll/.../common/event.rs:1575`) → `show_menu_from_callback(menu, pos)` → X11
  `show_menu_from_callback` (`x11/mod.rs:~3281`) → `show_fallback_menu` (`x11/mod.rs:~3330`):
  builds `menu_options` via `crate::desktop::menu::show_menu(...)` (sets
  `RelativeToParentWindow(menu_pos-parent_pos)`), sets `menu_options.parent_window_id = self.window`,
  then `self.pending_window_creates.push(menu_options)` (`x11/mod.rs:3413`) — a NEW child window is
  QUEUED (confirmed; it is not a move of the current window at this layer).
- The queue drains in the run loop → `X11Window::new(menu_options)` →
  `position_window_on_monitor(monitor, position, size, options.parent_window_id)`
  (`x11/mod.rs:~1592`): `RelativeToParentWindow` arm resolves
  `Self::resolve_parent_origin(parent_window_id)` (registry → parent's
  `current_window_state.position`), `XMoveWindow(self.display, self.window, x, y)`, then stores
  back `self.common.current_window_state.position = Initialized(x,y)` (only for the relative arm).
- **Reason through + targeted eprintln** (strip before commit): (a) is the queue actually drained
  and `X11Window::new` called for the menu? (b) in `position_window_on_monitor`, is `self` the CHILD
  (it should be — called from the child's `new()`); is `self.window` the child's X id, not the
  parent's? (c) the store-back must write the CHILD's state. (d) the X11 menu window is
  override-redirect + `size_to_content` → mapped late in `apply_size_to_content()`; confirm it maps
  AND positions (a popup that never maps + a parent that the WM nudged could masquerade as "parent
  moved"). (e) `parent_window_position` (in `show_fallback_menu`, read from
  `self.common.current_window_state.position`) and `resolve_parent_origin` read the SAME field → the
  final should equal `menu_pos`; verify the main window's stored position is `Initialized(true
  screen pos)` and not stale/(0,0).

### BUG 2 (cosmetic): menubar colors render light-on-dark-app ("probably a system style problem"
— user). `system:*` likely falling back to light defaults because `SystemColors` is
empty/unpopulated on X11. Find where `SystemColors` is built for the Linux/X11 backend
(`css/src/system.rs` + the dll's system-style construction) and confirm theme detection fills it.

### BUG 3 (cosmetic): the 3rd menubar item "View" renders as "V" (File/Edit/Help — also 4 chars —
render full); item box looks correctly sized (large gap before "Help") → text layout/measure quirk
on that node, not flex sizing. Investigate `layout/src/text3/` measurement for the item text node.
All items are built identically in `build_menubar_item` — so reason about why only the 3rd clips.

### BUG 4: contenteditable text blank on the FIRST draw, appears on the second draw
(user-confirmed; caret + typing DO work). First-frame render/relayout ordering bug. App:
`tests/e2e/contenteditable.c`. Investigate the first-frame path in `regenerate_layout` +
`generate_frame` (initial layout vs first display-list build).

### FEATURE A: expose `Menubar` + `Titlebar` as api.json widgets (repr(C) config). User asked.
- `Titlebar` is ALREADY in api.json (module `widgets`: `constructors.new` + `functions.dom`/
  `dom_with_buttons`). Confirm complete.
- Add a `Menubar` repr(C) widget struct to `layout/src/widgets/menubar.rs`:
  `#[repr(C)] pub struct Menubar { pub menu: Menu }` + `create(menu)->Self` + `dom(self)->Dom`
  (calls `build_menubar_dom`). Mirror the `Button`/`Titlebar` pattern
  (`layout/src/widgets/button.rs` + its api.json entry). Add the `Menubar` entry to `api.json`
  (module `widgets`) by hand-authoring `external`/`derive`/`struct_fields`/`constructors.create`/
  `functions.dom` mirroring `Titlebar`; run `autofix` (zero drift) → `normalize` → `codegen all`.

### FEATURE B: refactor the software **titlebar** injection to the SAME Dom-level pattern as the
menu bar (user: "the software titlebar should also be based on Dom, not StyledDom"). Today
`csd::wrap_user_dom_with_decorations` + `inject_software_titlebar` build a separate StyledDom and
`append_child`. Unify: at the Dom level (before `create_from_dom`) prepend the titlebar Dom
(`Titlebar::dom()/dom_with_buttons()`) with its CSS via `with_css`/`add_component_css(create_csd_stylesheet())`
so it's scoped in the main flatten; share one `inject_chrome(user_dom, window_state, system_style)`
helper for titlebar + menu bar. Titlebar currently WORKS (node-intrinsic `with_css_props`) — verify
no build regression; human runtime-tests.

### FEATURE C (later): DropDown/`<select>` (task #4) open-on-click + label + onchange, expose in
api.json. Migrate azul-paint header buttons → menu callbacks (task #5).

### NEXT MAJOR: Wayland. `RelativeToParentWindow` is the enabler (no absolute coords on Wayland);
the Wayland backend's `xdg_popup` positioner is still a gap (task #6). Build-verify all the above
for Wayland (blind) too.

## Environment gotchas (cost real time — heed them)
1. **STALE `/lib/libazul.so`** (root-owned, ~41MB, OLD ABI) shadows the fresh
   `target/release/libazul.so` (~276MB). C examples use a *relative* rpath, so from the wrong CWD
   they load the stale lib → immediate crash (exit 144). ALWAYS run examples with
   `LD_LIBRARY_PATH=/home/fs/Development/azul/target/release`. (Consider deleting the stale copy.)
2. **`libazul.so` is 276MB** (release, unstripped debug info). Investigate `debug=0`/stripping in
   the release profile — it bloats links + slows rebuilds.
3. **Harness exit 144** on long foreground commands / `sleep` is an artifact (the screenshot script
   exits 144 yet works). Use `run_in_background` for builds; have the human launch persistent GUI
   apps via the `!` prefix. Process name truncates to 15 chars → `pgrep -x contenteditable_test`
   fails; use `pgrep -f`.

## Rebuild chain (each ~3–4 min; never overlap)
`cargo run -p azul-doc -- codegen all` (only if api.json changed) → `timeout 1200 cargo build
--release -p azul-dll --features build-dll -j 4` (→ libazul.so) → `timeout 1200 cargo build
--release -p azul-paint -j 4` (rebuild examples — the `WindowCreateOptions` ABI changed). Fast
inner loop: `timeout 600 cargo check -p azul-dll -j 4`. Headless tests: `timeout 1200 cargo test
-p azul-dll --lib headless::tests -j 4 -- --test-threads=1` (23 tests). macOS/Windows cross-check:
`cargo check -p azul-dll --target {aarch64-apple-darwin,x86_64-pc-windows-gnu} -j 4`.

## Source of truth for state
`MENUBAR_INJECTION_PLAN.md` (top "⏸ MONDAY HANDOFF" block + dated "CURRENT STATE" sections).
Update it after each verified increment.
