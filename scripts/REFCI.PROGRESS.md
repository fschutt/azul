# REFCI progress (reference renderers in CI)

Branch `wt/refci-reference-renderers` from `d1dd0b783`. Report: `scripts/REFCI_2026_09_30.md`.

## DONE
- (none yet)

## IN PROGRESS
- WPT vendoring: `scripts/refci/htmlnorm.py`, `scripts/refci/vendor_wpt.py`, `tests/wpt/selection.txt`.

## NEXT
1. Vendor the curated WPT reftest subset + editing data into `tests/wpt/`.
2. Preliminary sweep through the prebuilt AzWidgets debug server; seed `tests/wpt/reftest_expectations.txt`.
3. Rust runner `layout/tests/wpt/` (harness = false, `required-features = ["wpt_tests"]`): reftests + editing.
4. Mail corpus vs Chrome: `scripts/refci/mail_boxes.py`, corpus in `tests/mail_corpus/`, AzMail sanitizer bin.
5. CI wiring in `.github/workflows/rust.yml`.
6. Report.

## Open questions
- none yet
