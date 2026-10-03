# MAILREF8 progress (wave 8, 2026-10-03)

Branch `wt/mailref8` from `45c6bf98b` (wave 7 integrated). Brief: scripts/waves/wave8/PLAN.md "MAILREF8".
Never compile; never touch layout/src/solver3/page_breaks.rs (nor display_list.rs: WPT8 owns it).

## DONE
- step 1: BEFORE = 531 mismatched boxes (18 mails, 0 errors; Chrome 154, AzPaint + azmail-sanitize of
  45c6bf98b's build, 760x1100). Output /Users/fschutt/Development/azul/target/refci/mail-wave8-base.
  Command (from the worktree): AZUL_APP=<main>/target/release/AzPaint AZUL_LIB_DIR=<main>/target/azul-lib
  AZMAIL_SANITIZE=<main>/target/release/azmail-sanitize run_capped.sh --cap-mb 3500 --seconds 900 --log ..
  -- python3 scripts/refci/mail_boxes.py --out <main>/target/refci/mail-wave8-base
  Per mail: cerberus fluid 78, hybrid 165, responsive 154; 03_outlook 18; 04_receipt 16; leemunroe 1;
  mailgun billing 26; postmark invoice 32, receipt 41; all others 0.

## Groups (largest first; first guesses, to be confirmed by probes)
- A (~397, cerberus x3): AzMail's paper is `display:inline-block; height:100%`. Chrome puts it 14px
  low (y 22): its baseline is the preheader's (max-height:0; overflow:hidden -> bottom margin edge)
  because tables are SKIPPED when an inline-block looks for its last line box (Blink
  LayoutTable::InlineBlockBaseline = -1 / LayoutNG). azul: y 8. Also the paper's height (azul 682,
  Chrome 1054 = content): percentage height of an inline-block in an auto-height body = auto.
- B (~73, postmark invoice + receipt): one inner table (azr-54 / azr-44): tbody 183 wide, rows'
  cells stacked vertically.
- C (26, mailgun billing): a row 412 tall (Chrome 323) -> +90 below.
- D (18, 03_outlook_reply): <p> 17 tall vs 16 (font / line-height of the reply's paragraphs).
- E (16, 04_receipt): rows 31 vs 34, <hr> 1 vs 2 tall.
- F (1, leemunroe): an inline-block a x +4.

## NEXT
- step 3: per group: probe (Chrome vs prebuilt), RED test in layout/tests/<sentence>.rs, fix, commit.

## Decisions
- (none yet)

## Open questions
- (none yet)
