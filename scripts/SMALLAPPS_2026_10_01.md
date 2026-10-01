# SMALLAPPS: AzCalculator, AzContacts and the shared app skeleton (2026-10-01)

Branch `wt/smallapps` from `16d19442c`. Nothing was compiled (house rule): every Rust file was
parse-checked with `rustfmt --check`, the Python scripts byte-compiled. The unit tests, the builds and the
two E2E scripts are for the parent (commands below). The model code was written test-first: for every model
file a RED commit (the API and the tests, the bodies `todo!()`) and a GREEN commit (the bodies).

## What was built

### Engine: the debug server's key names (`layout/src/e2e/`)
`key_down` / `key_up` knew letters, digits, the editing keys, arrows, F-keys and modifiers only; `plus`,
`slash`, `numpad_multiply`, ... parsed to nothing and the step was a silent no-op. `parse_virtual_keycode`
now names every punctuation key of `VirtualKeyCode` (word and character: `plus` / `+`, `slash` / `/`,
`period` / `.`, ...) and the numeric keypad (`numpad7` / `numpad_7`, `numpad_add`, `numpad_enter`, ...).
RED `6c7492e66` (new `layout/src/e2e/key_names_tests.rs`, `parse_virtual_keycode` -> `pub(crate)`), GREEN
`8a5e2f886`. Needed by the calculator E2E (operators by keyboard).

### `examples/azul-appkit` (package `azul-appkit`, lib `azul_appkit`) - build ledger F2 / F6 / F7
The skeleton every Azlin app shares (precedent: `azul-storage`, a shared example crate). Plain modules,
tested without libazul (`cargo test -p azul-appkit`):
- `args`: `--screen` (one of the app's screens), `--size WxH`, `--theme flat|flora`, `--mode
  system|light|dark` (both override settings.json for one run), `--shot PNG` / `--shot-delay-ms`,
  `--sample`, `--data-dir`, bare files; both spellings of a valued switch; unknown switches refused; `-h`.
- `data`: the data root `--data-dir > $AZLIN_DATA > <OS data dir>/Azlin`, one folder per app, keys as the
  user's S3 bucket will have them (`calculator/history.jsonl`, `contacts/<uuid>.vcf`); UUID v4 without a
  new crate.
- `settings`: `<app>/settings.json` (theme, mode, the app's string values); forgiving reads, stable writes.
- `shortcuts`: one table per app, `Mod` = Cmd on macOS / Ctrl elsewhere, grouped, searchable.
- `about`: the About rows. `files`: put / get / get-all / delete jobs on an azul-storage `Drive`.

Feature `azul` (`ui.rs`): the `Kit` (one RefAny per app: settings read at start, data root, the settings
page's state), `app_config` / `window_options` (`NoTitle`, `--size`, min size, theme + mode), `title_row`
(azul's `Titlebar`), the **settings page on azul's `ShellSettingsLayout`** (the app's categories, then
Appearance - Flat/Flora and System/Light/Dark applied at once via `set_theme` / `set_mode` and saved -,
Data, Keyboard shortcuts, About; search; Back), `handle_key` (Mod+, opens, F1 opens at the shortcuts,
Escape closes), `spawn_file_jobs` (file jobs on an azul `Thread` through azul-storage's `LocalDrive`, the
outcomes back through a write-back `FileReply`), the `--shot` timer (screenshot, write, exit).
**TODO(DIALOGS)** in `about.rs` / `ui.rs`: azul's About dialog (the DIALOGS task) plugs in there; today About
is a section of the settings page.

### `examples/azul-calculator` (package `AzCalculator`) - ledger step 1 / F9
Model (plain Rust, ~170 unit tests):
- `num`: exact decimals on `bigdecimal` 0.4.10 (already in Cargo.lock via turso_core, in the local
  registry): `0.1 + 0.2 = 0.3`, `1,280 x 0.19 = 243.2`, `1/3 x 3 = 1`; 50 working / 32 shown digits;
  overflow beyond 10^9999 and division by zero are errors, never panics (bigdecimal panics on /0 - checked
  first); f64 results (sin, ln, x^0.5) rounded to 15 digits; grouping, scientific notation, F-E; a number
  being typed keeps its trailing zeros.
- `expr`: tokens, a precedence parser (OR < XOR < AND < shifts < + - < x / mod < unary < ^ right-assoc <
  postfix), implicit multiplication, auto-closed parentheses, the percent rules (`a + b%` = a + a*b/100,
  `a x b%` = a*b/100), 21 functions, pi / e, DEG / RAD / GRAD with exact quarter turns, exp / 10^x beyond
  f64 range, syntax errors that say what is wrong; Programmer evaluation over the word size.
- `programmer`: QWORD/DWORD/WORD/BYTE wrapping, HEX/DEC/OCT/BIN, two's complement display, shifts, rotates,
  the bit field. `units`: 8 categories, exact factors, temperature offsets. `datecalc`: differences (years
  / months / days / weeks) and date arithmetic. `history`: `calculator/history.jsonl` (one JSON object per
  line, 200 kept), the memory.
- `calc`: the input model (start over after `=`, continue with the result, `=` repeats, operator
  replacement, `5 x =` = 25, function keys wrap the operand, +/- and the exponent sign, CE / C / Backspace,
  memory, bases / word sizes / bits, history recall, copy / paste) and the keyboard map.

Window (`ui.rs`) on **azul's S9 `UtilityShell`**: the Titlebar as its title row, the mode switch
(Standard, Scientific, Programmer, Date, Convert) + History / Settings buttons, display, keypads (Button
grids; Programmer dims digits its base refuses), the 64-bit field, the History / Memory panel (TabHeader)
beside the keypad from 620 px (Ctrl+H otherwise), the date and converter screens, appkit's settings page
plus "Display" and "History" sections. Every keypad key has a keyboard key (US key positions for shifted
characters; Windows' letters `@ q r s o t n l p`), Enter / Escape / Delete / Backspace / F9, Alt+1..5,
Ctrl+M/R/P/Q/L, F3-F5 (angles), F5-F8 (bases). Ctrl/Cmd+C copies the result (keydown + the Copy event),
Ctrl/Cmd+V pastes an expression through the Paste event (`get_clipboard_content` is filled only during a
paste). The history file is read when the window opens and written after each calculation (one write in
flight at a time). Stdout `AZCALC_*` lines for scripts.

### `examples/azul-contacts` (package `AzContacts`) - ledger A4
Model (~60 unit tests): `vcard` (unfold / fold at 75 octets without cutting a character, escaping,
parameters in every spelling incl. quoted values and vCard 2.1 bare types, groups `item1.TEL`, structured
and list values, several cards per file, broken input reported not fatal, writer with CRLF), `contact`
(names, company cards, mononyms, labelled phones / emails / addresses / URLs, birthday with or without a
year incl. Apple's 1604 convention, notes, photo as inline base64 (3.0) or data: URI (4.0), groups,
favourite, custom fields `X-AZLIN-FIELD`, unknown properties kept for the round trip, Apple `X-ABLabel`
labels, the edit form's checks), `book` (sort by first / last name with folded diacritics, `#` after Z,
letter sections, A-Z jump target, initials, search incl. phone digits, groups), `dupes` (same email /
phone (last 9 digits) / name, scores, ignore list, merge keeping both sides of every list), `store` (one
`contacts/<uid>.vcf` per contact, written as vCard 4.0, the file name is the identity, import preview new /
update / duplicate, export), `sample` (300 deterministic contacts per the plan: diacritics, Chinese and
Arabic names, a mononym, a company card, DE / US / JP addresses, 12 favourites, 8 groups, exactly 3
duplicate pairs).

Window (`ui.rs`) on **azul's S4 `PimShell`**: toolbar in the ribbon row, `ShellNavigationPane` (All
contacts, Favourites, Possible duplicates, the groups with counts), the list (search, sort, letter
sections, initials avatars, stars, the A-Z bar scrolling to a section via `scroll_node_into_view`), the
card (Edit, Favourite, Copy vCard, Export, Mail = copy the address until AzMail takes hand-offs, Delete
with confirmation), the edit form (labelled repeatable rows with add / remove, birthday as DD.MM.YYYY or
DD.MM., groups as removable chips, custom fields, notes, favourite, a photo picked with `FileDialog` and
stored as a data: URI, problems inline, Cancel asks before discarding), the import preview (a path or
FileDialog; checkboxes; an optional group), the merge screen (Left / Right per field, keep both notes,
"Not a duplicate" remembered in settings.json), a StatusBar with counts. Every write is a file job on a
Thread (`contacts/<uid>.vcf`, `exports/contacts-<time>.vcf`); `--sample` writes the 300 sample files into
an empty folder; a `.vcf` given on the command line opens its import preview. Keys: Mod+N / E / S / I / D,
Mod+Shift+E, Escape, Up / Down; appkit's Mod+, and F1. Stdout `AZCONTACTS_*` lines.

### E2E
- `scripts/azlin_e2e.py`: ONE driver for the Azlin apps' headless scripts (start with
  `AZ_BACKEND=headless AZ_DEBUG=<port>`, ops, `wait_frame` after every state change, stdout lines,
  screenshots).
- `scripts/azcalculator_e2e.py`: sample history load, 1280 x 0.19 by mouse, 0.1+0.2 and 7*6 by keyboard
  (needs the new key names), Backspace / Escape, Ctrl+C, history.jsonl content, Scientific sin(30)+2^10,
  Programmer F5 + 2A5F + bit 0, Convert / Date screens, settings Flora + Dark saved, screenshots.
- `scripts/azcontacts_e2e.py`: 300 sample files, import preview of a fixture `.vcf` (3.0 folded line,
  escaped comma; 2 new + 1 duplicate), Import, search `krug`, select, a new contact refused with a bad
  email then saved (file checked), merge of the first duplicate pair (one file goes), the A-Z bar,
  settings Flora + Dark, delete (file goes), screenshots.

## Commits

| hash | what |
|---|---|
| `6c7492e66` / `8a5e2f886` | e2e key names (RED / GREEN) |
| `7c4ce8bd6` / `75f052000` | azul-appkit (RED / GREEN + ui) |
| `02a3d355f` / `8fccfdc91` | AzCalculator model (RED / GREEN) |
| `2c1c8a6b2`, `93e30c7bd`, `c2ce0f107` | AzCalculator window (pieces, wired) |
| `c64368968`, `30d459a21` | azlin_e2e.py, azcalculator_e2e.py |
| `392058389` / `f176d2f3b` | AzContacts vcard (RED / GREEN) |
| `66d7fae9b` / `afee1f564` | contact (RED / GREEN) |
| `0d194a6e3` / `28a5ca6ed` | book (RED / GREEN) |
| `887b60931` / `89f0b41ef` | dupes (RED / GREEN) |
| `15a67f7d0` / `b85fc8269` | store (RED / GREEN) |
| `a5111bd72` / `716a23d94` | sample (RED / GREEN) |
| `8d1010329`, `d1ac2b4ae`, `75d7df15f`, `6148ee264` | AzContacts window (pieces, wired) |
| `1604b0505` | azcontacts_e2e.py |
| `chore(smallapps)` commits | SMALLAPPS.PROGRESS.md checkpoints |

## api.json

**No change.** No public azul API was added or changed: the apps use existing api.json entries only
(`azul::shells::{UtilityShell, PimShell, ShellNavigationPane, ShellSettingsLayout, ShellEmptyState,
ShellThemeScope}`, widgets, `CallbackInfo` methods). The key-name table is internal to the debug server.

## Registration (append-only)
Root `Cargo.toml` members `examples/azul-appkit`, `examples/azul-calculator`, `examples/azul-contacts`;
`scripts/workspace_test_members.txt` (three `tested:` lines); `.github/workflows/rust.yml` `dll_tests`:
"Run azul-appkit tests" (no libazul), "Run AzCalculator tests", "Run AzContacts tests" (AZ_LINK_PATH), and
the job's comment line. `Cargo.lock` gains `azul-appkit`, `AzCalculator`, `AzContacts` (and `bigdecimal`
becomes a direct dependency of AzCalculator; same version 0.4.10) on the first build.

## Least-sure-to-compile spots (read in this order)

1. `examples/azul-appkit/src/ui.rs`: `Timer::create(RefAny, shot_tick, info.get_system_time_fn())`,
   `info.callback_info.take_screenshot(DomId { inner: 0 }).into_result()` + `png.as_slice()`;
   `Thread::create(init, reply_to, file_thread)` and `ThreadWriteBackMsg::create(on_done, ..)` with
   `on_done: WriteBackCallbackType` carried in the init data; `window.create_callback =
   Some(Callback::create(on_create)).into()`; `OptionLogicalSize::Some(..)` for `min_dimensions`;
   `ShellSettingsLayout::add_section` (refmut) on a `let mut`.
2. RefAny borrow discipline: `downcast_ref` / `downcast_mut` take `&mut self`; every helper clones the
   RefAny into a `mut` local first. A `downcast_mut` of the app while a `Ref` of the same RefAny is alive
   returns `None` (runtime, not compile) - the callbacks take the app's state once.
3. `bigdecimal` 0.4.10 API used by `num.rs`: `from_str` with exponents, `with_prec`, `digits`,
   `normalized`, `as_bigint_and_exponent` (+ `BigInt::to_string`), `sqrt` (Option), `cbrt`, `powi`,
   `is_integer`, `BigDecimal::new(1.into(), -n)`, `ToPrimitive::to_f64/to_i64`, `Zero::is_zero`, owned
   `+ - * / %`, `Neg`.
4. `examples/azul-calculator/src/ui.rs`: `key_char` matching `VirtualKeyCode` variants (Plus, Asterisk,
   Caret, At, Numpad*); `info.get_current_window_state().size.dimensions.width`;
   `EventFilter::Focus(FocusEventFilter::Paste / Copy)` on the body; `DropDown::with_selected`,
   `TabHeader::with_active_tab`, `Switch::create(bool)`.
5. `examples/azul-contacts/src/ui.rs`: `info.get_node_id_by_id_attribute(dom, String)` +
   `NodeHierarchyItemId::into_raw() != 0` as "found" + `scroll_node_into_view(DomNodeId { dom, node },
   ScrollIntoViewOptions::start())`; `TextAreaState.text.as_slice()` -> chars; `FileDialog::open_file(..,
   on_photo_picked)` with `FileOpenResult::downcast(..)` (AzDrive's pattern); `Chip::with_on_remove`,
   `CheckBox::with_accessibility_name`, `StatusBarSegment::with_on_click`, `ShellNavigationGroup::with_count`.
6. `on_key` in AzContacts forwards to other callbacks by value (`on_new(data, info)`) after
   `info.prevent_default()`.
7. Name clash avoided: the app states are `CalcApp` / `ContactsApp` (the prelude has `App`).

## Test commands (the parent)

```sh
cargo test --release -p azul-layout --lib --features e2e-server e2e::key_names_tests
cargo test --release -p azul-appkit
AZ_LINK_PATH=$PWD/target/azul-lib cargo test --release -p AzCalculator --lib
AZ_LINK_PATH=$PWD/target/azul-lib cargo test --release -p AzContacts --lib
AZ_LINK_PATH=$PWD/target/azul-lib cargo build --release -p AzCalculator -p AzContacts
./scripts/workspace_test_coverage.sh
# E2E: libazul with the debug server; ONE app at a time, through the capped runner
<scratchpad>/run_capped.sh --cap-mb 1500 --seconds 180 --log /tmp/azcalc.log -- \
  env DYLD_LIBRARY_PATH=$PWD/target/azul-lib python3 scripts/azcalculator_e2e.py \
  --bin target/release/AzCalculator --out /tmp/azcalc-shots
<scratchpad>/run_capped.sh --cap-mb 1500 --seconds 240 --log /tmp/azcontacts.log -- \
  env DYLD_LIBRARY_PATH=$PWD/target/azul-lib python3 scripts/azcontacts_e2e.py \
  --bin target/release/AzContacts --out /tmp/azcontacts-shots
# screenshots per screen without the E2E:
target/release/AzCalculator --screen scientific --theme flora --mode dark --shot /tmp/sci.png --data-dir /tmp/azlin
```

## Decisions taken unattended
- The shared skeleton is a crate (`azul-appkit`), not azul code: switches, data layout and settings files
  are app-suite concerns; reusable WIDGETS would go into azul (none were needed).
- Precedence in every mode (GNOME / macOS), not Windows' immediate-execution Standard mode.
- Contact files are written as vCard 4.0 (TEL as free text, which RFC 6350 allows, so formatting survives).
- Calculator settings remember the last mode; contacts remember sort, export version, ignored pairs.

## What is left
- **Button toggle variant** (plan: 2nd, F-E, bit toggles) is not in azul: the active 2nd / F-E keys use
  `ButtonType::Primary`; the bits are plain clickable cells.
- **About dialog**: the DIALOGS task's dialog plugs in at the TODO(DIALOGS) marks.
- Photo PREVIEW (decode the data: URI into an `ImageRef` for `Avatar::create_with_image`); initials today.
- Phone layouts (S11 `MobileShell`) for both apps; CSV import with column mapping; group rename / colour;
  drag rows onto groups; list virtualization for thousands of contacts; locale-aware number formatting
  (`IcuLocalizer`) in the calculator; currency rates; export through a save dialog (it writes to
  `exports/` in the data folder now); hand-offs to AzMail / AzMeet / AzChat (copy for now).
- Known widget limits met: `TextArea` does not mirror Enter (InputFixes); birthday is a text field rather
  than `DatePicker` (year jump missing).
- The E2E scripts were never run here.
- Twins (reported, not merged): `examples/azul-shells/scripts/shells_e2e.py` carries its own copy of the
  App driver that `scripts/azlin_e2e.py` now offers; AzWriter's `args.rs` predates `azul_appkit::args`
  and could move onto it.
- Files outside the task touched minimally: `layout/src/e2e/full.rs` (one `pub(crate)` + the name table),
  `layout/src/e2e/mod.rs` (one test module), the three registration files (append-only).
