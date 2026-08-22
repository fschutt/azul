# `<transient-window>`: popups as real OS windows, one DOM tree

Written 2026-08-22. The follow-up to `-azul-app-region` (d51fa6e8f) and the
DWM frame extension (1149a90d1): now that an app can own its chrome, the next
thing it needs is a **popup that is a real window** — a colour picker that
opens below its swatch the way Chrome's `<input type=color>` does — without
leaving the one-tree DOM model.

## 0. The idea in one paragraph

An element `<transient-window>` lives in the ordinary DOM, as a child of the
node it is anchored to. While it is `closed` it lays out as nothing. When
`open`, the **engine** materialises its subtree as a transient OS window
(xdg_popup on Wayland, `_NET_WM_WINDOW_TYPE_POPUP_MENU` on X11, a borderless
`WS_POPUP` owned window on Windows, an `NSPanel` child on macOS) positioned
relative to the anchor, sized to its content unless told otherwise, and routes
input back into the SAME DOM so callbacks, state and styling are untouched.
On the web it degrades to a `<div>` with `position: absolute`. The app
never handles windows; it toggles one attribute.

## 1. What already exists — build on it, do not parallel it

Verified in the tree, 2026-08-22:

| need | already there | where |
|---|---|---|
| a popup that is a real OS window | context menus | `WindowType::Menu`; wayland `xdg_popup` (mod.rs:661), x11 `_NET_WM_WINDOW_TYPE_POPUP_MENU` (mod.rs:2154) |
| anchor-relative placement | `MenuPopupPosition::{BottomOfHitRect, TopOfHitRect, LeftOfHitRect, RightOfHitRect, …}` | `core/src/menu.rs:91` |
| opening a popup from a callback | `CallbackChange::OpenMenu { menu, position }` | `layout/src/callbacks.rs:480` |
| child-window lifecycle in the loop | `children: Vec<HeadlessWindow>` for menus/dialogs | `headless/mod.rs:2075` |
| UA defaults per element | `core/src/ua_css.rs::get_ua_property` | |
| a child DOM rendered inside a parent | `VirtualView` / iframe (`child_dom_id`, `content_offset`) | `display_list.rs:1132` |
| drag-to-move from CSS | `-azul-app-region` | d51fa6e8f |

**The menu path is 80% of this feature.** A context menu IS a transient window
whose content happens to be generated from a `Menu` struct. `<transient-window>`
is the same window with content that comes from the DOM instead. The plan is to
**lift the menu's window machinery into a general `TransientWindow`** and make
`Menu` a client of it — not to add a second popup system beside the first.

## 2. The DOM surface

```html
<div class="swatch" id="fg">
  <transient-window open="false" anchor="bottom" tearoff="true" id="fg-picker">
    <color-picker value="#e66465" />
  </transient-window>
</div>
```

`NodeType::TransientWindow` (new). Attributes (all also settable from Rust and
from api.json so every binding gets them):

| attribute | values | meaning |
|---|---|---|
| `open` | `true` / `false` | the ONLY thing an app toggles. Open ⇒ materialise. |
| `anchor` | `bottom` (default) `top` `left` `right` `cursor` | maps 1:1 onto `MenuPopupPosition` |
| `dismiss` | `outside` (default) `escape` `none` | outside-click / Esc closes it, like a menu; `none` for palettes |
| `tearoff` | `false` (default) `true` | see §5 |
| `shape` | CSS selector of a clip mask, or empty | see §6 |
| `size` | `content` (default) `WxH` | content-sized unless told |

`ua_css.rs` defaults for `transient-window`: `position: absolute;` (so the web
fallback needs no app CSS), `top: 100%` (opens below), `display: none` while
closed, `background: window-background`, `box-shadow: popup`. One place, and
the same stylesheet drives the native window's size.

Why an element and not a CSS property: a popup has CONTENT (children) and
STATE (`open`), which is what elements are for. `-azul-app-region` was right as
a property because it has neither.

## 3. Engine plumbing — the actual work

### 3a. Layout (azul-layout)

- A closed `<transient-window>` contributes **nothing** to layout — not
  `display:none` inside a parent's flow, but skipped, so its subtree is not
  laid out at all until opened. Cheap to keep a dozen in the tree.
- An open one is laid out as its **own root** with `available = content
  intrinsic size` (or the `size` attr), exactly how a `VirtualView` child DOM
  lays out in child-local coordinates. Reuse `child_dom_id` + the
  `DomLayoutResult` map. It gets its OWN `DomId`.
- Hit-test and placement: the transient DOM's placement is the anchor node's
  on-screen rect + the `anchor` offset. **Reuse the `content_offset` discipline
  from 05ecdd529** — three rects, and the hit test MUST use the same ones the
  renderer uses, or clicks land one character off. There is a contract test for
  that (`virtualview_hit_matches_render.rs`); extend it to transient windows.

### 3b. Window (azul-dll, per backend)

Generalise what `Menu` does into `TransientWindow { parent, anchor_rect,
placement, dom_id, dismiss, shape }`:

| backend | primitive | notes |
|---|---|---|
| Wayland | `xdg_popup` + `xdg_positioner` | exists for menus; positioner anchors natively — pass `anchor` through, do NOT compute coordinates (the compositor hides them) |
| X11 | override-redirect `_NET_WM_WINDOW_TYPE_POPUP_MENU` (exists) or `_NET_WM_WINDOW_TYPE_DROPDOWN_MENU` | transient-for = parent; grab pointer for `dismiss=outside` |
| Windows | `WS_POPUP \| WS_EX_TOOLWINDOW \| WS_EX_NOACTIVATE`, owner = parent HWND | `DwmExtendFrameIntoClientArea` (1149a90d1) so it keeps a shadow; `WS_EX_LAYERED` for §6 |
| macOS | `NSPanel` child with `.nonactivatingPanel`, `addChildWindow(_:ordered:)` | `NSWindow.hasShadow`; `contentView.wantsLayer` for §6 |

Event routing: input on the transient surface is dispatched into the
**parent's** `LayoutWindow` with `dom_id = transient`. That is what keeps one
tree: the callback `RefAny`s are the same objects, `set_css_property` works,
`trigger_virtual_view_rerender` works.

Dismiss: `outside` ⇒ a press anywhere not inside the transient closes it
(menus already do this; reuse). `escape` ⇒ the key-up path. Both set
`open=false` through the normal reconcile, so the app's next `layout()` sees
it closed — no side channel.

### 3b'. DECISION (2026-08-22, after reading all four backends): the surface is a full window

Every backend's popup is already a *complete* window — own `LayoutWindow`,
own WebRender renderer, own GL context — including Wayland's `xdg_popup`
(`WaylandPopup` embeds all of that). A "thin surface that paints the
parent's display list and forwards input" would mean re-architecting four
render paths; that is not the cheap lift §3b assumed. So:

- **The parent's `LayoutWindow` stays the source of truth.** It holds the
  `<transient-window>` nodes, runs the reconcile, lays each popup's subtree
  out under its own `DomId` (this gives the content size BEFORE any surface
  exists — the `xdg_positioner` needs it up front, and it kills the
  `size_to_content` 1×1-then-resize dance that mispositions Wayland menus),
  and emits a per-pass diff `{opened, moved, closed}`.
- **A popup surface is a child window created through the existing
  `queue_window_create` path** with the `Menu` window type (so every backend's
  popup treatment applies unchanged: override-redirect + `POPUP_MENU` on X11,
  `xdg_popup` on Wayland, borderless always-on-top on macOS/Windows), sized to
  the measured content, positioned `RelativeToParentWindow` from the anchor.
- **"One tree" is kept at the STATE level, not the surface level.** The popup's
  layout callback returns the extracted subtree (resolved style baked in, see
  `extract_subtree_as_dom`), so its callbacks are the same function pointers
  on the same `RefAny`s; any `RefreshDom` inside the popup escalates to the
  parent, whose rebuild re-extracts and pushes fresh content into the popup.
- **The channel between parent and popup is the popup's layout-callback ctx
  `RefAny`** (`TransientWindowData`: content, generation, closed, dismissed).
  The parent keeps a clone; no backend needs a cross-window "close by id",
  which does not exist (Wayland's popup is not even a registered window).
  Wake-up after writing the mailbox is `request_regeneration_all_windows`,
  the fan-out every backend already does inline for `RefreshDomAllWindows`.
- **Dismiss is engine-side and edge-triggered.** Escape in the popup, or the
  popup losing focus while `dismiss=outside`, sets `dismissed` in the mailbox;
  the parent's next pass marks the SOURCE NODE dismissed in the manager (so a
  still-`open` node does not reopen until `open` goes false→true), closes the
  surface, and fires `ComponentEventFilter::Dismissed` on the node so the app
  drops its own flag. The app never has to know which platform it is on.
- **Wayland is the one real port**: `WaylandPopup` is a click-only menu
  surface (no hover, drag, keys, scroll). It has to become "a `WaylandWindow`
  whose shell role is `xdg_popup`", driven by the full common event pipeline.
  That is the last step, after macOS/Windows/X11 — where popups already ride
  full windows — and the colour picker.

### 3c. The reconcile rule (the thing that will bite)

A transient window is a **child DOM attached to a node**, so it needs exactly
the treatment `DatasetMergeCallback` gives the map's tile cache: across a
parent rebuild, the SAME transient window must survive (not flicker closed and
re-open), with its `DomId` and its OS surface intact, when the anchor node is
reconciled to the same logical node. `transfer_states` (d386614cd carried
images across this boundary) is where that goes. Without it every parent
`RefreshDom` tears the popup down — the same class of bug as the screenshare
flicker.

## 4. The web fallback

`<transient-window>` ⇒ `<div class="az-transient-window" data-open="…">` with
the UA CSS above (`position:absolute; top:100%`). `open` toggles
`display`. `anchor` maps to `top/bottom/left/right: 100%`. `dismiss=outside`
⇒ a document-level click listener the codegen already knows how to emit for
menus. Nothing else — the point of §2's UA defaults is that the web version
needs no new CSS.

## 5. `tearoff` — drop zones and detaching

`tearoff="true"` lets the user drag the transient window OUT of its anchor,
at which point it becomes a free `WindowType::Normal` (or `Dialog`) toplevel
that is still the same DOM subtree. Think Photoshop palettes, GIMP tear-off
menus, Firefox tear-off tabs.

- The transient window's own title strip declares `-azul-app-region: drag`
  (d51fa6e8f). A drag that ends OUTSIDE the parent's rect re-parents: destroy
  the popup surface, create a toplevel at the drop point, keep `dom_id`.
- `tearoff="zone:<selector>"`: instead of a free toplevel, dropping onto a node
  matching the selector **re-anchors** the transient window there. That is
  the drop-zone half: dock/undock palettes between sidebars.
- Re-docking: a torn-off toplevel that is dragged back over its original anchor
  (or any matching zone) snaps back to transient. Hit-test the drop against
  zone rects in the PARENT's layout, which already exist.
- State lives in the DOM: `open`, `torn="true"`, and the torn window's
  position, so `layout()` can rebuild it deterministically and a headless e2e
  scenario can assert on it.

## 6. Shape — transparency, clip shapes, image masks

A popover with a pointer-arrow, a circular colour wheel, a non-rectangular
tool palette: the window needs a **shape**, not just a rectangle.

`shape="<selector>"` names a node whose rendered alpha becomes the window's
input AND visual shape. Implementation per backend:

| backend | mechanism |
|---|---|
| Wayland | `wl_surface.set_input_region` + ARGB buffer (transparency is free) |
| X11 | `XShape` (`ShapeBounding` + `ShapeInput`) from the alpha mask; 32-bit ARGB visual for true transparency |
| Windows | `WS_EX_LAYERED` + `UpdateLayeredWindow` with a per-pixel-alpha DIB; `SetWindowRgn` for the input region |
| macOS | `NSWindow.isOpaque = false`, `backgroundColor = .clear`; shape comes from the view's alpha automatically |

The mask source is the CPU renderer's existing alpha — `clip_mask` on an image
already exists (`disable_clip_masks` flag, `core/src/window.rs:641`), so the
rasteriser can produce the mask buffer; the new work is only handing it to the
windowing layer. Do it LAST: it is the most platform-specific part and the
colour picker does not need it.

## 7. The colour picker (the payoff)

Once §2–§3c land, `ColorInput` (which today "only reports the current colour so
the caller can open their own picker", `color_input.rs:190`) grows a real
picker:

```
<color-input value="#e66465">            ← swatch, already exists
  <transient-window anchor="bottom">     ← this plan
    <color-picker>                       ← new widget, plain DOM
      <saturation-value-plane />         ← 2D drag, `set_accessibility_value` on move
      <hue-slider />                     ← Slider with a rainbow track
      <eyedropper />                     ← needs screen capture; screencap.rs exists
      <rgb-fields />                     ← three NumberInputs + the R/G/B ⌃ mode toggle
    </color-picker>
  </transient-window>
</color-input>
```

Every child is an existing widget or a plain div. The picker publishes
`accessibility_value` = the hex string on every change (the slider and stepper
already do this, c5fcb87b7). It gets the `a11y-widget` name-warning like every
other widget (b7ab320ad). Nothing in it knows it is inside a window.

## 8. Order of work, with the stop-points

1. **Lift `Menu`'s window into `TransientWindow`** (3b) and make `Menu` use it.
   Stop-point: context menus still work on all 4 backends. No new behaviour
   yet; pure refactor with the existing menu e2e as the guard.
2. **`NodeType::TransientWindow` + layout skip/own-root** (2, 3a) + UA CSS +
   the web `<div>` fallback (4). Stop-point: headless e2e can open one, assert
   its placement via `assert_layout`, and the hit-test contract test passes.
3. **Reconcile survival** (3c). Stop-point: an e2e scenario that rebuilds the
   parent 60× with the popup open asserts `image-churn`-style that the popup's
   `DomId` never changed (add a `transient-churn` lint alongside image-churn).
4. **`dismiss`** (outside / escape). Stop-point: e2e closes it both ways.
5. **Colour picker** (7). Stop-point: AzWidgets demo, zero a11y warnings.
6. **`tearoff`** (5). Stop-point: drag out, drag back, zone re-anchor, each an
   e2e scenario.
7. **`shape`** (6). Last, per backend, behind a capability probe.

## 9. Risks worth naming now

- **Wayland positions are hidden.** Everything in 3a that "computes a screen
  position" must instead hand an anchor+gravity to `xdg_positioner`. Code that
  works on X11 by doing arithmetic will be WRONG on Wayland. Design the
  placement API as (anchor rect, gravity) from the start; never as (x, y).
- **Focus.** A transient window must not steal keyboard focus from the parent
  (Windows `WS_EX_NOACTIVATE`, macOS `.nonactivatingPanel`) or typing into the
  colour fields will blur the document behind. The blur-latch fixes from
  ae9442beb/800c14757 make focus transitions safe but do not decide WHERE focus
  goes.
- **The three-rect trap** (05ecdd529) applies verbatim. Write the hit-test
  contract test in step 2 BEFORE the first click handler.
- **Reconcile** (3c) is where the screenshare flicker and the map's orphaned
  tile cache both came from. It is step 3 for a reason: do not build the colour
  picker on a popup that closes every relayout.

## 10. Status — 2026-08-22 (branch `transient-window`)

| step | state | where |
|---|---|---|
| 1 lift Menu's window | **done (shared builder + shared dismiss)** — `popup_window_state` is the one "what a popup window is" for menus and transient windows; Escape/focus-loss dismiss serves window-based menus too. Menu CONTENT still comes from `menu_renderer`, not from `<transient-window>` nodes. | `dll/src/desktop/menu.rs`, `common/transient.rs` |
| 2 node type + layout + UA CSS + web fallback | **done** | `core/src/transient.rs`, `ua_css.rs`, `solver3/layout_tree.rs`, `dll/src/web/html_render.rs` |
| 3 reconcile survival | **done** (manager matched by source node, `NodeIdRemap`, content ids never reused; `transient-churn` lint NOT added) | `layout/src/transient.rs` |
| 3b surfaces | **done on macOS (verified on screen), X11/Windows ride the same Menu-window path (type-checked, not run), Wayland = `WaylandPopup` is now a FULL `PlatformWindow`** (its own `CommonWindowState`, the real regenerate/process pipeline, keyboard + pointer capture + engine dismissal) | `common/transient.rs`, `macos/mod.rs`, `linux/wayland/mod.rs` |
| 4 dismiss | **done** (outside press in the parent, Escape in popup or parent, focus loss; `ComponentEventFilter::Dismissed`; edge-triggered re-arm) | `common/transient.rs`, `common/event.rs` |
| 5 colour picker | **done** (plane/hue/alpha with pointer capture, hex + RGBA fields, themed div checkerboards, a11y names/values). No eyedropper, no R/G/B⌃ mode toggle. | `layout/src/widgets/color_input.rs` |
| 6 tearoff | **done** (grip drag off the anchor -> `Normal` toplevel, drag back docks, `tearoff="zone:.sel"` re-anchors, `torn` attribute + `TornOff`/`Docked` events + `set_transient_window_torn`; macOS verified on screen incl. the reverse drag). The drag runs in the popup's OWN pipeline; runtime `RelativeToParentWindow` re-placement on mac/win/x11, arithmetic on Wayland. | `layout/src/transient.rs`, `common/transient.rs`, `color_input.rs` |
| 7 shape | **not started** (deliberately last) | - |

Engine fixes that fell out of it (all general): `get_node_layout_rect` ignored
the dom id and halved sizes on HiDPI; the pre-cascade fast path forked widget
datasets from their callbacks; macOS closed windows crashed (display-link
retain after free, `releasedWhenClosed` double release) and had no
`RefreshDomAllWindows` fan-out; pointer capture did not exist.
