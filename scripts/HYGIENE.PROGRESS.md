# HYGIENE progress (wave 5, 2026-10-02) - branch wt/hygiene from 2e92c759b

Brief: scratchpad/wave5/HYGIENE.md. Findings: scripts/DEDUP_WIDGETS_API_2026_10_02.md F1, F3, F4, F5, F17, F26, F31.

## DONE
- Item 1 (F1): 0a98cab8f - $crate:: inner macros, RefAny qualified, 78 hand imports dropped (page_breaks.rs left; its import is now unused -> parent drops it).

- Item 2 (F17): 7a71b84b9 RED, 539e896d3 GREEN - is_vec_family(), vecslice not structural; 14 types listed in the commit.
- Item 3a (F3): c32fbb52b - CoreCallbackData::create(event, refany, cb) in core/src/callbacks.rs; 8 private hook() gone, tile delegates.
- Item 3b (F4): 2dd113562 - decl layout section (simple, display_flex, grow, px_*...), timeline/cell_grid/dialog_kit/shells use it.
- Item 3c (F4): c0467267f - style_kit builders merged into decl (rename table in the commit), style_kit = theme marker only.
- Item 4a (F5): cd17df664 RED, ae27dabc2 encoder (core xml::html::encode_text/encode_attribute), 50023726e + 54bbbedf1 RED caller tests, 8182f997a callers moved (cell_grid, selection, e2e builder, notification, xml test, dll html_render). AzMail copies stay (needs api.json).
- Item 4b (F31): d0f71856d tests on the one decoder, a1c20bf54 prepare_string on decode_character_references; decode_entities & co. deleted.
- Extra (found in F5): 0bd60d7e8 RED / 8f7e19808 fix - to_html font-family attribute injection.

## IN PROGRESS
- Item 5 (F26): layout micromail 0.1 -> 0.2 (read ~/Development/micromail CHANGELOG + src).

## NEXT
- Report scripts/HYGIENE_2026_10_02.md.

## Decisions / open questions
- F4: decl survives (fill = background, fill_box = 100% box); style_kit keeps the theme marker only (rename to `marker` is wave 6).
- F5: encoder drops XML-forbidden C0/U+FFFE/U+FFFF; text sites use encode_text (quotes plain) - 3 test expectations updated in RED commits.
- F5: AzMail copies left (app sees only the generated crate; MAILHTML owns html.rs) -> api.json Xml.encode_text/encode_attribute listed in report.
