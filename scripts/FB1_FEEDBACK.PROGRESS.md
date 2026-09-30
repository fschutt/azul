# FB1_FEEDBACK progress (branch wt/fb1-user-feedback, base 66db869f1)

## DONE
- Item 3 AzCalendar light/dark: e9fbed1ed (RED test) + 0dc871134 (fix, app CSS only; Button variants
  checked, no engine bug).
- Item 4 AzBuilder mode sync: d6658bd14 (RED: layout/src/e2e/mode_ops_tests.rs + builder-mode-smoke.mjs
  21/27) + 98a488f8d (get_mode / set_mode / get_theme / set_theme ops, page follows the app, docs;
  smoke 27/27, other builder smokes unchanged).
- Items 1 / 2 harness gap: 2fa21e71e (RED) + 1023db90c (headless resize = the shells' fast path);
  a6f611432 (scripts/fb1/azmeet_resize_probe.py + references on the restyle path);
  8e56c4400 (differential layout test: fast path vs relayout, AzMeet lobby + devices panel).
- Report scripts/FB1_FEEDBACK_2026_09_30.md.
- Follow-up: 8e56c4400 red on the parent's build (3/3) -> 43c226479 engine fix
  (`NodeCache::final_layout_current`: a memoised final layout only while its subtree holds what it wrote).

## IN PROGRESS
- (none)

## NEXT (for the parent / a follow-up)
- Parent: build 43c226479, run the 3 differential tests (expect green), the probe --compare, and
  the resize perf numbers.
- Screenshot AzCalendar in both modes on the rebuilt binary.

## Open questions
- none
