Task TEXTENG (azul Rust GUI toolkit, PR #476, wave 5). Read the house rules first and follow them exactly (never compile):
scripts/waves/wave5/house_rules.md
Your branch: `wt/texteng` from base commit `2e92c759b` (`git -C <your worktree> checkout -b wt/texteng 2e92c759b`). TASK name: `TEXTENG`.
Other wave-5 agents work in parallel on: MAILHTML (mail HTML/CSS parity, AzMail's html.rs), TABLES (table layout),
TEXTENG (line-height, inline-blocks in spans, text-edit formats), APIEXPORT (api exports + ctrl||meta sites),
HYGIENE (macro paths, autofix, helper twins), RTE (shared rich-text editor; AzNotes/AzMail compose), BLOCKS
(selection model, undo stack, switcher merge, preset shells), PIM (azul-pim crate; Calendar/Tasks/Contacts).
Stay in your area; where you must touch another's file keep the edit minimal and list it in your report.

GOAL: the text-engine follow-ups, each RED first:
1. `line-height: 19px` gives a line pitch of 19.55 px (paged layout, 11pt Helvetica-ish text; see
   layout/tests/a_line_height_in_points_sets_the_line_pitch.rs for the measuring helper `paged_pens`): find why an
   absolute line height comes out 0.55 px larger (half-leading rounding? line box including glyph ascent beyond
   the line height?) and fix it so the pitch equals the line-height exactly.
2. `line-height` in rem / vw / vh / vmin / vmax is rejected (css/src/props/style/text.rs parse_style_line_height,
   commit 78aae1c66 - the stored PercentageValue (positive = factor, negative = absolute px) cannot carry the
   context). Give StyleLineHeight a representation that can (an enum: Normal / Number / Length(PixelValue) /
   Percentage - mind the FFI rules: repr(C), api.json via the parent; list the change), resolve it in fc.rs where
   the font size and viewport are known, and make `em` and `%` compute to an absolute length that INHERITS as a
   length (CSS: a number inherits as a factor, a length/percentage as the computed length) - today em/% inherit
   as a factor. Update every reader.
3. An inline-block inside an inline span is sized from its max-content width, ignoring its own CSS width
   (`layout/src/solver3/fc.rs` collect_inline_span_recursive, the LayoutDisplay::InlineBlock arm): size it like the
   top-level atomic inline branch (calculate_used_size_for_node against atomic_inline_containing_block).
4. DEDUP_EDITORS: the text-edit report the app gets (DocumentTextEdit / the text-input callbacks) carries no
   formats: Ctrl+B with no selection is not reported to the app (the next typed text should be bold - "pending
   format"), and pasting bold text into a paragraph loses the bold, in AzMail, AzNotes and AzWriter alike. Design
   the engine side (formats in the edit report, a pending-format state the caret carries) RED first; the apps'
   adoption is RTE's (coordinate through your report: name the API).

Report `scripts/TEXTENG_2026_10_02.md` per the house rules (what was built, commits, api.json list, least-sure-to-compile
spots, the parent's test commands, what is left for wave 6). You are running unattended: decide, note decisions in
your progress file, continue; do not stop to ask. Do not spawn subagents.
