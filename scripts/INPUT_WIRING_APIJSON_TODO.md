# api.json deltas owed — run `azul-doc autofix` at fix-up time

Never hand-edit api.json. Each entry names the Rust type that is the source of truth.

## From step 3c (composition events)

- `azul_core::events::CompositionEventData` — new struct `{ data: String, cursor_begin: usize,
  cursor_end: usize }`. Needs a C-ABI representation (`AzString` + two `usize`).
- `azul_layout::managers::text_edit::CompositionPhase` — new enum `{ Start, Update, End }`.
- `CallbackInfo::get_composition_text() -> OptionAzString`
- `CallbackInfo::get_composition_cursor() -> Option<(usize, usize)>` — the tuple needs a named struct for
  the C ABI; propose `CompositionCursor { begin: usize, end: usize }` rather than a tuple.
- `CallbackInfo::is_composing() -> bool`
- `EventData::Composition(CompositionEventData)` — appended at the END of `EventData` for ABI stability.
  `EventData` is Rust-internal today, so this may need no api.json entry at all; confirm before adding.

## From step 2c (scroll phase)

- `azul_layout::managers::scroll_state::ScrollPhaseTransition` — new enum `{ Started, Ended }`. Internal to
  the manager; only needs exposing if `CallbackInfo` grows a phase accessor.
