# D1 - the AzWidgets demo shows every widget, in both themes (2026-09-29)

Branch `wt/d1-demo-every-widget`, cut from `fix/input-bugs-2026-09-19` at `2892031b7`.
Touches only `examples/azul-widgets/` (plus this report and the progress file). NOTHING was
compiled or run: every call was checked against the generated bindings in
`target/codegen/dll_api_external.rs` / `reexports.rs` of the main checkout (06:00 build, which
already has W1-W4 + the DatePicker theme). See "Least sure to compile" first.

## What the page shows now

The toolbar (W4) is unchanged in function: colour scheme System / Light / Dark
(`CallbackInfo::set_color_scheme`) and widget theme Flat / Flora (`Showcase::widget_theme`).
Both of its Segmented controls now follow the widget theme themselves.

| section | what changed |
|---|---|
| Inputs | `.with_theme(theme)` on NumberInput and ColorInput (TextInput, TextArea, Slider, Switch had it). The TextArea now hands its text back on every rebuild (`with_text` + `on_text_input` -> `Showcase::textarea_text`), like the TextInput. |
| Selection | `.with_theme` on RadioGroup and Segmented (CheckBox, DropDown had it). ComboBox has no widget theme - comment at the call site. |
| Display | `.with_theme` on the three Badges, Chip, Card, Divider. NEW: Frame ("Shipping" group box). Spinner: no fixed colour any more (each theme paints its native ink) and a row of three - the theme's default indicator, `SpinnerStyle::Spokes`, `SpinnerStyle::Ring` - each named under it. |
| Video | the VideoWidget gets the theme (`video::card(&state, theme)`), so its "no signal" poster follows it. |
| Feedback | `.with_theme` on Alert, Tooltip, the modal Dialog. |
| Navigation | `.with_theme` on Breadcrumb, Pagination, Stepper, Accordion. |
| Overlays | `.with_theme` on the popover Dialog and SplitPane. |
| Date & Time | `.with_theme` on DatePicker and TimePicker. |
| NEW: Every input type, in a Form | see below (`src/forms.rs`) |
| NEW: Raw HTML inputs | see below (`src/forms.rs`) |

Both new sections are LAST on the page, so nothing above them moved except by the Spinner
captions and the Frame (see "E2E").

### "Every input type, in a Form"

One `Form` (themed, `with_accessibility_name("Every input type")`) around two columns:

- text: Full name (`TextInput::create`), Password (`create_password`, pattern `.{8,}`), Search
  (`create_search`, the x / Escape clears), E-mail (`create_email`), Phone (`create_tel`, pattern
  `[0-9 +()-]{6,}`), Website (`create_url`), Postcode (text, pattern `[0-9]{5}` - "type a letter
  for the invalid look"), Notes (`<textarea>`: TextArea), Browser (`<input list>` + `<datalist>`:
  ComboBox);
- the rest: Quantity (NumberInput), Volume (Slider = range), Accent (ColorInput), Attachment
  (FileInput), Day (`DatePicker::create` = date), Month (`create_month`), Week
  (`create(..).with_mode(DatePickerMode::Week)`), Time (TimePicker, 24 h), Meeting
  (`DateTimeLocalPicker`), Newsletter (CheckBox), Plan (RadioGroup = radio), Favourite food
  (`DropDown::create([]).with_optgroup("Fruit", ..).with_optgroup("Vegetables", ..)`), Form id
  (`HiddenInput::create("form-id", "sign-up")`);
- buttons: `Button::create_submit("Submit")`, `create_reset("Reset")`,
  `create_image(send arrow, "Send (image button)")` (an 18x18 RGBA raw image made once at start,
  kept in the state; falls back to a submit button with the alt text) and a plain
  `Button::create("A plain button")` (type=button: sets a verdict line, neither submits nor resets).

Every themed one gets the page theme. Every value is the APP's (`forms::FormValues`): each control
is built from it and its hook writes back into it (text hooks return `DoNothing`, like the
existing TextInput; the rest return `RefreshDom`).

How the Form reads each control: TextInput / TextArea / DatePicker / DateTimeLocalPicker by their
live state under `with_name` (TextArea and NumberInput get `name` as an attribute on their root,
which holds the TextArea / TextInput state); HiddenInput by itself; every control that keeps
nothing the form can read (checkbox, slider, colour, file, time, radio, select, combobox) carries
`name` + `value` ATTRIBUTES on its root (`forms::named`), the value being the app's and rebuilt on
every change (W1: "any other node with name + value attributes contributes its value"). The
checkbox is only named while checked (HTML leaves an unchecked checkbox out).

- Submit (either submit button, or Enter in a text field) -> `on_form_submit(FormData)` prints
  one `name = value` line per entry (password as bullets) and a verdict: all valid, or which
  names fail (`FormData::invalid`) - the form itself paints those fields `:user-invalid`.
- Reset -> the Form's own reset empties its text fields, then `on_form_reset` puts
  `FormValues::initial()` back and rebuilds. It first ACKS all typed text
  (`info.mark_text_revision_synced(info.get_document_text_revision())`): an unacked engine edit
  outranks a rebuilt field that says otherwise (the W1 engine gap), so without the ack a field
  typed into after an earlier rebuild would keep its text. Every initial text is empty on purpose
  (the Form's reset can empty an engine text line but cannot type into it).

### "Raw HTML inputs"

One app-built `Form` around two columns of RAW controls that the engine replaces with widgets:

- "Built in Rust": `Dom::create_input(type, name, label, SmallAriaInfo::label(label))` +
  `.with_attribute(..)`: text (placeholder), email, range (min 0 / max 10 / value 7), color
  (value), date (value), checkbox (checked), a three-radio group (one name, Medium checked),
  `Dom::create_select` with two `create_optgroup_no_a11y` groups of `create_option_no_a11y`,
  type=submit ("Send raw") and type=reset ("Reset raw").
- "Parsed from XML": `Xml::from_str(RAW_XML)` + `Dom::create_from_parsed_xml`, the body's content
  mounted (the call returns a whole `html > body` document): month, week, time, datetime-local,
  number, search, text + `list=` a `<datalist>`, textarea, type=image without src (alt text is
  the label), hidden. Captions are `<p style=..>` inside the XML.

Every raw control has an `id` (the key the engine's form memory remembers the user's value under).
Submit prints the FormData the replaced controls hand over; Reset: the engine forgets the user
values and rebuilds from the HTML defaults, the app acks typed text as above. The section note says
the replacement has no theme input yet, so these are always flat (see Findings 1).

## E2E scenarios

- `e2e/global_hotkey.json` - NOT changed. It addresses nodes by text ("Enable Ctrl+Alt+K",
  "Disable Ctrl+Alt+K", "Retry Ctrl+Alt+K", "1 time(s) since it was enabled"); no new text
  contains those. The hotkey section moved down by the Spinner row's captions (~20 px) and the
  Frame (~80 px); it was already far below the fold, so if the scenario passed before it does not
  depend on the viewport.
- NEW `e2e/every_input_form.json` (unrun): (1) the first form - nothing printed at start, click
  `#form-submit`, assert `#form-data-{form-id,newsletter,plan,food,day,full-name}` exist, click
  `#form-reset`, assert `#form-data-form-id` gone; (2) the raw form - click `#raw-submit`, assert
  `#raw-form-data-{brightness,terms,size,pet,source}` exist (source = the XML hidden input).
  Each printed line has the id `<prefix>-<name>`; each click is preceded by
  `scroll_into_view` (selector, `block: center`, `behavior: instant`). The section note says
  "submit" / "reset" in lower case, so a click by TEXT on "Submit" / "Reset" lands on the
  buttons too. If (2) fails, suspect the replaced Button not keeping `id`, or the memory keys of
  the replaced range / checkbox / radio / select (G1's `replaced_submission`).

## Commits

| hash | what |
|---|---|
| `04ce4d69b` | progress file |
| `6417fbf9a` | feat: every themed widget follows the Flat / Flora toggle (+ Spinner row, Frame, video poster, TextArea round trip) |
| `97ddb2be9` | checkpoint |
| `3d29a3808` | feat: every HTML input type, as its widget inside a Form (`src/forms.rs`) |
| `25fdcf27d` | checkpoint |
| `802c35291` | feat: raw HTML inputs, replaced by the same widgets (+ Cargo.toml description) |
| `5edd7a92f` | style: rustfmt forms.rs, the ack comment |
| `bbfa6b5e2` | checkpoint |
| `141c193e2` | test: E2E scenario for both forms (+ line / button ids, lower-case note) |
| (this) | report + final checkpoint |

Checked without compiling: the preflight demo contracts (`check_demo_state_round_trip`,
`check_demo_accessibility`, `create_div_with_text`) pass over lib.rs AND forms.rs (ran their regex
logic by hand); a scan of every CSS literal in lib.rs / forms.rs / video.rs / notifications.rs
finds no fixed colour (all `system:`); `page_frame()`'s literal anchors in lib.rs are unchanged;
forms.rs is rustfmt-clean (stable rustfmt).

## Least sure to compile (check these first)

1. `forms.rs` imports: `azul::css::SpinnerStyle` is in `css` (not `widgets`) in the current
   `reexports.rs`; `azul::option::OptionImageRef`; `azul::image::{ImageRef, RawImage,
   RawImageData, RawImageFormat}`; `azul::dom::{AttributeType, SmallAriaInfo}`;
   `azul::str::String as AzString`.
2. `forms::send_icon`: the `RawImage { pixels: RawImageData::U8(vec.into()), width, height,
   premultiplied_alpha, data_format, tag: Vec::<u8>::new().into() }` literal and
   `ImageRef::create_rawimage(..) -> OptionImageRef`; `extend_from_slice(if .. { &INK } else
   { &CLEAR })` over two `[u8; 4]` consts.
3. `forms::xml_controls`: `Xml::from_str(..).into_result()` (the hand-written
   `AzResultXmlXmlError::into_result`), `document.children.as_slice().first().map(|b|
   b.children.clone())`. Also a RUNTIME dependency: the dylib must be built with the layout `xml`
   feature (the dll depends on azul-layout with `xml` on, and `build-dll` lists it).
4. `keep(&mut data, |v| v.browser = state.text)` / `|v| v.attachment = state.path` - FnOnce
   closures moving a field out of a by-value callback argument (edition-2021 precise capture).
5. `keep_text(.., |v, t| v.full_name = t)` - non-capturing closures coerced to
   `fn(&mut FormValues, AzString)`.
6. `DropDown::create(strs(&[]))` - an empty array literal typed through `strs(&[&str])`.
7. `Button::create_image(icon, "..")` takes `ImageRef` by value (not `Into`), matched out of
   `demo.send_icon.clone()`.
8. lib.rs `text_area_text(&TextAreaState) -> String` via `state.text.as_slice()` (`AzU32Vec`).
9. `Frame::create("Shipping", Dom)` + `with_flex_grow(0.0)` + `with_theme`.

## Findings (not fixed - outside `examples/azul-widgets/`)

1. **The raw-control replacement has no theme input** (`layout/src/form_controls.rs` has no
   `theme` at all): a raw `<input>` becomes a FLAT widget under a Flora page. Candidates: a
   `data-azul-theme` attribute on the raw node or an ancestor (the Form's `__azul-theme-*` class
   would do), or a window-level widget theme the pass reads.
2. **ComboBox and FileInput have no `theme`** (no `with_theme` in the bindings) - the only
   form widgets that look the same in both themes.
3. **No FFI way to read FormData outside `on_submit`**: `widgets::form::collect_form_data` is
   Rust-only (G1 suggests `CallbackInfo::get_form_data`), so a raw `<form>`'s carried Submit
   handler cannot read its values from C / the bindings. That is why the raw section uses an
   app-built `Form` around raw controls instead of a raw `<form>`.
4. **The reset ack is window-wide**: `mark_text_revision_synced` has no per-node form, so a
   form reset also retires the unacked typing of every OTHER field. The demo copes (every text
   field hands its text back; raw fields live in the form memory), but a ComboBox's
   typed-but-unpicked text falls back to its last pick. A per-node ack (or W1's proposed
   `SetNodeValue` change that supersedes the overlay) would let `reset_form` restore any initial
   text itself.
5. **Lint coverage**: `layout/tests/azul_widgets_demo_follows_the_theme.rs` (colour lint) and
   `scripts/preflight_contracts.py` (round trip / a11y names) read lib.rs (+ video.rs,
   notifications.rs) only. Add `src/forms.rs` to both (`include_str!` in `demo_styles()`; a
   glob in the two preflight checks) - forms.rs passes them today. Note for the lint: RAW_XML is
   one long literal with `:` and `;` (its `style='..'` attributes), which the lint will parse as a
   style; it names no colour, so it passes.
6. Not in the demo (no UiTheme, or app chrome rather than a control): TabHeader, ListView,
   TreeView, Ribbon, StatusBar, Backstage, QuickAccess, Titlebar (the demo draws its own), Map,
   NodeGraph, Camera / ScreenCapture / Microphone, Label (not in api.json), Menubar (Rust-only),
   Toast (the demo deliberately shows a native notification instead). Modal / Popover are shown
   through Dialog, which builds both.

## Behavioural risks (not compile)

- Whether `name` + `value` on the ROOT of CheckBox / Slider / ColorInput / FileInput / TimePicker
  / RadioGroup / DropDown / ComboBox is what `current_form_data` reads depends on those roots not
  holding a TextInput / TextArea / DatePicker state as dataset (they do not, as far as I read);
  NumberInput's root does hold its TextInput's state, so it is read live without a value.
- The reset relies on the RefreshDom after `on_reset` being a new layout generation (so the acked
  overlay entries are dropped) - `mark_text_revision_synced`'s doc says exactly that.
