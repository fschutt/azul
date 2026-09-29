# W2 - raw form controls become widgets (2026-09-29)

Branch `wt/w2-input-replacement` (cut from `fix/input-bugs-2026-09-19`, PR #476). Nothing was
compiled; see "Least sure to compile" before the first build.

## What it does

`Dom::create_input("range", ..)`, `<input type="range">` in XML, a `<select>` and a `<textarea>`
are REPLACED by the widget their type names. The pass is `azul_layout::form_controls::
resolve_form_controls_in_dom(&mut Dom, &FormControlMemory, scope)`; like `<icon>` resolution it
runs on the `Dom` tree BEFORE the cascade (a widget is a subtree; a `StyledDom` / `FastDom` is a
flat arena that cannot take one). It runs before Fluent and before icon resolution, because the
widgets contain `<icon>`s (drop-down arrow, calendar glyph).

Where it runs (every producer of an app DOM, the icon lesson):

| path | call | scope |
|---|---|---|
| layout callback DOM (shell) | `LayoutWindow::resolve_form_controls` in `dll/.../common/layout.rs` `regenerate_layout`, BEFORE `fingerprint_dom` | `FORM_SCOPE_ROOT` |
| any `style_user_dom` / `style_user_dom_for` | `LayoutWindow::style_user_dom_in_scope` (new; the old two delegate) | `FORM_SCOPE_ROOT` |
| VirtualView DOM | `invoke_virtual_view_callback_impl` -> `style_user_dom_in_scope` | `form_scope_of_virtual_view(dom, node)` |
| `measure_dom` / `measure_dom_shrink_to_fit` | `style_user_dom_in_scope` | `FORM_SCOPE_MEASURE` |
| E2E XML mount | `xml::parse_xml_to_styled_dom_resolving_icons` | fresh memory |

Why the shell resolves BEFORE the fingerprint: the pre-cascade fast path transfers callbacks and
datasets BY INDEX onto the retained `StyledDom`. A raw `<input>` is one node, its widget several;
a raw-DOM fingerprint would (when node counts happened to coincide) install callbacks on the
wrong widget nodes. Resolved first, the fresh DOM is exactly what a hand-built widget DOM is, and
`style_user_dom_for` later finds nothing to replace (one walk, early exit).

Not replaced (unchanged): the FastDom XML path (`parse_xml_to_styled_dom`, `domxml_from_str` /
`Dom::from_xml_string` - same as icons), the web backend (`dll/src/web/html_render.rs` renders
raw inputs as real HTML inputs, which is what a browser wants), the debug-server "insert node" op.

## The mapping table (`form_controls::INPUT_TYPE_WIDGETS`, one row per type)

| key | widget | notes |
|---|---|---|
| `text`, missing, unknown | TextInput | HTML's text state is the fallback |
| `checkbox` | CheckBox | `checked` |
| `radio` | RadioGroup with ONE empty-label option | index 0 = checked, `usize::MAX` = unchecked; a NAMED group is one control in the memory; checking one asks for RefreshDom, which unchecks its siblings |
| `color` | ColorInput | `value` via `color_from_hex`, default black |
| `file` | FileInput | value not settable (HTML); user pick remembered |
| `number` | NumberInput | value, min, max, placeholder |
| `range` | Slider | min 0 / max 100 / midpoint defaults, max<min -> min, value snapped to `step` (default 1, `any`) and clamped |
| `date` | DatePicker | `YYYY-MM-DD`; no value -> `min` -> 2000-01-01 |
| `time` | TimePicker (24h) | `HH:MM[:SS]` |
| `button` | Button | label = `value` |
| `<select>` | DropDown | options (label attr > text > value), last `selected` else first enabled; pick asks for RefreshDom |
| `<textarea>` | TextArea | text content (one leading newline dropped) else `value`; `rows`/`cols` -> height/width |
| text-like + `list=` naming a non-empty `<datalist>` | ComboBox | items = option values (not for `password`) |
| `password` | TextInput | `// WAVE2-GLUE(W1): password` |
| `search` | TextInput | `// WAVE2-GLUE(W1): search` |
| `email` | TextInput | `// WAVE2-GLUE(W1): email` |
| `tel` | TextInput | `// WAVE2-GLUE(W1): tel` |
| `url` | TextInput | `// WAVE2-GLUE(W1): url` |
| `month` | DatePicker | `// WAVE2-GLUE(W1): month` (`YYYY-MM` -> day 1) |
| `week` | DatePicker | `// WAVE2-GLUE(W1): week` (`YYYY-Www` -> Jan 1 + 7(w-1); not ISO-exact) |
| `datetime-local` | DatePicker | `// WAVE2-GLUE(W1): datetime-local` (date half only) |
| `reset` | Button "Reset" | `// WAVE2-GLUE(W1): reset` |
| `submit` | Button "Submit" | `// WAVE2-GLUE(W1): submit` |
| `image` | Button (label = `alt` or "Submit") | `// WAVE2-GLUE(W1): image` |
| `hidden` | invisible `div` (display:none) keeping ALL attributes incl. `value` | `// WAVE2-GLUE(W1): hidden` |
| `<optgroup>` inside `<select>` | flattened into the DropDown's choices | `// WAVE2-GLUE(W1): select optgroup` in `widget_for` and `collect_choices` |

Glue for W1: change the row's `FormWidget` (add a variant) and add one arm to `build()`; the
recorder pattern (below) is one small `extern "C" fn` per new change-hook signature.

## Attribute / identity mapping (`graft`)

- READ into the widget: value, placeholder, min/max/step, checked, maxlength (TextInput /
  TextArea `max_len`), size (text-like width), rows/cols (TextArea), list, options, textarea
  text, accessible name = `aria-label` > node a11y name > `title`.
- CARRIED to the widget ROOT: every other attribute (id, class, name, type, required, pattern,
  minlength, min/max/step, disabled, readonly, autofocus, data-*, custom) - form validation,
  `input_purpose` (soft keyboard), `is_reset_control` and CSS read them there. NOT carried: the
  live-state ones the widget now owns (Value, CheckedTrue/False, Selected, Placeholder), except
  on `hidden`. Plus a marker `data-azul-form-control="<widget name>"` on every replaced root.
- Inline style: widget's rules, then the node's (app wins, last-match). Of the node's `Dom.css`
  (`with_css`) sheets, rules that can only mean the node (`*{..}`, `*:hover{..}`) join the root's
  inline style; other rules (descendant/class selectors) stay a scoped sheet AFTER the widget's
  sheets, so an app can reach widget parts by class. Keyframes carried.
- Callbacks: APPENDED to the widget root's own (widget handler first, then the app's). Carried,
  not mapped onto typed hooks: the app's `(RefAny, CallbackInfo) -> Update` shape does not fit
  them and the typed hooks belong to the recorders. Composite widgets whose focus target is an
  inner part (radio row, combobox field, time spinners) only see bubbling events at the root.
- Tab index onto the root when the root is focusable; key, marker, context menu, menu bar,
  component origin, dataset (only if the widget root has none), a11y description / labelled-by /
  described-by.
- `disabled`: every node of the widget loses callbacks, tab index and contenteditable; app
  callbacks are not attached; root gets `opacity: 0.5` (before the app's style) and
  `AccessibilityState::Unavailable`. `readonly` (text-like only): contenteditable off, focus kept.

## State across the app's rebuilds

`LayoutWindow.form_control_memory: FormControlMemory` (`Arc<Mutex<..>>`). The replacement sets
each widget's typed change hook to a recorder (`on_toggle`, `on_change`, `on_value_change`,
`on_choice_change`, `on_select`, `on_path_change`, `on_text_input`) carrying
`(memory, key, defaults)`; the next build `recall`s the value while the app's defaults hash is
unchanged (HTML dirty-value rule: the app changing the attribute takes the control back and
forgets the user's value). Key = scope + kind + (`with_key` | first id | tree path + name);
named radio groups key by (scope, name). Bounded to 4096 entries, LRU-evicted. Recorders return
DoNothing except drop-down / radio / file (RefreshDom). Typed text ALSO survives through the
engine's unacked text overlay, like any hand-built TextInput.

## Opt-out

Per node: `data-azul-widget="none"` (`form_controls::OPT_OUT_ATTRIBUTE` / `OPT_OUT_VALUE`,
`form_controls::opt_out_attribute()` for Rust; plain attribute in XML). No global AppConfig
flag (would need api.json + plumbing into LayoutWindow; easy to add later as a bool the resolver
checks).

## XML

`core/src/xml.rs::apply_xml_node_attributes` now maps the attributes of input / select /
option / optgroup / textarea / datalist / button onto typed `AttributeType`s (`form_control_
attributes`): type, name, value, min, max, step, pattern, autocomplete, aria-label, title, alt,
src, minlength, maxlength, required, disabled, readonly, selected, checked; size / rows / cols /
multiple / accept / list / label / wrap / form / inputmode / dirname / capture as `Custom`;
`data-*` (except `data-l10n*`) as `Data`. Before, an XML `<input type="range">` had no type at
all (so XML reset buttons and validation attributes were also invisible). Booleans: present =
on, explicit `"false"` = off.

## Commits

- `6dba62aa5` test(form-controls): raw inputs, selects and textareas become widgets (RED;
  existing public API only, so it compiles against the pre-fix engine and fails at runtime)
- `59fd16e7e` feat(form-controls): raw inputs, selects and textareas become widgets
- `4800b3cf5` fix(xml): form elements keep their type, value and constraint attributes
- `b1a7b5bb1` test(form-controls): drop an unused import and a needless mut
- `09f86c276`, `ef6035f32` progress checkpoints (+ this report's commit)

Tests: `layout/tests/form_controls_become_widgets.rs` (registered at the END of `all.rs`) - one
per mapping family, every visible type, attribute mapping, disabled/readonly, tabindex/autofocus,
app callbacks on the root, opt-out, XML path, VirtualView, typed text across a rebuild (overlay),
clicked checkbox across a rebuild (widget's own click handler -> recorder -> memory), select
pick -> RefreshDom -> shown, app-changed default wins, radio group exclusivity, text memory
after an ack. Unit tests in `form_controls.rs` (table exhaustive vs `azul_core::dom::InputType`,
no duplicate rows, no-op on control-free DOMs, idempotence, descendant-count fixup, marker, hidden,
memory dirty rule + LRU, step snapping, date/time parsing, select default, scope-only rule test).

## Public API added (Rust; NO api.json change made or required)

azul-layout `form_controls` (cfg `widgets`): `resolve_form_controls_in_dom(dom: &mut Dom, memory:
&FormControlMemory, scope: u64) -> usize`; `FormControlMemory` (`remember(key, defaults,
FormValue)`, `recall(key, defaults) -> Option<FormValue>`, `forget`, `clear`, `len`, `is_empty`;
not `repr(C)`, Arc<Mutex> inside - Rust-only); `enum FormValue`; `enum FormWidget` +
`name()`; `static INPUT_TYPE_WIDGETS`; consts `OPT_OUT_ATTRIBUTE`, `OPT_OUT_VALUE`,
`REPLACED_MARKER_ATTRIBUTE`; `opt_out_attribute() -> AttributeType`; re-exports of the scopes.
azul-layout `window`: `LayoutWindow::form_control_memory` (field, cfg widgets),
`LayoutWindow::resolve_form_controls(&self, &mut Dom) -> usize` (cfg widgets),
`LayoutWindow::style_user_dom_in_scope(&self, Dom, &FullWindowState, u64) -> StyledDom`,
`pub const FORM_SCOPE_ROOT: u64 = 0`, `pub const FORM_SCOPE_MEASURE: u64 = u64::MAX`,
`pub const fn form_scope_of_virtual_view(DomId, NodeId) -> u64`.
If an FFI opt-out is wanted: `Dom::create*` stays; a `Dom::with_widget_replacement_disabled()`
(api.json: no args, returns Dom) would just push `opt_out_attribute()` - not added.

## Least sure to compile (check these first)

1. `form_controls::remember_with`: `make(&recorder)` where `recorder: Ref<'_, Recorder>` from
   `RefAny::downcast_ref` - relies on deref coercion `&Ref<T>` -> `&T`.
2. `widget_for`: `*ty == row` compares `&str` with `String` (`impl PartialEq<String> for &str`).
3. `graft`: `widget.root.style.keyframes.clone().into_library_owned_vec()` and
   `sheet.keyframes.as_slice()` (KeyframesVec impl_vec surface), the `Css { rules, keyframes }`
   literal, `dom.root.flags.set_tab_index(None)` (NodeFlags method via the pub `flags` field).
4. The four `LayoutWindow` sites with `#[cfg(feature = "widgets")] form_control_memory` (struct,
   `new()`, `memory_walk_coverage_is_exhaustive` destructure, the node-id destructure ~22500).
5. `window.rs` `style_user_dom_in_scope`: `#[cfg(..)] let _ = ...;` statements and `mut dom`
   (unused-mut warning only in a no-widgets/no-fluent build).
6. `core/src/xml.rs::form_control_attributes`: `xml_node.attributes.inner.iter()`, the `custom`
   closure, `continue` inside the match arms.
7. Test file: `CallbackInfoRefData` literal copied from `statusbar_live_label.rs`;
   `hook.callback.invoke(..)` on `DropDownOnChoiceChangeCallback` / `RadioGroupOnChangeCallback` /
   `TextInputOnTextInputCallback` (all have `impl_managed_callback!`).

## What is left / known gaps (mostly for W1 and the integration)

- Form reset (`form::default_values`) only sees `NodeType::Input|TextArea|Select`; replaced
  controls are divs now. W1's reset should find roots by `data-azul-form-control`, clear their
  memory entries (`FormControlMemory::forget/clear`) and RefreshDom.
- Validation's `value_of` reads `get_text_before_textinput(root)`; for a replaced TextInput the
  text sits in a `<p>` under the root, which that read may not descend into (text_input.rs says
  so) - a `required` text field could fail while filled. Verify when W1 wires FormData.
- Implicit submission: Enter in a replaced TextInput is a contenteditable host's Enter
  (split-block, vetoed by the widget), not `SubmitForm` as for a raw Input. W1's submit.
- TextArea (flat theme) binds its only callback to FocusReceived -> `default_on_virtual_key_down`
  (pre-existing oddity), so its `on_text_input` recorder may never fire; typed text still
  survives via the overlay.
- `<datalist>` stays in the tree (its options are UA display:none; the datalist itself has no UA
  `display:none` rule).
- File `accept` / `multiple`, `<select multiple>` / `size>1`, disabled `<option>`s: carried or
  ignored, no widget support. ISO week numbering approximated.
- Label-for association (`<label for=id>`) now targets the widget root (the id moved there);
  not tested.
- Parent: compile, run `layout --lib` (form_controls unit tests) and `layout --test all`, then the
  RED pass (`git apply -R` of 59fd16e7e + 4800b3cf5 should turn the new integration tests red).
