# House rules for every agent (azul, PR #476) - wave 6, 2026-10-03 (current)

You work in your own git worktree of /Users/fschutt/Development/azul (the azul Rust GUI toolkit). Your first
command creates your branch from the base commit your prompt names:
`git -C <your worktree> checkout -b <your branch> <base>`. Use `git -C <worktree>` and absolute paths inside your
worktree; never `cd` into or edit the main checkout.

## Never
- Never compile: no `cargo build/test/check/clippy/run/fmt`, and no rust-analyzer LSP (it builds `target/debug`,
  and the disk is small). The parent compiles once and runs the suites. Write code that compiles by reading the
  surrounding code carefully; list the spots you are least sure of. Non-cargo tools (python3, node, grep,
  clang -fsyntax-only on a file you wrote, `rustfmt --check` as a parse check) are fine.
- Never `git reset --hard`, `git stash`, `git checkout -- .`, `git clean`, never force anything, never rebase.
  Stage explicit paths only.
- Never edit `api.json` by hand: list every public API change in your report in api.json terms (constructors
  named `create*`, never `new`; callback args are `CallbackType` with fn_body
  `azul_layout::callbacks::Callback::create(callback).to_core()`; `&str` / `&[T]` args become `String` /
  `XxxVecSlice`; everything crossing the FFI is `repr(C)`, fields ordered by decreasing alignment so there is no
  padding - new fields go near the END of a struct; docs ASCII only).
- Never touch `layout/src/solver3/page_breaks.rs` or `layout/tests/a_padded_table_cell_stays_in_its_row.rs`
  (another session owns them).
- The Mac has 8 GB of RAM and panicked under memory pressure (2026-09-30, an app reached 17.7 GB): when you run
  the parent's prebuilt binaries headless, run ONE at a time, ALWAYS through the capped runner, and kill it when done:
  `/Users/fschutt/Development/azul/scripts/waves/tools/run_capped.sh --cap-mb 1500 --seconds 120 --log <file> -- env AZ_BACKEND=headless AZ_DEBUG=<port> /Users/fschutt/Development/azul/target/release/<App>`
  (it sets DYLD_LIBRARY_PATH, kills the app at 1.5 GB RSS or after --seconds, exit 137 = capped). Never run an
  app with a `<video>` in it (AzWidgets) for longer than needed. No rust-analyzer, ever (it took 5.7 GB).

## Always
- ROOT CAUSES (user ruling): find the root cause of every bug, fix it in the engine where it lives, no app
  workarounds for engine bugs.
- RED first: every behaviour change is a test commit that states the correct behaviour (test names are
  sentences), then the fix commit. New layout integration tests go in `layout/tests/<name>.rs`, registered in
  `layout/tests/all.rs` by APPENDING one `#[path]` + `mod` pair at the very end of the file.
- A headless test that settles its window with direct `regenerate_layout()` calls must then call
  `let _ = window.common.take_regeneration();` (a new window is born owing a rebuild; the frame path consumes it).
- Commit messages via a file (`git commit -F <file>`), ending with:
  Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
- Widget callbacks are invoked with `.invoke(..)`, never `(cb.cb)(..)`.
- In the shared theme files `layout/src/widgets/themes/flat.rs` / `flora.rs`, APPEND new code at the END under a
  `// ==== <widget> ====` banner; never reorder existing code.
- NO DUPLICATION (user ruling): before writing a helper, search for an existing one and use or extend it. One
  generator per concern, one helper per concern. If you find two existing twins, say so in your report.
- NAMING (user ruling): "theme" = the app theme (flat / flora / native / user themes); "mode" = dark / light /
  system (`DarkLightMode`, CSS condition `DynamicSelector::Mode(ModeCondition::..)`). New API never calls
  light/dark a "theme".
- CHECKPOINT: keep `scripts/<TASK>.PROGRESS.md` with DONE (commit hashes) / IN PROGRESS / NEXT / open questions,
  and commit it after every commit. If you are resumed, read it first and continue from NEXT.
- Finish with a report `scripts/<TASK>_2026_10_03.md`: what was built, commit list, the api.json list,
  least-sure-to-compile spots, the exact test commands for the parent, what is left. Commit it.
- Other agents run in parallel on other branches. Stay inside your task's files; if you must touch a file
  another task owns (listed in your prompt), keep the edit minimal and say so in the report.
- Theme parts: when a node's style stacks several merged parts (base, then a state part), stack them with
  `crate::widgets::themes::theme_blocks::stack_parts(base, extra)`.

## Wave 6 (2026-10-03) - read these first
- Your brief: `scripts/waves/wave6/<TASK>.md` (in your worktree). The plan: `scripts/waves/wave6/PLAN.md`.
  Report `scripts/<TASK>_2026_10_03.md`; progress `scripts/<TASK>.PROGRESS.md`.
- Earlier reports to read for your area: the DEDUP reviews (`scripts/DEDUP_{EDITORS,OFFICE,WIDGETS_API}_2026_10_02.md`,
  name the finding id you fix in the commit message) and the wave-5 reports (`scripts/{HYGIENE,TEXTENG,MAILHTML,
  APIEXPORT,BLOCKS,PIM,RTE,TABLES}_2026_10_02.md`), plus your app's own earlier reports (`scripts/*<APP>*_2026_*.md`).
- BUILDING BLOCKS that now exist (compiled, in api.json) - use them, never re-implement them:
  `RichTextEditor` + the `RichTextDoc` model (widgets/rich_text*; Markdown/HTML/plain serializers, one undo history);
  `ListSelection` (click / Ctrl / Shift selection, u64 keys, `key_of(name)`); `azul_appkit::UndoHistory<T>`;
  `CloseGuard` + `CallbackInfo::prevent_window_close()`; `azul-pim` (dates, RRULE/repeat, iCal/vCard lines,
  addresses, search, initials, the shared task store); `KeyModifiers::primary_down()` (Cmd on macOS, Ctrl
  elsewhere - never `ctrl || meta`); `Button::with_disabled` / `with_toggled`; `StatusBarZoom::create(percent,
  min, max)`; Ribbon `with_items` / `with_groups`; `ColorU::to_hex` / `parse_hex` / `parse_css`;
  `RawImage::create_rgba8` / `resized`; `DiskSpace::format_bytes`; `NodeData::get_attribute(s)`;
  `NodeType::get_text`; `TextAreaState::get_text`; `DatePicker::with_week_start`; `GlobalHotkey::matches`;
  `ShellThemeAccent::colors`; the standard dialogs (AboutDialog, ProgressDialog, MessageBox, FindReplaceDialog,
  ShellSettingsDialog); `azul-appkit` (args, data root, settings, shortcuts, about, file jobs, history, the settings
  page); the one theme-helper module `layout/src/widgets/themes/decl.rs` (style_kit is gone).
- USER RULINGS 2026-10-02 (in force):
  - IDS: an id that leaves the process is `azul::uuid::Uuid::from_seed(azul_storage::ids::random_seed())`
    (`Uuid::v4` / `short` are process-local markers: the same sequence in every run).
  - EXPORT / DATA: there is no S3 sync yet. Every durable write - exports included - goes INTO the data tree
    through the azul-storage `Drive`; a `.azlin/cache` manifest in the data root records what is there (INFRA6
    builds it inside the drive, apps do nothing extra), so the later S3/database sync only diffs the tree.
  - PREFIXES: every app's CSS classes and ids carry the app's prefix, like the widgets' `__azul_`:
    `__azmail_`, `__aznotes_`, `__azcal_`, ... Each name is a `const AzString` (`AzString::from_const_str`)
    defined ONCE in the app (an `ids` / `classes` module) - no duplicated string literals (they bloat size).
  - CLOSE: "document modified, save?" lives in the DOM event `EventFilter::Window(WindowEventFilter::
    CloseRequested)`; the veto is `info.prevent_window_close()` (or the `CloseGuard` widget). Never clear
    `flags.close_requested` by hand. `FullWindowState::close_callback` is being REMOVED (INFRA6) - do not use it.
- GENERATED-API GOTCHAS (from the wave-5 integration): the generated wrappers take `impl Into<FooVec>` - pass a
  `Vec` as it is (`.with_items(items)`), an explicit `.into()` has no target type (E0283). `&str` args are
  `String` (`impl Into<AzString>`): pass a literal as it is. A `Type.*` autofix export takes EVERY public method:
  list the exact methods for api.json in your report, and keep Rust-only helpers `pub(crate)`.
- The prebuilt binaries in /Users/fschutt/Development/azul/target/release (and target/azul-lib/libazul.dylib)
  are the wave-5 integration (aa59b2d84): you may run them (capped, one at a time - the runner holds a
  machine-wide lock) to see today's behaviour and to write RED tests against it.

## Apps (examples/azul-<name>), when your task builds or changes one
- One crate per app: `examples/azul-<name>/` with `Cargo.toml` (package `Az<Name>`, lib `az<name>`, bin
  `Az<Name>`; `azul = { path = "../../dll", package = "azul-dll", default-features = false, features =
  ["link-dynamic"] }` exactly as `examples/azul-drive/Cargo.toml`), `src/lib.rs` (the app; unit tests of its
  model live here), `src/main.rs` (thin). Model the structure on `examples/azul-drive` and `examples/azul-writer`.
- Register it: APPEND one line to the root `Cargo.toml` `[workspace] members` (after the last `examples/azul-*`),
  one line to `scripts/workspace_test_members.txt`, and one test step to the `dll_tests` job in
  `.github/workflows/rust.yml` next to the other app steps. Append-only; the parent resolves the conflicts.
- Window: `WindowDecorations::NoTitle` (module `azul::window`) + the app-drawn `Titlebar` widget, like every
  azul app. Built on the matching shell (S1..S11 / OfficeShell) - never a hand-rolled chrome.
- Looks: works in flat AND flora, light AND dark (`DarkLightMode`); the app follows the app theme and the OS mode.
- Data (user ruling, the S3 split): durable data are FILES in the per-user layout the S3 bucket will have later:
  `<data root>/<app>/...` (e.g. `notes/<notebook>/<note-uuid>.md`, `tasks/<list>/<task-uuid>.json`,
  `sheets/<uuid>.xlsx`), written through `azul-storage`'s `Drive` trait on a `LocalDrive` rooted at the user's
  data folder (`examples/azul-storage`, as AzDrive does), from an azul `Thread`, never from a callback. So later
  a `S3Drive` replaces the `LocalDrive` with no other change. No database for content. Sample data on first run
  (a `--sample` flag and an empty-state screen otherwise).
- Settings in a settings window or backstage page built on `ShellSettingsLayout`; About; keyboard shortcuts;
  `--screen <name> --theme <flat|flora> --mode <light|dark>` switches like AzWriter's `args.rs`.
- A headless E2E script `scripts/<app>_e2e.py` against the debug server (`AZ_BACKEND=headless AZ_DEBUG=<port>`,
  ops in `layout/src/e2e/full.rs`: get_node_layout, get_node_hierarchy, click, text_input, focus_node, key ops,
  screenshot, get_mode/set_mode, resize...) that walks the main flows and asserts on node layout / text; model it
  on `scripts/shells_e2e.py` and `examples/azul-drive/scripts/`. Every op that changes state is followed by a
  `wait_frame`.
- Reusable pieces go into azul (`layout/src/widgets/<widget>.rs` + both theme files + tests + manifest), never
  copied between apps; register new widgets by APPENDING to the lists in `layout/src/widgets/mod.rs` (the parent
  merges list conflicts by keeping both sides). App-only pieces stay in the app crate.
- The Rust API the app uses is the generated `azul` crate (the bindings of api.json): read `api.json` for the
  exact class / function names and argument types; a widget whose api.json entry does not exist yet is listed in
  your report in api.json terms (the parent generates it with `azul-doc autofix add`, never by hand).

## Outage resilience (user, 2026-10-01: "make yourselves resistant to network and battery outages")
A network drop or a power loss can end your run at any tool call; you will be resumed from your transcript
and your worktree. Lose nothing:
- Commit after EVERY completed unit (one test, one function, one file) - never hold more than ~15 minutes of
  uncommitted work. A RED commit may be followed by many small GREEN commits.
- Write big files in pieces (create the skeleton and commit it, then fill sections), not one giant write.
- Keep `scripts/<TASK>.PROGRESS.md` exact: the last commit, the precise next step ("next: GREEN for X in
  file Y, function Z"), open decisions. Commit it with the work. On resume read it FIRST, then `git status`
  and `git log -3`, and continue from the recorded next step - never redo finished work.
- Never leave the worktree half-edited across a long reasoning step: finish the edit, commit, then think.
