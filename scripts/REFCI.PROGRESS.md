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

## IN PROGRESS
- Mail corpus run (Chrome now started with scripts disabled: the hostile sample's onerror=alert hung CDP);
  results go into the report's section 5.
- Report `scripts/REFCI_2026_09_30.md` written except section 5.

## NEXT
1. Fill section 5 from the mail run; commit cdp.py/mail_boxes.py fix + report + progress.
2. Validate the doc/working/wpt-*.xht in Chrome (parse as XHTML), then try `target/release/azul-doc reftest`
   through the capped runner from this worktree (resolve_project_root walks up from cwd).
3. Hand over: the parent runs `AZ_WPT_BLESS=1 cargo test --release -p azul-layout --features wpt_tests --test wpt`.

## Open questions
- Two reftest mechanisms (doc/working vs Chrome; layout/tests/wpt test-vs-reference): keep both or drop `reftest.rs`?
- An AzWidgets-hosted sweep scored 36 passes vs 66 with AzPaint on the same pages; a mount-over-mount probe on
  AzPaint was clean. Something in the mount op with a `<video>`/timer-bearing app DOM?
