# SHEETS_ENGINE progress (fork of SHEETS; branch wt/sheets-engine from 06e80205d)

Resume: read this, then `git status`, `git log -3`, continue at NEXT. Spec:
/private/tmp/claude-501/-Users-fschutt-Development-azul/344f2a1f-485e-4b53-8631-15c97f8eeca1/scratchpad/sheets_engine_spec.md

## DONE
- Cargo.toml + engine.rs (trait, types, CellArea tests)

## NEXT (precise)
1. fake_engine.rs (FakeEngine + tests: SUM, undo/redo, insert_rows shifts)
2. ops.rs (+ tests against the fake)
3. worker.rs (+ tests), sample.rs (+ tests), storage.rs (+ tests with LocalDrive on a temp folder)
4. ironcalc_engine.rs (+ mapping tests + real-engine tests)
5. scripts/SHEETS_ENGINE_REPORT.md

## Deviations / notes
- DEFAULT_FONT_SIZE = 12 (IronCalc Font::default().sz), the spec's 13 was a placeholder.
- engine.rs also exports LAST_ROW / LAST_COLUMN (IronCalc's are crate-private).
