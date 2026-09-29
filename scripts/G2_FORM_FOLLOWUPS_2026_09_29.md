# G2 - the form / input follow-ups the input-widget wave left open (2026-09-29)

Branch `wt/g2-form-followups`, cut from `0a326afe5` (the pushed tip of `fix/input-bugs-2026-09-19`).
NOTHING was compiled or run (house rules). Every RED commit precedes its fix; several RED commits
use new API, so their RED check is "fails to compile or fails" (as W1 / G1). See "Least sure to
compile" before the first build.

## The audit (Step 1) and what became of each item

| # | item | at `0a326afe5` | now |
|---|---|---|---|
| 1 | `ChangeNodeText` cannot replace typed text; typed text survives a form reset and a programmatic value change | OPEN | FIXED `4488f5066` (RED `530b34601`, `f6ab00b72`) |
| 2 | the XML mount starts with a fresh `FormControlMemory` | OPEN | FIXED `3501809ce` (RED `aa9759757`) |
| 3 | raw `<button>` not replaced by the Button widget | OPEN | FIXED `4d069898c` (RED `65204755d`) |
| 4 | the raw `<form>`'s dataset is dropped | OPEN | FIXED `bd7c9ae88` (RED `bb026df70`) |
| 5 | ComboBox: typed (not picked) text is not the value | OPEN | FIXED `95e402b51` (RED `ba2456753`) |
| 6 | Password: undo restores a mask the value cannot follow | OPEN | FIXED `826c75a76` (RED `42d80d0a9`) |
| 7 | checkbox / radio / slider / colour / file / dropdown missing from FormData | DONE for REPLACED controls (G1 `750d77133`), PARTIAL for widgets the app builds | FIXED for app-built widgets `15fe865df` (RED `e55acc1d2`) |
| 8a | Enter-to-submit | DONE (W1 `73d7ba7c4`) | - |
| 8b | datalist `display:none` | DONE (G1 `a9bf67b8a`) | - |
| 8c | `accept` / `multiple` on file inputs | OPEN | FIXED `cecf199d3` (RED `98f92d649`), `f4dc1e7e2` |
| 8d | week in ISO format | DONE (G1 `bbfc2f689`) | - |
| 9 | `collect_form_data` not in api.json | OPEN (list only) | FFI-shaped `CallbackInfo::get_form_data` `dd50eb26b` (RED `655aeb03a`) + the api.json list below |

The audit table with file:line references is the first section of
`scripts/G2_FORM_FOLLOWUPS.PROGRESS.md`.

## What was built

### 1. A value the app sets replaces what the user typed (engine)

Why typed text won: the engine keeps uncommitted typing in the content overlay
(`overlay.rs` `ContentOverlay::text`, keyed by the IFC root - a text field's value `<p>`), which
outranks the DOM until the app catches up (a new generation rendering the typed text, or an ack).
`ChangeNodeText` wrote only the DOM text node, and was a no-op when the DOM already held the string
- exactly the reset shape (DOM "ann", overlay "annbob").

- `LayoutWindow::set_node_text(dom, node, &text) -> bool` is now THE implementation of
  `ChangeNodeText`, shared by the shell (`dll/.../common/event.rs`) and the headless runner
  (`e2e/runner.rs`) - they each had a copy. It writes the DOM text AND retires the overlay entry of
  the IFC holding the node (`typed_text_root_of`: the node or its nearest ancestor with an entry),
  re-shaping it from the DOM (`spliced_text_with_preedits` + `reshape_text_node`). Carets move
  across the change on the next pass (`shift_carets_across_generation`). A byte-identical write is
  still a no-op unless typing covers the node.
- A NEW GENERATION that renders another text than the one the user typed over replaces the typing:
  `DirtyTextNode::typed_over` (the DOM text at the first keystroke, read by
  `overlay::dom_text_of`, kept while the entry lives) and `ContentOverlay::gc_app_set_text`, run in
  the layout funnel only when `new_generation`. The same text again keeps the typing (the app did
  not adopt it); the typed text retires it (the existing equality rule). `gc_converged_text` now
  shares `dom_text_of`.
- Widgets: `text_input::replace_engine_line` is one `ChangeNodeText` (the select-all + delete detour
  that could only EMPTY a field is gone; `pub(crate)`, the TextArea has the same shape);
  `restore_text_input` re-texts the line even when its mirror already agrees; `reset_form` puts
  EVERY text field of the form back (`reset_values`): named -> its initial value, a replaced raw
  control -> its registered default (`FormControlMemory::default_of`), anything else -> the text it
  was built with (`built_value`; a password built non-empty is skipped - its DOM holds the mask),
  text areas included (`restore_text_area`).

### 2. The XML mount uses the window's memory

`LayoutWindow::style_xml_document(xml, provider, style)` (cfg `xml`) replaces the document's raw
controls through `resolve_form_controls` (window memory, window app theme). One generator behind it
and `parse_xml_to_styled_dom_resolving_icons`: `xml::styled_xml_document(xml, provider, style,
resolve_form_controls)`. The DLL's mount path (`dll/.../common/layout.rs`) calls the window's.

### 3. Raw `<button>`

Row `("<button>", Button(Submit))`; a `<button type=submit|reset|button>` takes that `<input>` row,
any other type is submit (HTML's default). Text content = label; richer content (icon, image,
markup) stays the button's content, its text naming it for AT when the app gave no name. Roots
wearing `__azul-native-button` (Button widgets) are never replaced (also keeps the pass
idempotent). `text_content` now leaves an `<icon>`'s spec text out.

### 4. The raw form's dataset

`widgets::form::form_state_of(node)`: the dataset when it is a `FormStateWrapper`, else the payload
of the node's own `Submit` handler (the same state). `form_for` puts the raw form's dataset back on
the form node; `enclosing_form`, `submit_form`, `reset_form` and `is_form_widget` all use the one
helper.

### 5. ComboBox typed text

New hook `ComboBoxStateWrapper::on_text_input` (`ComboBoxOnTextInput`, same signature as
`on_select`), fired by the typing / Backspace handlers (`report_typed_text`, the hook taken out of
the state before it runs). The state is the combobox root's dataset (FormData probes
`ComboBoxStateWrapper`: the field's text). The replacement records picks AND typing. Value logic
only; S2's focus / active-descendant code untouched (the two handler tails changed by one line each).

### 6. Password undo

`record_text_edit_undo` (the one commit point of every text edit's history) records nothing for a
host that says `type=password` (`LayoutWindow::is_password_field` via `form::input_purpose`) - GTK
and Qt behaviour; the undo shortcut then finds nothing to restore there.

### 7. App-built widgets in FormData

`form_controls::hand_built_entries(state, own_value)` spells a widget's state with the same
`Spelling` the replaced controls use (checkbox / switch, slider, colour, file, drop-down label, time
picker). `widgets::form` hands it every state the named control carries (dataset, handler payloads,
descendants' - `built_states` / `carried_states`) and reads a radio group's chosen option label off
its row (`nth_child_text`). Order unchanged ahead of it: live text-like state, then the replaced
control's memory, then this, then the `value` attribute.

### 8c. `accept` / `multiple`

`FileInputStateWrapper.accept` / `.multiple`, `FileInputState.paths` (+ `create_with_paths`),
`FileInput::set/with_accept`, `set/with_multiple`, `set/with_paths`. The click builds the dialog
filter from `accept` (`accept_patterns`: extension, MIME type, `image/*`; descriptor = the accept
text) and opens `FileDialog::open_multiple_files` when `multiple` (new resume
`fileinput_on_files_picked`; both resumes store through `set_picked_files`). Several files label
"N files". The replacement reads both attributes, remembers every pick (`FormValue::Files`), and a
form submits one entry per file (`Submission::Entries`), one empty entry for none.

### 9. `CallbackInfo::get_form_data`

`info.get_form_data(node) -> OptionFormData` - an inherent `impl CallbackInfo` in `widgets/form.rs`
next to `collect_form_data` (callbacks.rs untouched), `OptionFormData` via `impl_option!`.

## Commits (in order)

`3d1c5a6a9` progress/audit - `530b34601` RED 1 - `f6ab00b72` test tweak - `4488f5066` fix 1 -
`42d80d0a9` RED 6 - `826c75a76` fix 6 - `aa9759757` RED 2 - `3501809ce` fix 2 - `65204755d` RED 3 -
`4d069898c` fix 3 - `bb026df70` RED 4 - `bd7c9ae88` fix 4 - `ba2456753` RED 5 - `95e402b51` fix 5 -
`98f92d649` RED 8c - `cecf199d3` fix 8c - `e55acc1d2` RED 7 - `15fe865df` fix 7 - `655aeb03a` RED 9 -
`dd50eb26b` feat 9 - `f4dc1e7e2` rename - this report + final checkpoint.

## api.json (autofix; nothing edited by hand)

- `CallbackInfo.get_form_data(node: DomNodeId) -> OptionFormData` (fn_body
  `object.get_form_data(node)`); new type `OptionFormData` (`repr(C, u8)` enum `None` / `Some(FormData)`).
- `ComboBoxStateWrapper`: field `on_text_input: OptionComboBoxOnTextInput` APPENDED after `on_select`.
- Callback family (mirror `ComboBoxOnSelect*`): `ComboBoxOnTextInputCallbackType = extern "C"
  fn(RefAny, CallbackInfo, ComboBoxState) -> Update`, `ComboBoxOnTextInput { callback, refany }`,
  `OptionComboBoxOnTextInput`, `ComboBoxOnTextInputCallback`; managed symbols
  `AzApp_setComboBoxOnTextInputCallbackInvoker`, `AzComboBoxOnTextInputCallback_createFromHostHandle`,
  `AzComboBoxOnTextInputCallback_createFromHostHandleByref`.
- `ComboBox.set_on_text_input(data: RefAny, callback: ComboBoxOnTextInputCallbackType)`,
  `ComboBox.with_on_text_input(data, callback) -> ComboBox` (same shape as `set/with_on_select`).
- `FileInputStateWrapper`: fields `accept: StringVec`, `multiple: bool` APPENDED (in that order).
- `FileInputState`: field `paths: StringVec` APPENDED; `FileInputState.create_with_paths(paths:
  StringVec) -> FileInputState`.
- `FileInput.set_paths(paths: StringVec)`, `with_paths(paths) -> FileInput`, `set_accept(accept:
  StringVec)`, `with_accept(accept) -> FileInput`, `set_multiple(multiple: bool)`,
  `with_multiple(multiple) -> FileInput`.

Rust-only (no api.json): `LayoutWindow::set_node_text`, `LayoutWindow::style_xml_document` (cfg
xml), `overlay::DirtyTextNode.typed_over` (pub field), `overlay::dom_text_of`,
`FormControlMemory::default_of`, `form_controls::FormValue::Files` (replaces `Path`),
`form_controls::Submission::Entries` (replaces `Nothing` / `Value`), the `"<button>"` table row.

## Files outside the task list (kept minimal)

`layout/src/window.rs`, `layout/src/overlay.rs` (the engine piece of item 1, item 6's commit point,
item 2's method), `dll/src/desktop/shell2/common/event.rs` (the `ChangeNodeText` arm now calls
`set_node_text`), `dll/src/desktop/shell2/common/layout.rs` (mount -> `style_xml_document`),
`layout/src/xml/mod.rs` (one generator), `layout/src/widgets/file_input.rs` (item 8c),
`layout/src/widgets/node_graph.rs` (two test literals of `FileInputState`),
`layout/src/e2e/runner.rs` (its `ChangeNodeText` arm + tests). combobox.rs: value logic only (S2).
form.rs: no theme-merge call site touched (U1).

Twins unified: the `ChangeNodeText` apply logic lived twice (shell + runner) -> `set_node_text`;
the node-attribute key reads in `widgets/form.rs` -> `memory_key_at` / `within_form`. Twin left:
the headless runner mounts XML through the FastDom path (`parse_xml_to_styled_dom`: no control
replacement, no icons) while the DLL uses the resolving path - switching it would move every
mounted scenario of the corpus.

## Least sure to compile (check these first)

1. `widgets/form.rs` `reset_values`: `replaced_key_of(info, field).and_then(|key| match
   info.get_layout_window().form_control_memory.default_of(key) {..})` followed by
   `default.or_else(|| built_value(info, field))` (a shared then a unique capture of `*info`).
2. `widgets/form.rs` `current_form_data`: the `unknown` closure capturing `info` shared with a nested
   `|n| nth_child_text(info, node, n)`; `built_value`'s block with `?` / `return None` holding a
   `get_layout_window()` borrow, then `info.get_dataset` (`&mut`).
3. `widgets/form.rs` `text_fields`: `data.get_dataset().is_some_and(is_text_field_state)` (fn item
   for `FnOnce(&RefAny) -> bool`); `within_form`'s `impl FnMut(NodeId, &NodeData) -> Option<T>`.
4. `widgets/form.rs` `form_state_of`: `node.get_dataset().filter(|ds| is_form_state(ds))` (`&&RefAny`
   into `&RefAny`), `c.callback.cb == default_on_form_submit_event as usize`; the second inherent
   `impl CallbackInfo` block (in `widgets/form.rs`) and `impl_option!(FormData, OptionFormData, ..)`.
5. `file_input.rs` `accept_patterns`: the `push` closure mutably borrowing `out`, `KNOWN:
   &[(&str, &[&str])]`, `find(|(mime, _)| *mime == token)` (`&str == String`); `const fn
   set_multiple(&mut self)` / `const fn with_multiple(mut self)` on `FileInput`.
6. `window.rs` `set_node_text`: `let Some(unchanged) = self.layout_results.get(..).and_then(|lr|
   lr.styled_dom.node_data.as_container().get(node_id).map(..)) else {..}` (a temporary container);
   `is_password_field`: `core::iter::once(node).chain(host)` with `host: Option<DomNodeId>`.
7. `overlay.rs` `gc_app_set_text`: the `let (Some(..), Some(..)) = (..) else { return true; }` inside
   `retain`, `String == &str`.
8. `form_controls.rs` Button arm: `dom.children = raw.children.clone()`, the a11y name set through
   `get_accessibility_info().cloned()` / `set_accessibility_info`; `widget_for`'s `*t == ty`.
9. `combobox.rs`: the second `impl_managed_callback!` family (new symbol names), `report_typed_text`.
10. Tests: runner `value_leaf` (`hierarchy[host]`), the nested `fn typed_into` in
    `a_password_field_keeps_no_undo_history`, `Callback::from_core(handler.callback).invoke(..)` in
    `combobox_text::type_into` (moves the `callback` field out of an owned clone).

## Test commands (release, as usual)

- `cargo test --release -p azul-layout --lib --features e2e-server -- e2e::runner::tests::change_node_text e2e::runner::tests::a_form_reset_puts_every_typed_field_back_named_or_not e2e::runner::tests::a_password_field_keeps_no_undo_history`
- `cargo test --release -p azul-layout --test all -- app_set_text_beats_typing form_controls_become_widgets text_ack_survives_relayout`
- `cargo test --release -p azul-layout --lib -- overlay widgets::form widgets::text_input widgets::combobox widgets::file_input widgets::node_graph form_controls xml::`
- then the full suites (layout `--lib` with and without `e2e-server`, layout `--test all`, dll
  `--lib --features build-dll`) and the e2e JSON corpus (`--test e2e_json`): item 1 changes the
  overlay GC at a new generation and every `ChangeNodeText`.
- RED pass: `git apply -R` each fix commit above; the RED commits for 2, 8c, 9 (and the typed-over
  / `create_with_paths` API) fail to compile, the others fail at runtime.

## What is left

- Clear-after-send by REBUILD: an app that adopted the typing without rendering it (its hook stored
  the text, no rebuild) and then renders the value the user typed over (a field built empty, cleared
  back to "") - the engine cannot tell that from "not adopted"; the app acks
  (`mark_text_revision_synced`) or sets the text through `ChangeNodeText`. A per-node ack, or a
  "controlled field" flag a TextInput with a hook sets, would close it.
- A programmatic set does not clear the field's undo history (HTML does): an undo after a reset can
  bring the typing back into the engine line (the TextInput mirror adopts it on the next edit).
- Unnamed, app-built password fields built non-empty are not reset (their DOM holds only the mask).
- A raw input's dataset reaches the widget root only when that root has none (TextInput, TextArea,
  Slider, ColorInput, ComboBox, date pickers keep their own state there).
- The headless runner's XML mount path (FastDom) - see above.
- D1's demo acks all typed text before its reset rebuild; with item 1 a Form's own reset no longer
  needs it (examples untouched).
- Unchanged from before: `<select multiple>`, disabled `<option>`s, image-button click coordinates,
  `capture` / directory pickers on file inputs.
