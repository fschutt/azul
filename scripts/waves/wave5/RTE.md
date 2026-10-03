Task RTE (azul Rust GUI toolkit, PR #476, wave 5). Read the house rules first and follow them exactly (never compile):
scripts/waves/wave5/house_rules.md
Your branch: `wt/rte` from base commit `2e92c759b` (`git -C <your worktree> checkout -b wt/rte 2e92c759b`). TASK name: `RTE`.
Other wave-5 agents work in parallel on: MAILHTML (mail HTML/CSS parity, AzMail's html.rs), TABLES (table layout),
TEXTENG (line-height, inline-blocks in spans, text-edit formats), APIEXPORT (api exports + ctrl||meta sites),
HYGIENE (macro paths, autofix, helper twins), RTE (shared rich-text editor; AzNotes/AzMail compose), BLOCKS
(selection model, undo stack, switcher merge, preset shells), PIM (azul-pim crate; Calendar/Tasks/Contacts).
Stay in your area; where you must touch another's file keep the edit minimal and list it in your report.

GOAL: ONE rich-text editor widget shared by AzMail compose, AzNotes and AzWriter (DEDUP_EDITORS: read its
editor comparison and the proposed design + 7-step adoption order first).
Build `layout/src/widgets/rich_text_editor.rs` (+ both themes, tests, manifest) from AzNotes' editor as the base
(examples/azul-notes/src/doc.rs, editor.rs, markdown.rs: 5 formats, 9 block kinds, nested lists/checklists,
Markdown shortcuts, "typing continues the format on the left"), with Writer's tables / alignment / page breaks /
undo (examples/azul-writer/src/ir.rs) and Mail's quote depth + HTML and plain-text writers
(examples/azul-mail/src/editor.rs, compose.rs) as features; state type, callbacks, serializers (HTML, Markdown,
plain text), one undo stack. Fix in the shared editor, RED first: AzMail's `sync_text` (editor.rs:367-399)
flattens a paragraph to one text node (bold/links lost on the next rebuild or Send); Ctrl/Cmd+B/I/U over a
selection does nothing in Mail/Writer; Mail's Bold pressed twice wraps another <b> instead of toggling off;
Writer's two undo stacks. Then adopt it in AzNotes (the base) and AzMail compose; AzWriter is wave 6 (write its
adoption plan in your report). TEXTENG designs the engine's format-carrying edit report; use what exists, and
name what you need from it in your report.

Report `scripts/RTE_2026_10_02.md` per the house rules (what was built, commits, api.json list, least-sure-to-compile
spots, the parent's test commands, what is left for wave 6). You are running unattended: decide, note decisions in
your progress file, continue; do not stop to ask. Do not spawn subagents.
