# AZMAIL_EXPLORATION progress

Branch `wt/x-mail-explore`, base `34a8fe46f`. Exploration only: report + RED tests that prove bugs.
Scratch (probes, samples, screenshots): session scratchpad `xmail/` (not committed; the report
lists what each probe showed).

## DONE
- `100923856` progress checkpoint.
- `a40976f60` RED `layout/tests/a_full_width_rule_in_a_spanning_table_cell_renders.rs`.
- `42d4a0885` progress.
- `c818942d2` RED `layout/src/telemetry/crash_mail.rs` `smtp_sink_tests` (4 tests: plaintext after
  STARTTLS, no MIME-Version, no dot-stuffing, bare LF).
- `a001cde2a` RED `layout/tests/a_linear_gradient_puts_its_colours_where_css_says.rs` (3 tests).
- Q1 measured (8 samples x 3 passes, headless AZ_E2E mount on the release dylib), Q2 crate facts
  (crates.io API), Q3 R2 / Email Service facts (Cloudflare docs), Q4 wire proof (Python STARTTLS
  replay + port reachability), Q5 editing stack survey.

## IN PROGRESS
- Writing `scripts/ideas/AZMAIL_EXPLORATION_2026_09_30.md`.

## NEXT
- Final report `scripts/AZMAIL_EXPLORATION_2026_09_29.md` (house-rule name).

## Open questions
- Headless screenshots paint text in the SYSTEM mode colour on a white canvas (dark Mac -> light
  grey text on white): not reproducible across machines.
- Headless runs whose `setup` shrinks the window below the app's size draw the first text line
  displaced to the right and smeared (seen at 360/420/500 px wide, not at 640/760). Harness or
  engine: not root-caused.
