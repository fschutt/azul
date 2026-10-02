# MAILHTML progress (wave 5, 2026-10-02)

Branch `wt/mailhtml` from `2e92c759b`. Task brief: scratchpad/wave5/MAILHTML.md.

## DONE
- max-height clamps the content-based auto height: RED 624fd5d56, FIX f964430fe (cerberus x3, y +97)
- ex/ch = 0.5em: RED 5617786bb, FIX 3365a29db (02_gmail_reply blockquote x +33)
- Helvetica/Times/Courier browser ascent +15%: RED 2323e1fe0, FIX 2bdef8c6a (postmark x3 line heights)
- only <a href> underlined: RED f8a8e92c7, FIX 14da95e75 (postmark invoice "support team")
- unresolved markup <img> 0x0 (DEDUP A3.8): RED 75e791812, FIX 13cfea16c
- B20 sanitizer on Xml::create_from_html: RED 5ee033b34, REFACTOR 1ff01f689 (~360 lines gone)

## IN PROGRESS
- deciding: strut metrics / percentage heights

## NEXT
- per mail: first diverging box -> root cause -> RED test in layout/tests/<sentence>.rs -> fix
- strut from the real first-available font (gmail/apple `<div><br></div>` 16 vs 18 px)
- percentage height under auto-height parent (decide)

## Decisions
- percentage height under an auto-height parent: azul resolves it against the inherited available
  height (fc.rs layout_bfc `children_containing_block_size`, height_is_auto branch), Chrome treats it as
  auto (CSS 2.2 10.5). Hits mailgun/postmark/cerberus azr-1/2 (`body {height:100%}` mapped onto the
  paper). NOT changed yet: 49 `height: 100%` sites in apps/widgets may rely on it; decide after the rest.
- Helvetica line-height normal: Chrome (and Safari) on macOS add 15% of (ascent+descent) to the ascent
  of Times/Helvetica/Courier (Blink simple_font_data_mac); azul uses the font's hhea (1.0 for Helvetica)
  -> postmark text lines 16 vs 18 px. Candidate, later.
- Corpus triage (first divergences): see the report draft section.
- FOR TABLES (not mine, do not edit table code): (1) `cell_is_inline_formatting_context` (fc.rs) needs
  loose text: `<td><span>x</span></td>` / `<td><a style=display:inline-block>` take the block branch, the
  span/a becomes a left-aligned block box, td text-align ignored (receipt prices, postmark button whose
  white label paints at the left edge = invisible); (2) receipt outer td: inner table y +8 (cell content
  measured without the p's margins? vertical-align middle offset); (3) mailgun `<td style=display:block>`
  inside a tr: no box at all (19-40 missing boxes per mailgun mail) - CSS 2.2 17.2.1 anonymous cell;
  (4) cerberus hybrid/responsive `width:100%` table 1081 wide in a 680 max-width div (min-content too big);
  (5) newsletter/leemunroe: auto table around an inline-block button is 43-48px too narrow (inline-block
  padding missing from the cell's max-content).
- AzMail policy observation (not changed, for the user): the sanitizer drops `class`/`id` but keeps the
  mail's class-based `<style>` rules (scoped) - they can never match. Keeping classes needs prefixing
  (a mail must not reach the app's own class names). Decision for the user.
