# REFCI progress (reference renderers in CI)

Branch `wt/refci-reference-renderers` from `d1dd0b783`. Report: `scripts/REFCI_2026_09_30.md`.

## DONE
- `c72e1e7c1` progress file
- `b11e3c09d` scripts/refci/htmlnorm.py + vendor_wpt.py (WPT vendoring tooling)
- `9a9c1c4ab` tests/wpt/: 210 WPT reftests (upstream a6e66db3709b) + 28 local reftests + 743 editing cases
- `a365b0f13` layout/tests/wpt/ runner (reftest + editing), `wpt_tests` feature, `[[test]] wpt`

## IN PROGRESS
- Preliminary reftest sweep through the prebuilt AzWidgets debug server (scratchpad sweep.py) to seed
  tests/wpt/reftest_expectations.txt; finding: remounting a second document over a first leaks state
  (fixed in the sweep by `unmount` between mounts).
- Mail corpus vs Chrome: scripts/refci/cdp.py (pipe CDP), azul_debug.py (debug server client) done,
  tests/mail_corpus/ fetched; mail_boxes.py next.

## NEXT
1. Seed tests/wpt/reftest_expectations.txt (sweep) and editing_expectations.txt (UNSUPPORTED exact, rest FAIL).
2. mail_boxes.py + AzMail `azmail-sanitize` bin; run it on the corpus.
3. CI wiring in `.github/workflows/rust.yml` (wpt_reference job; optional mail_boxes job).
4. Report.

## Open questions
- none yet
