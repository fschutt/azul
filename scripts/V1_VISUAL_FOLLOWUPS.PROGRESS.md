# V1 - visual follow-ups of the widget-theme wave (PROGRESS)

Branch `wt/v1-visual-followups`, cut from `0a326afe5`. Nothing compiled (house rule).

## Audit (2026-09-29, at 0a326afe5)

| # | Item | Status | Evidence |
|---|---|---|---|
| 1 | 4-sided box-shadow draws four stacked shadows | STILL OPEN | `css/src/props/property.rs` expands `box-shadow` into the four `-azul-box-shadow-{left,right,top,bottom}` slots with the SAME shadow; `layout/src/solver3/display_list.rs` (~6299) pushes one full `DisplayListItem::BoxShadow` per slot. Engine bug: the painter reads four slots as four shadows. W3a worked round it (`decl::shadow` writes the bottom slot only). |
| 2 | Spinner ring does not grow / shrink | STILL OPEN | `spinner.rs` draws a fixed 135-degree arc (`ARC_SWEEP_DEG`) that only rotates (800 ms). |
| 3 | Accordion chevron | STILL OPEN | `accordion.rs` TODO2: the header has NO disclosure indicator at all (title only). |
| 4 | Flora DARK focus ring #2F4A85 barely visible | PARTIAL | W3a/W3b widgets ring in `DARK_GLOW`; the shared `flora::FOCUS_BORDER_*_DARK` consts (buttons, text fields via `FIELD_BORDER_STATES`) still use `DARK_ACC` #2F4A85 (1.8:1 on --fl-sur #232323). |
| 5 | segmented / stepper / pagination / date_picker keep stale colours | STILL OPEN | `set_css_property` writes baked light-or-dark colours (user overrides outrank the cascade); mode twins: `segmented::window_is_dark`, `stepper::renders_dark`, `pagination::renders_dark`, `date_picker::window_is_dark` (also called by `text_input::paint_invalid_ring`). No runtime API replaces a node's conditional declarations. |

## DONE
- item 1: 09b31b32d RED (`layout/tests/a_box_shadow_paints_once.rs`), f042df374 fix (`getters::get_box_shadows`, painter paints each distinct slot shadow once)

- item 4: 7f001d54a RED (`flora.rs` `night_focus_ring_tests`, appended), 4f299a1e8 fix (`FOCUS_BORDER_*_DARK` = `DARK_GLOW`)

## IN PROGRESS
- item 3

## NEXT
3. item 3 (transform tween + chevron)
4. item 2 (arc sweep)
5. item 5 (engine: `CallbackInfo::set_node_inline_style`; widgets)

## Open questions
