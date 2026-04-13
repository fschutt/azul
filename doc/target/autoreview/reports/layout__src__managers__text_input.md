# Review: layout/src/managers/text_input.rs

## Summary
- Lines: 244
- Public functions: 5 (`resulting_text`, `into_option`, `new`, `record_input`, `get_pending_changeset`, `clear_changeset`, `get_pending_events`)
- Public structs/enums: 4 (`PendingTextEdit`, `OptionPendingTextEdit`, `TextInputSource`, `TextInputManager`)
- Findings: 0 high, 2 medium, 0 low

## Findings

### [MEDIUM] EventData::None for Input events — callbacks cannot inspect edit details
- **Location**: `text_input.rs:235`
- **Details**: The `get_pending_events` implementation creates `Input` events with `EventData::None`. The comment on line 233-234 says "Callbacks can query changeset via text_input_manager.get_pending_changeset()" but this means the event itself carries no information about what changed. There is no `TextInputEventData` variant in the `EventData` enum. This is a design gap — the event system has specialized data variants for Mouse, Keyboard, Scroll, Touch, Clipboard, etc., but not for text input.
- **Evidence**: `EventData` enum at `core/src/events.rs:417-434` has no text-input-specific variant.
- **Recommendation**: Consider adding a `TextInput(TextInputEventData)` variant to `EventData` so callbacks can access edit information directly from the event, consistent with other event types.

### [MEDIUM] Module-level documentation — adequate but references "Phase 3.5+"
- **Location**: `text_input.rs:1-33`
- **Details**: The module doc is well-written and explains the two-phase architecture clearly. However, the `EventProvider` trait comment in `core/src/events.rs:2221` references "Phase 3.5+" which is a planning artifact.
- **Evidence**: `core/src/events.rs:2221`: `// Unified Event Determination System (Phase 3.5+)`
- **Recommendation**: No action needed in this file. The phase reference is in `core/src/events.rs`, not here.

## System Documentation
- System identified: yes — Text Input / IME system (part of the event/input handling pipeline)
- Existing doc: none (no `doc/guide/text-input.md` or similar)
- Doc needed: A guide document covering the text input pipeline — how keyboard/IME/a11y input flows through `TextInputManager`, the record/apply two-phase approach, integration with `CursorManager` and `text3::edit`, and how contenteditable nodes handle text changes. Related files: `layout/src/managers/text_input.rs`, `layout/src/window.rs` (apply_text_changeset), `dll/src/desktop/shell2/common/event.rs`, platform-specific IME handling.
