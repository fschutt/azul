# Tooltip is not a macOS tooltip; colour picker needs a real transient popover window

Written 2026-08-22 on worktree `debug-slider-scroll-2026-08-22` (branch
`fix/open-bugs-wave-2026-08-22`). Read-only investigation: no source was
edited, nothing was built. Line numbers are from this worktree unless a path
is prefixed with `master:` (the shared checkout's `master`, read via
`git show master:<path>`).

## Symptom (user, testing the downloaded AzWidgets demo on macOS)

> "tooltip isn't done properly like a real macOS tooltip — also we should have
> a proper color picker but with a custom window popover — so this tests if we
> can make a 'custom transient popover window (real OS window)'."

## Status

**Confirmed, two separate defects, both build on work already in flight on
master.**

1. The tooltip the demo shows is NOT the NSPanel tooltip at all — it is the
   `Tooltip` *widget*, an in-DOM `<p>` revealed by flipping `opacity` on
   `MouseEnter` with no delay, a hard-coded dark style, a fixed 22 px offset
   and no way out of the window. The NSPanel tooltip that does exist
   (`macos/tooltip.rs`) is only reachable through `title` / `alt` /
   `aria-label` attributes, which the demo never sets — and even that panel is
   a Windows-95-style yellow box with a 7 pt/char width heuristic.
2. A colour picker popover has NO surface to live in yet on macOS: master's
   `<transient-window>` engine (node, layout, reconcile, mailbox, dismiss,
   6/6 headless e2e) is done, but on macOS its popup is created as a plain
   borderless `NSWindow` through the generic window path — not an `NSPanel`,
   not a child window, not non-activating, and its `RelativeToParentWindow`
   placement is (very likely) Y-flipped. `ColorInput` itself has no picker:
   it "only reports the current colour so the caller can open their own".

The transient-window engine is being built by ANOTHER session on master
(commits `2b5dcf2d1`, `cba7a2e4d`, `c8d31ab97`, `a7482b8b3`, 2026-08-22).
This report does not propose re-implementing any of it — only what has to be
built on top: the macOS surface, the tooltip as a client of it, and the
colour picker as the test case.

---

## 1. How tooltips work today — two systems that do not know each other

| | `Tooltip` widget (what the demo shows) | attribute tooltip (`title`/`alt`/`aria-label`) |
|---|---|---|
| where | `layout/src/widgets/tooltip.rs` | `layout/src/window.rs:312-343` (timer cb), `:6416-6474`; `dll/src/desktop/shell2/common/event.rs:7131-7168`; `dll/src/desktop/shell2/macos/tooltip.rs` |
| surface | in-DOM `<p class="__azul-native-tooltip-tip">`, `position:absolute` inside a `position:relative` wrapper (`tooltip.rs:75-80, 82-118`) | real `NSPanel` (`macos/tooltip.rs:59-74`) |
| trigger | `MouseEnter` on the wrapper → `opacity: 100` immediately (`tooltip.rs:231-237`); `MouseLeave` → `opacity: 0` | hover onto a node with `get_accessible_label()` → one-shot `TOOLTIP_DELAY_TIMER_ID` after `hover_time_ms`, callback emits `CallbackChange::ShowTooltip` at the cursor (`window.rs:312-343`, `callbacks.rs:2243-2249`) |
| delay | **0 ms** | `InputMetrics::hover_time_ms` = **400 ms** (`css/src/system.rs:806`, the Windows `SPI_GETMOUSEHOVERTIME` default); macOS `system_style.rs` never overrides it (only `double_click_time_ms`, `:317`) |
| placement | `top: 22px; left: 0` of the wrapper (`tooltip.rs:51, 84-85`) — a `Button` is ~28-30 px tall (padding 6+6 + 14 px text, `button.rs:196-197,310`), so the tip overlaps the button's bottom | panel TOP-LEFT placed exactly AT the cursor hotspot (`macos/tooltip.rs:162-168`, `mod.rs:6019-6046`) — the pointer covers the first glyphs |
| escapes the window? | no — clipped by the window edge and any `overflow:hidden`/scroll ancestor (the demo's column is `overflow-y: auto`, `examples/azul-widgets/src/lib.rs:444`) | yes (own panel, `NSPopUpMenuWindowLevel`) |
| look | `#333` @ 240 alpha, white 12 px text, 4 px radius, nowrap (`tooltip.rs:54-69, 82-118`) | rgba(1, 1, 0.85, 0.95) **yellow**, hard-coded `blackColor` text, default 13 pt `NSTextField` font, no radius, no border (`macos/tooltip.rs:76-97`) |
| sizing | layout | `chars().count() * 7.0 + 10`, clamp 50..400, fixed **25 pt height**, single line (`macos/tooltip.rs:22-37, 129-146`) — long, CJK or emoji text is clipped |
| dismissal | leave wrapper | only `TooltipTimerAction::Stop` (hover moved to a node without a label, `window.rs:6447-6474`; `event.rs:7154-7161`). Not on click, key, scroll, drag, window deactivation — and `setHidesOnDeactivate(false)` is set explicitly (`macos/tooltip.rs:71`) |
| HiDPI | logical px (fine) | AppKit points (fine); `_dpi_factor` ignored (`macos/tooltip.rs:123`) |
| e2e | unit tests on the DOM shape only | **none**: the headless runner reports `ShowTooltip`/`HideTooltip` as `unsupported("tooltips are a second platform window")` (`layout/src/e2e/runner.rs:2546-2551`) |

The widget's own module doc admits this (`tooltip.rs:16-21`, `TODO2`): "a CSS
simplification of a 'real' floating popover … does not measure the anchor's
height, flip when near a screen edge, or escape an `overflow: hidden`
ancestor. A future revision could route through the window-popup / menu popup
path." `Popover` carries the same `TODO2` (`popover.rs:16-25`) plus "no
click-outside to dismiss and no Escape handling".

The demo instantiates exactly that widget:
`Tooltip::new(Button::create("Hover me").dom(), "I am a tooltip!").dom()`
(`examples/azul-widgets/src/lib.rs:295-296`). Nothing in the demo carries a
`title`/`alt`/`aria-label` attribute (`labelled()` uses
`with_accessibility_name`, which sets `AccessibilityInfo` only,
`core/src/dom.rs:6391-6396` — not an attribute), so the NSPanel path is never
exercised by the demo. What the user saw is the in-DOM widget.

### 1a. Deviations from a real macOS tooltip (`NSView.toolTip` / NSToolTipManager)

Reference behaviour (AppKit-internal numbers are approximate; verify against
Finder before hard-coding):

| macOS | in-DOM widget | NSPanel path |
|---|---|---|
| appears after `NSInitialToolTipDelay` (~1 s default, user-settable) and only while the app is active | instantly | 400 ms (Windows default), shown even if the window is not key |
| small system font (`NSFont.toolTipsFont(ofSize: 0)`, 11 pt), `labelColor` | 12 px, white | 13 pt default field font, black |
| system appearance: light/dark material (`NSVisualEffectView` `.toolTip`), hairline border, ~4-5 pt radius, soft shadow | dark always | yellow always, square |
| positioned just below the pointer (offset ≈ cursor height) so the arrow does not cover it; clamped to `screen.visibleFrame` | 22 px below the wrapper top, overlapping the anchor | top-left AT the hotspot; no clamp — off-screen near the right/bottom edge |
| wraps long text to multiple lines | `white-space: nowrap` | single line, 25 pt, clipped |
| hides on pointer leaving the rect, mouse-down, key-down, scroll, app deactivation | leave only | hover-change only; survives deactivation |
| its own window: escapes the app window, sits above it | clipped | yes |
| text is the `toolTip`, NOT the accessibility label | n/a | `aria-label` wins over `title` (`core/src/dom.rs:3421-3432`): any `create_input`/`create_textarea`/`create_select` node (`dom.rs:2425-2484`) shows its a11y NAME as a tooltip |

### 1b. The other backends, for scale

All four tooltip backends are hand-rolled look-alikes: X11 draws a yellow
override-redirect window with the same 7 px/char heuristic
(`linux/x11/tooltip.rs:18-25`), Wayland rasterises into a `wl_subsurface`
(`linux/wayland/tooltip.rs:1-13`), Windows is the only native one
(`TOOLTIPS_CLASS`, `windows/tooltip.rs:1-11`). None is e2e-testable. The
transient-window path replaces all four text renderers with ONE DOM-rendered
tip that the headless harness can lay out and assert on.

---

## 2. The transient-window work: what master already provides

Plan: `scripts/TRANSIENT_WINDOW_PLAN_2026_08_22.md` (this worktree has the
first version; master's `a7482b8b3` adds §3b' "DECISION: the surface is a
full window"). The stop-points in plan §8 map onto the commits:

| plan step | commit | what exists |
|---|---|---|
| 2. `NodeType::TransientWindow` + layout skip + UA CSS + XML | `cba7a2e4d` | `master:core/src/transient.rs:33-249` — `TransientAnchor {Bottom,Top,Left,Right,Cursor}`, `TransientDismiss {Outside,Escape,None}`, `TransientWindowConfig {open, anchor, dismiss, size: OptionLogicalSize, tearoff}` inline in `NodeType` (48 B guard). Placement is an EDGE, never coordinates (Wayland). `<transient-window>` parses in `master:core/src/xml.rs:2623-2644`; a bare tag is CLOSED. UA CSS `display:block; position:absolute; top:100%` (`master:core/src/ua_css.rs:108-122, 593-599`). `layout_tree::get_display_type` returns `display:none` for the node in its PARENT (`master:layout/src/solver3/layout_tree.rs:3888-3903`) — open or closed it never takes part in the parent's flow. |
| 2/3. own-dom layout + reconcile survival | `c8d31ab97` | `master:layout/src/transient.rs` — `TransientPlacement::resolve` (`:64-74`, one place for coordinate arithmetic), `collect_open_transient_windows` (`:89-112`, anchor = the node's PARENT rect), `transient_dom_id` from `0x1000_0000` (`:136-138`), `TransientWindowManager` (`:269-286`) matched by `source_node` across rebuilds, `TransientDiff {opened, moved, closed}` with `merge` cancelling open+close pairs (`:294-339`), `reconcile` (`:415-470`) with edge-triggered `dismissed` nodes. `LayoutWindow::layout_transient_content` (`master:layout/src/window.rs:3459+`) lays the extracted subtree out under its own `DomId` and measures the children's far edge; `extract_subtree_as_dom` (`master:core/src/transient.rs:330-367`) bakes the parent cascade's resolved style inline and rewrites the root to `Div`. Bug fixed on the way: `get_node_layout_rect` ignored `DomNodeId::dom`. Wired in `master:dll/src/desktop/shell2/common/layout.rs:1309-1318, 1749-1818`. |
| 3b/4. surface + dismiss | `a7482b8b3` | `master:dll/src/desktop/shell2/common/transient.rs` — popup = a `WindowType::Menu`, `decorations: None`, `is_always_on_top`, non-resizable child created through the existing `queue_window_create` path, sized to the engine's measurement (`size_to_content: false`), positioned `RelativeToParentWindow(placement.resolve(..))` (`popup_create_options`, `:147-210`). The popup's layout callback returns the extracted subtree, so callbacks are the same fn pointers on the same `RefAny`s (`:98-111`). Parent↔popup channel = the popup's layout-callback ctx `TransientWindowData {content, placement, content_size, generation, closed, dismissed}` (`:65-83`). `sync_parent` (`:239-341`), `poll_popup` (`:354-363`), `popup_dismiss_cause` (Escape under outside\|escape, focus loss under outside, `:379-405`), `dismiss_outside_on_press` (a fresh press in the PARENT closes `outside` popups, `:416-444`). Trait hooks with defaults in `master:dll/src/desktop/shell2/common/event.rs:3198-3280` (`registry_window_id`, `request_regeneration_all_windows`, `sync_transient_windows`, `poll_transient_mailbox`, `process_transient_dismissal`), called at `:2909, :2916, :2929, :7086`. `ComponentEventFilter::Dismissed` / `EventType::Dismiss` (`master:core/src/events.rs:2347, 2672`). Per-backend `request_regeneration_all_windows` for macOS/Windows/X11/Wayland (macOS `master:.../macos/mod.rs:3411`). |
| e2e | `c8d31ab97`, `a7482b8b3` | `master:dll/tests/transient_window_layout.rs` (432 lines, 6 tests at `:176, :190, :250, :316, :356, :389`): closed opens nothing; open → child window queued at the measured 240x160 and the popup lays the same content out at that size; open→keep→close across rebuilds (same id, empty diff, no leaked result); a press in the parent dismisses once and `Dismissed` fires once; an ignoring app gets no zombie; Escape in the popup reaches the parent. |

Not on master yet (plan §8 steps 5-7 and the per-backend surface work):
colour picker (§7), `tearoff` (§5), `shape` (§6), a `Dom::create_transient_window(cfg)`
builder and api.json exposure (`git grep TransientWindow master -- api.json`
is empty — the Rust e2e builds the node via `NodeData::create_node`), and a
way to flip `open` at RUNTIME from a callback (today `open` lives in the
inline `NodeType` config and only changes through a `layout()` rebuild — the
e2e toggles app state and relayouts). The plan also names Wayland as "the one
real port" (§3b'), deliberately last.

---

## 3. What a REAL OS popover on macOS still needs (the surface half)

On macOS there is no `WindowType`-specific code at all
(`grep WindowType dll/src/desktop/shell2/macos/mod.rs` → nothing). A transient
popup queued by master's `sync_transient_windows` is created by the run loop
exactly like a toplevel (`dll/src/desktop/shell2/run.rs:695-720` and
`:866-905`: `MacOSWindow::new_with_fc_cache` → `register_window`), which means:

| need (plan §3b macOS row, §9 "Focus") | today | where |
|---|---|---|
| `NSPanel` with `.nonactivatingPanel` | a plain `NSWindow` created `Titled\|Closable\|Miniaturizable\|Resizable` (`mod.rs:3974-3991`), then `decorations: None` strips Titled/Closable/Mini but KEEPS `Resizable` (MWA-B11, `:4137-4146`), then `apply_initial_window_state` removes `Resizable` because `is_resizable=false` (`:5018-5021`) | result: a borderless, non-resizable `NSWindow`. AppKit's `canBecomeKeyWindow` is NO for such a window (no title bar, no resize bar) and nothing overrides it (`grep canBecomeKey` → nothing). So `makeKeyAndOrderFront` (`:4663`) only orders it front — good, no focus steal — but the popup can NEVER receive keyboard: master's `popup_dismiss_cause` reads Escape and `window_focused` from the POPUP's own state (`master:common/transient.rs:388-403`); on macOS neither edge ever happens (the parent stays key and takes the Escape). Typing a hex value into a picker field is impossible. A non-activating `NSPanel` that overrides `canBecomeKeyWindow → YES` is what gives keyboard to the popup WITHOUT activating it or resigning the parent's main status. |
| `addChildWindow(_:ordered: .above)` | never called | the popup does not follow the parent when it is dragged, and it is ordered by LEVEL only |
| level | `is_always_on_top` → `NSFloatingWindowLevel` (`:5012-5015`) | floats above OTHER apps' windows too; a plain `NSWindow` has `hidesOnDeactivate = NO`, so after Cmd-Tab the popup stays on top of the other app. (An `NSPanel` defaults to `hidesOnDeactivate = YES`; a child window inherits ordering from its parent and needs no floating level.) |
| placement `RelativeToParentWindow` | `position_window_on_monitor` computes `frame.origin.y = screen.origin.y + (parent_top_left_y + offset.y)` (`mod.rs:7851-7863`) | the parent's stored position is TOP-DOWN (`:4676-4691`: `top_left_y = primary_height - frame.origin.y - height`), but an `NSRect` origin is BOTTOM-UP and the popup's own height is not subtracted. **Very likely a Y-flip: a popup meant for (148, 172) top-down lands 172 pt from the BOTTOM of the screen.** UNVERIFIED — cannot be run here. The `Initialized` arm has the same arithmetic but is corrected afterwards by `apply_initial_window_state`'s `setFrameTopLeftPoint` with the primary-height flip (`:4995-5010`); `RelativeToParentWindow` gets no second pass (`sync_window_state` skips it explicitly, `:5121-5123`) and is also resolved against `monitor_id` index 0 rather than the parent's screen (`:7806-7831`). The window-based fallback menu shares this path but is dead on macOS by default (`use_native_context_menus` → `NSMenu`, `:3416-3440`; fallback at `:6168-6201`), which is why nobody has seen it. |
| dismiss on a click OUTSIDE every app window | `dismiss_outside_on_press` only sees presses in the PARENT (`master:common/transient.rs:416-444`); the popup's focus-loss edge needs the popup to have been key | a click on the desktop or another app neither presses the parent nor un-keys a never-key popup → the popup stays. Needs `NSApplicationDidResignActive` / the parent's `windowDidResignKey` (`mod.rs:2750-2796`) to dismiss `outside` popups. |
| shadow, transparency, rounded corners (plan §6) | opaque rect, default shadow | `isOpaque=false; backgroundColor=.clear; hasShadow=true` on the panel + the content's own radius; the arrow shape is §6 and can wait |
| cost per popup | a full `MacOSWindow` with its own GL context and WebRender renderer per open (plan §3b' accepted this) | fine for a picker; for a TOOLTIP (opened on every hover) the popup should request `HwAcceleration::Disabled` (`core/src/window.rs:116-120, 181-185` — the CPU view exists) or be pooled |

For the other backends (brief, from the code): X11 already treats
`WindowType::Menu` specially — override-redirect, `_NET_WM_WINDOW_TYPE_POPUP_MENU`,
`XGrabPointer` (`linux/x11/mod.rs:1824, 2069-2071, 2154, 4420-4426`) and has a
`_NET_WM_WINDOW_TYPE_TOOLTIP` mapping for `WindowType::Tooltip` (`:2155`);
Wayland has `xdg_popup` but only as the click-only `WaylandPopup`
(`linux/wayland/mod.rs:631-671`, plan §3b' "the one real port"); Windows has
no `WindowType::Menu` handling at all (`grep WindowType::Menu windows/mod.rs`
→ nothing) and needs `WS_POPUP|WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE` + owner
HWND (plan §3b table). macOS is the first because it is what the user tested.

## 4. How context menus already become real OS windows on macOS

Two paths, both queued never shown inline (`mod.rs:3416-3440` explains the
re-entrancy reason):

- `use_native_context_menus` (default): an `NSMenu` via
  `popUpMenuPositioningItem_atLocation_inView` (`mod.rs:3059`, built in
  `macos/menu.rs:235-258`). Native look, native dismiss — but it is an
  `NSMenu`, not a window we draw into; useless for a picker or a rich tooltip.
- fallback: `show_fallback_menu` → `crate::desktop::menu::show_menu`
  (`dll/src/desktop/menu.rs:389-492`) → `WindowCreateOptions` with
  `WindowType::Menu`, `is_always_on_top`, `decorations: None`,
  `RelativeToParentWindow`, `size_to_content: true`, a `menu_layout_callback`
  with `MenuWindowData` in the callback ctx — pushed to
  `pending_window_creates` and created by the run loop as above.

Master's `popup_create_options` is a deliberate copy of that second path
(`master:common/transient.rs:147-210`, "`Menu` is the window type on purpose")
with two improvements: the size is the engine's measurement (no 1x1-then-resize
dance) and the callback ctx is the parent↔popup mailbox. So the reusable
primitive for popover/tooltip/colour picker on macOS IS this queue — what is
missing is only how `MacOSWindow::new` treats a `Menu`/`Tooltip`-type request
(§3 above). The `NSPanel` tooltip in `macos/tooltip.rs` is the other half of
the primitive: it already shows how to make a non-key floating panel and
convert window-local → global top-down coordinates (`mod.rs:6019-6046`,
MWA-B9 primary-screen flip) — that conversion is the one
`position_window_on_monitor` lacks.

## 5. `ColorInput` and the native `ColorPickerDialog`

- `layout/src/widgets/color_input.rs`: a 14x14 swatch (`:96-102`) whose
  `MouseUp` handler invokes `on_value_change` with the CURRENT colour and
  nothing else — "No built-in color picker dialog — the on_value_change
  callback receives the current color so the caller can open their own picker"
  (`:193-200`). It carries `accessibility_name` (`:26-32`) but `dom()` never
  declares accessibility info (`grep with_accessibility color_input.rs` →
  nothing besides the field), so the name is inert today.
- `layout/src/desktop/dialogs.rs:218-249`: `ColorPickerDialog::open(title,
  default) -> OptionColorU` runs `tfd::ColorChooser::run_modal()` — a BLOCKING
  native modal (on macOS an `NSColorPanel`-ish tfd dialog), not anchored, not
  a popover, returns RGB without alpha.
- The demo shows the bare swatch: `ColorInput::create(ColorU {255, 87, 51})`
  (`examples/azul-widgets/src/lib.rs:117-118`), no callback, so clicking it
  does nothing visible.

What the plan's §7 picker needs, and what exists for each piece:

| §7 child | building block | status |
|---|---|---|
| `<saturation-value-plane>` 2D drag | the `Slider`'s drag protocol: `get_cursor_relative_to_node`, `get_hit_node_rect`, `set_css_property` on pointer down/move/up/leave (`layout/src/widgets/slider.rs:433-520`) generalised to two axes; background = two layers `linear-gradient(to right, #fff, hsl(h,100%,50%))` over `linear-gradient(to top, #000, transparent)` — `StyleBackgroundContentVec` supports multiple layers and `LinearGradient` (`css/src/props/style/background.rs:63, 185`) | new widget code; no HSV helper exists (`hsl_to_rgb` is private inside the CSS colour parser, `css/src/props/basic/color.rs:1367`) — add `ColorU::{to_hsv, from_hsv}` |
| `<hue-slider>` | `Slider` with a custom `track_style` (public field, `slider.rs:64-67`) carrying a 6-stop rainbow gradient, `0..360` | exists |
| `<rgb-fields>` / hex | three `NumberInput`s + one `TextInput` with `on_focus_lost`/enter commit | exist |
| `<eyedropper>` | `screencap.rs` capture | exists; optional, last |
| a11y | `set_accessibility_value(hex)` on every change (c5fcb87b7), name warning (b7ab320ad) | exists |
| the popup | `<transient-window anchor="bottom" dismiss="outside">` child of the swatch | engine on master; macOS surface per §3 |

Note that the picker lives in a transient CONTENT dom (`DomId >= 0x1000_0000`),
so every `get_node_layout_rect`/cursor-relative call inside it depends on the
`DomNodeId::dom` fix from `c8d31ab97` — already on master, and the reason the
picker must not be built on this worktree's older layout crate.

---

## 6. Proposed plan (builds on master; nothing here re-does the engine)

### (a) Tooltip as a transient window on macOS

1. **macOS popup surface** (prerequisite for both (a) and (b); ~1-2 days +
   verification on a Mac, not headless-testable):
   - In `MacOSWindow::new`, branch on `options.window_state.flags.window_type`:
     `Menu`/`Tooltip` → allocate an `NSPanel` subclass (define_class, like
     `WindowDelegate`) with style `Borderless | NonactivatingPanel`, override
     `canBecomeKeyWindow → YES` for `Menu` (keyboard without activation),
     `NO` for `Tooltip` (never key, `orderFrontRegardless` only);
     `setHidesOnDeactivate(true)`, `setHasShadow(true)`, `setOpaque(false)`,
     `backgroundColor = clear`; skip the Titled/Resizable dance entirely.
   - After creation, `parent.addChildWindow(panel, ordered: .above)` using
     `parent_window_id` (the registry key IS the `NSWindow*`,
     `master:common/event.rs:3198-3210`); drop `NSFloatingWindowLevel` for
     children. Remove the child in `drain_closed_windows`.
   - Fix `position_window_on_monitor`'s `RelativeToParentWindow` arm
     (`mod.rs:7851-7863`): convert with the parent's `convertRectToScreen`
     exactly as `show_tooltip` does (`mod.rs:6019-6046`) and then
     `setFrameTopLeftPoint`, so the primary-screen flip and the parent's
     actual screen are both right; clamp to `screen.visibleFrame` (flip
     `Bottom`→`Top` when it would fall off the bottom — `TransientPlacement`
     carries the anchor rect, so the flip is one `resolve` with the opposite
     edge).
   - Dismiss on app/parent deactivation: in the parent's `windowDidResignKey`
     (`mod.rs:2750`) and an `NSApplicationDidResignActive` observer, call the
     shared `dismiss_outside_on_press`-equivalent for `outside` popups (a new
     `dismiss_outside_on_focus_loss(lw)` in `common/transient.rs`, engine-side
     and shared — the Windows `WM_ACTIVATE`/X11 `FocusOut` paths will use it).
   - Popup windows created for `WindowType::Tooltip` request
     `HwAcceleration::Disabled` (CPU view) so a hover does not create a GL
     context.
2. **Engine: runtime `open`** (~0.5 day, small, engine-side — coordinate with
   the master session): `CallbackInfo::set_transient_open(DomNodeId, bool)`
   that rewrites the `TransientWindowConfig.open` bit in the styled dom's
   `NodeType` in place and requests relayout, the way `set_css_property`
   mutates style without a rebuild. Without it a hover-driven tooltip would
   need a full `layout()` per hover.
3. **Tooltip = transient window** (~1 day):
   - `Tooltip::dom()` emits `[anchor, <transient-window open=false anchor="cursor"
     dismiss="none" class="__azul-native-tooltip-tip"><p>text</p></transient-window>]`
     instead of the absolutely-positioned `<p>`; its `MouseEnter` handler arms
     `TOOLTIP_DELAY_TIMER_ID` (the existing timer, `window.rs:6416-6434`) and
     the timer callback calls `set_transient_open(tip, true)`; `MouseLeave`,
     any `MouseDown`, key-down and scroll set it back to `false`. The
     `title`/`alt` attribute path keeps emitting `ShowTooltip` but the shell
     routes it into the same machinery (synthesised `TransientPlacement` with
     `anchor: Cursor`) so there is ONE tooltip renderer; drop `aria-label` from
     the tooltip text priority (`core/src/dom.rs:3421-3432` stays for a11y,
     the tooltip lookup becomes `title > alt` only).
   - `popup_create_options` gets a `WindowType` parameter: `Tooltip` when
     `dismiss == None && anchor == Cursor` (X11 then gets
     `_NET_WM_WINDOW_TYPE_TOOLTIP` for free, `x11/mod.rs:2155`).
   - UA CSS for `.__azul-native-tooltip-tip` from `SystemStyle` (macOS: 11 pt
     system font, `tooltip` background/text colours that already exist in the
     `SystemStyle` palette, 1 px hairline, 5 px radius, 4x8 px padding, wrap at
     ~300 px) — the same CSS is the web fallback.
   - Delay: `macos/system_style.rs` reads `NSInitialToolTipDelay` from
     `NSUserDefaults` into `input.hover_time_ms` (fallback 1000 ms) so the
     400 ms Windows default stops applying on macOS.
   - Position: `Cursor` anchor + a fixed (0, +cursor-height) offset in
     `TransientPlacement::resolve` for `WindowType::Tooltip`, then the §1
     clamp.
   - Delete `macos/tooltip.rs`, `x11/tooltip.rs`, `wayland/tooltip.rs` and
     the Win32 `TOOLTIPS_CLASS` wrapper once the transient path is verified on
     each backend (Windows last: its native control is the one that looks
     right today).

### (b) The colour-picker popover as THE transient-window test case

4. **`ColorU` HSV helpers** in `azul_css` (`to_hsv`/`from_hsv`, pure, unit
   tested against the CSS parser's `hsl()` round trip) — 0.5 day.
5. **`ColorPicker` widget** (`layout/src/widgets/color_picker.rs`, ~2 days incl.
   autotests in the existing `autotest_generated` style): SV plane (2D drag,
   thumb ring moved via `set_css_property`), hue `Slider` with gradient
   track, alpha `Slider` (checkerboard track), hex `TextInput`, R/G/B
   `NumberInput`s, `on_change(ColorU)`; publishes `accessibility_value` =
   hex; zero a11y-lint warnings (plan §8 step 5 stop-point).
6. **`ColorInput` integration** (0.5 day): the swatch gains `open: bool` in
   `ColorInputStateWrapper`; `dom()` emits
   `swatch > <transient-window anchor="bottom" dismiss="outside">` holding the
   picker; the click handler flips `open` (via `set_transient_open`, or by
   returning `RefreshDom` until step 2 lands); a `Dismissed` handler
   (`ComponentEventFilter::Dismissed`) clears it; the picker's `on_change`
   updates the swatch background live through `set_css_property` and fires
   the existing `on_value_change`. `ColorPickerDialog::open` stays as the
   modal alternative.
7. **AzWidgets demo** (0.5 day): replace the inert swatch with the live one;
   hover + click instructions in the labels; api.json entries for
   `TransientWindowConfig`/`ColorPicker` via the autofix commands (never
   hand-curated) so every binding gets them.

### How to verify

Headless (extends `master:dll/tests/transient_window_layout.rs`, same
harness, runs in CI on all OSes):
- **tooltip**: hover the anchor → nothing queued before `hover_time_ms`;
  after the timer fires, exactly one `WindowType::Tooltip` window is queued,
  `anchor == Cursor`, content is the `<p>`, `content_size` equals the laid-out
  text box; moving off the anchor / a press / Escape closes it and the
  `TransientDiff` is `{closed: [id]}`; `ShowTooltip` from a `title` attribute
  produces the same window (removes the runner's `unsupported("ShowTooltip")`
  case, `layout/src/e2e/runner.rs:2546-2551`).
- **picker**: click the swatch → one `Menu`-type popup anchored `Bottom` at
  `(swatch.x, swatch.bottom)` with the picker's measured size
  (`assert_layout`); drag on the SV plane inside the popup's content dom
  changes the swatch colour in the PARENT (proves the one-tree claim and the
  `DomNodeId::dom` fix); a press in the parent dismisses it and `Dismissed`
  fires once; Escape in the popup dismisses it; rebuilding the parent 60x
  with the popup open never changes its `content_dom` (plan §8 step 3's
  `transient-churn` lint).
- **hit-test contract**: extend `virtualview_hit_matches_render.rs` to a
  transient content dom (plan §3a, "the three-rect trap").

On a Mac (manual, the only way to see the surface): popup opens directly
under the swatch on the primary AND on a secondary monitor (the Y-flip);
typing in the hex field does not blur the document behind (non-activating);
dragging the parent drags the popup along (child window); Cmd-Tab hides it;
clicking the desktop closes it; the tooltip appears ~1 s after hovering
"Hover me", just below the pointer, in the system appearance, and goes away on
the first click.

### Effort

- macOS surface (1): 1-2 days + a Mac for verification.
- runtime `open` (2): 0.5 day, engine-side.
- tooltip (3): 1 day on macOS; 0.5 day per further backend to retire its
  bespoke tooltip renderer.
- picker (4-7): 3-4 days.
- Total ≈ 6-8 days, of which only (1) is blocked on hardware.

### Overlaps — read before starting

- **The `<transient-window>` engine is being done by ANOTHER session on
  master** (`2b5dcf2d1`, `cba7a2e4d`, `c8d31ab97`, `a7482b8b3`, and the plan
  says Wayland and the colour picker are next in ITS queue). Everything in §2
  must be consumed, not re-implemented: no second popup system, no second
  dismiss logic, no second reconcile. Step 2 (runtime `open`) touches
  `LayoutWindow`/`CallbackInfo` and must be agreed with that session or land
  after its next commit; steps 1, 3 and 4-7 are additive (macOS shell, widget
  code, CSS) and do not conflict.
- This worktree's layout crate predates `c8d31ab97` (no `transient.rs`, no
  `DomNodeId::dom` fix), so the picker cannot be started here — it must be
  done on master or a branch cut after `a7482b8b3`.
- The tooltip NSPanel in `macos/tooltip.rs` and the `Tooltip`/`Popover`
  widgets' `TODO2`s are all superseded by (a); do not polish them separately.
- The `RelativeToParentWindow` Y-flip (§3) also affects the window-based
  fallback context menu on macOS — one fix for both.
- `-azul-app-region` (d51fa6e8f) and the DWM frame work (1149a90d1) are the
  same arc; `tearoff` (plan §5) will reuse them and is out of scope here.

---

Report written to
`/Users/fschutt/Development/azul/.claude/worktrees/debug-slider-scroll-2026-08-22/scripts/BUGS_2026_08_22_tooltip_transient_popover.md`.
