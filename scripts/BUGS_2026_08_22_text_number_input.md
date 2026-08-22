# NumberInput is 1.5× taller than TextInput, hello-world's counter floats — and a focused, empty TextInput shows no caret

Written 2026-08-22 on worktree `debug-slider-scroll-2026-08-22` (branch
`fix/open-bugs-wave-2026-08-22`). Read-only investigation: no source was
edited, nothing was built or run. Every line number is from this worktree.
All pixel numbers come from measuring the user's screenshot with PIL
(column/row scans, see §2) — not from estimates.

## Symptoms (user, AzWidgets + hello-world demos, macOS Retina)

> "Text Input still not properly working?"

> "Number Input too large (vertically) compared to text input although they
> share the same code? — Also noticed this problem on the 'hello world'
> [demo]: affects numbers only for some reason?"

Screenshot (514×372 device px, 2× Retina — the 1 px widget border is exactly
2 device rows): section title "Inputs", caption "TextInput", an EMPTY field
with a blue (focused) border, caption "NumberInput", a field with a grey
border containing "42" at the left. The NumberInput box is visibly taller
than the TextInput box; the "42" sits vertically centred in it. The focused
TextInput shows **no caret and no placeholder** — its interior has zero
non-white pixels.

## Status

| # | Symptom | Verdict |
|---|---|---|
| 1 | NumberInput taller than TextInput | **CONFIRMED, root-caused.** Not digits, not font fallback: the widget's value `<p>` inherits the UA stylesheet's `p { margin: 1em 0 }` (`core/src/ua_css.rs:584-585`) and `TEXT_INPUT_LABEL_PROPS` never zeroes it (`layout/src/widgets/text_input.rs:481-493`). TextInput in the demo is EMPTY (its `<p>` has no line box, only the two 11 px margins → 26 px); NumberInput holds "42" (one 13 px line + the same margins → 39 px). The numbers match the screenshot to ±0.5 px (§2). Regression from `c70c25a04` (2026-08-18), which turned the bare text node into `<p>` blocks. |
| 2 | hello-world "affects numbers only" | **CONFIRMED, same root cause.** `33c853d20` (2026-08-21) changed the counter from `AzDom_createTextDoNotUseWithoutBlockLevelWrapper` to `AzDom_createPWithText` (`examples/c/hello-world.c:61`); the `<p>` inherits `font-size: 32px` from its wrapper (`:64`) so it gets **32 px of margin above and below** the digit. It is "numbers only" because the counter is the only `<p>` in the example — the button label is a different box. The guide figure `doc/guide/en/screenshots/hello-world.png` (re-rendered 2026-08-20) already shows the floating "5". |
| 3 | "Text Input still not properly working?" | **PARTIALLY CONFIRMED — two concrete defects + the height bug above.** (a) A focused EMPTY TextInput paints **no caret**: `UnifiedLayout::get_cursor_rect` returns `None` when the layout has no clusters (`layout/src/text3/cache.rs:5436-5498`) and the caret painter then `continue`s (`layout/src/solver3/display_list.rs:3445-3447`). (b) The placeholder is hidden the moment focus arrives (`text_input.rs:1146-1149`) — by design, but together with (a) a focused empty field looks dead, exactly what the screenshot shows. (c) Because of #1 the field **grows from 26 px to 39 px on the first keystroke** and shrinks back when emptied. Typing, IME, Backspace/Delete, arrows, Home/End, shift/drag selection, Cmd+C/V/X and the caret blink are all wired and covered by tests (§5). |

---

## 1. The two widgets really share the code — so the text is the only variable

`NumberInput::dom()` formats the number into `text_input.text_input_state.inner.text`,
installs its validator and focus-lost hook, and returns
`self.text_input.dom()` (`layout/src/widgets/number_input.rs:268-283`).
There is no stepper, no wrapper, no extra line. `TextInput::dom()`
(`text_input.rs:897-1027`) builds, on every platform:

```
div.__azul-native-text-input-container      position:relative; padding 2/1; border 1px; overflow hidden;
│                                           contenteditable=true; tab_index Auto   (macOS props :325-422)
├── p.__azul-native-text-input-placeholder   position:absolute; top 2px; left 2px; font-size 11px;
│   └── Text(placeholder)                    contenteditable=false                  (:538-549)
└── p.__azul-native-text-input-label         display:block; white-space:pre; font-size 11px;
    └── Text(value)                          font-family system:ui                  (:481-493)
```

Neither `<p>` style list contains a `margin-*` property. The a11y additions
(`with_accessibility_info`, `:953-958`; `c5fcb87b7`) are metadata on the
container and create no node. The demo wraps each widget in
`labelled()` — a `display:flex; flex-direction:column` div with a 12 px bold
`<span>` caption (`examples/azul-widgets/src/lib.rs:59-77`), identical for
both widgets (`:108-115`).

So the only difference between the two rendered boxes is the text inside
the label `<p>`: `""` for `TextInput::create().with_placeholder(...)` versus
`"42"` for `NumberInput::create(42.0)`.

## 2. Root cause: the UA `p { margin: 1em 0 }` lands inside the widget

### The stylesheet

`core/src/ua_css.rs` gives every `<p>` Chrome's defaults:

```
584:        (NT::P, PT::MarginTop) => Some(&MARGIN_TOP_1EM),
585:        (NT::P, PT::MarginBottom) => Some(&MARGIN_BOTTOM_1EM),
```

(`MARGIN_TOP_1EM` = `PixelValue::const_em(1)`, `:247-257`; the rule dates
from `0f7010fe8`, 2025-11-17, and is pinned intentionally by
`layout/tests/h1_p_margin_collapse.rs:48-58`). The block-layout getter
consults it whenever the author set nothing
(`layout/src/solver3/getters.rs:653-661`, margins at `:3390-3412`), and
`LayoutTree::resolve_box_props` resolves the `em` against the ELEMENT's own
font size (`layout/src/solver3/layout_tree.rs:1555-1572`) — 11 px for the
widget's `<p>`, 32 px for hello-world's.

### The arithmetic, checked against the screenshot

Screenshot rows (device px, from a column scan at x=58/60/300):
TextInput border rows 131-132 and 181-182 → outer height **52 dev = 26.0 px**.
NumberInput border rows 257-258 and 332-333 → **77 dev = 38.5 px**.
"42" ink rows 288-303 → digit height **16 dev = 8 px**; baseline at
(304-257)/2 = **23.5 px** below the outer top edge.

| box | model | predicted | measured |
|---|---|---|---|
| TextInput (empty `<p>`, no line box) | border 1+1, padding 1+1, `<p>` margins 11+11, `<p>` height 0 | **26.0** | **26.0** |
| NumberInput ("42") | 4 + 11 + line 13.1 + 11 | **39.1** | **38.5** |
| "42" baseline | 1+1+11 + ascent 0.952×11 = 10.47 | **23.5** | **23.5** |
| "42" digit height | SF Pro / Helvetica Neue digits ≈ 0.70-0.73 em × 11 | 7.7-8.0 | **8.0** |

Line 13.1 px = `line-height: normal` = (ascent − descent + lineGap) × size /
upem (`layout/src/text3/cache.rs:132-143`, OS/2 typo metrics preferred,
`:2358-2385`): SF Pro (1950 + 494 + 0)/2048 = 1.193, Helvetica Neue
(952 + 213 + 28)/1000 = 1.193 → 13.1 px at 11 px either way.

Why the empty `<p>` keeps BOTH margins (22 px) instead of collapsing them
through (11 px): `is_empty_block` (`layout/src/solver3/fc.rs:9442-9472`)
treats a node with an `inline_layout_result` as non-empty, and an empty
text node still produces an (item-less) inline layout. So the BFC path
(`fc.rs:1782-1870`) places a 0 px block with 11 px above and below. That is
also why the TextInput looked "normal" — 26 px is coincidentally a plausible
single-line field height, while the same box with text in it is 39 px.

### Why "numbers only" is a red herring (the digit / font-fallback hypothesis)

- Font fallback is per-codepoint coverage (`split_text_by_font_coverage`,
  `layout/src/text3/cache.rs:8317`; `shape_with_font_fallback`, `:8448-8540`).
  The `system:ui` chain on macOS is "System Font" → "Helvetica Neue" →
  "Lucida Grande" → sans-serif/serif/monospace
  (`css/src/system.rs:1193-1200`, constants `:1084/:1118/:1120`;
  `build_font_selector_stack`, `getters.rs:3893-3960`). Every font in that
  chain covers ASCII digits, so "42" shapes as ONE segment — `AZ_FONT_FALLBACK_DEBUG=1`
  (`cache.rs:8469-8476`) prints a `[FONT FALLBACK] text needs N font segments`
  line only when a split happens.
- The measured digit height (8 px) and baseline (23.5 px) are exactly the
  primary font at 11 px; a fallback face with a larger ascent/descent would
  have moved the baseline.
- `macos/system_style.rs:278-292` reads `[NSFont systemFontOfSize:0].familyName`
  into `fonts.ui_font`; it is not on this path (the widget tables use the
  literal `"system:ui"`, `text_input.rs:79-83`, expanded by
  `SystemFontType::from_css_str`).
- Any text would do it: type "ab" into the TextInput and it grows to the
  same 39 px. The demo only ever shows a NON-EMPTY `<p>` inside a
  NumberInput, and hello-world's only `<p>` is the counter — hence the
  impression that digits are special.

### When it regressed

| commit | date | what it did | effect |
|---|---|---|---|
| `0513d01fd` | 2026-08-13 | Button + ribbon labels become `<p>`-wrapped text (`button.rs:504`) | first widgets exposed to the UA `<p>` margin (flex path, see §3) |
| `c70c25a04` | 2026-08-18 | TextInput value/placeholder: `display:inline-block` text → `<p>` blocks + `white-space:pre` | **this report's #1 and #3c** |
| `33c853d20` | 2026-08-21 | 223 call sites → `create_p_with_text`, incl. `examples/c/hello-world.c:61` | **#2** (and the 190 other C/C++/Python example sites) |

Before `c70c25a04` the value was a bare `NodeType::Text` with no box, so no
margin could apply. None of the `<p>`-wrapping commits added a margin reset,
and no test noticed because the real-layout harnesses start their stylesheet
with `* { margin: 0; padding: 0; }`
(`layout/tests/text_edit_seam_regressions.rs:28-31`).

## 3. Secondary finding: `em` margins in flex items resolve against 16 px, not the element

The flex/grid bridge builds the taffy style from the same getters but
converts lengths with `pixel_value_to_pixels_fallback`
(`layout/src/solver3/taffy_bridge.rs:46-58`):

```
54:        SizeMetric::Em | SizeMetric::Rem => Some(pv.number.get() * DEFAULT_FONT_SIZE),
```

(`DEFAULT_FONT_SIZE` = 16, `css/src/props/basic/pixel.rs:24`). Margins,
padding, border and inset all go through it (`:514-531`, `:806-828`),
although the element's real font size is computed three lines earlier and
used only for width/height `calc()` (`:714-721`). So a `<p>` label that is a
flex ITEM — the Button's (`button.rs:504`, container `display:inline-flex`,
`:209-216`), the ribbon's, the menubar's — gets **16 px top and bottom
margin regardless of its 14 px font**. Predicted Button height: 6 + 16 +
16.7 + 16 + 6 ≈ 61 px instead of ≈ 29 px. UNVERIFIED (nothing was run); it
is the same family of bug and the fix in §7-C is one line, so it is worth a
measurement before the next release.

## 4. hello-world

`examples/c/hello-world.c:58-66`: the counter text goes into
`AzDom_createPWithText` inside a `div` with `font-size: 32px`. The `<p>`
inherits 32 px, so the UA margin is 32 px above and 32 px below a 38 px line
— the digit occupies a 102 px band. The `Button` next to it carries its own
`<p>` (§3). The guide's own figure reproduces it: `doc/guide/en/hello-world.md:31-36`
renders `<p style="font-size: 50px;">5</p>` and the PNG shows the "5" ink
starting 62 logical px below the window top (= the 50 px `<p>` margin that
escaped through `<body>`'s 8 px margin, plus ascent − digit height), with the
button 170 px down. In a browser this HTML would look the same — the
engine is correct here; the EXAMPLE changed shape. The counter can use
`AzDom_createDivWithText` (in the API since `794aef237`, `api.json:18022`;
`core/src/dom.rs:5825`) or the `<p>` can get `margin: 0`.

## 5. TextInput: what works, what does not

### Works (with the evidence)

| feature | path | coverage |
|---|---|---|
| focus on click | mouse-down climbs from the deepest hit node to the nearest `is_focusable()` ancestor — the container — so clicking the `contenteditable=false` placeholder or the 0 px label still focuses the field (`dll/src/desktop/shell2/common/event.rs:7754-7800`) | `layout/tests/text_edit_seam_regressions.rs:250` |
| caret seeding | `finalize_pending_focus_changes` seeds the last cluster, or `(run 0, byte 0)` for an empty node (`layout/src/window.rs:6863-6969`, fallback `:6956`) | `layout/tests/text_edit_seam_regressions.rs:657` |
| typing (macOS) | `keyDown:` → `inputContext.handleEvent` → `insertText:` → `handle_text_input` → `CallbackChange::CreateTextInput`; if the IME does not consume, `handle_key_down` stages the char with `record_text_input` BEFORE the pass so a `VirtualKeyDown` callback can `prevent_default()` (`macos/mod.rs:1009-1080`; `macos/events.rs:640-735`, `:747-800`) | `text_edit_seam_regressions.rs:693/710`, `caret_follows_typing.rs` |
| IME preedit | `setMarkedText:` → `set_preedit` (`macos/mod.rs:1324-1400`); composition never enters the text store | `text_edit_seam_regressions.rs:186/201/219/589/729` |
| Backspace / Delete / arrows / Home / End / Enter | engine default actions (`layout/src/default_actions.rs:90-270`); the widget vetoes Enter in a single-line field (`text_input.rs:1330-1340`) and `\n` in pastes (`:1244-1247`) | `default_actions.rs` tests, `:420-533` |
| selection (shift, drag, double-click) | `process_mouse_click_for_selection`, `TextSelectionDrag` from the default input interpreter (`event.rs:7377-7400`) | `text_edit_seam_regressions.rs:326/350/389/422` |
| Cmd+C / V / X / A | default interpreter, primary modifier = Cmd on macOS (`core/src/events.rs:4109-4126`); `ClipboardManager` paste/copy flows (`layout/src/managers/clipboard.rs:1-60`) | `core/src/events.rs:4238` |
| caret blink | `CURSOR_BLINK_TIMER_ID`, on immediately after focus | `e2e/bug-caret-off-after-focus.json` |
| placeholder show/hide | hidden on focus (`:1146-1149`) and on first accepted insert (`:1275-1276`); shown again on focus-lost-empty (`:1173-1175`) and delete-to-empty (`:1223-1224`) | `text_input.rs` unit tests (`:1355-`) |
| callbacks | `on_text_input` veto via `prevent_default` (`:1280-1285`), `on_virtual_key_down` (`:1294-1345`), `on_focus_lost` (`:1159-1189`) | `number_input.rs:1214-1260` |

### Does not work / gaps

1. **No caret in an empty field** (`display_list.rs:3445-3447` ← `cache.rs:5436-5498`).
   The editing session exists (keystrokes land), but nothing is painted
   until the first character arrives. No test covers an empty editable: the
   caret scenarios all mount text (`e2e/bug-caret-off-after-focus.json`).
2. **Focused + empty = blank box.** The placeholder is hidden on focus
   (`:1146-1149`, design choice; browsers keep it until the first
   character). Combined with (1) the user sees nothing at all — the
   screenshot's TextInput.
3. **26 → 39 px height jump** on the first keystroke and back on
   delete-to-empty (§2). Any flex-column parent re-flows around it.
4. **NumberInput cannot be edited into a negative or fractional number from
   scratch.** `validate_text_input` parses the WHOLE buffer with
   `str::parse::<f32>` and vetoes the keystroke on failure
   (`number_input.rs:335-349`); `"-"`, `"+"`, `"."`, `"1e"` are all
   `Err`, and the table pins `"-"` and `"."` as "must be rejected"
   (`MALFORMED`, `:438-465`). Typing `-` into "42" (caret at 0) is vetoed;
   so is deleting "42" and typing `-5` (the `-` never lands), or `.5`.
5. **Dead fields.** `NumberInput::accessibility_name` (`:94`, `:135-138`;
   doc says "forwarded into the accessibility declaration it already
   builds") and `NumberInput::style` (`:89`) are never read in `dom()`
   (`:268-283`); the name the demo passes via `with_accessibility_name`
   only works because `labelled()` sets it on the returned Dom.
6. **macOS/mobile container has no `font-size`/`font-family`** (`:325-422`)
   while Windows' does (`:210`, `:281`); the container inherits the page
   font (the demo body is `sans-serif` at the 16 px initial size,
   `lib.rs:440`) and only the two `<p>` force 11 px `system:ui`. Harmless
   today, but anything measured on the container (caret style, future
   `min-height: 1lh`) would use the wrong font.
7. **No end-to-end test exercises the widget through a real layout.** The
   82 unit tests in `text_input.rs` build a `DomLayoutResult` with an empty
   `LayoutTree` (`:1596-1615`) — they check the handlers, not the box.

What "still not properly working" most plausibly means on macOS: the user
clicked the TextInput, saw the placeholder vanish and no caret appear (1+2),
possibly typed and watched the field jump (3). Key delivery itself is fine
— the IME/keyDown path has been exercised since `9d4926a74`
("keystrokes no longer lost").

## 6. NumberInput ≠ TextInput structurally? No.

Nothing in `NumberInput` adds a box: no stepper buttons, no column wrapper,
no extra line. `NumberInput::style` is unused. The a11y value is a string on
the container's `AccessibilityInfo`. The two containers differ only in
state (focused → blue `on_focus` border colour, `:419-436`) and content.

## 7. Proposed fixes

**A. Zero the UA margin on every widget-internal `<p>` (the actual bug).**
Add `margin-top/bottom/left/right: 0` to `TEXT_INPUT_LABEL_PROPS` and
`TEXT_INPUT_PLACEHOLDER_PROPS` (all three `cfg` variants, `text_input.rs:447-549`),
and `TEXT_AREA_LABEL_PROPS` / `_PLACEHOLDER_PROPS` (`text_area.rs:202/225`).
Better: one shared `P_RESET: &[CssPropertyWithConditions]` in
`layout/src/widgets/mod.rs` and a shape test that walks every widget's DOM
and asserts each `NodeType::P` carries an explicit margin-top and
margin-bottom — the `<p>` convention now spans button, label, text_area,
drop_down, combobox, date_picker, time_picker, ribbon, menubar, tooltip,
chip, avatar, accordion, breadcrumb, pagination, segmented, tabs, tree_view
(the `create_p` call sites), and none of them resets it.
The placeholder's `top: 2px` was tuned with the 11 px margin on top of it;
after the reset it should become `top: 1px` to sit on the label's line.

**B. Give the empty field a line.** With A alone an empty TextInput
collapses to 4 px (border + padding): the empty `<p>` has no line box.
Options, cheapest first: (i) `min-height` on the container like
`TextArea` already does (`text_area.rs:95`), e.g. 13 px for the 11 px font;
(ii) `min-height: 1lh`-equivalent — a `LayoutMinHeight` resolved from the
label's `line-height: normal` via `LineHeight::resolve_with_metrics`
(`cache.rs:146`); (iii) engine-level: an empty block that is an editing host
(or its first inline child) lays out one strut line, which is what makes
the caret in fix D trivial. (iii) is the browser-like one (Firefox's "bogus
`<br>`"); (i) unblocks the release.

**C. Resolve `em` in the flex bridge against the element.** In
`taffy_bridge.rs`, thread the already-computed `em_size`/`rem_size`
(`:714-721`) into `multi_value_to_lpa_margin` / `multi_value_to_lp` /
`pixel_to_lp` instead of `DEFAULT_FONT_SIZE` (`:46-58`). Also fixes every
author `margin: 0.5em` on a flex item. Pin with a test: a flex row with a
`<p style="font-size:10px">` item must get a 10 px UA margin, not 16.

**D. Paint a caret in an empty editable.** In the caret builder
(`display_list.rs:3438-3460`): when `get_cursor_rect` is `None` AND the
layout has no clusters, synthesise the rect at the content-box origin of
the IFC root with height = the node's `line-height: normal` (the strut the
IFC already computes, `cache.rs:2059-2068`) and width = `style.width`.
Alternatively let `get_cursor_rect` itself return that rect when `items`
is empty — then `window.rs:8970` (scroll-into-view) benefits too.

**E. Keep the placeholder visible while focused-and-empty (optional,
browser parity).** Drop the hide in `default_on_focus_received`
(`:1146-1149`); the insert path already hides it on the first accepted
character (`:1275-1276`) and the delete-to-empty path re-shows it
(`:1223-1224`).

**F. NumberInput: accept transient prefixes.** In `validate_text_input`
treat a buffer that is a PREFIX of a valid float (`-`, `+`, `.`, `-.`,
`1e`, `1e-`, trailing `.`) as `TextInputValid::Yes` with `number`
unchanged, and only reject buffers that can never become a number
(`"abc"`, `" 1"`). Update the `MALFORMED` table accordingly.

**G. hello-world and the examples.** Either switch the counter to
`AzDom_createDivWithText` (no UA margin; the commit's stated reason for
`<p>` — a bare text node competing as a flex item — does not apply inside
a block `div`) or keep `<p>` and add `margin: 0` via `AzDom_addCssProperty`.
The same decision applies to the other 190 example sites of `33c853d20`;
the guide fence (`doc/guide/en/hello-world.md:31-36`) should match the
shipped example once decided.

**H. Forward `NumberInput::accessibility_name` and `style`** in `dom()`,
or delete the fields.

## 8. How to verify

1. **Digits-vs-letters + margin pin (headless e2e, no widget code).** New
   `e2e/bug-p-ua-margin-in-text-field.json` modelled on
   `e2e/mock-font-exact-metrics.json` ("Azul Mock Mono": advance 0.5 em,
   ascent 0.8 em, descent 0.2 em → `line-height: normal` = 1.0 em exactly):
   ```
   mount: <div class="f" id="digits"><p>42</p></div>
          <div class="f" id="letters"><p>ab</p></div>
          <div class="f" id="empty"><p></p></div>
   css:   .f { padding: 1px; border: 1px solid black; width: 200px; }
          p  { font-family: "Azul Mock Mono"; font-size: 20px; }
   assert_layout #digits  height == #letters height   (64 = 4 + 20 + 20 + 20 today)
   assert_layout #empty   height == 44                (4 + 0 + 20 + 20 today)
   ```
   The first assertion proves "42" and "ab" are the same line box (kills the
   fallback theory); the 64/44 values document the UA margin. After fix A
   is applied to a widget-style rule set (`p { margin: 0 }` in the css)
   the expectations become 24 / 4 (then 24 / 24 with fix B-iii).
2. **Widget-level (Rust, real layout).** A test in
   `layout/tests/` using the `text_edit_seam_regressions.rs:50-70` harness
   but WITHOUT the `* { margin: 0 }` reset: lay out
   `TextInput::create().dom()` and `NumberInput::create(42.0).dom()` inside
   a 300 px column and assert the two container rects have the same height,
   equal to border + padding + one 11 px line (≈ 17 px with the system
   font; exact with a registered mock font via
   `FontManager::register_named_font`, `layout/tests/mock_font_metrics.rs:250`).
   Then type "x" into the TextInput through `record_text_input` +
   `apply_text_changeset` and assert the height did not change (#3c).
3. **Caret in an empty field (headless e2e).** Clone
   `e2e/bug-caret-off-after-focus.json` with `<div id="ed" contenteditable="true"></div>`
   (empty), `focus_node`, `wait_frame`, `get_cursor_state` →
   `has_cursor:true`, then `snapshot_frame` and `assert_changed` against a
   pre-focus snapshot with `min_damage_rects: 1` — fails today (nothing is
   painted), passes with fix D. Add `key_down {"key":"a","text":"a"}` +
   `assert_text "#ed" == "a"` to prove the session was live all along.
4. **Flex `em` margin (fix C).** `assert_layout` on a `display:flex` row
   containing `<p style="font-size:10px">x</p>`: the row's height must be
   10 (line) + 10 + 10 (margins), not 10 + 16 + 16.
5. **Font fallback sanity on the real machine.** Run the AzWidgets demo
   with `AZ_FONT_FALLBACK_DEBUG=1`; no `[FONT FALLBACK]` line may mention
   "42".
6. **Guide figure.** `azul-doc autoreview autodoc-screenshots` after G; the
   "5" should start ≈ 8 px below the top, not 62.

## 9. Effort

| item | estimate |
|---|---|
| A + placeholder `top` retune + shape test | 2 h |
| B-i (container `min-height`) | 0.5 h; B-iii (engine strut line for empty editing hosts) 0.5-1 d |
| C (flex bridge `em`) + test | 1-2 h |
| D (empty-field caret) + e2e | 3-4 h |
| E | 0.5 h |
| F + `MALFORMED` update | 1-2 h |
| G (hello-world + 190 example sites, mechanical) | 1-2 h |
| H | 0.5 h |

Release-blocking: A + B-i + D (the three things the user can see). C is a
strong candidate for the same PR because it is the same margin on every
Button.

## 10. Overlaps / related

- `scripts/BUGS_2026_08_22_tooltip_transient_popover.md` assumes "a
  `Button` is ~28-30 px tall" from the source; if §3 holds, the live button
  is ~61 px and the tooltip offset analysis there shifts accordingly.
- `0513d01fd` (ribbon `<p>` labels, 2026-08-13) and its
  `layout/tests/ribbon_tab_whitespace.rs` law measure `<p>` border-box
  heights, which exclude margins — so the 16 px flex margin of §3 would
  not have tripped it.
- `71177243e` (a11y names 12/12) added `NumberInput::accessibility_name`;
  gap #5 is the follow-up.
- The `<transient-window>` work on master and the slider/scroll items of
  this bugfix wave do not touch any of these files.
- Memory note `session_2026_08_21_lints_a11y.md` lists a "div-as-text"
  lint; the `<p>`-everywhere convention it enforced is what brought the UA
  margin into the widgets — the lint should stay, the widget `<p>`s need
  the reset (fix A), not a return to bare text.

File written: `scripts/BUGS_2026_08_22_text_number_input.md`
