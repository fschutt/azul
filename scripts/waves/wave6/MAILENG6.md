Task MAILENG6 (azul Rust GUI toolkit, PR #476, wave 6). Read first, in this order, and follow them exactly (never compile):
1. scripts/waves/house_rules.md (the house rules - wave-6 version: building blocks, user rulings, API gotchas)
2. scripts/waves/wave6/PLAN.md (the eleven tasks, who owns which files, the contracts between tasks)
Your branch: `wt/maileng6` from the base commit named in your launch prompt (`git -C <your worktree> checkout -b wt/maileng6 <base>`).
TASK name: `MAILENG6`. Report `scripts/MAILENG6_2026_10_03.md`, progress `scripts/MAILENG6.PROGRESS.md` (commit it after every commit).

GOAL: mail HTML at Chrome parity - the engine leftovers of MAILHTML and TABLES, each RED first, measured on the mail
corpus (scripts/refci/mail_boxes.py: baseline 625 mismatched boxes at 2e92c759b; the parent re-measures wave 5 and
wave 6 - you may run it yourself: it needs target/release/azmail-sanitize + AzPaint (prebuilt) + Chrome, through
scripts/waves/tools/run_capped.sh --cap-mb 3500). Read scripts/MAILHTML_2026_10_02.md, TABLES_2026_10_02.md,
TEXTENG_2026_10_02.md, REFCI_2026_09_30.md first. Items (verify each against Chrome first):
1. THE FONT BUG (TABLES' ignored test): a text run next to a bold/italic element is DROPPED (no glyphs, no width, no
   break point) unless some other regular text in the page loads its font - a font-loading / fallback bug, root cause it.
2. Percentage heights inside an auto-height parent (`body { height: 100% }` makes a mail's background stop one screen
   down): CSS 2.1 10.5 - but 49 `height: 100%` uses in apps/widgets may depend on today's behaviour: grep them, run the
   affected apps (capped) before and after, list what changes.
3. Chrome rounds font metrics (a 16px Arial line is 18px in Chrome, 18.4px in azul) - the largest remaining drift.
4. `line-height: 20px` lines stack at 20.78px; any line-height anywhere makes every unset line 1.2em (MAILHTML).
5. Tables: `<td style="display:block">` gets no box (mailgun 19-40 missing boxes), span/inline-block-only cells ignore
   text-align (Postmark's invisible button label), the receipt's outer cell -8px, RTL + border-collapse borders.
6. inline-flex / inline-grid / inline-table and <img> inside spans are not measured as boxes (TEXTENG); the strut's
   x-height / cap-height from the face's OS/2 metrics.
7. MAILHTML's Helvetica/Times/Courier "+15% ascent" list (layout/src/font.rs browser_compat_ascent): check whether it
   is a name list standing in for a metrics rule (hhea vs OS/2 typo metrics, USE_TYPO_METRICS); keep it only if it
   is exactly what Chrome/WebKit do.
Files: layout/src/solver3 (NOT page_breaks.rs), layout/src/text3, layout/src/font*.rs, css; not widgets, not e2e.

Report `scripts/MAILENG6_2026_10_03.md` per the house rules (what was built, commits, the api.json list - exact methods,
never `Type.*`; least-sure-to-compile spots; the parent's test commands; what you saw broken and did not fix; what is
left). You are running unattended: decide, note decisions in your progress file, continue; do not stop to ask. Do not
spawn subagents.
