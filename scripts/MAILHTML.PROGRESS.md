# MAILHTML progress (wave 5, 2026-10-02)

Branch `wt/mailhtml` from `2e92c759b`. Task brief: scratchpad/wave5/MAILHTML.md.

## DONE
- max-height clamps the content-based auto height: RED 624fd5d56, FIX f964430fe (cerberus x3, y +97)

## IN PROGRESS
- `ex` unit (gmail reply blockquote `margin: 0 0 0 .8ex`)

## NEXT
- per mail: first diverging box -> root cause -> RED test in layout/tests/<sentence>.rs -> fix
- DEDUP_EDITORS B20: AzMail html.rs tokenizer -> Xml::create_from_html (RED sanitizer tests first)
- DEDUP_EDITORS A3.8: unfetched img 300x150 hole (sizing.rs)

## Decisions
- percentage height under an auto-height parent: azul resolves it against the inherited available
  height (fc.rs layout_bfc `children_containing_block_size`, height_is_auto branch), Chrome treats it as
  auto (CSS 2.2 10.5). Hits mailgun/postmark/cerberus azr-1/2 (`body {height:100%}` mapped onto the
  paper). NOT changed yet: 49 `height: 100%` sites in apps/widgets may rely on it; decide after the rest.
- Helvetica line-height normal: Chrome (and Safari) on macOS add 15% of (ascent+descent) to the ascent
  of Times/Helvetica/Courier (Blink simple_font_data_mac); azul uses the font's hhea (1.0 for Helvetica)
  -> postmark text lines 16 vs 18 px. Candidate, later.
- Corpus triage (first divergences): see the report draft section.
