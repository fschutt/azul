# R2-INPUT (PKG R2-INPUT-IO-TOOLING, wave 9 round 2) - progress

Branch wt/r2-input, base 440991077. Brief: scripts/waves/wave9/ROUND2.md "PKG R2-INPUT-IO-TOOLING" + D2.

## DONE
- I1 paste op: RED e546b77cb (veto test), GREEN 467f8f2a3 (CallbackChange::Paste + simulate_paste, DebugEvent::Paste,
  runner apply_paste, dll arm, OP_POLICY row, un-ignored the bold-HTML RED).
- I2 set_copy_content doc: 13cc33031 (no RED, a doc; PARENT refreshes api.json's doc).
- I3 request queue per thread under cfg(test): RED 8a28f4314, GREEN 721158323.
- I4 json / txt media types: RED 594b1fe9d, GREEN 811f73625.

## IN PROGRESS
- I5 lower_first dedup (7 copies) + autotest's Rust-file walker.

## NEXT
- I5, then D2 (focus_node delegates in full.rs, RED in runner.rs tests), then the report
  scripts/R2_INPUT_2026_10_05.md.

## Open questions
(none)
