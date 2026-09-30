# EDITOR progress (RTF mail editing gaps)

Branch `wt/editor-mail-compose` from `d1dd0b783`. Report: `scripts/EDITOR_2026_09_30.md`.
UNCOMPILED (house rule): every RED is derived from reading the code.

## DONE

- E-TYPESTYLE: RED e243b7267, fix e3e6b93a5. Shared WPT-format fixture
  `layout/tests/common/editing_harness.rs` (registered once in all.rs).
- E-ARROW: RED 177ae7a16, fix 17037cacf.

## IN PROGRESS

- E-XBLOCK: a document selection's delete / replace keeps the surviving runs and styles.

## NEXT

1. E-PASTE: `ClipboardContent.html`, platform read / write of the HTML flavour, rich default paste.
2. E-SET: `reset_editor_content` - replace a live editor's content from code.
3. E-NESTED: Enter / Backspace act on the caret's innermost block.

## api.json so far

- enum `TextFormat` (azul_core::events::TextFormat): Bold, Italic, Underline, Strikethrough.
- `CallbackInfo::toggle_text_format(host: DomNodeId, format: TextFormat)`.

## Open questions

- A selection's Ctrl+B is left to the app (the engine only keeps the caret's typing style).
