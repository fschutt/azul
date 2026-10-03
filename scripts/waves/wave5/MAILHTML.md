Task MAILHTML (azul Rust GUI toolkit, PR #476, wave 5). Read the house rules first and follow them exactly (never compile):
scripts/waves/wave5/house_rules.md
Your branch: `wt/mailhtml` from base commit `2e92c759b` (`git -C <your worktree> checkout -b wt/mailhtml 2e92c759b`). TASK name: `MAILHTML`.
Other wave-5 agents work in parallel on: MAILHTML (mail HTML/CSS parity, AzMail's html.rs), TABLES (table layout),
TEXTENG (line-height, inline-blocks in spans, text-edit formats), APIEXPORT (api exports + ctrl||meta sites),
HYGIENE (macro paths, autofix, helper twins), RTE (shared rich-text editor; AzNotes/AzMail compose), BLOCKS
(selection model, undo stack, switcher merge, preset shells), PIM (azul-pim crate; Calendar/Tasks/Contacts).
Stay in your area; where you must touch another's file keep the edit minimal and list it in your report.

GOAL: AzMail shows real-world HTML mail like Chrome does (the user: "get closer to Chrome parity for mail").
The measuring tool is `scripts/refci/mail_boxes.py` (corpus `tests/mail_corpus/`, SOURCES.tsv; azul vs headless Chrome,
box by box; read its docstring and scripts/REFCI_2026_09_30.md, scripts/R1_MAIL_RENDER_2026_09_30.md,
scripts/MAILVIEW_2026_09_30.md). The parent runs it (it needs prebuilt binaries + Chrome) - the baseline numbers are
at the end of this brief; write down which mails/boxes each fix should move, the parent re-measures at integration.
Work from the corpus: for each mail, find the FIRST diverging box (the tool reports it - the baseline's index.html
files are listed below), root-cause it in the engine, RED test in `layout/tests/<sentence>.rs` with the minimal HTML
(no network, no fonts beyond what the existing tests use), fix. Expected classes (verify, do not assume):
legacy presentational HTML outside tables (`center`, `font` size/face/color, `bgcolor`/`background`, `align` on
img/p/div, `hspace`/`vspace`, `border` on img, `width`/`height` attributes on img), `<img>` that is not fetched
(AzMail blocks remote images until "download images": DEDUP_EDITORS item - an unfetched img without a size leaves
an empty 300x150 hole; the root cause is the engine's size fallback `layout/src/solver3/sizing.rs` ~609-624 - Chrome
shows a broken/blocked image with no size as its alt text inline, or 0x0 without alt; match Chrome), the
`display:none` preheader, `@media (max-width: ...)` rules, `!important`, `mso-*` and unknown properties ignored
without dropping their rule, CSS resets, `line-height` (pt now works), web fonts falling back.
Also DEDUP_EDITORS: AzMail's ~600-line HTML tokenizer in `examples/azul-mail/src/html.rs` duplicates the lenient
parser exported as `Xml::create_from_html` (core/src/xml.rs): replace it by the shared parser (keep the sanitizer's
POLICY in AzMail - what is dropped/kept - but parse once, with the engine's parser), RED tests first for the
sanitizer's behaviour so nothing regresses. Tables are TABLES' (do not edit table layout code; if a mail's first
divergence is a table, note it for TABLES in your report).
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

Report `scripts/MAILHTML_2026_10_02.md` per the house rules (what was built, commits, api.json list, least-sure-to-compile
spots, the parent's test commands, what is left for wave 6). You are running unattended: decide, note decisions in
your progress file, continue; do not stop to ask. Do not spawn subagents.
