# Next-session prompt — software menu-bar injection (steps 2 & 3)

> Paste everything below the line into the next session. It is self-contained.
> Work autonomously through the night; commit each verified step.

---

You are continuing work on the **azul** GUI library (`/home/fs/Development/azul`, branch
`mobile-ios-android`). Your job this session: implement **software menu-bar injection,
steps 2 and 3**, per the committed plan **`MENUBAR_INJECTION_PLAN.md`** (read it first,
in full). Work autonomously and incrementally; verify each step before moving on; commit
each verified step.

## Read these first (in order)
1. `MENUBAR_INJECTION_PLAN.md` — the actual plan; steps 2 & 3 are spelled out mechanically.
2. Memory: `~/.claude/projects/-home-fs-Development-azul/memory/` — especially
   `azul-codegen-pipeline.md` (build), `azul-css-cascade-model.md` (#47 / inline CSS is now
   node-scoped), `azul-this-machine.md` and `azul-runtime-debug-knobs.md` (this box: X11/XFCE,
   nouveau; `AZ_BACKEND`, `AZ_HEADLESS_SNAPSHOT_PATH`).

## State as of this handoff (all committed, branch `mobile-ios-android`)
- **#47 node-scoped inline CSS** — DONE (`b663e141e`). `with_css("…")` now applies node-only,
  so the menubar widget's inline CSS is safe (won't leak tree-wide).
- **Menubar step 1 — DBus detection** — DONE (`865f8e193`). `dbus::native_global_menu_available()`
  (dll `…/linux/dbus/mod.rs`) returns true iff `com.canonical.AppMenu.Registrar` owns a name on
  the session bus. On this XFCE box it returns **false → inject** (the path you're building).
- **Cross-platform GPU→CPU fallback** — DONE (`e71e3fde6`), `libazul.so` rebuilt. Every desktop
  backend now falls back to a CPU window on GPU-init failure instead of "no window".
- **Plan doc** — committed (`30fa5124f`).
- Verified today: contenteditable opens a window on both default-GPU and forced-`AZ_BACKEND=cpu`;
  CPU path is clean open→type→close (exit 0). **GPU teardown segfaults (task #6)** — so for any
  real-window run, **use `AZ_BACKEND=cpu`** (GPU is buggy on close on this box).

## The work

### Step 2 — inject a software menu bar
Mechanical recipe is in `MENUBAR_INJECTION_PLAN.md §"Step 2 implementation"`. Anchors:
- **New widget** `layout/src/widgets/menubar.rs` (register in `layout/src/widgets/mod.rs`):
  `build_menubar_dom(menu: &Menu) -> Dom` = flex-row container (class `__azul-native-menubar`,
  inline `with_css` is fine now) with one item per top-level `MenuItem::String`. Each item =
  a div + label + a MouseUp `CoreCallbackData` whose `refany = RefAny::new(submenu_Menu)`.
  Callback `menubar_item_click(data, info)` downcasts the `Menu` and calls
  `info.open_menu_for_hit_node(menu.clone())`.
- **Callback-wiring template**: `layout/src/widgets/titlebar.rs:376`
  (`.with_callbacks(vec![CoreCallbackData{ event: EventFilter::Hover(HoverEventFilter::MouseUp),
  callback: CoreCallback{ cb: FN as usize, ctx: OptionRefAny::None }, refany: RefAny::new(DATA) }])`).
- **Positioning** (the window-positioning path under test):
  `CallbackInfo::open_menu_for_hit_node(menu) -> bool` (`layout/src/callbacks.rs:1781`) positions
  the popup at the hit node's **bottom-left** (`open_menu_for_node:1757`) = a dropdown under the bar.
- **Inject** in `dll/src/desktop/shell2/common/layout.rs`: add `inject_software_menubar(user_dom,
  &menu) -> StyledDom` mirroring `inject_software_titlebar` (`:1175`, builds container via
  `Dom::create_html()` + `append_child`). Call it in `regenerate_layout` (`:149`) **BEFORE** the
  CSD titlebar block (`:286`) so order is titlebar / menubar / user content.
  Gate: inject only when the root carries a menu AND `!native_global_menu_available()` (cache the
  DBus result per-process — don't round-trip every frame). Read the menu off the root:
  `styled_dom.node_data.as_container()[NodeId::ZERO].get_menu_bar().cloned()` → `Option<Box<Menu>>`.
- **Menu API** (`core/src/menu.rs`): `Menu{ items: MenuItemVec }`, `Menu::create(MenuItemVec)`;
  `MenuItem::String(StringMenuItem{ label, callback, children, .. })`.

### Step 3 — `<select>`/`<option>` dropdown (no submenus)
Per `MENUBAR_INJECTION_PLAN.md §"Step 3"`: a `<select>` rendered as a closed box (current value);
on click, build a `Menu` from the `<option>`s and `open_menu_for_hit_node` (same path, positioned
below, no submenus). The select holds a backreference (`RefAny` + `Callback`) to the user's
`onchange`; picking an option closes the popup and invokes onchange with the chosen value.

## Build & verify (autonomous — no human to click overnight)
- **Test app** is `examples/azul-paint` — it declares the menu at `examples/azul-paint/src/lib.rs:711-733`
  (File/Edit/View, each with children). It links the dll via **`link-static`**, so:
  - **Build:** `cargo build -r -p azul-paint` → `target/release/azul-paint`.
    This compiles your `dll/` + `layout/` changes straight in — **no `libazul.so` rebuild needed**.
- **Visual-verify the BAR renders (primary autonomous check):**
  `AZ_BACKEND=headless AZ_HEADLESS_SNAPSHOT_PATH=/tmp/mb.png target/release/azul-paint`
  then **Read `/tmp/mb.png`** — the File/Edit/View row must appear at the top of the window.
  (Headless renders the INITIAL layout via cpurender; this is the way to see pixels with no display.)
- **Verify the DROPDOWN opens on click:** synthetic `AZ_DEBUG` mouse events may NOT drive hit-test
  on X11 (known caveat — see `azul-linux-release-debugging.md`). Prefer: (a) a focused unit/integration
  test that calls the click callback and asserts `open_menu_for_hit_node` returns true / a popup
  WindowCreateOptions is produced; and/or (b) run on the real display for the morning:
  `AZ_BACKEND=cpu DISPLAY=:0.0 target/release/azul-paint` with logging, leaving a note for the user
  to click File/Edit/View. **Always `AZ_BACKEND=cpu` for real-window runs** (GPU teardown crash).
- **Prefer unit tests over full-app rebuilds** where the logic is testable in isolation
  (e.g. `build_menubar_dom` structure, submenu construction, onchange wiring).

## Rules / preferences
- **Root causes, not symptoms.** Refactor freely; you own azul-css / azul-core / azul-dll / layout.
- **You may break `api.json`.** For an *internal-only* change, just `cargo build`. If you add/modify
  a **public `#[repr(C)]` type** (e.g. a select/option type that crosses FFI), regenerate bindings via
  the pipeline in `azul-codegen-pipeline.md`: `azul-doc autofix` → `autofix apply <patch>` →
  `azul-doc normalize` → `codegen all` → `cargo build -r -p azul-dll --features build-dll`.
  Do **not** hand-edit `api.json`.
- **Commit each verified step** semantically (conventional-commit style) on `mobile-ios-android`.
  Do **not** push. End commit messages with:
  `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`
- Keep the CSD titlebar and contenteditable working — don't regress them.
- If `native_global_menu_available()` is ever true (KDE global menu / Unity / appmenu-gtk),
  do **not** inject — exporting the menu over DBus is a separate, later TODO; just leave the
  user DOM untouched on that path.

## Done =
- **Step 2:** clicking File/Edit/View in `azul-paint` (real window, `AZ_BACKEND=cpu`, X11) opens that
  item's children as a dropdown positioned **directly below** the item; the bar renders in the headless
  snapshot. Committed.
- **Step 3:** a `<select>` renders as a closed box; clicking opens a Menu built from its `<option>`s via
  the same open-menu path; choosing one invokes the user's `onchange` backreference and closes the popup;
  no submenus. Committed.
- Update `MENUBAR_INJECTION_PLAN.md` status checkboxes as you complete each step, and leave a short
  end-of-session note (what's done/verified, what's left) for the morning.
