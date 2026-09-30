# EDITOR progress (RTF mail editing gaps)

Branch `wt/editor-mail-compose` from `d1dd0b783`. Report: `scripts/EDITOR_2026_09_30.md`.
UNCOMPILED (house rule): every RED is derived from reading the code.

## DONE

- E-TYPESTYLE: RED e243b7267, fix e3e6b93a5. Shared WPT-format fixture
  `layout/tests/common/editing_harness.rs` (registered once in all.rs).
- E-ARROW: RED 177ae7a16, fix 17037cacf.
- E-XBLOCK: RED 4065a0488, fix 143138b8f (new `layout/src/rich_blocks.rs`).
- E-PASTE: RED 46ab2340c, fix 9b4ea6f33 (new `layout/src/paste_html.rs`; `ClipboardContent.html`;
  dll clipboard reads / writes the HTML flavour; `LayoutWindow::paste_clipboard_content`).
- (2026-09-30 18:15 the Mac panicked mid E-PASTE; the worktree survived, resumed 21:00.)

- E-SET: RED 5111e1865, fix 30cd5f1ab: `LayoutWindow::reset_editor_content`,
  `CallbackInfo::reset_editor_content`, `place_pending_caret`; the caret text snapshot is keyed by
  block element too (a latent shift after any session move between blocks of one host).
- E-NESTED: RED 92ec5e88a, fix (next commit): `structural_edit_node` = the caret's block element.

## IN PROGRESS

- Report `scripts/EDITOR_2026_09_30.md`.

## NEXT

- (nothing after the report; item 7's deferred selection items were not needed to unblock 1-6)

## api.json so far

- enum `TextFormat` (azul_core::events::TextFormat): Bold, Italic, Underline, Strikethrough.
- `CallbackInfo::toggle_text_format(host: DomNodeId, format: TextFormat)`.

## Open questions

- A selection's Ctrl+B is left to the app (the engine only keeps the caret's typing style).
