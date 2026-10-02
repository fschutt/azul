# MAILHTML progress (wave 5, 2026-10-02)

Branch `wt/mailhtml` from `2e92c759b`. Task brief: scratchpad/wave5/MAILHTML.md.
Report: `scripts/MAILHTML_2026_10_02.md` (DONE, committed).

## DONE
- max-height clamps the content-based auto height: RED 624fd5d56, FIX f964430fe + cf6f441bd (cerberus x3, y +97)
- ex/ch = 0.5em: RED 5617786bb, FIX 3365a29db (02_gmail_reply blockquote x +33)
- Helvetica/Times/Courier browser ascent +15%: RED 2323e1fe0, FIX 2bdef8c6a (postmark x3 line heights)
- only <a href> underlined: RED f8a8e92c7, FIX 14da95e75 (postmark invoice "support team")
- unresolved markup <img> 0x0 (DEDUP A3.8): RED 75e791812, FIX 13cfea16c
- B20 sanitizer on Xml::create_from_html: RED 5ee033b34, REFACTOR 1ff01f689 (~360 lines gone)
- !important no longer invalidates a declaration: RED f6a9d6aff, FIX bbeafd052
- strut from the first available font (A, D, gap/2 each): RED 79a8cda4a, FIX f3112215b (gmail/apple blank lines)
- presentational attributes pass through to core's hints; renamed elements keep meaning as style:
  RED a5fe411d1, FEAT 9f45430c0
- report scripts/MAILHTML_2026_10_02.md (sections 1-8)

## IN PROGRESS
- nothing

## NEXT
- nothing for this wave; wave-6 items are in the report, section 8

## Decisions
- percentage height under an auto-height parent: NOT changed this wave (49 app/widget `height: 100%`
  sites, cannot verify without compiling); documented in the report.
- `ex`/`ch` as 0.5em at parse time (CSS Values 4 fallback) instead of a new SizeMetric (FFI enum).
- `!important`: flag stripped, precedence not modelled (priority >= INLINE is node-only scoped).
- the max-height clamp skips table boxes (CSS 2.2 17.5.3 undefined, Chrome ignores) and inline boxes.
- presentational attributes: passed through to core's presentational_css (one generator) instead of
  the sanitizer's own partial CSS mapping; body/font/img keep a sanitizer conversion (renamed or no
  engine hint).
- an attribute-less <body> is unwrapped (the tree builder implies one for every fragment).
- FOR TABLES (not edited): see report section 2 "For TABLES" (5 items).
- AzMail policy observation for the user: classes dropped but class rules kept (report section 8).
