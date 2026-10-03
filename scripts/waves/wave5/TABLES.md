Task TABLES (azul Rust GUI toolkit, PR #476, wave 5). Read the house rules first and follow them exactly (never compile):
scripts/waves/wave5/house_rules.md
Your branch: `wt/tables` from base commit `2e92c759b` (`git -C <your worktree> checkout -b wt/tables 2e92c759b`). TASK name: `TABLES`.
Other wave-5 agents work in parallel on: MAILHTML (mail HTML/CSS parity, AzMail's html.rs), TABLES (table layout),
TEXTENG (line-height, inline-blocks in spans, text-edit formats), APIEXPORT (api exports + ctrl||meta sites),
HYGIENE (macro paths, autofix, helper twins), RTE (shared rich-text editor; AzNotes/AzMail compose), BLOCKS
(selection model, undo stack, switcher merge, preset shells), PIM (azul-pim crate; Calendar/Tasks/Contacts).
Stay in your area; where you must touch another's file keep the edit minimal and list it in your report.

GOAL: table layout at Chrome parity (the plan's TABLE-C; read scripts/TABLE_A_2026_10_01.md and
scripts/TABLE_B_2026_10_01.md first - what they fixed and left). Sources of RED tests: WPT css/CSS2/tables
(tests/wpt + the expectations file - burn it down), the mail corpus newsletters (scripts/refci/mail_boxes.py; the
parent runs it, baseline below), and these known items:
1. DEDUP_WIDGETS_API: commit 407cc8c98 loosened `layout/tests/a_narrow_table_wraps_its_cells_to_fit.rs` from >= 3
   to >= 2 baselines. Restore the real assertion: each cell on exactly two baselines, both cells sharing them, and
   the cell widths matching the CSS Tables 3 / CSS 2.1 17.5.2.2 auto-layout split (measure min/max in the test),
   then fix the engine until it holds.
2. 56b105f60 rewrote two table tests to avoid an inline-block min-content bug WITHOUT a RED test for that bug:
   find it in that commit's diff, write the RED test, fix the bug, and restore the two tests' original intent.
3. Then newsletters end to end: nested tables, colspan/rowspan in the column sizing, percentage + fixed + auto
   columns mixed, `table-layout: fixed`, border-collapse widths, vertical-align, caption sides, empty cells,
   `width` attributes on td/table, tables inside inline formatting contexts.
Files: the table code in `layout/src/solver3/` (fc.rs table functions, sizing.rs table parts, table helpers) and
layout/tests. MAILHTML owns non-table mail HTML; TEXTENG owns inline-blocks in spans.
Baseline: measured by the parent at 2e92c759b (Chrome 154, 760x1100, AzMail sanitizer): 625 mismatched boxes in 18 mails (874 before TABLE-A/B). Per mail (first diverging box = usually the root cause):
  cerberus/cerberus-fluid.html                   68 boxes,  67 mismatched (0 missing); first: <div> azr-2 height +115
  cerberus/cerberus-hybrid.html                 145 boxes, 143 mismatched (0 missing); first: <div> azr-1 height -634
  cerberus/cerberus-responsive.html             131 boxes, 129 mismatched (0 missing); first: <div> azr-1 height -666
  exploration/01_newsletter.html                 25 boxes,   6 mismatched (0 missing); first: <table> azr-31 width -48, height +25
  exploration/02_gmail_reply.html                21 boxes,  16 mismatched (0 missing); first: <div> azr-1 height +43
  exploration/03_outlook_reply.html              25 boxes,   0 mismatched (0 missing); first: -
  exploration/04_receipt.html                    30 boxes,  25 mismatched (0 missing); first: <table> azr-6 y +8
  exploration/05_apple_mail_reply.html           10 boxes,   3 mismatched (0 missing); first: <div> azr-13 y -5
  exploration/06_thunderbird_reply.html           9 boxes,   0 mismatched (0 missing); first: -
  exploration/07_hostile.html                     7 boxes,   0 mismatched (0 missing); first: -
  exploration/08_legacy_uppercase.html           17 boxes,   0 mismatched (0 missing); first: -
  leemunroe/email-inlined.html                   30 boxes,   1 mismatched (0 missing); first: <table> azr-18 width -43
  mailgun/action.html                            25 boxes,  25 mismatched (19 missing); first: <div> azr-1 height +694
  mailgun/alert.html                             27 boxes,  27 mismatched (21 missing); first: <div> azr-1 height +630
  mailgun/billing.html                           46 boxes,  46 mismatched (40 missing); first: <div> azr-1 height +305
  postmark/invoice.html                          66 boxes,  49 mismatched (0 missing); first: <p> azr-18 y -5
  postmark/receipt.html                          68 boxes,  49 mismatched (0 missing); first: <div> azr-1 height -271
  postmark/welcome.html                          55 boxes,  39 mismatched (0 missing); first: <div> azr-1 height -133
mail_boxes: 18 mails, 625 mismatched boxes, 0 errors in 13s; report target/refci/mail-baseline-2e92c759b/index.html
Full report (read-only, in the MAIN checkout): /Users/fschutt/Development/azul/target/refci/mail-baseline-2e92c759b/index.html, summary.tsv and <mail>/boxes.json (both engines' rects per azr-<n> element). The corpus HTML is tests/mail_corpus/ in your worktree. Acceptance: the count goes down at the parent's re-measure, no mail gets worse.

Report `scripts/TABLES_2026_10_02.md` per the house rules (what was built, commits, api.json list, least-sure-to-compile
spots, the parent's test commands, what is left for wave 6). You are running unattended: decide, note decisions in
your progress file, continue; do not stop to ask. Do not spawn subagents.
