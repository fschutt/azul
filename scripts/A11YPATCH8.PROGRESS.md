# A11YPATCH8 progress (branch wt/a11ypatch8, base 5745afee6)

Task: the accessibility tree sends patches (accesskit TreeUpdate with only changed / added / removed nodes) and
NOTHING when a frame changed nothing a11y-visible. Brief: scripts/waves/wave8/PLAN.md "A11YPATCH8".

## DONE
- (none yet)

## IN PROGRESS
- reading the a11y code, measuring the a11y cost per tick on the current build

## NEXT
- measure (AZ_PROFILE=cpu tick scenario, /Users/fschutt/Development/azul-work/lp8/)

## Decisions
- (none yet)

## Open questions
- (none yet)

## Measurement (2026-10-03, prebuilt AzWidgets of 0de2a2529, headless 900x1300, /Users/fschutt/Development/azul-work/a11yp8/)
- `a11y_update_tree` = 2880 / 2906 / 2956 / 3041 / 3179 / 3194 us per call (AZ_PROFILE=cpu, 3472-node page, 4 DOMs).
- Unprofiled: a knob tick (incremental_relayout) 19.9 - 21.7 ms, a no-op relayout 11.1 - 11.6 ms -> the a11y
  rebuild is ~15% of a tick and ~27% of a no-op relayout.
- It runs after EVERY layout pass (window.rs layout_and_generate_display_list_impl tail, `update_a11y_tree`); the
  lint printed right after it shows in every tick. Its span is missing from the tick tables only because spans
  closing after a relayout's last per-DOM flush never reach a [CPU] table (`shell_incremental_relayout`,
  `register_scroll_nodes` are missing the same way; a get_profile_report right after a tick drained nothing).
