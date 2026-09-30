# AZMAIL_EXPLORATION progress

Branch `wt/x-mail-explore`, base `34a8fe46f`. Exploration only: report + RED tests that prove bugs.
Scratch (probes, samples, screenshots): `$SCRATCH/xmail/` (session scratchpad, not committed).

## DONE
- `100923856` progress checkpoint.
- `a40976f60` RED `layout/tests/a_full_width_rule_in_a_spanning_table_cell_renders.rs`: `<td colspan=2><hr>` +
  a two-cell row panics the CPU renderer (reproduced headless on the release dylib via the E2E `mount` op).
- Q1 measured: 8 mail samples x 3 passes (raw / html5-normalized / legacy-rewritten) through
  AzWidgets headless `AZ_E2E` + `mount`; screenshots in scratch.
- Q5 editing stack surveyed (Explore agent; key lines to spot-check before the report).

## IN PROGRESS
- Q4 crash mail RED tests (local SMTP sink in the test; STARTTLS without the `tls` feature,
  no MIME-Version, no dot-stuffing).

## NEXT
- Q2 crates (versions, licenses, maintenance, supply-chain rules).
- Q3 mailbox worker + S3 auth design.
- Report `scripts/ideas/AZMAIL_EXPLORATION_2026_09_30.md`, final report `scripts/AZMAIL_EXPLORATION_2026_09_29.md`.

## Open questions
- Headless screenshots paint text in the SYSTEM mode colour on a white canvas (dark Mac -> light
  grey text on white): screenshots are not reproducible across machines. Note in the report.
