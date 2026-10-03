# REFCI progress (reference renderers in CI)

Branch `wt/refci-reference-renderers` from `d1dd0b783`. Report: `scripts/REFCI_2026_09_30.md`.

## DONE
- `c72e1e7c1` progress file
- `b11e3c09d` scripts/refci/htmlnorm.py + vendor_wpt.py (WPT vendoring tooling)
- `9a9c1c4ab` tests/wpt/: 210 WPT reftests (upstream a6e66db3709b) + 28 local reftests + 743 editing cases
- `a365b0f13` layout/tests/wpt/ runner (reftest + editing), `wpt_tests` feature, `[[test]] wpt`
- `bc30f88db` progress
- `15a4ecc02` seeded tests/wpt/reftest_expectations.txt (sweep: 66 pass / 144 fail) + editing_expectations.txt
- `5d314c403` mail corpus vs Chrome: scripts/refci/{cdp,azul_debug,mail_boxes,fetch_mail_corpus}.py, tests/mail_corpus/, azmail-sanitize bin
- `3fee20fac` CI: wpt_reference job + on-demand mail_corpus_vs_chrome job
- `e1d723632` every WPT test page also in doc/working/ (azul-doc reftest vs Chrome), per the user's mid-task ask
- `44fa524fa` Chrome runs no scripts (the hostile mail hung CDP)
- `deff3a5e0` report + progress; Chrome emulates light mode
- mail corpus run: 18 mails, 874 mismatched boxes, first divergences in the report (section 5)
- `azul-doc reftest` run from this worktree (capped): 262 pages, 136 pass; wpt-*: 111/210 at the 0.5 % budget,
  with the false-pass caveat in the report (section 2.0)
- (last commit) fuzzy meta kept in the .xht pages; report sections 2.0 / 5 / 12 final

## IN PROGRESS
- nothing

## NEXT (for the parent)
1. `AZ_WPT_BLESS=1 cargo test --release -p azul-layout --features wpt_tests --test wpt`, commit the blessed lists.
2. `AZ_LINK_PATH=$PWD/target/azul-lib cargo build --release -p AzMail --bin azmail-sanitize`, then
   `python3 scripts/refci/mail_boxes.py` (needs Chrome + a prebuilt app with the debug server).
3. Decide the per-page budget for wpt-* pages in doc/src/reftest (report 2.0), add CI passes to doc/reftest_baseline.txt.

## Open questions
- Two reftest mechanisms (doc/working vs Chrome; layout/tests/wpt test-vs-reference): keep both (the report argues yes) or drop `reftest.rs`?
- An AzWidgets-hosted sweep scored 36 passes vs 66 with AzPaint on the same pages; a mount-over-mount probe on
  AzPaint was clean. Something in the mount op with a `<video>`/timer-bearing app DOM?
