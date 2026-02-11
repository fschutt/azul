# Remaining Bugs – Hello World Titlebar & Click

Follow-up to `HELLO_WORLD_LAYOUT_INVESTIGATION.md` (bugs 1–5 fixed, commits
`3085fed1`…`c2ead850`).  This file tracks the issues that are still open after
the first round of fixes.

---

## Bug 7 – Title text clipped to intrinsic width (flex-grow missing)

| Field          | Value |
|----------------|-------|
| **Severity**   | Visual – title truncated |
| **File**       | `layout/src/widgets/titlebar.rs` → `build_title_style()` |
| **Root cause** | The title `<text>` node is a flex-item inside the titlebar flex-row container but has **no `flex-grow: 1`** (and no explicit `width`). The flex algorithm therefore sizes the item to its intrinsic text width (71.9 px for "Hello World" at 13 px). Combined with `overflow-x: hidden` this clips the text to that narrow box. |

### Evidence (debug data)

```
Titlebar container  (Node 1):  display=flex, flex-direction=row, justify-content=center
                               padding-left=78px,  used_size = (0, 0, 400, 28)
Title text          (Node 2):  formatting_context=Inline, overflow_x=hidden
                               used_size = (78.0, 6.3, 71.9, 15.5)
                               ← should be (78.0, 6.3, 322.0, 15.5)
Display-list clip:  PushClip at (78, 6.3, 71.9, 15.5) → text glyphs clipped
```

### Fix

Add `flex-grow: 1` (and optionally `min-width: 0`) to the title text style in
`build_title_style()`.  The text will then expand to fill the remaining space
`400 − 78 = 322 px`, and `overflow-x: hidden` + `text-overflow: ellipsis` will
only kick in when the title is genuinely longer than the available space.

```rust
// In build_title_style(), add:
props.push(CssPropertyWithConditions::simple(
    CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1)),
));
props.push(CssPropertyWithConditions::simple(
    CssProperty::const_min_width(LayoutMinWidth::const_px(0)),
));
```

---

## Bug 8 – Title text color hardcoded (#4c4c4c), ignores dark mode

| Field          | Value |
|----------------|-------|
| **Severity**   | Visual – wrong colour on dark backgrounds |
| **File**       | `layout/src/widgets/titlebar.rs` → `build_title_style()` |
| **Root cause** | The title colour is a compile-time constant `ColorU { r:76, g:76, b:76, a:255 }` (`#4c4c4c`). It is never read from `SystemStyle.colors.text` and therefore does not adapt when the system is in dark mode or when the user has a translucent sidebar-material titlebar. |

### Fix

`SoftwareTitlebar` already receives the `SystemStyle` via `from_system_style()`.
Store the resolved title colour in a new `title_color: ColorU` field and use it
in `build_title_style()`.  Fall back to `#4c4c4c` (light) / `#e5e5e5` (dark) when
`SystemStyle.colors.text` is `None`.

---

## Bug 9 – `padding_horizontal` in `TitlebarMetrics` is never read

| Field          | Value |
|----------------|-------|
| **Severity**   | Dead code / design gap |
| **File**       | `css/src/system.rs` (struct `TitlebarMetrics`), `layout/src/widgets/titlebar.rs` |
| **Root cause** | `TitlebarMetrics` has a field `padding_horizontal: OptionPixelValue` (set to `8 px` on every platform), but `SoftwareTitlebar::from_system_style()` **never reads it**. The field's intended purpose (extra inner padding on both sides of the titlebar content area) is silently ignored. |

### Fix

Either:
1. **Use it** – add `padding_horizontal` to both `padding_left` and `padding_right`
   in `from_system_style()`, OR
2. **Remove it** – delete the field if it is truly not needed (the button-area
   width already includes the optical gap).

Option 1 is preferred because macOS titlebars have an 8 px inset on the
non-button side.

---

## Bug 10 – `discover_macos_style()` never queries actual titlebar metrics

| Field          | Value |
|----------------|-------|
| **Severity**   | Hardcoded values may be wrong on future macOS versions |
| **File**       | `css/src/system.rs` → `discover_macos_style()` |
| **Root cause** | `discover_macos_style()` calls `defaults::macos_modern_light()` (or dark), which uses `TitlebarMetrics::macos()` with hardcoded `height = 28`, `button_area_width = 78`, `button_side = Left`.  **No attempt** is made to query the OS for the real values (e.g. via `NSWindow.contentLayoutRect` or by measuring the standardWindowButton frame). If Apple changes the traffic-light geometry in a future macOS release these values will be wrong. |

### Fix (deferred – low priority)

On macOS, query the actual button positions at app startup through one of:
- `NSWindow.standardWindowButton(.closeButton)!.frame` → derive button_area_width
- `NSWindow.titlebarHeight` (private but widely used)
- Heuristic: `NSProcessInfo.processInfo.operatingSystemVersion` and a lookup table

For now the hardcoded values are correct for macOS 11–15.  Mark the constants
with a `// Verified: macOS 11 – macOS 15 Sequoia` comment so future maintainers
know when to re-check.

---

## Bug 11 – Button click does not increment counter (event delivery)

| Field          | Value |
|----------------|-------|
| **Severity**   | Functional – click callback never fires |
| **File**       | `dll/src/desktop/shell2/common/debug_server.rs` (debug path), or real event pipeline |
| **Root cause** | **Not yet fully diagnosed.**  After the debug API sends a `click` at (66, 89) (inside the button), the counter text remains "5".  Possible causes: |

1. **Debug API `click` does not synthesize a full LeftMouseDown → LeftMouseUp
   sequence** – the real event pipeline may require both events (or a proper
   `MouseUp` to fire the callback).
2. **`on_click` callback is attached via `AzButton_setOnClick`** which may
   register a `LeftMouseUp` filter – verify that the debug API generates
   both `MouseDown` and `MouseUp`.
3. **After `Update::RefreshDom` is returned**, the framework must call the layout
   callback again – the debug API may not trigger a re-layout / re-render.

### Next steps

1. Trace the real event pipeline (`process_event` → `fire_callbacks` → `on_click`).
2. Verify the debug API synthesizes both mouse-down + mouse-up.
3. Test with a real mouse click (not via debug API) to isolate whether the
   bug is in the debug API or the real event flow.

---

## Bug 12 – Titlebar should use block layout, not flex

| Field          | Value |
|----------------|-------|
| **Severity**   | Design / cosmetic |
| **File**       | `layout/src/widgets/titlebar.rs` → `build_container_style()` |
| **Root cause** | The titlebar container currently uses `display: flex; flex-direction: row; justify-content: center`.  On macOS (and most platforms) the native titlebar is simply a block-level bar with a single centred text span.  Using flex introduces unnecessary complexity and the flex-grow bug (Bug 7). |

### Fix (optional, after Bug 7)

Change the container from `display: flex` to `display: block`.  The title text
can be centred via `text-align: center` (already present).  The `padding-left` /
`padding-right` already reserves button space.  With `display: block` the text
node would automatically fill the content box, and `overflow: hidden` + `text-overflow: ellipsis` would work correctly without needing `flex-grow`.

---

## Summary – Fix order

| Priority | Bug | Fix effort | Commit message |
|----------|-----|------------|----------------|
| 1        | Bug 7  | 2 lines   | `fix(titlebar): add flex-grow:1 to title text so it fills remaining space` |
| 2        | Bug 8  | ~15 lines | `fix(titlebar): read title text color from SystemStyle` |
| 3        | Bug 9  | ~5 lines  | `fix(titlebar): apply padding_horizontal from TitlebarMetrics` |
| 4        | Bug 11 | investigate | `fix(debug): synthesize full click sequence in debug API` |
| 5        | Bug 12 | ~10 lines | `refactor(titlebar): use display:block instead of flex` |
| 6        | Bug 10 | deferred  | `chore(system): add version comment to macOS titlebar constants` |
