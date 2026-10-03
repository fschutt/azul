# MAILREF8 progress (wave 8, 2026-10-03)

Branch `wt/mailref8` from `45c6bf98b` (wave 7 integrated). Brief: scripts/waves/wave8/PLAN.md "MAILREF8".
Never compile; never touch layout/src/solver3/page_breaks.rs (nor display_list.rs: WPT8 owns it).

## DONE
- (none yet)

## IN PROGRESS
- step 1: re-measure the mail corpus on the prebuilt binaries (target/release AzPaint + azmail-sanitize,
  scripts/refci/mail_boxes.py, output target/refci/mail-wave8-base in the MAIN checkout's target).

## NEXT
- step 2: group the mismatches by root cause (largest first), write them down here.
- step 3: per group: probe (Chrome vs prebuilt), RED test in layout/tests/<sentence>.rs, fix, commit.

## Decisions
- (none yet)

## Open questions
- (none yet)
