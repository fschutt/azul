<!-- As given to the wave-5 agents (2026-10-02). The /private/tmp paths are gone (wiped by the 2026-10-02 reboot);
     the tools now live in scripts/waves/tools/. -->
# House rules for every agent (azul, PR #476) - wave 5, 2026-10-02

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
  `/private/tmp/claude-501/-Users-fschutt-Development-azul/344f2a1f-485e-4b53-8631-15c97f8eeca1/scratchpad/run_capped.sh --cap-mb 1500 --seconds 120 --log <file> -- env AZ_BACKEND=headless AZ_DEBUG=<port> /Users/fschutt/Development/azul/target/release/<App>`
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
- Finish with a report `scripts/<TASK>_2026_10_02.md`: what was built, commit list, the api.json list,
  least-sure-to-compile spots, the exact test commands for the parent, what is left. Commit it.
- Other agents run in parallel on other branches. Stay inside your task's files; if you must touch a file
  another task owns (listed in your prompt), keep the edit minimal and say so in the report.
- Theme parts: when a node's style stacks several merged parts (base, then a state part), stack them with
  `crate::widgets::themes::theme_blocks::stack_parts(base, extra)`.

## Wave 5 (2026-10-02) - read these first
- Report file: `scripts/<TASK>_2026_10_02.md`; progress file `scripts/<TASK>.PROGRESS.md`.
- The duplication / reuse / api.json reviews of the last night are in `scripts/DEDUP_EDITORS_2026_10_02.md`,
  `scripts/DEDUP_OFFICE_2026_10_02.md`, `scripts/DEDUP_WIDGETS_API_2026_10_02.md`. Read the parts your task names.
  Every item you fix: name its finding id (e.g. "DEDUP_OFFICE D7") in the commit message and the report.
- Widgets to build on (compiled, tested, in api.json): `scripts/SHELLS_2026_09_30.md` (OfficeShell, the S1..S11
  shells, ShellNavigationPane, ShellCommandPalette, ShellSettingsLayout, ShellEmptyState, ShellThemeScope),
  `scripts/MAILWIDGETS_2026_09_30.md` (MessageList, ReadingPane, InfoBar, ToDoBar, ModuleSwitcher, WizardLayout,
  DatePicker, StatusBar), `scripts/DIALOGS_2026_10_01.md` (dialog_kit, wizard pages, standard dialogs:
  AboutDialog, ProgressDialog, MessageBox, FindReplaceDialog, ShellSettingsDialog).
- IDS (user ruling 2026-10-02): an id that leaves the process (a file or folder name, a drive / S3 key, a record
  another device reads) is `azul::uuid::Uuid::from_seed(azul_storage::ids::random_seed())`. `Uuid::v4()` /
  `Uuid::short()` are process-local markers only (deterministic: the same sequence in every run).
- Fixed today, do not work around them: key ops' modifiers (Shift+8 is `*` headless), Backspace/Delete reach app
  handlers on a non-editable focus, `line-height` in pt/in/cm/mm/em, percent-wide images in an inline context,
  multi-column blocks (`solver3/multicol.rs`), clipped content no longer adds PDF pages.

## API module moves in the base of wave B (16d19442c) - the generated names
`azul::window::WindowDecorations`, `azul::dom::StyledDom`, `azul::video::{VideoDecoder, VideoEncoder}`,
`azul::widgets::{UiTheme, MapTheme, SpinnerStyle}`, `azul::shells::*` (OfficeShell, ShellPane, the S1..S11 shells,
ShellNavigationPane, ShellCommandPalette, ShellSettingsLayout, ShellEmptyState, ShellThemeScope), callback wrappers
`*Callback` in `azul::dom`, their `*CallbackType` in `azul::callbacks`, `Option*` in `azul::option`, `*Vec` in
`azul::vec`. When in doubt, read api.json in your base (module = the key above the class).

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
